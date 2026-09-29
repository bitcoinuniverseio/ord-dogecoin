//! Dogemap feed storage and snapshot readers.
//!
//! Storage (P-02): `DOGEMAP_FEED_META` holds the database generation id, the
//! creation coverage start and the journal's first and last height;
//! `DOGEMAP_FEED_JOURNAL` holds, per block, the location of every inscription
//! the block moved and of every prefilter-passing inscription it created.
//! Both are written inside the block's own write transaction, so a crash
//! commits them together with the block or not at all, and a savepoint
//! restore on a reorg rolls them back with everything else. The database id
//! and creation coverage are carried across that restore by
//! `Reorg::handle_reorg`; the journal range is not, because the restored
//! journal genuinely ends at the savepoint.
//!
//! Reads (P-01): every feed response takes its identity, checkpoint and
//! indexed facts from one redb read transaction. Node RPC data (transaction
//! positions, first reveal blocks, output scripts) is fetched after that
//! transaction is dropped and, where it names a height, re-checked against
//! the index in a second short transaction that must still show the same
//! identity.

use {
  super::*,
  crate::dogemap_feed::{self as wire, BlockCursor, EventsHashHeader, FeedErrorCode},
  bitcoin::secp256k1::rand::{self, RngCore},
  bitcoincore_rpc::jsonrpc,
  serde_json::{json, Map, Value},
  std::ops::Range,
};

const META_DATABASE_ID: &str = "databaseId";
const META_CREATION_COVERAGE: &str = "creationCoverageFromHeight";
const META_JOURNAL_START: &str = "journalStartHeight";
const META_JOURNAL_LAST: &str = "journalLastHeight";

/// Stands in for an inscription number the updater could not read; the feed
/// resolves it from the entry table at read time or refuses the block.
const UNKNOWN_NUMBER: u64 = u64::MAX;

const JOURNAL_RECORD_VERSION: u8 = 1;
const JOURNAL_FIXED_BYTES: usize = 1 + 1 + 1 + 36 + 8 + 44 + 44 + 32 + 4 + 8 + 4;

/// RPC calls made on behalf of one feed request time out after this long.
const FEED_RPC_TIMEOUT: Duration = Duration::from_secs(10);

/// Blocks whose computed events are kept, keyed by full identity.
const BLOCK_CACHE_ENTRIES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JournalKind {
  Creation,
  Transfer,
}

/// Where a journaled inscription landed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum JournalDestination {
  /// An output of the transaction being processed (the coinbase for fees).
  Assigned {
    satpoint: SatPoint,
    value: u64,
    script_pubkey: Vec<u8>,
  },
  /// Past the coinbase outputs; the satpoint is the null outpoint and the
  /// lost-sat offset the index stores.
  Lost { satpoint: SatPoint },
}

/// One journal row. A creation's `txid`/`tx_index` are its completing reveal
/// transaction; a transfer's are the transaction whose outputs received the
/// inscription: the spending transaction, or the coinbase for a fee.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JournalRecord {
  pub(crate) kind: JournalKind,
  pub(crate) inscription_id: InscriptionId,
  pub(crate) inscription_number: u64,
  pub(crate) from: SatPoint,
  pub(crate) to: JournalDestination,
  pub(crate) txid: Txid,
  pub(crate) tx_index: u32,
}

impl JournalRecord {
  fn encode(&self) -> Vec<u8> {
    let (status, satpoint, value, script): (u8, SatPoint, u64, &[u8]) = match &self.to {
      JournalDestination::Assigned {
        satpoint,
        value,
        script_pubkey,
      } => (0, *satpoint, *value, script_pubkey),
      JournalDestination::Lost { satpoint } => (1, *satpoint, 0, &[]),
    };
    let mut bytes = Vec::with_capacity(JOURNAL_FIXED_BYTES + script.len());
    bytes.push(JOURNAL_RECORD_VERSION);
    bytes.push(match self.kind {
      JournalKind::Creation => 0,
      JournalKind::Transfer => 1,
    });
    bytes.push(status);
    bytes.extend_from_slice(&self.inscription_id.store());
    bytes.extend_from_slice(&self.inscription_number.to_be_bytes());
    bytes.extend_from_slice(&self.from.store());
    bytes.extend_from_slice(&satpoint.store());
    bytes.extend_from_slice(&self.txid.store());
    bytes.extend_from_slice(&self.tx_index.to_be_bytes());
    bytes.extend_from_slice(&value.to_be_bytes());
    let script_len = u32::try_from(script.len()).expect("output scripts are far below 4 GiB");
    bytes.extend_from_slice(&script_len.to_be_bytes());
    bytes.extend_from_slice(script);
    bytes
  }

  fn decode(bytes: &[u8]) -> Option<Self> {
    if bytes.len() < JOURNAL_FIXED_BYTES || bytes[0] != JOURNAL_RECORD_VERSION {
      return None;
    }
    let mut at = 3;
    let mut take = |n: usize| {
      let slice = &bytes[at..at + n];
      at += n;
      slice
    };
    let inscription_id = InscriptionId::load(take(36).try_into().ok()?);
    let inscription_number = u64::from_be_bytes(take(8).try_into().ok()?);
    let from = SatPoint::load(take(44).try_into().ok()?);
    let satpoint = SatPoint::load(take(44).try_into().ok()?);
    let txid = Txid::load(take(32).try_into().ok()?);
    let tx_index = u32::from_be_bytes(take(4).try_into().ok()?);
    let value = u64::from_be_bytes(take(8).try_into().ok()?);
    let script_len = usize::try_from(u32::from_be_bytes(take(4).try_into().ok()?)).ok()?;
    if bytes.len() != JOURNAL_FIXED_BYTES.checked_add(script_len)? {
      return None;
    }
    let script_pubkey = bytes[JOURNAL_FIXED_BYTES..].to_vec();
    let kind = match bytes[1] {
      0 => JournalKind::Creation,
      1 => JournalKind::Transfer,
      _ => return None,
    };
    let to = match bytes[2] {
      0 => JournalDestination::Assigned {
        satpoint,
        value,
        script_pubkey,
      },
      1 if value == 0 && script_pubkey.is_empty() => JournalDestination::Lost { satpoint },
      _ => return None,
    };
    Some(Self {
      kind,
      inscription_id,
      inscription_number,
      from,
      to,
      txid,
      tx_index,
    })
  }
}

/// Records the updater gathered for one block, in processing order: every
/// non-coinbase transaction in block order, then the coinbase with the fee
/// carriers, and within a transaction by input offset.
#[derive(Default)]
pub(crate) struct JournalRecorder {
  pub(crate) records: Vec<JournalRecord>,
}

impl JournalRecorder {
  pub(crate) fn creation(
    &mut self,
    inscription: &Inscription,
    inscription_id: InscriptionId,
    inscription_number: u64,
    from: SatPoint,
    to: JournalDestination,
    reveal_txid: Txid,
    reveal_tx_index: u32,
  ) {
    // Only candidates are journaled as creations; every creation, candidate
    // or not, stays derivable from the inscription tables.
    if wire::candidate_prefilter(inscription.body().unwrap_or_default()) {
      self.records.push(JournalRecord {
        kind: JournalKind::Creation,
        inscription_id,
        inscription_number,
        from,
        to,
        txid: reveal_txid,
        tx_index: reveal_tx_index,
      });
    }
  }

  pub(crate) fn transfer(
    &mut self,
    inscription_id: InscriptionId,
    inscription_number: Option<u64>,
    from: SatPoint,
    to: JournalDestination,
    txid: Txid,
    tx_index: u32,
  ) {
    self.records.push(JournalRecord {
      kind: JournalKind::Transfer,
      inscription_id,
      inscription_number: inscription_number.unwrap_or(UNKNOWN_NUMBER),
      from,
      to,
      txid,
      tx_index,
    });
  }
}

/// The destination record for `satpoint`, an output of `tx` or lost.
pub(crate) fn destination(tx: &Transaction, satpoint: SatPoint) -> Result<JournalDestination> {
  if satpoint.outpoint == OutPoint::null() {
    return Ok(JournalDestination::Lost { satpoint });
  }
  let output = tx
    .output
    .get(usize::try_from(satpoint.outpoint.vout)?)
    .ok_or_else(|| {
      anyhow!(
        "inscription assigned to missing output {}",
        satpoint.outpoint
      )
    })?;
  Ok(JournalDestination::Assigned {
    satpoint,
    value: output.value,
    script_pubkey: output.script_pubkey.to_bytes(),
  })
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct FeedMeta {
  database_id: Option<[u8; 16]>,
  creation_coverage_from: Option<u32>,
  journal_start: Option<u32>,
  journal_last: Option<u32>,
}

fn meta_height(
  table: &impl ReadableTable<&'static str, &'static [u8]>,
  key: &str,
) -> Result<Option<u32>> {
  table
    .get(key)?
    .map(|value| {
      <[u8; 4]>::try_from(value.value())
        .map(u32::from_be_bytes)
        .map_err(|_| anyhow!("malformed {key} in DOGEMAP_FEED_META"))
    })
    .transpose()
}

fn read_meta_table(table: &impl ReadableTable<&'static str, &'static [u8]>) -> Result<FeedMeta> {
  Ok(FeedMeta {
    database_id: table
      .get(META_DATABASE_ID)?
      .map(|value| {
        <[u8; 16]>::try_from(value.value())
          .map_err(|_| anyhow!("malformed databaseId in DOGEMAP_FEED_META"))
      })
      .transpose()?,
    creation_coverage_from: meta_height(table, META_CREATION_COVERAGE)?,
    journal_start: meta_height(table, META_JOURNAL_START)?,
    journal_last: meta_height(table, META_JOURNAL_LAST)?,
  })
}

fn read_meta(rtx: &redb::ReadTransaction) -> Result<FeedMeta> {
  match rtx.open_table(DOGEMAP_FEED_META) {
    Ok(table) => read_meta_table(&table),
    Err(redb::TableError::TableDoesNotExist(_)) => Ok(FeedMeta::default()),
    Err(error) => Err(error.into()),
  }
}

/// Create the generation id and coverage start once. Returns whether anything
/// was written.
fn ensure_meta(
  table: &mut Table<&'static str, &'static [u8]>,
  creation_coverage_from: u32,
) -> Result<bool> {
  let mut written = false;
  if table.get(META_DATABASE_ID)?.is_none() {
    let mut id = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut id);
    table.insert(META_DATABASE_ID, id.as_slice())?;
    log::info!("dogemap feed: created database id {}", hex::encode(id));
    written = true;
  }
  if table.get(META_CREATION_COVERAGE)?.is_none() {
    table.insert(
      META_CREATION_COVERAGE,
      creation_coverage_from.to_be_bytes().as_slice(),
    )?;
    written = true;
  }
  Ok(written)
}

/// Write the block's journal rows and advance the journal range, inside the
/// block's write transaction. A block that does not directly follow the last
/// journaled one (an older binary indexed in between, or this is the first
/// block of this binary) starts a new journal range, so the reported
/// transfer coverage never spans blocks that were not journaled.
pub(crate) fn record_block(
  wtx: &WriteTransaction,
  height: u32,
  records: Vec<JournalRecord>,
  creation_coverage_from: u32,
) -> Result {
  let mut meta = wtx.open_table(DOGEMAP_FEED_META)?;
  ensure_meta(&mut meta, creation_coverage_from)?;
  let start = meta_height(&meta, META_JOURNAL_START)?;
  let last = meta_height(&meta, META_JOURNAL_LAST)?;
  let contiguous = start.is_some() && last.and_then(|last| last.checked_add(1)) == Some(height);
  if !contiguous {
    if let Some(last) = last {
      log::info!("dogemap feed: journal restarts at {height} (last journaled block {last})");
    }
    meta.insert(META_JOURNAL_START, height.to_be_bytes().as_slice())?;
  }
  meta.insert(META_JOURNAL_LAST, height.to_be_bytes().as_slice())?;

  let mut journal = wtx.open_table(DOGEMAP_FEED_JOURNAL)?;
  let stale = journal
    .range((height, 0)..=(height, u32::MAX))?
    .map(|entry| entry.map(|(key, _)| key.value()))
    .collect::<Result<Vec<(u32, u32)>, StorageError>>()?;
  for key in stale {
    journal.remove(key)?;
  }
  for (seq, record) in records.iter().enumerate() {
    journal.insert((height, u32::try_from(seq)?), record.encode().as_slice())?;
  }
  Ok(())
}

/// What survives a savepoint restore: the generation id and coverage start.
pub(crate) struct PreservedMeta {
  database_id: Option<[u8; 16]>,
  creation_coverage_from: Option<u32>,
}

pub(crate) fn preserve_meta(rtx: &redb::ReadTransaction) -> Result<PreservedMeta> {
  let meta = read_meta(rtx)?;
  Ok(PreservedMeta {
    database_id: meta.database_id,
    creation_coverage_from: meta.creation_coverage_from,
  })
}

pub(crate) fn reinstate_meta(wtx: &WriteTransaction, preserved: PreservedMeta) -> Result {
  let mut meta = wtx.open_table(DOGEMAP_FEED_META)?;
  if let Some(id) = preserved.database_id {
    meta.insert(META_DATABASE_ID, id.as_slice())?;
  }
  if let Some(height) = preserved.creation_coverage_from {
    meta.insert(META_CREATION_COVERAGE, height.to_be_bytes().as_slice())?;
  }
  Ok(())
}

/// A refusal with a typed code, or an internal failure.
pub(crate) enum FeedFailure {
  Refused(FeedErrorCode, String),
  Internal(Error),
}

/// Indexed data that contradicts the feed's invariants; served as
/// `content_unavailable` for the affected request, never as an empty result.
#[derive(Debug)]
pub(crate) struct IntegrityError(String);

impl Display for IntegrityError {
  fn fmt(&self, f: &mut Formatter) -> fmt::Result {
    write!(f, "index integrity: {}", self.0)
  }
}

impl std::error::Error for IntegrityError {}

fn integrity<T>(message: impl Into<String>) -> Result<T> {
  Err(anyhow!(IntegrityError(message.into())))
}

impl From<Error> for FeedFailure {
  fn from(error: Error) -> Self {
    match error.downcast::<IntegrityError>() {
      Ok(integrity) => Self::Refused(FeedErrorCode::ContentUnavailable, integrity.to_string()),
      Err(error) => Self::Internal(error),
    }
  }
}

macro_rules! feed_failure_from {
  ($($error:ty),*) => {
    $(
      impl From<$error> for FeedFailure {
        fn from(error: $error) -> Self {
          Self::Internal(error.into())
        }
      }
    )*
  };
}

feed_failure_from!(redb::TransactionError, redb::TableError, redb::StorageError);

fn refuse<T>(code: FeedErrorCode, message: impl Into<String>) -> Result<T, FeedFailure> {
  Err(FeedFailure::Refused(code, message.into()))
}

fn node_failure(error: Error) -> FeedFailure {
  FeedFailure::Refused(
    FeedErrorCode::ContentUnavailable,
    format!("node RPC failed: {error}"),
  )
}

/// Identity and coverage as one read transaction sees them.
#[derive(Debug, Clone)]
pub(crate) struct FeedIdentity {
  pub(crate) database_id: Option<String>,
  pub(crate) reorg_epoch: u64,
  pub(crate) checkpoint: Option<(u32, BlockHash)>,
  pub(crate) creation_coverage_from: u32,
  journal_start: Option<u32>,
  journal_last: Option<u32>,
}

impl FeedIdentity {
  /// This binary journaled every block up to the checkpoint since
  /// `journal_start`.
  pub(crate) fn journal_active(&self) -> bool {
    match (self.journal_start, self.journal_last, self.checkpoint) {
      (Some(_), Some(last), Some((height, _))) => last == height,
      _ => false,
    }
  }

  pub(crate) fn transfer_coverage_from(&self) -> Option<u32> {
    if self.journal_active() {
      self
        .journal_start
        .map(|start| start.max(self.creation_coverage_from))
    } else {
      None
    }
  }

  fn transfers_journaled(&self, height: u32) -> bool {
    self.journal_active()
      && self.journal_start.is_some_and(|start| height >= start)
      && self.journal_last.is_some_and(|last| height <= last)
  }
}

fn read_identity(rtx: &redb::ReadTransaction, default_coverage: u32) -> Result<FeedIdentity> {
  let meta = read_meta(rtx)?;
  let reorg_epoch = rtx
    .open_table(STATISTIC_TO_COUNT)?
    .get(&Statistic::Reorgs.key())?
    .map(|value| value.value())
    .unwrap_or(0);
  let checkpoint = rtx
    .open_table(HEIGHT_TO_BLOCK_HASH)?
    .range(0..)?
    .next_back()
    .transpose()?
    .map(|(height, hash)| (height.value(), BlockHash::load(*hash.value())));
  Ok(FeedIdentity {
    database_id: meta.database_id.map(hex::encode),
    reorg_epoch,
    checkpoint,
    creation_coverage_from: meta.creation_coverage_from.unwrap_or(default_coverage),
    journal_start: meta.journal_start,
    journal_last: meta.journal_last,
  })
}

fn block_hash_at(rtx: &redb::ReadTransaction, height: u32) -> Result<Option<BlockHash>> {
  Ok(
    rtx
      .open_table(HEIGHT_TO_BLOCK_HASH)?
      .get(height)?
      .map(|hash| BlockHash::load(*hash.value())),
  )
}

/// One creation that passed the prefilter.
struct Candidate {
  number: u64,
  id: InscriptionId,
  completion_txid: Txid,
  part_count: usize,
  inscription: Inscription,
}

/// Everything one read transaction contributes to a block page.
struct GatheredBlock {
  block_hash: BlockHash,
  parent_hash: Option<BlockHash>,
  creation_count: u64,
  candidates: Vec<Candidate>,
  journal: Option<Vec<JournalRecord>>,
}

fn load_stored_transaction(
  txid_to_tx: &impl ReadableTable<&'static [u8], &'static [u8]>,
  txid: &[u8],
) -> Result<Transaction> {
  let Some(tx) = txid_to_tx.get(txid)? else {
    return integrity(format!(
      "stored reveal transaction {} missing",
      Txid::from_slice(txid)
        .map(|t| t.to_string())
        .unwrap_or_default()
    ));
  };
  Ok(consensus::encode::deserialize(tx.value())?)
}

/// The inscriptions created at `height` form one contiguous run of
/// inscription numbers: the updater assigns `next_number` only while it
/// processes a block, and blocks are processed in height order (a reorg
/// restores a savepoint, rewinding numbers with the blocks). `entry.height` is
/// therefore non-decreasing in the number, and the run is found by binary
/// search. Every number in the run is checked to belong to `height`, so a
/// database that breaks the invariant is refused rather than misread.
fn creation_range(
  number_to_id: &impl ReadableTable<u64, &'static InscriptionIdValue>,
  id_to_entry: &impl ReadableTable<&'static InscriptionIdValue, InscriptionEntryValue>,
  height: u32,
) -> Result<Range<u64>> {
  let Some(first) = number_to_id.first()?.map(|(number, _)| number.value()) else {
    return Ok(0..0);
  };
  let Some(last) = number_to_id.last()?.map(|(number, _)| number.value()) else {
    return Ok(0..0);
  };
  let end = last
    .checked_add(1)
    .ok_or_else(|| anyhow!("inscription number overflow"))?;

  let height_of = |number: u64| -> Result<u32> {
    let Some(id) = number_to_id.get(number)? else {
      return integrity(format!("inscription number {number} missing"));
    };
    let Some(entry) = id_to_entry.get(id.value())? else {
      return integrity(format!("inscription entry for number {number} missing"));
    };
    Ok(InscriptionEntry::load(entry.value()).height)
  };

  // First number in [lo, hi) whose height satisfies `past`.
  let partition = |mut lo: u64, mut hi: u64, past: &dyn Fn(u32) -> bool| -> Result<u64> {
    while lo < hi {
      let mid = lo + (hi - lo) / 2;
      if past(height_of(mid)?) {
        hi = mid;
      } else {
        lo = mid + 1;
      }
    }
    Ok(lo)
  };

  let start = partition(first, end, &|h| h >= height)?;
  let stop = partition(start, end, &|h| h > height)?;
  Ok(start..stop)
}

fn gather_block(
  rtx: &redb::ReadTransaction,
  height: u32,
  block_hash: BlockHash,
  identity: &FeedIdentity,
) -> Result<GatheredBlock> {
  let parent_hash = match height.checked_sub(1) {
    Some(parent) => Some(
      block_hash_at(rtx, parent)?
        .ok_or_else(|| anyhow!(IntegrityError(format!("block hash at {parent} missing"))))?,
    ),
    None => None,
  };

  let number_to_id = rtx.open_table(INSCRIPTION_NUMBER_TO_INSCRIPTION_ID)?;
  let id_to_entry = rtx.open_table(INSCRIPTION_ID_TO_INSCRIPTION_ENTRY)?;
  let id_to_txids = rtx.open_table(INSCRIPTION_ID_TO_TXIDS)?;
  let txid_to_tx = rtx.open_table(INSCRIPTION_TXID_TO_TX)?;

  let range = creation_range(&number_to_id, &id_to_entry, height)?;
  let mut candidates = Vec::new();

  for number in range.clone() {
    let Some(id) = number_to_id.get(number)? else {
      return integrity(format!("inscription number {number} missing"));
    };
    let id_value = *id.value();
    let id = InscriptionId::load(id_value);
    let Some(entry) = id_to_entry.get(&id_value)? else {
      return integrity(format!("inscription entry for {id} missing"));
    };
    let entry = InscriptionEntry::load(entry.value());
    if entry.height != height || entry.inscription_number != number {
      return integrity(format!(
        "inscription {id} numbered {number} records height {} and number {}",
        entry.height, entry.inscription_number
      ));
    }

    let Some(txids) = id_to_txids.get(&id_value)? else {
      return integrity(format!("reveal transactions of {id} missing"));
    };
    let txids = txids.value();
    if txids.is_empty() || txids.len() % 32 != 0 {
      return integrity(format!("reveal transaction list of {id} malformed"));
    }
    let part_count = txids.len() / 32;

    // Cheap path: the first reveal on its own. A single-part inscription is
    // complete here; a multipart one whose first part already carries more
    // body bytes than the prefilter admits cannot be a candidate.
    let first = load_stored_transaction(&txid_to_tx, &txids[..32])?;
    let (first_outcome, prefix_len) = Inscription::parse_first_part(&first);
    if part_count > 1
      && first_outcome == ParsedInscription::Partial
      && prefix_len > wire::PREFILTER_MAX_BODY_BYTES
    {
      continue;
    }

    let parsed = if part_count == 1 {
      first_outcome
    } else {
      let mut txs = Vec::with_capacity(part_count);
      for chunk in txids.chunks_exact(32) {
        txs.push(load_stored_transaction(&txid_to_tx, chunk)?);
      }
      Inscription::from_transactions(txs)
    };

    let ParsedInscription::Complete(inscription) = parsed else {
      return integrity(format!(
        "stored reveal transactions of {id} no longer parse as a complete inscription"
      ));
    };

    if !wire::candidate_prefilter(inscription.body().unwrap_or_default()) {
      continue;
    }

    candidates.push(Candidate {
      number,
      id,
      completion_txid: Txid::from_slice(&txids[txids.len() - 32..])?,
      part_count,
      inscription,
    });
  }

  let journal = if identity.transfers_journaled(height) {
    let table = rtx.open_table(DOGEMAP_FEED_JOURNAL)?;
    let mut records = Vec::new();
    for entry in table.range((height, 0)..=(height, u32::MAX))? {
      let (key, value) = entry?;
      let (_, seq) = key.value();
      let Some(record) = JournalRecord::decode(value.value()) else {
        return integrity(format!("journal record {height}/{seq} malformed"));
      };
      records.push(record);
    }
    Some(records)
  } else {
    None
  };

  Ok(GatheredBlock {
    block_hash,
    parent_hash,
    creation_count: range.end - range.start,
    candidates,
    journal,
  })
}

/// A block's complete ordered events and their digest.
pub(crate) struct BuiltBlock {
  parent_hash: Option<String>,
  transfers_scope: &'static str,
  creation_count: u64,
  events: Vec<Value>,
  events_hash: String,
}

#[derive(Clone, PartialEq, Eq)]
struct BlockCacheKey {
  database_id: String,
  reorg_epoch: u64,
  height: u32,
  block_hash: BlockHash,
  transfer_coverage_from: Option<u32>,
}

/// Bounded cache of built blocks. A key names the generation, epoch, height,
/// block hash and transfer coverage, all of which fix the events exactly.
#[derive(Default)]
pub(crate) struct FeedBlockCache {
  entries: Mutex<VecDeque<(BlockCacheKey, Arc<BuiltBlock>)>>,
}

impl FeedBlockCache {
  fn get(&self, key: &BlockCacheKey) -> Option<Arc<BuiltBlock>> {
    let entries = self.entries.lock().unwrap();
    entries
      .iter()
      .find(|(entry_key, _)| entry_key == key)
      .map(|(_, built)| built.clone())
  }

  fn insert(&self, key: BlockCacheKey, built: Arc<BuiltBlock>) {
    let mut entries = self.entries.lock().unwrap();
    if entries.iter().any(|(entry_key, _)| *entry_key == key) {
      return;
    }
    if entries.len() >= BLOCK_CACHE_ENTRIES {
      entries.pop_front();
    }
    entries.push_back((key, built));
  }
}

pub(crate) struct FeedBlockRequest {
  pub(crate) height: u32,
  pub(crate) block_hash: String,
  pub(crate) database_id: String,
  pub(crate) reorg_epoch: u64,
  pub(crate) cursor: Option<BlockCursor>,
  pub(crate) limit: usize,
}

pub(crate) struct FeedBodyRequest {
  pub(crate) inscription_id: InscriptionId,
  pub(crate) block_hash: String,
  pub(crate) database_id: String,
  pub(crate) reorg_epoch: u64,
  pub(crate) offset: usize,
  pub(crate) length: usize,
}

/// A node client for one feed request, with its own timeout and the RPC
/// credentials read now, so a node restart that rotated its cookie does not
/// leave the feed with a stale one.
pub(crate) struct FeedRpc(Client);

impl FeedRpc {
  pub(crate) fn block_count(&self) -> Result<u64> {
    Ok(self.0.get_block_count()?)
  }

  fn genesis_hash(&self) -> Result<BlockHash> {
    Ok(self.0.get_block_hash(0)?)
  }

  fn chain(&self) -> Result<String> {
    let info: Value = self.0.call("getblockchaininfo", &[])?;
    info["chain"]
      .as_str()
      .map(str::to_owned)
      .ok_or_else(|| anyhow!("getblockchaininfo has no chain"))
  }

  fn block_txids(&self, hash: BlockHash) -> Result<Vec<Txid>> {
    let block: Value = self
      .0
      .call("getblock", &[json!(hash.to_string()), json!(true)])?;
    block["tx"]
      .as_array()
      .ok_or_else(|| anyhow!("getblock {hash} returned no transaction list"))?
      .iter()
      .map(|txid| {
        txid
          .as_str()
          .ok_or_else(|| anyhow!("getblock {hash} returned a non-string txid"))?
          .parse::<Txid>()
          .map_err(Error::from)
      })
      .collect()
  }

  fn transaction_block(&self, txid: Txid) -> Result<BlockHash> {
    let tx: Value = self
      .0
      .call("getrawtransaction", &[json!(txid.to_string()), json!(true)])?;
    tx["blockhash"]
      .as_str()
      .ok_or_else(|| anyhow!("transaction {txid} is not confirmed according to the node"))?
      .parse::<BlockHash>()
      .map_err(Error::from)
  }

  fn block_height(&self, hash: BlockHash) -> Result<u32> {
    let header: Value = self
      .0
      .call("getblockheader", &[json!(hash.to_string()), json!(true)])?;
    let height = header["height"]
      .as_u64()
      .ok_or_else(|| anyhow!("getblockheader {hash} has no height"))?;
    Ok(u32::try_from(height)?)
  }

  fn transaction(&self, txid: Txid) -> Result<Transaction> {
    Ok(self.0.get_raw_transaction(&txid)?)
  }
}

pub(crate) fn provider_commit() -> &'static str {
  env!("UNIVERSE_PROVIDER_COMMIT")
}

fn provider_commit_known() -> bool {
  wire::is_lower_hex(provider_commit(), 40)
}

/// Display address of a location script under the contract's network table.
/// The rust-dogecoin fork encodes Regtest with the testnet P2PKH version (113),
/// while Dogecoin Core 1.14.9 regtest uses 111 (P2SH 196), so regtest
/// addresses are encoded here. Mainnet and testnet keep the library encoding.
fn dogemap_feed_address(chain: Chain, script: &Script) -> Option<String> {
  if chain != Chain::Regtest {
    return chain
      .address_from_script(script)
      .ok()
      .map(|address| address.to_string());
  }
  let bytes = script.as_bytes();
  let (version, hash) = if script.is_p2pkh() {
    (111u8, &bytes[3..23])
  } else if script.is_p2sh() {
    (196u8, &bytes[2..22])
  } else {
    return None;
  };
  let mut payload = vec![version];
  payload.extend_from_slice(hash);
  Some(bitcoin::util::base58::check_encode_slice(&payload))
}

impl Index {
  /// Create the feed's generation id at server start, so it exists before the
  /// first new block. One tiny write transaction when absent, none after.
  pub(crate) fn dogemap_feed_initialize(&self) -> Result {
    let meta = read_meta(&self.database.begin_read()?)?;
    if let Some(recorded) = meta.creation_coverage_from {
      if recorded != self.first_inscription_height {
        log::warn!(
          "dogemap feed: creation coverage was recorded from height {recorded}, but this process runs with first inscription height {}; the recorded value is reported",
          self.first_inscription_height
        );
      }
    }
    if meta.database_id.is_some() && meta.creation_coverage_from.is_some() {
      return Ok(());
    }
    let wtx = self.begin_write()?;
    {
      let mut table = wtx.open_table(DOGEMAP_FEED_META)?;
      ensure_meta(&mut table, self.first_inscription_height)?;
    }
    wtx.commit()?;
    Ok(())
  }

  pub(crate) fn dogemap_feed_rpc(&self) -> Result<FeedRpc> {
    let (user, pass) = self.auth.clone().get_user_pass()?;
    let builder = jsonrpc::simple_http::SimpleHttpTransport::builder()
      .url(&self.rpc_url)
      .map_err(|error| anyhow!("invalid RPC URL: {error}"))?
      .timeout(FEED_RPC_TIMEOUT);
    let builder = match user {
      Some(user) => builder.auth(user, pass),
      None => builder,
    };
    Ok(FeedRpc(Client::from_jsonrpc(
      jsonrpc::client::Client::with_transport(builder.build()),
    )))
  }

  /// Refuse to serve a node or an index that belongs to another network.
  /// The node check is skipped while the node is unreachable; the index keeps
  /// polling it and the feed reports `node_unavailable` until it answers.
  pub(crate) fn dogemap_feed_check_network(&self) -> Result {
    let network = self.chain.to_string();
    let expected = wire::genesis_hash(&network)
      .ok_or_else(|| anyhow!("{network} is not a Dogecoin Core network"))?;
    let genesis = self.chain.genesis_block().block_hash().to_string();
    ensure!(
      genesis == expected,
      "built-in {network} genesis {genesis} does not match Dogecoin Core's {expected}"
    );

    if let Some(indexed) = block_hash_at(&self.database.begin_read()?, 0)? {
      ensure!(
        indexed.to_string() == expected,
        "index at {} starts at block {indexed}, not the {network} genesis {expected}",
        self.path.display()
      );
    }

    match self.dogemap_feed_rpc().and_then(|rpc| {
      let chain = rpc.chain()?;
      let genesis = rpc.genesis_hash()?;
      Ok((chain, genesis))
    }) {
      Ok((chain, genesis)) => {
        let expected_chain = match self.chain {
          Chain::Mainnet => "main",
          Chain::Testnet => "test",
          Chain::Regtest => "regtest",
          Chain::Signet => "signet",
        };
        ensure!(
          chain == expected_chain,
          "Dogecoin Core reports chain {chain}, but ord runs on {network}"
        );
        ensure!(
          genesis.to_string() == expected,
          "Dogecoin Core genesis {genesis} is not the {network} genesis {expected}"
        );
      }
      Err(error) => log::warn!("dogemap feed: node network check deferred: {error}"),
    }
    Ok(())
  }

  fn dogemap_feed_identity_json(&self, identity: &FeedIdentity) -> Map<String, Value> {
    let network = self.chain.to_string();
    let genesis_hash = wire::genesis_hash(&network).unwrap_or_default();
    let mut map = Map::new();
    map.insert("chain".into(), json!("dogecoin"));
    map.insert("network".into(), json!(network));
    map.insert("genesisHash".into(), json!(genesis_hash));
    map.insert("providerVersion".into(), json!(env!("CARGO_PKG_VERSION")));
    map.insert("providerCommit".into(), json!(provider_commit()));
    map.insert("parserProfile".into(), json!(wire::PARSER_PROFILE));
    map.insert("orderProfile".into(), json!(wire::ORDER_PROFILE));
    map.insert("candidateFilter".into(), json!(wire::CANDIDATE_FILTER));
    map.insert("feedVersion".into(), json!(wire::FEED_VERSION));
    map.insert("databaseSchema".into(), json!(wire::DATABASE_SCHEMA));
    map.insert("databaseId".into(), json!(identity.database_id));
    map.insert("reorgEpoch".into(), json!(identity.reorg_epoch.to_string()));
    map
  }

  fn dogemap_feed_checkpoint_json(identity: &FeedIdentity) -> Value {
    match identity.checkpoint {
      Some((height, hash)) => json!({
        "height": height.to_string(),
        "blockHash": hash.to_string(),
        "reorgEpoch": identity.reorg_epoch.to_string(),
      }),
      None => Value::Null,
    }
  }

  /// `GET /api/v1/dogemap-feed/capabilities`. `node_height` is the cached
  /// node tip, fetched outside any read transaction.
  pub(crate) fn dogemap_feed_capabilities(&self, node_height: Option<u64>) -> Result<Value> {
    let identity = read_identity(&self.database.begin_read()?, self.first_inscription_height)?;

    // Readiness: an indexed checkpoint, an active journal, the node tip at
    // most MAX_READY_LAG_BLOCKS ahead, and a known build commit.
    let unavailable_reason = match (node_height, identity.checkpoint) {
      _ if self.is_unrecoverably_reorged() => Some("recovering"),
      (None, _) => Some("node_unavailable"),
      (Some(_), None) => Some("catching_up"),
      _ if identity.database_id.is_none() || !identity.journal_active() => Some("journal_inactive"),
      (Some(node_height), Some((height, _))) => match node_height.checked_sub(u64::from(height)) {
        None => Some("node_unavailable"),
        Some(lag) if lag > wire::MAX_READY_LAG_BLOCKS => Some("catching_up"),
        Some(_) if !provider_commit_known() => Some("provider_commit_unknown"),
        Some(_) => None,
      },
    };

    let mut map = Map::new();
    map.insert("schemaVersion".into(), json!(wire::CAPABILITIES_SCHEMA));
    map.extend(self.dogemap_feed_identity_json(&identity));
    map.insert(
      "indexedCheckpoint".into(),
      Self::dogemap_feed_checkpoint_json(&identity),
    );
    map.insert(
      "creationCoverageFromHeight".into(),
      json!(identity.creation_coverage_from.to_string()),
    );
    map.insert(
      "transferCoverageFromHeight".into(),
      json!(identity.transfer_coverage_from().map(|h| h.to_string())),
    );
    map.insert("ready".into(), json!(unavailable_reason.is_none()));
    map.insert("unavailableReason".into(), json!(unavailable_reason));
    map.insert(
      "nodeHeight".into(),
      json!(node_height.map(|h| h.to_string())),
    );
    map.insert("maxPageLimit".into(), json!(wire::MAX_PAGE_LIMIT));
    map.insert(
      "maxInlineBodyBytes".into(),
      json!(wire::MAX_INLINE_BODY_BYTES),
    );
    map.insert(
      "maxBodyChunkBytes".into(),
      json!(wire::MAX_BODY_CHUNK_BYTES),
    );
    map.insert("bodyRangePolicy".into(), json!(wire::BODY_RANGE_POLICY));
    map.insert(
      "eventsHashAlgorithm".into(),
      json!(wire::EVENTS_HASH_ALGORITHM),
    );
    map.insert("maxLocationIds".into(), json!(wire::MAX_LOCATION_IDS));
    Ok(Value::Object(map))
  }

  /// Identity checks shared by the block and body routes.
  fn dogemap_feed_check_identity(
    identity: &FeedIdentity,
    database_id: &str,
    reorg_epoch: u64,
  ) -> Result<(), FeedFailure> {
    let Some(current) = &identity.database_id else {
      return refuse(
        FeedErrorCode::CoverageUnavailable,
        "the feed has no database generation yet",
      );
    };
    if current != database_id {
      return refuse(
        FeedErrorCode::SnapshotReplaced,
        format!("databaseId is {current}"),
      );
    }
    if identity.reorg_epoch != reorg_epoch {
      return refuse(
        FeedErrorCode::SnapshotReplaced,
        format!("reorgEpoch is {}", identity.reorg_epoch),
      );
    }
    Ok(())
  }

  /// `GET /api/v1/dogemap-feed/blocks/{height}`.
  pub(crate) fn dogemap_feed_block(
    &self,
    request: &FeedBlockRequest,
    cache: &FeedBlockCache,
  ) -> Result<Value, FeedFailure> {
    // Identity, coverage and every indexed fact from one read transaction;
    // a cached block is valid for exactly the identity it was built under.
    let (identity, key, gathered): (_, _, Result<Arc<BuiltBlock>, GatheredBlock>) = {
      let rtx = self.database.begin_read()?;
      let identity = read_identity(&rtx, self.first_inscription_height)?;
      Self::dogemap_feed_check_identity(&identity, &request.database_id, request.reorg_epoch)?;
      let Some((indexed_height, _)) = identity.checkpoint else {
        return refuse(FeedErrorCode::CoverageUnavailable, "nothing is indexed yet");
      };
      if request.height > indexed_height {
        return refuse(
          FeedErrorCode::CoverageUnavailable,
          format!(
            "height {} is above the indexed height {indexed_height}",
            request.height
          ),
        );
      }
      if request.height < identity.creation_coverage_from {
        return refuse(
          FeedErrorCode::CoverageUnavailable,
          format!(
            "height {} is below creation coverage, which starts at {}",
            request.height, identity.creation_coverage_from
          ),
        );
      }
      let Some(block_hash) = block_hash_at(&rtx, request.height)? else {
        return refuse(
          FeedErrorCode::CoverageUnavailable,
          format!("height {} is not indexed", request.height),
        );
      };
      if block_hash.to_string() != request.block_hash {
        return refuse(
          FeedErrorCode::SnapshotReplaced,
          format!("block {} is {block_hash}", request.height),
        );
      }
      let key = BlockCacheKey {
        database_id: request.database_id.clone(),
        reorg_epoch: request.reorg_epoch,
        height: request.height,
        block_hash,
        transfer_coverage_from: identity.transfer_coverage_from(),
      };
      let gathered = match cache.get(&key) {
        Some(built) => Ok(built),
        None => Err(gather_block(&rtx, request.height, block_hash, &identity)?),
      };
      (identity, key, gathered)
    };

    let built = match gathered {
      Ok(built) => built,
      Err(gathered) => {
        let built = Arc::new(self.dogemap_feed_build_block(&identity, request, gathered)?);
        cache.insert(key, built.clone());
        built
      }
    };

    self.dogemap_feed_block_page(&identity, request, &built)
  }

  fn dogemap_feed_location_json(&self, destination: &JournalDestination) -> Value {
    match destination {
      JournalDestination::Assigned {
        satpoint,
        value,
        script_pubkey,
      } => Self::dogemap_feed_assigned_json(
        self.chain,
        satpoint.outpoint,
        satpoint.offset,
        *value,
        script_pubkey,
      ),
      JournalDestination::Lost { satpoint } => Self::dogemap_feed_lost_json(satpoint.offset),
    }
  }

  fn dogemap_feed_assigned_json(
    chain: Chain,
    outpoint: OutPoint,
    offset: u64,
    value: u64,
    script_pubkey: &[u8],
  ) -> Value {
    let address = dogemap_feed_address(chain, &Script::from(script_pubkey.to_vec()));
    json!({
      "status": "assigned",
      "outpoint": outpoint.to_string(),
      "offset": offset.to_string(),
      "valueKoinu": value.to_string(),
      "scriptPubKeyHex": hex::encode(script_pubkey),
      "address": address,
    })
  }

  fn dogemap_feed_lost_json(offset: u64) -> Value {
    json!({
      "status": "lost",
      "outpoint": null,
      "offset": offset.to_string(),
      "valueKoinu": null,
      "scriptPubKeyHex": null,
      "address": null,
    })
  }

  fn dogemap_feed_build_block(
    &self,
    identity: &FeedIdentity,
    request: &FeedBlockRequest,
    gathered: GatheredBlock,
  ) -> Result<BuiltBlock, FeedFailure> {
    let height = request.height;

    // Journal rows: creations keyed by inscription, transfers in order.
    let (creation_records, transfers) = match &gathered.journal {
      Some(records) => {
        let mut creations = HashMap::new();
        let mut transfers = Vec::new();
        for record in records {
          match record.kind {
            JournalKind::Creation => {
              creations.insert(record.inscription_id, record);
            }
            JournalKind::Transfer => transfers.push(record),
          }
        }
        if creations.len() != gathered.candidates.len()
          || gathered
            .candidates
            .iter()
            .any(|candidate| !creations.contains_key(&candidate.id))
        {
          return Err(
            anyhow!(IntegrityError(format!(
              "journal creations at {height} disagree with the inscription tables"
            )))
            .into(),
          );
        }
        (creations, Some(transfers))
      }
      None => (HashMap::new(), None),
    };

    // Node facts the index does not keep: the completion transaction's
    // position when the block was not journaled, and the first reveal's block
    // for multipart inscriptions.
    let needs_positions = gathered
      .candidates
      .iter()
      .any(|candidate| !creation_records.contains_key(&candidate.id));
    let multipart = gathered
      .candidates
      .iter()
      .filter(|candidate| candidate.part_count > 1)
      .map(|candidate| candidate.id.txid)
      .collect::<Vec<Txid>>();

    let mut positions = HashMap::new();
    let mut origins = HashMap::new();
    if needs_positions || !multipart.is_empty() {
      let rpc = self.dogemap_feed_rpc().map_err(node_failure)?;
      if needs_positions {
        for (position, txid) in rpc
          .block_txids(gathered.block_hash)
          .map_err(node_failure)?
          .into_iter()
          .enumerate()
        {
          positions.insert(txid, u32::try_from(position).map_err(Error::from)?);
        }
      }
      for txid in &multipart {
        let block = rpc.transaction_block(*txid).map_err(node_failure)?;
        let origin_height = rpc.block_height(block).map_err(node_failure)?;
        origins.insert(*txid, (origin_height, block));
      }

      // The node's answers must describe the chain this snapshot indexed.
      let rtx = self.database.begin_read()?;
      let now = read_identity(&rtx, self.first_inscription_height)?;
      if now.database_id != identity.database_id || now.reorg_epoch != identity.reorg_epoch {
        return refuse(
          FeedErrorCode::SnapshotReplaced,
          "the index was replaced or rolled back while the node was queried",
        );
      }
      if block_hash_at(&rtx, height)? != Some(gathered.block_hash) {
        return refuse(
          FeedErrorCode::SnapshotReplaced,
          format!("block {height} changed while the node was queried"),
        );
      }
      for (txid, (origin_height, origin_block)) in &origins {
        if *origin_height > height || block_hash_at(&rtx, *origin_height)? != Some(*origin_block) {
          return refuse(
            FeedErrorCode::ContentUnavailable,
            format!(
              "the node places reveal {txid} in block {origin_block} at {origin_height}, which is not on the indexed chain"
            ),
          );
        }
      }
    }

    let block_hash = gathered.block_hash.to_string();
    let mut events = Vec::new();

    for candidate in &gathered.candidates {
      let record = creation_records.get(&candidate.id);
      let completion_index = match record {
        Some(record) => {
          if record.txid != candidate.completion_txid {
            return Err(
              anyhow!(IntegrityError(format!(
                "journal names {} as the reveal of {}, the index {}",
                record.txid, candidate.id, candidate.completion_txid
              )))
              .into(),
            );
          }
          record.tx_index
        }
        None => match positions.get(&candidate.completion_txid) {
          Some(position) => *position,
          None => {
            return refuse(
              FeedErrorCode::ContentUnavailable,
              format!(
                "the node does not list reveal {} in block {block_hash}",
                candidate.completion_txid
              ),
            )
          }
        },
      };
      let (origin_height, origin_block) = if candidate.part_count > 1 {
        let (origin_height, origin_block) = origins[&candidate.id.txid];
        (origin_height, origin_block.to_string())
      } else {
        (height, block_hash.clone())
      };

      let body = candidate.inscription.body().unwrap_or_default();
      let inline = body.len() <= wire::MAX_INLINE_BODY_BYTES;
      let body_ref = if inline {
        Value::Null
      } else {
        json!(format!(
          "/api/v1/dogemap-feed/inscriptions/{}/body?blockHash={block_hash}&databaseId={}&reorgEpoch={}",
          candidate.id, request.database_id, request.reorg_epoch
        ))
      };
      let delegate = candidate.inscription.delegate.as_ref().map(|raw| {
        json!({
          "rawBase64": base64::encode(raw),
          "inscriptionId": candidate.inscription.delegate().map(|id| id.to_string()),
        })
      });

      events.push(json!({
        "type": "creation",
        "eventOrdinal": events.len().to_string(),
        "inscriptionId": candidate.id.to_string(),
        "inscriptionNumber": candidate.number.to_string(),
        "origin": {
          "txid": candidate.id.txid.to_string(),
          "height": origin_height.to_string(),
          "blockHash": origin_block,
        },
        "completion": {
          "txid": candidate.completion_txid.to_string(),
          "height": height.to_string(),
          "blockHash": block_hash,
          "txIndex": completion_index.to_string(),
        },
        "partCount": candidate.part_count.to_string(),
        "contentTypeBase64": candidate.inscription.content_type.as_ref().map(base64::encode),
        "rawBody": {
          "encoding": "base64",
          "byteLength": body.len().to_string(),
          "sha256": wire::sha256_hex(body),
          "bytes": if inline { json!(base64::encode(body)) } else { Value::Null },
          "bodyRef": body_ref,
        },
        "delegate": delegate,
        "location": record.map(|record| self.dogemap_feed_location_json(&record.to)),
      }));
    }

    if let Some(transfers) = &transfers {
      let unresolved = transfers
        .iter()
        .any(|record| record.inscription_number == UNKNOWN_NUMBER);
      let entries = if unresolved {
        Some(self.database.begin_read()?)
      } else {
        None
      };
      for record in transfers {
        let number = if record.inscription_number == UNKNOWN_NUMBER {
          let rtx = entries.as_ref().expect("opened for unresolved numbers");
          match rtx
            .open_table(INSCRIPTION_ID_TO_INSCRIPTION_ENTRY)?
            .get(&record.inscription_id.store())?
          {
            Some(entry) => InscriptionEntry::load(entry.value()).inscription_number,
            None => {
              return Err(
                anyhow!(IntegrityError(format!(
                  "transfer of {} has no inscription entry",
                  record.inscription_id
                )))
                .into(),
              )
            }
          }
        } else {
          record.inscription_number
        };
        events.push(json!({
          "type": "transfer",
          "eventOrdinal": events.len().to_string(),
          "inscriptionId": record.inscription_id.to_string(),
          "inscriptionNumber": number.to_string(),
          "txid": record.txid.to_string(),
          "txIndex": record.tx_index.to_string(),
          "from": {
            "outpoint": record.from.outpoint.to_string(),
            "offset": record.from.offset.to_string(),
          },
          "to": self.dogemap_feed_location_json(&record.to),
        }));
      }
    }

    let network = self.chain.to_string();
    let genesis_hash = wire::genesis_hash(&network).unwrap_or_default();
    let parent_hash = gathered.parent_hash.map(|hash| hash.to_string());
    let transfers_scope = if transfers.is_some() {
      "complete"
    } else {
      "not-journaled"
    };
    let header = EventsHashHeader {
      network: &network,
      genesis_hash,
      height,
      block_hash: &block_hash,
      parent_hash: parent_hash.as_deref(),
      transfers_scope,
      creation_count: gathered.creation_count,
    };
    let events_hash = wire::events_hash(&header, &events).map_err(Error::from)?;

    Ok(BuiltBlock {
      parent_hash,
      transfers_scope,
      creation_count: gathered.creation_count,
      events,
      events_hash,
    })
  }

  fn dogemap_feed_block_page(
    &self,
    identity: &FeedIdentity,
    request: &FeedBlockRequest,
    built: &BuiltBlock,
  ) -> Result<Value, FeedFailure> {
    let total = built.events.len();
    let database_id: [u8; 16] = hex::decode(&request.database_id)
      .ok()
      .and_then(|bytes| bytes.try_into().ok())
      .ok_or_else(|| {
        FeedFailure::Refused(
          FeedErrorCode::InvalidRequest,
          "databaseId is malformed".into(),
        )
      })?;
    let block_hash: [u8; 32] = hex::decode(&request.block_hash)
      .ok()
      .and_then(|bytes| bytes.try_into().ok())
      .ok_or_else(|| {
        FeedFailure::Refused(
          FeedErrorCode::InvalidRequest,
          "blockHash is malformed".into(),
        )
      })?;
    let events_hash: [u8; 32] = hex::decode(&built.events_hash)
      .map_err(Error::from)?
      .try_into()
      .map_err(|_| anyhow!("eventsHash is not 32 bytes"))?;

    let start = match &request.cursor {
      None => 0,
      Some(cursor) => {
        if cursor.database_id != database_id
          || cursor.reorg_epoch != request.reorg_epoch
          || cursor.height != request.height
          || cursor.block_hash != block_hash
          || cursor.events_hash != events_hash
        {
          return refuse(
            FeedErrorCode::SnapshotReplaced,
            "the cursor belongs to another snapshot of this block",
          );
        }
        let next = usize::try_from(cursor.next_ordinal).map_err(Error::from)?;
        if next == 0 || next >= total {
          return refuse(FeedErrorCode::InvalidCursor, "cursor position out of range");
        }
        next
      }
    };
    let end = start.saturating_add(request.limit).min(total);
    let next_cursor = if end < total {
      Some(
        BlockCursor {
          database_id,
          reorg_epoch: request.reorg_epoch,
          height: request.height,
          block_hash,
          events_hash,
          next_ordinal: u32::try_from(end).map_err(Error::from)?,
        }
        .encode(),
      )
    } else {
      None
    };

    let mut map = Map::new();
    map.insert("schemaVersion".into(), json!(wire::BLOCK_SCHEMA));
    map.extend(self.dogemap_feed_identity_json(identity));
    map.insert("height".into(), json!(request.height.to_string()));
    map.insert("blockHash".into(), json!(request.block_hash));
    map.insert("parentHash".into(), json!(built.parent_hash));
    map.insert(
      "indexedCheckpoint".into(),
      Self::dogemap_feed_checkpoint_json(identity),
    );
    map.insert(
      "scope".into(),
      json!({
        "creations": wire::CANDIDATE_FILTER,
        "transfers": built.transfers_scope,
      }),
    );
    map.insert(
      "creationCount".into(),
      json!(built.creation_count.to_string()),
    );
    map.insert("totalEvents".into(), json!(total.to_string()));
    map.insert("eventsHash".into(), json!(built.events_hash));
    map.insert("events".into(), json!(built.events[start..end]));
    map.insert("nextCursor".into(), json!(next_cursor));
    map.insert("complete".into(), json!(end == total));
    Ok(Value::Object(map))
  }

  /// `GET /api/v1/dogemap-feed/inscriptions/{id}/body`.
  pub(crate) fn dogemap_feed_body(&self, request: &FeedBodyRequest) -> Result<Value, FeedFailure> {
    let (identity, body) = {
      let rtx = self.database.begin_read()?;
      let identity = read_identity(&rtx, self.first_inscription_height)?;
      Self::dogemap_feed_check_identity(&identity, &request.database_id, request.reorg_epoch)?;
      let id_value = request.inscription_id.store();
      let Some(entry) = rtx
        .open_table(INSCRIPTION_ID_TO_INSCRIPTION_ENTRY)?
        .get(&id_value)?
        .map(|entry| InscriptionEntry::load(entry.value()))
      else {
        return refuse(
          FeedErrorCode::UnknownInscription,
          format!("inscription {} is not indexed", request.inscription_id),
        );
      };
      let completion_block = block_hash_at(&rtx, entry.height)?;
      if completion_block.map(|hash| hash.to_string()).as_deref()
        != Some(request.block_hash.as_str())
      {
        return refuse(
          FeedErrorCode::SnapshotReplaced,
          format!(
            "inscription {} completed in another block at height {}",
            request.inscription_id, entry.height
          ),
        );
      }
      let Some(txids) = rtx.open_table(INSCRIPTION_ID_TO_TXIDS)?.get(&id_value)? else {
        return Err(
          anyhow!(IntegrityError(format!(
            "reveal transactions of {} missing",
            request.inscription_id
          )))
          .into(),
        );
      };
      let txids = txids.value().to_vec();
      if txids.is_empty() || txids.len() % 32 != 0 {
        return Err(
          anyhow!(IntegrityError(format!(
            "reveal transaction list of {} malformed",
            request.inscription_id
          )))
          .into(),
        );
      }
      let txid_to_tx = rtx.open_table(INSCRIPTION_TXID_TO_TX)?;
      let mut txs = Vec::with_capacity(txids.len() / 32);
      for chunk in txids.chunks_exact(32) {
        txs.push(load_stored_transaction(&txid_to_tx, chunk)?);
      }
      let ParsedInscription::Complete(inscription) = Inscription::from_transactions(txs) else {
        return Err(
          anyhow!(IntegrityError(format!(
            "stored reveal transactions of {} no longer parse",
            request.inscription_id
          )))
          .into(),
        );
      };
      (identity, inscription.into_body().unwrap_or_default())
    };

    let length = body.len();
    if request.offset > length || (request.offset == length && length > 0) {
      return refuse(
        FeedErrorCode::InvalidRequest,
        format!(
          "offset {} is outside the {length}-byte body",
          request.offset
        ),
      );
    }
    let end = request.offset.saturating_add(request.length).min(length);
    let chunk = &body[request.offset..end];

    let mut map = Map::new();
    map.insert("schemaVersion".into(), json!(wire::BODY_SCHEMA));
    map.extend(self.dogemap_feed_identity_json(&identity));
    map.insert(
      "inscriptionId".into(),
      json!(request.inscription_id.to_string()),
    );
    map.insert("encoding".into(), json!("base64"));
    map.insert("offset".into(), json!(request.offset.to_string()));
    map.insert("length".into(), json!(chunk.len().to_string()));
    map.insert("byteLength".into(), json!(length.to_string()));
    map.insert("sha256".into(), json!(wire::sha256_hex(&body)));
    map.insert("bytes".into(), json!(base64::encode(chunk)));
    map.insert(
      "nextOffset".into(),
      json!((end < length).then(|| end.to_string())),
    );
    map.insert("complete".into(), json!(end == length));
    Ok(Value::Object(map))
  }

  /// `GET /api/v1/dogemap-feed/locations`. Satpoints, numbers and values come
  /// from one read transaction; output scripts the index does not store are
  /// read from the node for exactly the outpoint the snapshot names.
  pub(crate) fn dogemap_feed_locations(&self, ids: &[InscriptionId]) -> Result<Value, FeedFailure> {
    enum Found {
      Missing,
      Lost {
        number: u64,
        offset: u64,
      },
      Assigned {
        number: u64,
        satpoint: SatPoint,
        value: Option<u64>,
        script: Option<Vec<u8>>,
      },
    }

    let (identity, found) = {
      let rtx = self.database.begin_read()?;
      let identity = read_identity(&rtx, self.first_inscription_height)?;
      if identity.database_id.is_none() {
        return refuse(
          FeedErrorCode::CoverageUnavailable,
          "the feed has no database generation yet",
        );
      }
      if identity.checkpoint.is_none() {
        return refuse(FeedErrorCode::CoverageUnavailable, "nothing is indexed yet");
      }
      let id_to_satpoint = rtx.open_table(INSCRIPTION_ID_TO_SATPOINT)?;
      let id_to_entry = rtx.open_table(INSCRIPTION_ID_TO_INSCRIPTION_ENTRY)?;
      let outpoint_to_value = rtx.open_table(OUTPOINT_TO_VALUE)?;
      let txid_to_tx = rtx.open_table(INSCRIPTION_TXID_TO_TX)?;
      let transactions = if self.index_transactions {
        Some(rtx.open_table(TRANSACTION_ID_TO_TRANSACTION)?)
      } else {
        None
      };

      let mut found = Vec::with_capacity(ids.len());
      for id in ids {
        let id_value = id.store();
        let (Some(satpoint), Some(entry)) = (
          id_to_satpoint
            .get(&id_value)?
            .map(|satpoint| SatPoint::load(*satpoint.value())),
          id_to_entry
            .get(&id_value)?
            .map(|entry| InscriptionEntry::load(entry.value())),
        ) else {
          found.push(Found::Missing);
          continue;
        };
        if satpoint.outpoint == OutPoint::null() {
          found.push(Found::Lost {
            number: entry.inscription_number,
            offset: satpoint.offset,
          });
          continue;
        }
        let value = outpoint_to_value
          .get(&satpoint.outpoint.store())?
          .map(|value| value.value());
        let stored = match &transactions {
          Some(table) => table
            .get(&satpoint.outpoint.txid.store())?
            .map(|tx| tx.value().to_vec()),
          None => None,
        };
        let stored = match stored {
          Some(tx) => Some(tx),
          None => txid_to_tx
            .get(satpoint.outpoint.txid.store().as_slice())?
            .map(|tx| tx.value().to_vec()),
        };
        let script = match stored {
          Some(tx) => {
            let tx: Transaction = consensus::encode::deserialize(&tx).map_err(Error::from)?;
            let vout = usize::try_from(satpoint.outpoint.vout).map_err(Error::from)?;
            tx.output
              .get(vout)
              .map(|output| output.script_pubkey.to_bytes())
          }
          None => None,
        };
        found.push(Found::Assigned {
          number: entry.inscription_number,
          satpoint,
          value,
          script,
        });
      }
      (identity, found)
    };

    // Scripts (and, for outputs the value table lacks, values) of exactly
    // the outpoints above, each transaction fetched once.
    let mut fetched: HashMap<Txid, Transaction> = HashMap::new();
    let missing = found
      .iter()
      .filter_map(|found| match found {
        Found::Assigned {
          satpoint,
          value,
          script,
          ..
        } if value.is_none() || script.is_none() => Some(satpoint.outpoint.txid),
        _ => None,
      })
      .collect::<BTreeSet<Txid>>();
    if !missing.is_empty() {
      let rpc = self.dogemap_feed_rpc().map_err(node_failure)?;
      for txid in missing {
        fetched.insert(txid, rpc.transaction(txid).map_err(node_failure)?);
      }
    }

    let mut locations = Vec::with_capacity(ids.len());
    for (id, found) in ids.iter().zip(found) {
      locations.push(match found {
        Found::Missing => json!({
          "inscriptionId": id.to_string(),
          "inscriptionNumber": null,
          "found": false,
          "location": null,
        }),
        Found::Lost { number, offset } => json!({
          "inscriptionId": id.to_string(),
          "inscriptionNumber": number.to_string(),
          "found": true,
          "location": Self::dogemap_feed_lost_json(offset),
        }),
        Found::Assigned {
          number,
          satpoint,
          value,
          script,
        } => {
          let output = fetched.get(&satpoint.outpoint.txid).and_then(|tx| {
            tx.output
              .get(usize::try_from(satpoint.outpoint.vout).ok()?)
              .cloned()
          });
          let value = value.or(output.as_ref().map(|output| output.value));
          let script = script.or(output.map(|output| output.script_pubkey.to_bytes()));
          let (Some(value), Some(script)) = (value, script) else {
            return refuse(
              FeedErrorCode::ContentUnavailable,
              format!("output {} of {id} could not be resolved", satpoint.outpoint),
            );
          };
          json!({
            "inscriptionId": id.to_string(),
            "inscriptionNumber": number.to_string(),
            "found": true,
            "location": Self::dogemap_feed_assigned_json(
              self.chain,
              satpoint.outpoint,
              satpoint.offset,
              value,
              &script,
            ),
          })
        }
      });
    }

    let mut map = Map::new();
    map.insert("schemaVersion".into(), json!(wire::LOCATIONS_SCHEMA));
    map.extend(self.dogemap_feed_identity_json(&identity));
    map.insert(
      "indexedCheckpoint".into(),
      Self::dogemap_feed_checkpoint_json(&identity),
    );
    map.insert("locations".into(), Value::Array(locations));
    Ok(Value::Object(map))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  // Vectors from Dogecoin Core 1.14.9 `validateaddress` on regtest (campaign
  // 2026-09-29) and the testnet encoding of the same key hash.
  #[test]
  fn regtest_addresses_use_the_regtest_version_bytes() {
    let p2pkh = Script::from(hex::decode("76a9143fbf0c955774efb1c3672c7d21bfedaff4d8f4f288ac").unwrap());
    assert_eq!(
      dogemap_feed_address(Chain::Regtest, &p2pkh).as_deref(),
      Some("mmL1jU46wt7NX4AxsebvZBh4siURhgSgyk")
    );
    assert_eq!(
      dogemap_feed_address(Chain::Testnet, &p2pkh).as_deref(),
      Some("na1DhgegNF389vT8vVGZXSEe8izK9XfdmV")
    );
    let p2sh = Script::from(hex::decode("a9143fbf0c955774efb1c3672c7d21bfedaff4d8f4f287").unwrap());
    assert_eq!(
      dogemap_feed_address(Chain::Regtest, &p2sh),
      dogemap_feed_address(Chain::Testnet, &p2sh)
    );
    assert!(dogemap_feed_address(Chain::Regtest, &p2sh).unwrap().starts_with('2'));
    assert_eq!(dogemap_feed_address(Chain::Regtest, &Script::new()), None);
  }
}
