use serde::Serialize;
use std::collections::BTreeMap;

use crate::Inscription;

pub const INVENTORY_LIMIT_DEFAULT: usize = 250;
pub const INVENTORY_LIMIT_MAXIMUM: usize = 1_000;
pub const FUNDING_LIMIT_DEFAULT: usize = 20;
pub const FUNDING_LIMIT_MAXIMUM: usize = 50;

pub fn checked_inventory_limit(limit: Option<usize>) -> Result<usize, &'static str> {
  let limit = limit.unwrap_or(INVENTORY_LIMIT_DEFAULT);
  if !(1..=INVENTORY_LIMIT_MAXIMUM).contains(&limit) {
    return Err("inventory limit must be between 1 and 1000");
  }
  Ok(limit)
}

pub fn checked_funding_limit(limit: Option<usize>) -> Result<usize, &'static str> {
  let limit = limit.unwrap_or(FUNDING_LIMIT_DEFAULT);
  if !(1..=FUNDING_LIMIT_MAXIMUM).contains(&limit) {
    return Err("funding limit must be between 1 and 50");
  }
  Ok(limit)
}

/// Decode an offset cursor. The DRC-20 catalog is ordered deterministically
/// by ticker, so a numeric offset is a stable position: a cursor can neither
/// skip a ticker nor return one twice within a single indexed height.
pub fn checked_offset_cursor(cursor: Option<&str>) -> Result<usize, &'static str> {
  let Some(cursor) = cursor else {
    return Ok(0);
  };
  if cursor.is_empty() || cursor.len() > 19 || !cursor.bytes().all(|b| b.is_ascii_digit()) {
    return Err("cursor must be a decimal offset");
  }
  cursor
    .parse::<usize>()
    .map_err(|_| "cursor is out of range")
}

pub(crate) fn resolved_content_metadata(
  inscription: &Inscription,
  delegate: Option<&Inscription>,
) -> (Option<String>, Option<usize>) {
  let content = delegate.unwrap_or(inscription);
  (
    content.content_type().map(str::to_owned),
    content.content_length(),
  )
}

#[derive(Debug, PartialEq, Serialize)]
pub struct InventoryLocation {
  pub txid: String,
  pub vout: u32,
  pub offset: String,
  pub value: String,
  pub script_pubkey: String,
  pub address: Option<String>,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct InscriptionInventoryItem {
  pub inscription_id: String,
  pub inscription_number: String,
  pub genesis_height: u32,
  pub timestamp: u32,
  pub content_type: Option<String>,
  pub content_length: Option<usize>,
  pub subsidy_sats: String,
  pub location: Option<InventoryLocation>,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct InscriptionInventory {
  pub chain: &'static str,
  pub block_count: u32,
  pub block_hash: String,
  pub subsidy_schedule_hash: String,
  pub inventory_complete: bool,
  pub next_cursor: Option<String>,
  pub inscriptions: Vec<InscriptionInventoryItem>,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct Drc20TransferableInventoryItem {
  pub ticker: String,
  pub amount_atomic: String,
  pub decimals: u8,
  pub max_atomic: String,
  pub limit_atomic: String,
  pub transfer_inscription_id: String,
  pub inscription_number: String,
  pub owner_address: String,
  pub genesis_height: u32,
  pub transaction_index: u32,
  pub inscription_index: u32,
  pub location: Option<InventoryLocation>,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct Drc20TransferableInventory {
  pub chain: &'static str,
  pub block_count: u32,
  pub block_hash: String,
  pub inventory_complete: bool,
  pub transferables: Vec<Drc20TransferableInventoryItem>,
}

/// One inscription in the field layout upstream `ord` answers for
/// `GET /inscription/:id` under `Accept: application/json`, so a consumer
/// written against upstream reads this fork unchanged. Every integer is an
/// exact JSON number: `serde_json` prints a `u64` without rounding, and the
/// explorer overlay reads the JSON source text, so nothing is lost at 2^53.
/// `charms`, `parents`, `child_count`, `rune` and `metaprotocol` exist for
/// layout compatibility; this fork does not index them, so they are empty.
#[derive(Debug, PartialEq, Serialize)]
pub struct InscriptionDetail {
  pub chain: &'static str,
  /// The configured Dogecoin network, the same string `/api/v1/capabilities`
  /// carries.
  pub network: String,
  pub id: String,
  pub number: u64,
  /// `None` when the current output is not an address.
  pub address: Option<String>,
  pub content_type: Option<String>,
  pub content_length: Option<usize>,
  /// Genesis height.
  pub height: u32,
  /// Genesis fee in koinu.
  pub fee: u64,
  /// Value of the current output in koinu.
  pub value: u64,
  /// `None` without `--index-sats`.
  pub sat: Option<u64>,
  /// `txid:vout:offset` of the current location.
  pub satpoint: String,
  /// `txid:vout` of the current output.
  pub output: String,
  pub genesis_transaction: String,
  /// Unix seconds of the genesis block.
  pub timestamp: u32,
  pub charms: Vec<String>,
  pub parents: Vec<String>,
  pub child_count: u32,
  pub rune: Option<String>,
  pub metaprotocol: Option<String>,
  pub previous: Option<String>,
  pub next: Option<String>,
}

/// One transaction output in the field layout upstream `ord` answers for
/// `GET /output/:outpoint` under `Accept: application/json`. `runes` carries
/// the Dunes balance of the output under the upstream key so a consumer
/// written against upstream reads this fork unchanged; `sat_ranges` is
/// `None` without `--index-sats`.
#[derive(Debug, PartialEq, Serialize)]
pub struct OutputDetail {
  pub chain: &'static str,
  pub network: String,
  /// `txid:vout`.
  pub outpoint: String,
  /// `None` when the script is not an address.
  pub address: Option<String>,
  /// Whether the index has processed the transaction of this output.
  pub indexed: bool,
  pub inscriptions: Vec<String>,
  /// Dunes on the output keyed by spaced name.
  pub runes: BTreeMap<String, OutputDuneBalance>,
  pub sat_ranges: Option<Vec<(u64, u64)>>,
  /// The output script as assembly, as upstream renders it.
  pub script_pubkey: String,
  pub spent: bool,
  pub transaction: String,
  /// Value in koinu.
  pub value: u64,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct OutputDuneBalance {
  pub amount: u128,
  pub divisibility: u8,
  pub symbol: Option<char>,
}

/// One DRC-20 deployment with its indexed protocol state.
///
/// The transferable inventory answers "what can be spent right now". It is
/// not a token catalog: a valid deployment with no outstanding transferable
/// has no row there, so building an index from it hides real tokens. This
/// projection is the catalog, taken straight from the indexed DRC-20 state.
/// Index capabilities, so a consumer can tell "this chain has no DRC-20
/// tokens" apart from "this database cannot answer DRC-20 questions".
#[derive(Debug, PartialEq, Serialize)]
pub struct IndexCapabilities {
  pub chain: &'static str,
  /// The configured Dogecoin network: `mainnet`, `testnet`, `regtest` or
  /// `signet`. Lets a consumer verify the network instead of inferring it
  /// from a port.
  pub network: String,
  pub block_count: u32,
  pub block_hash: String,
  pub drc20: bool,
  pub dunes: bool,
  pub sats: bool,
  pub transactions: bool,
  /// Whether this binary retains a per-operation DRC-20 decision in the same
  /// write transaction as the ledger change. Additive: an older consumer
  /// ignores it.
  #[serde(rename = "drc20Decisions")]
  pub drc20_decisions: bool,
  /// First height whose decisions are retained, `None` until the table has
  /// recorded its first block on this database.
  #[serde(rename = "drc20DecisionsFromHeight")]
  pub drc20_decisions_from_height: Option<u32>,
}

/// The block an operation decision, or an unevaluated inscription, belongs to.
#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Drc20DecisionCheckpoint {
  pub height: u32,
  pub block_hash: String,
}

/// Where decision coverage starts and how far the index has read. An
/// inscription below `decisions_from_height` has no retained verdict, and
/// the ledger must not be read backwards to invent one.
#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Drc20DecisionCoverage {
  pub decisions_from_height: Option<u32>,
  pub indexed_height: u32,
}

/// One DRC-20 operation verdict.
///
/// `verdict` is `accepted` or `rejected` only when the indexer evaluated this
/// exact operation against the ledger and retained the outcome. Everything
/// else is `not-evaluated` with a `reason` naming why: the DRC-20 index is
/// disabled, the block predates decision coverage, or the inscription carries
/// no DRC-20 operation at all. A consumer must never treat `not-evaluated`
/// as either acceptance or rejection.
#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Drc20OperationDecision {
  pub inscription_id: String,
  pub txid: String,
  /// Inscription index within the inscription's transaction.
  pub index: u32,
  pub operation: Option<&'static str>,
  pub tick: Option<String>,
  /// Ledger-effective atomic amount as an exact decimal string; present only
  /// on accepted mint, inscribe-transfer and transfer operations.
  pub amount: Option<String>,
  pub verdict: &'static str,
  pub reason: Option<String>,
  pub ruleset: &'static str,
  pub checkpoint: Option<Drc20DecisionCheckpoint>,
  pub reorg_epoch: u64,
  pub coverage: Drc20DecisionCoverage,
}

/// Every retained decision for the operations carried by one transaction.
/// A transaction carrying no DRC-20 operation lists none; `coverage` says
/// whether that absence is meaningful.
#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Drc20TransactionDecisions {
  pub txid: String,
  pub decisions: Vec<Drc20OperationDecision>,
  pub coverage: Drc20DecisionCoverage,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct Drc20TokenInventoryItem {
  pub ticker: String,
  pub deploy_inscription_id: String,
  pub deploy_inscription_number: String,
  pub decimals: u8,
  pub max_atomic: String,
  pub limit_atomic: String,
  pub minted_atomic: String,
  pub remaining_atomic: String,
  pub holder_count: usize,
  pub deployed_height: u32,
  pub deployed_timestamp: u32,
  pub deployed_by: String,
  pub latest_mint_number: String,
  pub complete: bool,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct Drc20TokenInventory {
  pub chain: &'static str,
  /// False when this database was created without `--index-drc20`.
  pub drc20_index_enabled: bool,
  pub block_count: u32,
  pub block_hash: String,
  pub inventory_complete: bool,
  pub total_count: usize,
  pub next_cursor: Option<String>,
  pub tokens: Vec<Drc20TokenInventoryItem>,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct Drc20TokenDetail {
  pub chain: &'static str,
  pub drc20_index_enabled: bool,
  pub block_count: u32,
  pub block_hash: String,
  pub inventory_complete: bool,
  pub token: Drc20TokenInventoryItem,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct Drc20HolderInventoryItem {
  pub address: String,
  pub overall_atomic: String,
  pub transferable_atomic: String,
  pub available_atomic: String,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct Drc20HolderInventory {
  pub chain: &'static str,
  /// False when this database was created without `--index-drc20`.
  pub drc20_index_enabled: bool,
  pub block_count: u32,
  pub block_hash: String,
  pub ticker: String,
  pub inventory_complete: bool,
  pub total_count: usize,
  pub next_cursor: Option<String>,
  pub holders: Vec<Drc20HolderInventoryItem>,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct FundingInventoryItem {
  pub txid: String,
  pub vout: u32,
  pub value_sats: String,
  pub script_pubkey: String,
  pub raw_previous_transaction: String,
  pub confirmations: u32,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct FundingInventory {
  pub chain: &'static str,
  pub block_count: u32,
  pub block_hash: String,
  pub address: String,
  pub inventory_complete: bool,
  pub total_count: usize,
  pub truncated: bool,
  pub inputs: Vec<FundingInventoryItem>,
}

/// One etched dune, with every quantity as an exact atomic string.
///
/// A dune's supply arithmetic is u128, which no JSON number can carry, so
/// every amount is a string of the exact digits, the same contract the
/// DRC-20 inventory uses. `divisibility` is the dune's own shift and is the
/// only rule by which an amount here may be scaled for display.
#[derive(Debug, PartialEq, Serialize)]
pub struct DuneTokenInventoryItem {
  /// The spaced name, exactly as the protocol displays it.
  pub dune: String,
  /// The `block:index` identifier of the etching.
  pub dune_id: String,
  pub number: String,
  pub symbol: Option<String>,
  pub divisibility: u8,
  pub etching_txid: String,
  pub supply_atomic: String,
  pub premine_atomic: String,
  pub mints_atomic: String,
  pub burned_atomic: String,
  pub etched_height: String,
  pub etched_timestamp: u64,
  /// Whether the terms allow a mint in the next block, per the index.
  pub mintable: bool,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct DuneTokenInventory {
  pub chain: &'static str,
  /// False when this database was created without `--index-dunes`.
  pub dune_index_enabled: bool,
  pub block_count: u32,
  pub block_hash: String,
  pub inventory_complete: bool,
  pub total_count: usize,
  pub next_cursor: Option<String>,
  pub tokens: Vec<DuneTokenInventoryItem>,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct DuneTokenDetail {
  pub chain: &'static str,
  pub dune_index_enabled: bool,
  pub block_count: u32,
  pub block_hash: String,
  pub inventory_complete: bool,
  pub token: DuneTokenInventoryItem,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn offset_cursors_reject_anything_that_is_not_a_decimal_position() {
    assert_eq!(checked_offset_cursor(None), Ok(0));
    assert_eq!(checked_offset_cursor(Some("0")), Ok(0));
    assert_eq!(checked_offset_cursor(Some("4200")), Ok(4200));
    assert!(checked_offset_cursor(Some("")).is_err());
    assert!(checked_offset_cursor(Some("-1")).is_err());
    assert!(checked_offset_cursor(Some("12a")).is_err());
    assert!(checked_offset_cursor(Some(&"9".repeat(20))).is_err());
  }

  #[test]
  fn delegated_inventory_metadata_describes_the_bytes_served_by_content() {
    let inscription = Inscription::new(
      Some(b"text/plain".to_vec()),
      Some(b"delegate-reference".to_vec()),
    );
    let delegate = Inscription::new(Some(b"image/png".to_vec()), Some(vec![0; 42]));

    assert_eq!(
      resolved_content_metadata(&inscription, Some(&delegate)),
      (Some("image/png".to_string()), Some(42)),
    );
    assert_eq!(
      resolved_content_metadata(&inscription, None),
      (Some("text/plain".to_string()), Some(18)),
    );
  }
}
