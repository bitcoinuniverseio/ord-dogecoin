use ord::authority_api::{
  checked_funding_limit, checked_inventory_limit, checked_offset_cursor, Drc20HolderInventory,
  Drc20HolderInventoryItem, Drc20TokenDetail, Drc20TokenInventory, Drc20TokenInventoryItem,
  Drc20TransferableInventory, Drc20TransferableInventoryItem, DuneTokenDetail,
  DuneTokenInventory, DuneTokenInventoryItem, FundingInventory,
  FundingInventoryItem, IndexCapabilities, InscriptionInventory, InscriptionInventoryItem,
  InventoryLocation,
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
    total_count: 1,
    truncated: false,
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
  assert_eq!(encoded["total_count"], 1);
  assert_eq!(encoded["truncated"], false);
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

#[test]
fn bounds_offset_cursors() {
  assert_eq!(checked_offset_cursor(None), Ok(0));
  assert_eq!(checked_offset_cursor(Some("0")), Ok(0));
  assert_eq!(checked_offset_cursor(Some("250")), Ok(250));
  assert!(checked_offset_cursor(Some("")).is_err());
  assert!(checked_offset_cursor(Some("-1")).is_err());
  assert!(checked_offset_cursor(Some("1.5")).is_err());
  assert!(checked_offset_cursor(Some("0x10")).is_err());
  assert!(checked_offset_cursor(Some(&"9".repeat(20))).is_err());
}

/// The DRC-20 token catalog is a different question from the transferable
/// inventory. A deployed token with nothing currently transferable must still
/// appear, and every quantity has to survive as an exact string so a u128
/// supply is never rounded through a JSON number.
#[test]
fn serializes_a_drc20_token_catalog_independent_of_transferable_inventory() {
  let inventory = Drc20TokenInventory {
    chain: "dogecoin",
    drc20_index_enabled: true,
    block_count: 6_400_001,
    block_hash: "ab".repeat(32),
    inventory_complete: true,
    total_count: 1,
    next_cursor: Some("250".to_string()),
    tokens: vec![Drc20TokenInventoryItem {
      ticker: "DOGI".to_string(),
      deploy_inscription_id: format!("{}i0", "cd".repeat(32)),
      deploy_inscription_number: u64::MAX.to_string(),
      decimals: 8,
      max_atomic: u128::MAX.to_string(),
      limit_atomic: "100000000".to_string(),
      // No outstanding transferable, yet the deployment is still real.
      minted_atomic: "0".to_string(),
      remaining_atomic: u128::MAX.to_string(),
      holder_count: 0,
      deployed_height: 5_000_000,
      deployed_timestamp: 1_700_000_000,
      deployed_by: "D6VhYBz1fKqA4A3nQrVZqfDkFvX2F4j3Zq".to_string(),
      latest_mint_number: "0".to_string(),
      complete: false,
    }],
  };

  let encoded = serde_json::to_value(inventory).unwrap();
  assert_eq!(encoded["chain"], json!("dogecoin"));
  assert_eq!(encoded["inventory_complete"], true);
  assert_eq!(encoded["next_cursor"], json!("250"));
  assert_eq!(
    encoded["tokens"][0]["max_atomic"],
    json!(u128::MAX.to_string())
  );
  assert_eq!(
    encoded["tokens"][0]["remaining_atomic"],
    json!(u128::MAX.to_string())
  );
  assert_eq!(encoded["tokens"][0]["minted_atomic"], json!("0"));
  assert_eq!(encoded["tokens"][0]["holder_count"], json!(0));
  assert_eq!(encoded["tokens"][0]["complete"], false);
  assert_eq!(
    encoded["tokens"][0]["deploy_inscription_number"],
    json!(u64::MAX.to_string())
  );
}

#[test]
fn serializes_drc20_holder_balances_as_exact_atomic_strings() {
  let inventory = Drc20HolderInventory {
    chain: "dogecoin",
    drc20_index_enabled: true,
    block_count: 6_400_001,
    block_hash: "ab".repeat(32),
    ticker: "DOGI".to_string(),
    inventory_complete: true,
    total_count: 1,
    next_cursor: None,
    holders: vec![Drc20HolderInventoryItem {
      address: "D6VhYBz1fKqA4A3nQrVZqfDkFvX2F4j3Zq".to_string(),
      overall_atomic: u128::MAX.to_string(),
      transferable_atomic: "100000000".to_string(),
      available_atomic: (u128::MAX - 100_000_000).to_string(),
    }],
  };

  let encoded = serde_json::to_value(inventory).unwrap();
  assert_eq!(encoded["ticker"], json!("DOGI"));
  assert_eq!(encoded["next_cursor"], json!(null));
  assert_eq!(
    encoded["holders"][0]["overall_atomic"],
    json!(u128::MAX.to_string())
  );
  assert_eq!(
    encoded["holders"][0]["available_atomic"],
    json!((u128::MAX - 100_000_000).to_string())
  );
}

/// RC-3 regression. A database created without `--index-drc20` answered every
/// DRC-20 query with `200 []`, which reads downstream as "this chain has no
/// tokens". The capability has to be reported explicitly so an empty catalog
/// can never be mistaken for a healthy one.
#[test]
fn reports_index_capabilities_so_an_empty_result_is_never_ambiguous() {
  let capabilities = IndexCapabilities {
    chain: "dogecoin",
    block_count: 6_400_001,
    block_hash: "ab".repeat(32),
    drc20: false,
    dunes: false,
    sats: false,
    transactions: true,
  };

  let encoded = serde_json::to_value(capabilities).unwrap();
  assert_eq!(encoded["chain"], json!("dogecoin"));
  assert_eq!(encoded["drc20"], false);
  assert_eq!(encoded["transactions"], true);
  assert_eq!(encoded["block_count"], json!(6_400_001));
}

#[test]
fn drc20_payloads_state_that_the_index_can_answer_drc20() {
  let inventory = Drc20TokenInventory {
    chain: "dogecoin",
    drc20_index_enabled: true,
    block_count: 6_400_001,
    block_hash: "ab".repeat(32),
    inventory_complete: true,
    total_count: 0,
    next_cursor: None,
    tokens: vec![],
  };

  let encoded = serde_json::to_value(inventory).unwrap();
  // An empty catalog is only meaningful alongside this flag.
  assert_eq!(encoded["drc20_index_enabled"], true);
  assert_eq!(encoded["total_count"], json!(0));
  assert_eq!(encoded["tokens"].as_array().unwrap().len(), 0);
}

#[test]
fn serializes_one_drc20_definition_with_the_same_exact_contract() {
  let detail = Drc20TokenDetail {
    chain: "dogecoin",
    drc20_index_enabled: true,
    block_count: 6_400_001,
    block_hash: "ab".repeat(32),
    inventory_complete: true,
    token: Drc20TokenInventoryItem {
      ticker: "DOGI".to_string(),
      deploy_inscription_id: format!("{}i0", "cd".repeat(32)),
      deploy_inscription_number: "1".to_string(),
      decimals: 8,
      max_atomic: u128::MAX.to_string(),
      limit_atomic: "100000000".to_string(),
      minted_atomic: "200000000".to_string(),
      remaining_atomic: (u128::MAX - 200_000_000).to_string(),
      holder_count: 1,
      deployed_height: 5_000_000,
      deployed_timestamp: 1_700_000_000,
      deployed_by: "D6VhYBz1fKqA4A3nQrVZqfDkFvX2F4j3Zq".to_string(),
      latest_mint_number: "2".to_string(),
      complete: false,
    },
  };

  let encoded = serde_json::to_value(detail).unwrap();
  assert_eq!(encoded["chain"], json!("dogecoin"));
  assert_eq!(encoded["inventory_complete"], true);
  assert_eq!(encoded["token"]["max_atomic"], json!(u128::MAX.to_string()));
  assert_eq!(
    encoded["token"]["remaining_atomic"],
    json!((u128::MAX - 200_000_000).to_string())
  );
}

#[test]
fn serializes_dune_amounts_as_exact_u128_strings() {
  let inventory = DuneTokenInventory {
    chain: "dogecoin",
    dune_index_enabled: true,
    block_count: 6_400_001,
    block_hash: "ab".repeat(32),
    inventory_complete: true,
    total_count: 1,
    next_cursor: Some("1".to_string()),
    tokens: vec![DuneTokenInventoryItem {
      dune: "SUCH•WOW•DUNE".to_string(),
      dune_id: "5084000:1".to_string(),
      number: u64::MAX.to_string(),
      symbol: Some("D".to_string()),
      divisibility: 8,
      etching_txid: "cd".repeat(32),
      supply_atomic: u128::MAX.to_string(),
      premine_atomic: "1000000000".to_string(),
      mints_atomic: "21000".to_string(),
      burned_atomic: "0".to_string(),
      etched_height: "5084000".to_string(),
      etched_timestamp: 1_700_000_000,
      mintable: true,
    }],
  };

  let encoded = serde_json::to_value(inventory).unwrap();
  // u128::MAX has no JSON number; only the exact string survives the trip.
  assert_eq!(
    encoded["tokens"][0]["supply_atomic"],
    json!(u128::MAX.to_string())
  );
  assert_eq!(encoded["tokens"][0]["divisibility"], json!(8));
  assert_eq!(encoded["dune_index_enabled"], true);
  assert_eq!(encoded["next_cursor"], json!("1"));
}

#[test]
fn dune_payloads_state_that_the_index_can_answer_dunes() {
  // An empty catalog is only meaningful alongside this flag: the endpoint
  // fails closed when the database never indexed dunes, so a served empty
  // list is a true "none etched yet" rather than an unanswerable question.
  let inventory = DuneTokenInventory {
    chain: "dogecoin",
    dune_index_enabled: true,
    block_count: 6_400_001,
    block_hash: "ab".repeat(32),
    inventory_complete: true,
    total_count: 0,
    next_cursor: None,
    tokens: vec![],
  };

  let encoded = serde_json::to_value(inventory).unwrap();
  assert_eq!(encoded["dune_index_enabled"], true);
  assert_eq!(encoded["total_count"], json!(0));
  assert_eq!(encoded["tokens"].as_array().unwrap().len(), 0);
}

#[test]
fn serializes_one_dune_definition_with_a_symbol_free_of_invention() {
  let detail = DuneTokenDetail {
    chain: "dogecoin",
    dune_index_enabled: true,
    block_count: 6_400_001,
    block_hash: "ab".repeat(32),
    inventory_complete: true,
    token: DuneTokenInventoryItem {
      dune: "BARE•DUNE".to_string(),
      dune_id: "5084001:0".to_string(),
      number: "2".to_string(),
      symbol: None,
      divisibility: 0,
      etching_txid: "ef".repeat(32),
      supply_atomic: "0".to_string(),
      premine_atomic: "0".to_string(),
      mints_atomic: "0".to_string(),
      burned_atomic: "0".to_string(),
      etched_height: "5084001".to_string(),
      etched_timestamp: 1_700_000_001,
      mintable: false,
    },
  };

  let encoded = serde_json::to_value(detail).unwrap();
  // A dune with no symbol reports null, never a placeholder glyph.
  assert_eq!(encoded["token"]["symbol"], serde_json::Value::Null);
  assert_eq!(encoded["token"]["mintable"], false);
}
