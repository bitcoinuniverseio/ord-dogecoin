use serde::Serialize;

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
