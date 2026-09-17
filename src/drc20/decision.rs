use {
  super::OperationType,
  crate::InscriptionId,
  bitcoin::{hashes::Hash, BlockHash, Txid},
  serde::{Deserialize, Serialize},
};

/// Rule set the decisions were produced under. Bump when the ledger rules in
/// `drc20_updater.rs` change semantics, so a consumer can refuse evidence
/// produced by rules it does not understand.
pub const DRC20_RULESET: &str = "drc20-v1";

/// Serialized record layout. Stored as the first field so an older binary
/// reading a newer record fails on the version rather than misreading fields.
pub const DECISION_RECORD_VERSION: u8 = 1;

/// Composite key: the txid of the transaction that carried the operation,
/// followed by the inscription id. One inscription carries at most two
/// operations, its inscription (deploy, mint or inscribe-transfer, where the
/// txid equals the inscription's txid) and its first transfer (a later txid),
/// so the inscription id alone cannot address one operation.
pub type OperationDecisionKey = [u8; 68];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verdict {
  Accepted,
  Rejected,
}

impl Verdict {
  pub fn as_str(self) -> &'static str {
    match self {
      Verdict::Accepted => "accepted",
      Verdict::Rejected => "rejected",
    }
  }
}

/// The retained outcome of evaluating one DRC-20 operation against the ledger.
///
/// Written in the same write transaction that applied (or refused) the ledger
/// change, so the record and the balances it explains can never disagree, and
/// rolled back together with them when a reorg restores a savepoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperationDecision {
  pub version: u8,
  pub txid: Txid,
  pub inscription_id: InscriptionId,
  pub operation: OperationType,
  pub tick: String,
  /// Ledger-effective atomic amount. Present only for accepted mint,
  /// inscribe-transfer and transfer operations (a mint records the amount
  /// after the supply cut-off). A rejected operation changed nothing, so it
  /// has no ledger amount; the reason string carries what was attempted.
  pub amount: Option<u128>,
  pub verdict: Verdict,
  /// `DRC20Error` display string when rejected.
  pub reason: Option<String>,
  pub height: u32,
  pub block_hash: BlockHash,
  /// Number of savepoint rollbacks the index had performed when this block
  /// was indexed. A record re-created after a rollback carries a higher value.
  pub reorg_epoch: u64,
  pub ruleset: String,
}

impl OperationDecision {
  pub fn key(txid: Txid, inscription_id: InscriptionId) -> OperationDecisionKey {
    let mut key = [0u8; 68];
    key[..32].copy_from_slice(txid.as_inner());
    key[32..64].copy_from_slice(inscription_id.txid.as_inner());
    key[64..].copy_from_slice(&inscription_id.index.to_be_bytes());
    key
  }

  /// Range bounds covering every operation carried by `txid`.
  pub fn txid_range(txid: Txid) -> (OperationDecisionKey, OperationDecisionKey) {
    let mut start = [0u8; 68];
    let mut end = [0xffu8; 68];
    start[..32].copy_from_slice(txid.as_inner());
    end[..32].copy_from_slice(txid.as_inner());
    (start, end)
  }

  pub fn store(&self) -> Vec<u8> {
    bincode::serialize(self).expect("operation decision serializes")
  }

  pub fn load(bytes: &[u8]) -> Option<Self> {
    let decision = bincode::deserialize::<Self>(bytes).ok()?;
    (decision.version == DECISION_RECORD_VERSION).then_some(decision)
  }
}

impl OperationType {
  pub fn as_str(&self) -> &'static str {
    match self {
      OperationType::Deploy => "deploy",
      OperationType::Mint => "mint",
      OperationType::InscribeTransfer => "inscribe-transfer",
      OperationType::Transfer => "transfer",
    }
  }
}
