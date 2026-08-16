use ord::authority_api::{
  checked_funding_limit, checked_inventory_limit, Drc20TransferableInventory,
  Drc20TransferableInventoryItem, FundingInventory, FundingInventoryItem, InscriptionInventory,
  InscriptionInventoryItem, InventoryLocation,
};
use serde_json::json;

#[test]
fn bounds_inventory_pages() {
  assert_eq!(checked_inventory_limit(None), Ok(250));
  assert_eq!(checked_inventory_limit(Some(1)), Ok(1));
  assert_eq!(checked_inventory_limit(Some(1_000)), Ok(1_000));
  assert!(checked_inventory_limit(Some(0)).is_err());
  assert!(checked_inventory_limit(Some(1_001)).is_err());
}

#[test]
fn bounds_funding_pages() {
  assert_eq!(checked_funding_limit(None), Ok(20));
  assert_eq!(checked_funding_limit(Some(1)), Ok(1));
  assert_eq!(checked_funding_limit(Some(50)), Ok(50));
  assert!(checked_funding_limit(Some(0)).is_err());
  assert!(checked_funding_limit(Some(51)).is_err());
}

#[test]
fn serializes_exact_cardinal_funding_proofs() {
  let inventory = FundingInventory {
    chain: "dogecoin",
    block_count: 6_400_001,
    block_hash: "ab".repeat(32),
    address: "D6VhYBz1fKqA4A3nQrVZqfDkFvX2F4j3Zq".to_string(),
    inventory_complete: true,
    inputs: vec![FundingInventoryItem {
      txid: "cd".repeat(32),
      vout: u32::MAX,
      value_sats: u64::MAX.to_string(),
      script_pubkey: "76a914000000000000000000000000000000000000000088ac".to_string(),
      raw_previous_transaction: "0100000001".to_string(),
      confirmations: u32::MAX,
    }],
  };

  let encoded = serde_json::to_value(inventory).unwrap();
  assert_eq!(encoded["inventory_complete"], true);
  assert_eq!(
    encoded["inputs"][0]["value_sats"],
    json!(u64::MAX.to_string())
  );
  assert_eq!(encoded["inputs"][0]["vout"], json!(u32::MAX));
}

#[test]
fn serializes_complete_inscription_inventory_with_subsidy_provenance() {
  let inventory = InscriptionInventory {
    chain: "dogecoin",
    block_count: 6_400_001,
    block_hash: "ab".repeat(32),
    subsidy_schedule_hash: "12".repeat(32),
    inventory_complete: true,
    next_cursor: Some(u64::MAX.to_string()),
    inscriptions: vec![InscriptionInventoryItem {
      inscription_id: format!("{}i1", "cd".repeat(32)),
      inscription_number: u64::MAX.to_string(),
      genesis_height: 5_000_000,
      timestamp: 1_700_000_000,
      content_type: Some("image/png".to_string()),
      content_length: Some(42),
      subsidy_sats: "100000000000000".to_string(),
      location: None,
    }],
  };

  let encoded = serde_json::to_value(inventory).unwrap();
  assert_eq!(encoded["inventory_complete"], true);
  assert_eq!(encoded["subsidy_schedule_hash"], json!("12".repeat(32)));
  assert_eq!(encoded["next_cursor"], json!(u64::MAX.to_string()));
  assert_eq!(
    encoded["inscriptions"][0]["subsidy_sats"],
    json!("100000000000000")
  );
}

#[test]
fn serializes_exact_drc20_and_custody_values_as_strings() {
  let inventory = Drc20TransferableInventory {
    chain: "dogecoin",
    block_count: 6_400_001,
    block_hash: "ab".repeat(32),
    inventory_complete: true,
    transferables: vec![Drc20TransferableInventoryItem {
      ticker: "DOGI".to_string(),
      amount_atomic: u128::MAX.to_string(),
      decimals: 8,
      max_atomic: u128::MAX.to_string(),
      limit_atomic: "100000000".to_string(),
      transfer_inscription_id: format!("{}i0", "cd".repeat(32)),
      inscription_number: u64::MAX.to_string(),
      owner_address: "D6VhYBz1fKqA4A3nQrVZqfDkFvX2F4j3Zq".to_string(),
      genesis_height: 5_000_000,
      transaction_index: 42,
      inscription_index: 0,
      location: Some(InventoryLocation {
        txid: "ef".repeat(32),
        vout: 1,
        offset: u64::MAX.to_string(),
        value: u64::MAX.to_string(),
        script_pubkey: "76a914000000000000000000000000000000000000000088ac".to_string(),
        address: Some("D6VhYBz1fKqA4A3nQrVZqfDkFvX2F4j3Zq".to_string()),
      }),
    }],
  };

  let encoded = serde_json::to_value(inventory).unwrap();
  assert_eq!(encoded["inventory_complete"], true);
  assert_eq!(
    encoded["transferables"][0]["amount_atomic"],
    json!(u128::MAX.to_string())
  );
  assert_eq!(
    encoded["transferables"][0]["location"]["offset"],
    json!(u64::MAX.to_string())
  );
  assert_eq!(
    encoded["transferables"][0]["location"]["value"],
    json!(u64::MAX.to_string())
  );
}
