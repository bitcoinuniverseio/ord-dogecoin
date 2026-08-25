use serde::Serialize;

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
  cursor.parse::<usize>().map_err(|_| "cursor is out of range")
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

/// One DRC-20 deployment with its indexed protocol state.
///
/// The transferable inventory answers "what can be spent right now". It is
/// not a token catalog: a valid deployment with no outstanding transferable
/// has no row there, so building an index from it hides real tokens. This
/// projection is the catalog, taken straight from the indexed DRC-20 state.
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
  pub block_count: u32,
  pub block_hash: String,
  pub inventory_complete: bool,
  pub total_count: usize,
  pub next_cursor: Option<String>,
  pub tokens: Vec<Drc20TokenInventoryItem>,
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
