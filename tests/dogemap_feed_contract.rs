//! End-to-end contract of the `dogemap-feed-v1` provider feed (Dogemap
//! contract v1, section 2).
//!
//! Every chain test indexes regtest blocks served by `test-bitcoincore-rpc`
//! through the production `ord` binary and reads the feed over HTTP, so the
//! journal written by the updater, the single-transaction readers and the
//! wire format are proved together. The golden and RFC 8785 tests exercise
//! the exact serializer the server uses.

mod dogemap_support;

use {
  bitcoin::{hashes::Hash, Script},
  dogemap_support::{assert_error, envelope, envelope_head, envelope_tail, Chain, DEFAULT_SUBSIDY},
  ord::dogemap_feed::{self as wire, BlockCursor, EventsHashHeader},
  redb::{Database, TableDefinition},
  serde_json::{json, Value},
  test_bitcoincore_rpc::TransactionTemplate,
};

const DOGEMAP_FEED_META: TableDefinition<&str, &[u8]> = TableDefinition::new("DOGEMAP_FEED_META");
const DOGEMAP_FEED_JOURNAL: TableDefinition<(u32, u32), &[u8]> =
  TableDefinition::new("DOGEMAP_FEED_JOURNAL");

const REGTEST_GENESIS: &str = "3d2160a3b5dc4a9d62e7e66a295f70313ac808440ef7400d6c0772171ce973a5";

fn b64(bytes: &[u8]) -> String {
  base64::encode(bytes)
}

fn is_decimal(value: &Value) -> bool {
  value
    .as_str()
    .is_some_and(|s| wire::parse_decimal_u64(s).is_some())
}

fn p2pkh(byte: u8) -> Script {
  Script::new_p2pkh(&bitcoin::PubkeyHash::from_slice(&[byte; 20]).unwrap())
}

/// Recompute a block page's eventsHash from its own fields.
fn recompute_events_hash(page: &Value, events: &[Value]) -> String {
  let header = EventsHashHeader {
    network: page["network"].as_str().unwrap(),
    genesis_hash: page["genesisHash"].as_str().unwrap(),
    height: page["height"].as_str().unwrap().parse().unwrap(),
    block_hash: page["blockHash"].as_str().unwrap(),
    parent_hash: page["parentHash"].as_str(),
    transfers_scope: page["scope"]["transfers"].as_str().unwrap(),
    creation_count: page["creationCount"].as_str().unwrap().parse().unwrap(),
  };
  wire::events_hash(&header, events).unwrap()
}

fn assert_identity(value: &Value) {
  assert_eq!(value["chain"], "dogecoin");
  assert_eq!(value["network"], "regtest");
  assert_eq!(value["genesisHash"], REGTEST_GENESIS);
  assert_eq!(value["providerVersion"], env!("CARGO_PKG_VERSION"));
  assert!(
    wire::is_lower_hex(value["providerCommit"].as_str().unwrap(), 40),
    "{value}"
  );
  assert_eq!(value["parserProfile"], "doginals-trac-1.0.2-compat-v1");
  assert_eq!(value["orderProfile"], "ord-dogecoin-inscription-number-v1");
  assert_eq!(value["candidateFilter"], "dogemap-candidate-prefilter-v1");
  assert_eq!(value["feedVersion"], "dogemap-feed-v1");
  assert_eq!(value["databaseSchema"], "6");
  assert!(wire::is_lower_hex(
    value["databaseId"].as_str().unwrap(),
    32
  ));
  assert!(is_decimal(&value["reorgEpoch"]));
}

#[test]
fn golden_events_hash_vector_is_reproduced() {
  let golden: Value = serde_json::from_str(
    &std::fs::read_to_string(dogemap_support::repository_file(
      "docs/contract/eventsHash-golden-v1.json",
    ))
    .unwrap(),
  )
  .unwrap();
  let block = &golden["block"];
  let events = block["events"].as_array().unwrap();
  let header = EventsHashHeader {
    network: block["network"].as_str().unwrap(),
    genesis_hash: block["genesisHash"].as_str().unwrap(),
    height: block["height"].as_str().unwrap().parse().unwrap(),
    block_hash: block["blockHash"].as_str().unwrap(),
    parent_hash: block["parentHash"].as_str(),
    transfers_scope: block["scope"]["transfers"].as_str().unwrap(),
    creation_count: block["creationCount"].as_str().unwrap().parse().unwrap(),
  };

  let input = wire::events_hash_input(&header, events);
  assert_eq!(input, golden["hashInput"]);
  let serialization = wire::rfc8785(&input).unwrap();
  assert_eq!(serialization, golden["serialization"].as_str().unwrap());
  assert_eq!(
    wire::sha256_hex(serialization.as_bytes()),
    golden["sha256"].as_str().unwrap()
  );
  assert_eq!(
    wire::events_hash(&header, events).unwrap(),
    golden["sha256"].as_str().unwrap()
  );
  // The vector covers a value above 2^53 exactly.
  assert!(serialization.contains("\"valueKoinu\":\"9007199254740993\""));
}

#[test]
fn rfc8785_subset_sorts_by_utf16_and_escapes_like_json_stringify() {
  // U+1F600 is a surrogate pair (D83D DE00) and sorts before U+FFFD in
  // UTF-16 order although its code point is larger.
  let value = json!({
    "b": [null, true, false, "x"],
    "a": { "\u{fffd}": "1", "\u{1f600}": "2", "Z": "3" },
    "c": "quote\" backslash\\ controls\u{8}\u{c}\n\r\t\u{1}\u{1f} slash/ e\u{e9} \u{2028}",
  });
  assert_eq!(
    wire::rfc8785(&value).unwrap(),
    "{\"a\":{\"Z\":\"3\",\"\u{1f600}\":\"2\",\"\u{fffd}\":\"1\"},\"b\":[null,true,false,\"x\"],\"c\":\"quote\\\" backslash\\\\ controls\\b\\f\\n\\r\\t\\u0001\\u001f slash/ e\u{e9} \u{2028}\"}"
  );
  assert_eq!(
    wire::rfc8785(&json!({"n": 1})),
    Err(wire::Rfc8785Error::Number)
  );
}

#[test]
fn prefilter_is_a_superset_of_the_dogemap_grammar() {
  for digits in ["0", "1", "9", "10", "4771259", "4294967295", "99999999999"] {
    assert!(wire::candidate_prefilter(
      format!("{digits}.dogemap").as_bytes()
    ));
  }
  // Rejected by the grammar, still candidates: the filter only narrows.
  for body in [
    "01.dogemap",
    "x.DOGEMAP",
    " 1.dogemap",
    "1.dogemap\n",
    "1.2.dogemap",
    ".dogemap",
  ] {
    assert!(wire::candidate_prefilter(body.as_bytes()), "{body}");
  }
  assert!(!wire::candidate_prefilter(b""));
  assert!(!wire::candidate_prefilter(b"4771259.dogema"));
  assert!(!wire::candidate_prefilter(b"4771259dogemap"));
  let long = format!("{}.dogemap", "1".repeat(57));
  assert_eq!(long.len(), 65);
  assert!(!wire::candidate_prefilter(long.as_bytes()));
  assert!(wire::candidate_prefilter(&long.as_bytes()[1..]));
}

#[test]
fn cursors_round_trip_and_reject_anything_else() {
  let cursor = BlockCursor {
    database_id: [7; 16],
    reorg_epoch: u64::MAX,
    height: 4_771_259,
    block_hash: [1; 32],
    events_hash: [2; 32],
    next_ordinal: 500,
  };
  let encoded = cursor.encode();
  assert_eq!(BlockCursor::decode(&encoded), Some(cursor));
  assert!(encoded.len() < 200);
  assert_eq!(BlockCursor::decode(""), None);
  assert_eq!(BlockCursor::decode(&encoded[1..]), None);
  assert_eq!(BlockCursor::decode(&format!("{encoded}A")), None);
  assert_eq!(BlockCursor::decode(&format!("+{}", &encoded[1..])), None);
  let mut bytes = base64::decode_config(&encoded, base64::URL_SAFE_NO_PAD).unwrap();
  bytes[0] = 2;
  assert_eq!(
    BlockCursor::decode(&base64::encode_config(bytes, base64::URL_SAFE_NO_PAD)),
    None
  );
}

#[test]
fn capabilities_report_identity_coverage_and_readiness() {
  let chain = Chain::new();
  chain.rpc.mine_blocks(2);
  let server = chain.serve();
  server.wait_for_block_count(3);
  let capabilities = server.wait_for_checkpoint(2);

  assert_eq!(
    capabilities["schemaVersion"],
    "dogemap-feed-capabilities-v1"
  );
  assert_identity(&capabilities);
  assert_eq!(capabilities["reorgEpoch"], "0");
  assert_eq!(
    capabilities["indexedCheckpoint"],
    json!({
      "height": "2",
      "blockHash": chain.block_hash(2),
      "reorgEpoch": "0",
    })
  );
  assert_eq!(capabilities["creationCoverageFromHeight"], "0");
  assert_eq!(capabilities["transferCoverageFromHeight"], "0");
  assert_eq!(capabilities["nodeHeight"], "2");
  assert_eq!(capabilities["ready"], true, "{capabilities}");
  assert_eq!(capabilities["unavailableReason"], Value::Null);
  assert_eq!(capabilities["maxPageLimit"], 500);
  assert_eq!(capabilities["maxInlineBodyBytes"], 4096);
  assert_eq!(capabilities["maxBodyChunkBytes"], 65536);
  assert_eq!(capabilities["bodyRangePolicy"], "base64-chunks-v1");
  assert_eq!(
    capabilities["eventsHashAlgorithm"],
    "sha256-rfc8785-dogemap-feed-v1"
  );
  assert_eq!(capabilities["maxLocationIds"], 100);

  // The generation id is created once and survives further blocks.
  let database_id = capabilities["databaseId"].clone();
  chain.rpc.mine_blocks(1);
  let capabilities = server.wait_for_checkpoint(3);
  assert_eq!(capabilities["databaseId"], database_id);
}

#[test]
fn a_block_without_creations_has_a_complete_empty_manifest() {
  let chain = Chain::new();
  chain.rpc.mine_blocks(2);
  let server = chain.serve();
  server.wait_for_checkpoint(2);

  let (status, page) = server.block(1, &chain.block_hash(1), "");
  assert_eq!(status, 200, "{page}");
  assert_eq!(page["schemaVersion"], "dogemap-feed-block-v1");
  assert_identity(&page);
  assert_eq!(page["height"], "1");
  assert_eq!(page["blockHash"], chain.block_hash(1));
  assert_eq!(page["parentHash"], chain.block_hash(0));
  assert_eq!(page["indexedCheckpoint"]["height"], "2");
  assert_eq!(
    page["scope"],
    json!({"creations": "dogemap-candidate-prefilter-v1", "transfers": "complete"})
  );
  assert_eq!(page["creationCount"], "0");
  assert_eq!(page["totalEvents"], "0");
  assert_eq!(page["events"], json!([]));
  assert_eq!(page["nextCursor"], Value::Null);
  assert_eq!(page["complete"], true);
  assert_eq!(page["eventsHash"], recompute_events_hash(&page, &[]));

  // Genesis has no parent.
  let (status, page) = server.block(0, &chain.block_hash(0), "");
  assert_eq!(status, 200, "{page}");
  assert_eq!(page["parentHash"], Value::Null);
  assert_eq!(page["eventsHash"], recompute_events_hash(&page, &[]));
}

#[test]
fn every_creation_is_counted_and_only_candidates_are_emitted() {
  let chain = Chain::new();
  let rpc = &chain.rpc;
  rpc.mine_blocks(5);
  let owner = p2pkh(0x11);

  let inscribe = |input: usize, content_type: &[u8], body: &[u8], output_script: Script| {
    rpc.broadcast_tx(TransactionTemplate {
      inputs: &[(input, 0, 0)],
      script_sig: envelope(content_type, &[body]),
      output_script,
      ..Default::default()
    })
  };
  let long = format!("{}.dogemap", "7".repeat(57));
  let candidate = inscribe(1, b"text/plain", b"4771259.dogemap", owner.clone());
  let plain = inscribe(2, b"text/plain", b"hello", Script::new());
  let uppercase = inscribe(3, b"text/html", b"x.DOGEMAP", Script::new());
  let too_long = inscribe(4, b"text/plain", long.as_bytes(), Script::new());
  let block = rpc.mine_blocks(1).remove(0);
  let hash = block.header.block_hash().to_string();

  let server = chain.serve();
  server.wait_for_checkpoint(6);
  let (page, events) = server.whole_block(6, &hash, 500);

  assert_eq!(page["creationCount"], "4");
  assert_eq!(page["totalEvents"], "2");
  assert_eq!(page["scope"]["transfers"], "complete");
  assert_eq!(page["eventsHash"], recompute_events_hash(&page, &events));

  let position = |txid| {
    block
      .txdata
      .iter()
      .position(|tx| tx.txid() == txid)
      .unwrap()
      .to_string()
  };

  // Ordered by inscription number, which follows block order.
  assert_eq!(events[0]["inscriptionId"], format!("{candidate}i0"));
  assert_eq!(events[1]["inscriptionId"], format!("{uppercase}i0"));
  let numbers = events
    .iter()
    .map(|event| {
      event["inscriptionNumber"]
        .as_str()
        .unwrap()
        .parse::<u64>()
        .unwrap()
    })
    .collect::<Vec<u64>>();
  assert!(numbers[0] < numbers[1]);

  let owner_address = bitcoin::Address::from_script(&owner, bitcoin::Network::Regtest)
    .unwrap()
    .to_string();
  assert_eq!(
    events[0],
    json!({
      "type": "creation",
      "eventOrdinal": "0",
      "inscriptionId": format!("{candidate}i0"),
      "inscriptionNumber": numbers[0].to_string(),
      "origin": {"txid": candidate.to_string(), "height": "6", "blockHash": hash},
      "completion": {
        "txid": candidate.to_string(),
        "height": "6",
        "blockHash": hash,
        "txIndex": position(candidate),
      },
      "partCount": "1",
      "contentTypeBase64": b64(b"text/plain"),
      "rawBody": {
        "encoding": "base64",
        "byteLength": "15",
        "sha256": wire::sha256_hex(b"4771259.dogemap"),
        "bytes": b64(b"4771259.dogemap"),
        "bodyRef": null,
      },
      "delegate": null,
      "location": {
        "status": "assigned",
        "outpoint": format!("{candidate}:0"),
        "offset": "0",
        "valueKoinu": DEFAULT_SUBSIDY.to_string(),
        "scriptPubKeyHex": hex::encode(owner.as_bytes()),
        "address": owner_address,
      },
    })
  );
  // A non-address script keeps its raw bytes; the address is absent.
  assert_eq!(events[1]["location"]["scriptPubKeyHex"], "");
  assert_eq!(events[1]["location"]["address"], Value::Null);
  assert_eq!(events[1]["contentTypeBase64"], b64(b"text/html"));
  assert_eq!(events[1]["completion"]["txIndex"], position(uppercase));

  // The non-candidates exist, they are just not emitted.
  for txid in [plain, too_long] {
    let (status, locations) = server.json(&format!("/api/v1/dogemap-feed/locations?ids={txid}i0"));
    assert_eq!(status, 200, "{locations}");
    assert_eq!(locations["locations"][0]["found"], true);
  }
}

#[test]
fn a_multipart_candidate_carries_its_first_reveal_and_completion() {
  let chain = Chain::new();
  let rpc = &chain.rpc;
  rpc.mine_blocks(3);

  let first = rpc.broadcast_tx(TransactionTemplate {
    inputs: &[(1, 0, 0)],
    script_sig: envelope_head(b"text/plain", 3, &[b"4771", b"259."]),
    ..Default::default()
  });
  let origin_block = rpc.mine_blocks(1).remove(0);
  let second = rpc.broadcast_tx(TransactionTemplate {
    inputs: &[(4, 1, 0)],
    script_sig: envelope_tail(0, &[b"dogemap"]),
    ..Default::default()
  });
  let completion_block = rpc.mine_blocks(1).remove(0);
  let origin_hash = origin_block.header.block_hash().to_string();
  let completion_hash = completion_block.header.block_hash().to_string();

  let server = chain.serve();
  server.wait_for_checkpoint(5);

  // A partial reveal is not a creation.
  let (page, events) = server.whole_block(4, &origin_hash, 10);
  assert_eq!(page["creationCount"], "0");
  assert!(events.is_empty());

  let (page, events) = server.whole_block(5, &completion_hash, 10);
  assert_eq!(page["creationCount"], "1");
  assert_eq!(events.len(), 1);
  let creation = &events[0];
  assert_eq!(creation["inscriptionId"], format!("{first}i0"));
  assert_eq!(creation["partCount"], "2");
  assert_eq!(
    creation["origin"],
    json!({"txid": first.to_string(), "height": "4", "blockHash": origin_hash})
  );
  assert_eq!(
    creation["completion"],
    json!({
      "txid": second.to_string(),
      "height": "5",
      "blockHash": completion_hash,
      "txIndex": "1",
    })
  );
  assert_eq!(creation["rawBody"]["bytes"], b64(b"4771259.dogemap"));
  assert_eq!(creation["location"]["outpoint"], format!("{second}:0"));
  assert_eq!(page["eventsHash"], recompute_events_hash(&page, &events));

  // The body route is bound to the completion block.
  let capabilities = server.capabilities();
  let identity = format!(
    "databaseId={}&reorgEpoch={}",
    capabilities["databaseId"].as_str().unwrap(),
    capabilities["reorgEpoch"].as_str().unwrap()
  );
  let (status, body) = server.json(&format!(
    "/api/v1/dogemap-feed/inscriptions/{first}i0/body?blockHash={completion_hash}&{identity}"
  ));
  assert_eq!(status, 200, "{body}");
  assert_eq!(body["bytes"], b64(b"4771259.dogemap"));
  let (status, body) = server.json(&format!(
    "/api/v1/dogemap-feed/inscriptions/{first}i0/body?blockHash={origin_hash}&{identity}"
  ));
  assert_error(status, &body, 409, "snapshot_replaced");
}

#[test]
fn transfers_are_journaled_with_offsets_scripts_fees_and_lost_sats() {
  let chain = Chain::new();
  let rpc = &chain.rpc;
  rpc.mine_blocks(6);
  let holder = p2pkh(0x22);

  // Block 7: three candidates, each alone on a 50 DOGE output.
  let inscribe = |input: usize, body: &[u8]| {
    rpc.broadcast_tx(TransactionTemplate {
      inputs: &[(input, 0, 0)],
      script_sig: envelope(b"text/plain", &[body]),
      ..Default::default()
    })
  };
  let moved = inscribe(1, b"1.dogemap");
  let fee = inscribe(2, b"2.dogemap");
  let lost = inscribe(3, b"3.dogemap");
  rpc.mine_blocks(1);

  // Block 8: `moved` is the second input, so it sits 50 DOGE into the
  // inputs and lands at a nonzero offset of the first 70 DOGE output; `fee`
  // is spent entirely as fee and lands in the coinbase.
  let spend = rpc.broadcast_tx(TransactionTemplate {
    inputs: &[(4, 0, 0), (7, 1, 0)],
    outputs: 2,
    output_values: &[7_000_000_000, 3_000_000_000],
    output_script: holder.clone(),
    ..Default::default()
  });
  let fee_spend = rpc.broadcast_tx(TransactionTemplate {
    inputs: &[(5, 0, 0), (7, 2, 0)],
    fee: DEFAULT_SUBSIDY,
    ..Default::default()
  });
  let fee_block = rpc.mine_blocks(1).remove(0);

  // Block 9: the coinbase pays no subsidy, so the fee carrying `lost` falls
  // past the coinbase outputs.
  rpc.broadcast_tx(TransactionTemplate {
    inputs: &[(6, 0, 0), (7, 3, 0)],
    fee: DEFAULT_SUBSIDY,
    ..Default::default()
  });
  let lost_block = rpc.mine_blocks_with_subsidy(1, 0).remove(0);

  let server = chain.serve();
  server.wait_for_checkpoint(9);

  let fee_hash = fee_block.header.block_hash().to_string();
  let (page, events) = server.whole_block(8, &fee_hash, 1);
  assert_eq!(page["creationCount"], "0");
  assert_eq!(page["scope"]["transfers"], "complete");
  assert_eq!(page["eventsHash"], recompute_events_hash(&page, &events));
  assert_eq!(events.len(), 2);

  let coinbase = fee_block.txdata[0].txid();
  assert_eq!(
    events[0],
    json!({
      "type": "transfer",
      "eventOrdinal": "0",
      "inscriptionId": format!("{moved}i0"),
      "inscriptionNumber": "0",
      "txid": spend.to_string(),
      "txIndex": "1",
      "from": {"outpoint": format!("{moved}:0"), "offset": "0"},
      "to": {
        "status": "assigned",
        "outpoint": format!("{spend}:0"),
        "offset": DEFAULT_SUBSIDY.to_string(),
        "valueKoinu": "7000000000",
        "scriptPubKeyHex": hex::encode(holder.as_bytes()),
        "address": bitcoin::Address::from_script(&holder, bitcoin::Network::Regtest)
          .unwrap()
          .to_string(),
      },
    })
  );
  // The fee is settled by the coinbase, processed after every other
  // transaction, at the subsidy plus the fee's position.
  assert_eq!(events[1]["inscriptionId"], format!("{fee}i0"));
  assert_eq!(events[1]["txid"], coinbase.to_string());
  assert_eq!(events[1]["txIndex"], "0");
  assert_eq!(events[1]["from"]["outpoint"], format!("{fee}:0"));
  assert_eq!(events[1]["to"]["status"], "assigned");
  assert_eq!(events[1]["to"]["outpoint"], format!("{coinbase}:0"));
  assert_eq!(events[1]["to"]["offset"], DEFAULT_SUBSIDY.to_string());
  assert_eq!(
    events[1]["to"]["valueKoinu"],
    (2 * DEFAULT_SUBSIDY).to_string()
  );
  let _ = fee_spend;

  let lost_hash = lost_block.header.block_hash().to_string();
  let (_, events) = server.whole_block(9, &lost_hash, 10);
  assert_eq!(events.len(), 1);
  assert_eq!(events[0]["inscriptionId"], format!("{lost}i0"));
  assert_eq!(events[0]["txid"], lost_block.txdata[0].txid().to_string());
  assert_eq!(events[0]["to"]["status"], "lost");
  assert_eq!(events[0]["to"]["outpoint"], Value::Null);
  assert!(is_decimal(&events[0]["to"]["offset"]));
  assert_eq!(events[0]["to"]["valueKoinu"], Value::Null);
  assert_eq!(events[0]["to"]["scriptPubKeyHex"], Value::Null);
  assert_eq!(events[0]["to"]["address"], Value::Null);

  // Current locations agree with the last journaled move of each.
  let (status, locations) = server.json(&format!(
    "/api/v1/dogemap-feed/locations?ids={moved}i0,{fee}i0,{lost}i0,{}i0",
    bitcoin::Txid::all_zeros()
  ));
  assert_eq!(status, 200, "{locations}");
  assert_eq!(locations["schemaVersion"], "dogemap-feed-locations-v1");
  assert_identity(&locations);
  assert_eq!(locations["indexedCheckpoint"]["height"], "9");
  let list = locations["locations"].as_array().unwrap();
  assert_eq!(list[0]["location"]["outpoint"], format!("{spend}:0"));
  assert_eq!(list[0]["location"]["offset"], DEFAULT_SUBSIDY.to_string());
  assert_eq!(
    list[0]["location"]["scriptPubKeyHex"],
    hex::encode(holder.as_bytes())
  );
  assert_eq!(list[1]["location"]["outpoint"], format!("{coinbase}:0"));
  assert_eq!(list[1]["location"]["scriptPubKeyHex"], "");
  assert_eq!(list[2]["location"]["status"], "lost");
  assert_eq!(list[2]["inscriptionNumber"], "2");
  assert_eq!(
    list[3],
    json!({
      "inscriptionId": format!("{}i0", bitcoin::Txid::all_zeros()),
      "inscriptionNumber": null,
      "found": false,
      "location": null,
    })
  );
}

#[test]
fn values_above_two_to_the_53_stay_exact() {
  const LARGE: u64 = 9_007_199_254_740_993;
  let chain = Chain::with_subsidies(&[(2, LARGE)]);
  let rpc = &chain.rpc;
  rpc.mine_blocks(1);
  rpc.mine_blocks_with_subsidy(1, LARGE);
  let txid = rpc.broadcast_tx(TransactionTemplate {
    inputs: &[(2, 0, 0)],
    script_sig: envelope(b"text/plain", &[b"2.dogemap"]),
    ..Default::default()
  });
  let block = rpc.mine_blocks(1).remove(0);

  let server = chain.serve();
  server.wait_for_checkpoint(3);
  let (status, text) = server
    .get(&{
      let capabilities = server.capabilities();
      format!(
        "/api/v1/dogemap-feed/blocks/3?blockHash={}&databaseId={}&reorgEpoch={}",
        block.header.block_hash(),
        capabilities["databaseId"].as_str().unwrap(),
        capabilities["reorgEpoch"].as_str().unwrap()
      )
    })
    .unwrap();
  assert_eq!(status, 200, "{text}");
  // Checked on the wire text, before any JSON number parsing could round it.
  assert!(
    text.contains("\"valueKoinu\":\"9007199254740993\""),
    "{text}"
  );
  let (status, locations) = server.json(&format!("/api/v1/dogemap-feed/locations?ids={txid}i0"));
  assert_eq!(status, 200);
  assert_eq!(
    locations["locations"][0]["location"]["valueKoinu"],
    "9007199254740993"
  );
}

#[test]
fn pages_and_cursors_are_bound_to_one_snapshot() {
  let chain = Chain::new();
  let rpc = &chain.rpc;
  rpc.mine_blocks(5);
  for input in 1..=5 {
    rpc.broadcast_tx(TransactionTemplate {
      inputs: &[(input, 0, 0)],
      script_sig: envelope(b"text/plain", &[format!("{input}.dogemap").as_bytes()]),
      ..Default::default()
    });
  }
  let block = rpc.mine_blocks(1).remove(0);
  rpc.mine_blocks(1);
  let hash = block.header.block_hash().to_string();

  let server = chain.serve();
  let capabilities = server.wait_for_checkpoint(7);
  let database_id = capabilities["databaseId"].as_str().unwrap().to_string();

  // Any page size yields the same events and hash.
  let (whole, all) = server.whole_block(6, &hash, 500);
  assert_eq!(all.len(), 5);
  for limit in [1, 2, 3] {
    let (page, events) = server.whole_block(6, &hash, limit);
    assert_eq!(page["eventsHash"], whole["eventsHash"]);
    assert_eq!(events, all);
  }
  assert_eq!(whole["eventsHash"], recompute_events_hash(&whole, &all));

  let (_, first) = server.block(6, &hash, "&limit=2");
  assert_eq!(first["complete"], false);
  assert_eq!(first["events"].as_array().unwrap().len(), 2);
  let cursor = first["nextCursor"].as_str().unwrap().to_string();

  let query = |height: u64, hash: &str, database_id: &str, epoch: &str, extra: &str| {
    server.json(&format!(
      "/api/v1/dogemap-feed/blocks/{height}?blockHash={hash}&databaseId={database_id}&reorgEpoch={epoch}{extra}"
    ))
  };

  // Stale or foreign identity: 409.
  let other_hash = chain.block_hash(5);
  let (status, value) = query(6, &other_hash, &database_id, "0", "");
  assert_error(status, &value, 409, "snapshot_replaced");
  let (status, value) = query(6, &hash, &"0".repeat(32), "0", "");
  assert_error(status, &value, 409, "snapshot_replaced");
  let (status, value) = query(6, &hash, &database_id, "1", "");
  assert_error(status, &value, 409, "snapshot_replaced");
  // A cursor for block 6 presented for block 5.
  let (status, value) = query(
    5,
    &other_hash,
    &database_id,
    "0",
    &format!("&cursor={cursor}"),
  );
  assert_error(status, &value, 409, "snapshot_replaced");

  // Malformed cursors and parameters: 400.
  let mut tampered = cursor.clone();
  tampered.pop();
  let (status, value) = query(6, &hash, &database_id, "0", &format!("&cursor={tampered}"));
  assert_error(status, &value, 400, "invalid_cursor");
  let (status, value) = query(6, &hash, &database_id, "0", "&cursor=%2B%2B");
  assert_error(status, &value, 400, "invalid_cursor");
  for extra in [
    "&limit=0",
    "&limit=501",
    "&limit=01",
    "&limit=-1",
    "&extra=1",
  ] {
    let (status, value) = query(6, &hash, &database_id, "0", extra);
    assert_error(status, &value, 400, "invalid_request");
  }
  let (status, value) = query(6, &hash.to_uppercase(), &database_id, "0", "");
  assert_error(status, &value, 400, "invalid_request");
  let (status, value) = query(6, &hash, &database_id, "00", "");
  assert_error(status, &value, 400, "invalid_request");
  let (status, value) = server.json(&format!("/api/v1/dogemap-feed/blocks/06?blockHash={hash}"));
  assert_error(status, &value, 400, "invalid_request");
  let (status, value) = server.json(&format!(
    "/api/v1/dogemap-feed/blocks/6?databaseId={database_id}&reorgEpoch=0"
  ));
  assert_error(status, &value, 400, "invalid_request");

  // Beyond the index: 503, never an empty block.
  let (status, value) = query(8, &hash, &database_id, "0", "");
  assert_error(status, &value, 503, "coverage_unavailable");
  let (status, value) = query(u64::from(u32::MAX), &hash, &database_id, "0", "");
  assert_error(status, &value, 503, "coverage_unavailable");
  let (status, value) = query(u64::from(u32::MAX) + 1, &hash, &database_id, "0", "");
  assert_error(status, &value, 400, "invalid_request");
}

#[test]
fn heights_below_creation_coverage_are_unavailable_not_empty() {
  // Headers below the first inscription height are fetched in batches of
  // 1000; 1003 of them cross a batch boundary.
  let mut chain = Chain::new();
  chain.args.push("--first-inscription-height=1003".into());
  chain.rpc.mine_blocks(1004);
  let server = chain.serve();
  let capabilities = server.wait_for_checkpoint(1004);
  assert_eq!(capabilities["creationCoverageFromHeight"], "1003");
  assert_eq!(capabilities["transferCoverageFromHeight"], "1003");

  let (status, value) = server.block(1002, &chain.block_hash(1002), "");
  assert_error(status, &value, 503, "coverage_unavailable");
  let (status, value) = server.block(1003, &chain.block_hash(1003), "");
  assert_eq!(status, 200, "{value}");
  // The batched headers were indexed under their own hashes.
  assert_eq!(value["parentHash"], chain.block_hash(1002));
  for height in [0, 999, 1000, 1001] {
    let (status, value) = server.block(height, &chain.block_hash(height), "");
    assert_error(status, &value, 503, "coverage_unavailable");
  }
  let (_, text) = server.get("/blockhash/999").unwrap();
  assert_eq!(text, chain.block_hash(999));
}

#[test]
fn locations_come_from_one_snapshot_and_are_bounded() {
  let chain = Chain::new();
  chain.rpc.mine_blocks(1);
  let server = chain.serve();
  server.wait_for_checkpoint(1);

  let unknown = format!("{}i0", "ab".repeat(32));
  let (status, value) = server.json(&format!("/api/v1/dogemap-feed/locations?ids={unknown}"));
  assert_eq!(status, 200, "{value}");
  assert_eq!(value["indexedCheckpoint"]["height"], "1");
  assert_eq!(value["locations"][0]["found"], false);

  let ids = vec![unknown.clone(); 101].join(",");
  let (status, value) = server.json(&format!("/api/v1/dogemap-feed/locations?ids={ids}"));
  assert_error(status, &value, 400, "invalid_request");
  let ids = vec![unknown.clone(); 100].join(",");
  let (status, value) = server.json(&format!("/api/v1/dogemap-feed/locations?ids={ids}"));
  assert_eq!(status, 200, "{value}");
  assert_eq!(value["locations"].as_array().unwrap().len(), 100);

  for bad in [
    "",
    "nope",
    &format!("{}i0", "AB".repeat(32)),
    &format!("{}i01", "ab".repeat(32)),
    &format!("{}i4294967296", "ab".repeat(32)),
  ] {
    let (status, value) = server.json(&format!("/api/v1/dogemap-feed/locations?ids={bad}"));
    assert_error(status, &value, 400, "invalid_request");
  }
  let (status, value) = server.json("/api/v1/dogemap-feed/locations");
  assert_error(status, &value, 400, "invalid_request");
}

#[test]
fn body_chunks_reassemble_the_intrinsic_body() {
  let chain = Chain::new();
  let rpc = &chain.rpc;
  rpc.mine_blocks(1);
  let pieces = [[0xa5u8; 200], [0x5au8; 200], [0x00u8; 200]];
  let txid = rpc.broadcast_tx(TransactionTemplate {
    inputs: &[(1, 0, 0)],
    script_sig: envelope(
      b"application/octet-stream",
      &[&pieces[0], &pieces[1], &pieces[2]],
    ),
    ..Default::default()
  });
  let block = rpc.mine_blocks(1).remove(0);
  let hash = block.header.block_hash().to_string();
  let expected = pieces.concat();

  let server = chain.serve();
  let capabilities = server.wait_for_checkpoint(2);
  let identity = format!(
    "databaseId={}&reorgEpoch={}",
    capabilities["databaseId"].as_str().unwrap(),
    capabilities["reorgEpoch"].as_str().unwrap()
  );
  let body = |query: &str| {
    server.json(&format!(
      "/api/v1/dogemap-feed/inscriptions/{txid}i0/body?blockHash={hash}&{identity}{query}"
    ))
  };

  // A 600-byte body is not a candidate: counted, not emitted.
  let (page, events) = server.whole_block(2, &hash, 10);
  assert_eq!(page["creationCount"], "1");
  assert!(events.is_empty());

  let mut assembled = Vec::new();
  let mut offset = Some("0".to_string());
  while let Some(at) = offset {
    let (status, chunk) = body(&format!("&offset={at}&length=256"));
    assert_eq!(status, 200, "{chunk}");
    assert_eq!(chunk["schemaVersion"], "dogemap-feed-body-v1");
    assert_identity(&chunk);
    assert_eq!(chunk["inscriptionId"], format!("{txid}i0"));
    assert_eq!(chunk["encoding"], "base64");
    assert_eq!(chunk["offset"], at);
    assert_eq!(chunk["byteLength"], "600");
    assert_eq!(chunk["sha256"], wire::sha256_hex(&expected));
    let bytes = base64::decode(chunk["bytes"].as_str().unwrap()).unwrap();
    assert_eq!(chunk["length"], bytes.len().to_string());
    assembled.extend(bytes);
    offset = chunk["nextOffset"].as_str().map(str::to_owned);
    assert_eq!(chunk["complete"], offset.is_none());
  }
  assert_eq!(assembled, expected);

  let (status, whole) = body("");
  assert_eq!(status, 200);
  assert_eq!(whole["length"], "600");
  assert_eq!(whole["complete"], true);
  assert_eq!(whole["nextOffset"], Value::Null);

  let (status, value) = body("&offset=600");
  assert_error(status, &value, 400, "invalid_request");
  let (status, value) = body("&length=65537");
  assert_error(status, &value, 400, "invalid_request");
  let (status, value) = body("&length=0");
  assert_error(status, &value, 400, "invalid_request");
  let (status, value) = server.json(&format!(
    "/api/v1/dogemap-feed/inscriptions/{}i0/body?blockHash={hash}&{identity}",
    "cd".repeat(32)
  ));
  assert_error(status, &value, 404, "unknown_inscription");
  let (status, value) = server.json(&format!(
    "/api/v1/dogemap-feed/inscriptions/{txid}i0/body?blockHash={}&{identity}",
    chain.block_hash(1)
  ));
  assert_error(status, &value, 409, "snapshot_replaced");
}

/// A database whose journal started later (an upgrade): earlier blocks keep
/// their creations, read positions from the node, and report transfers as
/// not journaled rather than empty.
#[test]
fn history_before_the_journal_start_is_not_journaled() {
  let chain = Chain::new();
  let rpc = &chain.rpc;
  rpc.mine_blocks(2);
  let candidate = rpc.broadcast_tx(TransactionTemplate {
    inputs: &[(1, 0, 0)],
    script_sig: envelope(b"text/plain", &[b"1.dogemap"]),
    ..Default::default()
  });
  let old_block = rpc.mine_blocks(1).remove(0);
  chain.index_once();

  // Make the database look like one written by a binary without the feed.
  {
    let database = Database::open(chain.index_path()).unwrap();
    let wtx = database.begin_write().unwrap();
    assert!(wtx.delete_table(DOGEMAP_FEED_META).unwrap());
    assert!(wtx.delete_table(DOGEMAP_FEED_JOURNAL).unwrap());
    wtx.commit().unwrap();
  }

  rpc.mine_blocks(1);
  let server = chain.serve();
  let capabilities = server.wait_for_checkpoint(4);
  assert_eq!(capabilities["transferCoverageFromHeight"], "4");
  assert_eq!(capabilities["ready"], true, "{capabilities}");

  let old_hash = old_block.header.block_hash().to_string();
  let (page, events) = server.whole_block(3, &old_hash, 10);
  assert_eq!(page["scope"]["transfers"], "not-journaled");
  assert_eq!(page["creationCount"], "1");
  assert_eq!(events.len(), 1);
  assert_eq!(events[0]["inscriptionId"], format!("{candidate}i0"));
  assert_eq!(events[0]["completion"]["txIndex"], "1");
  assert_eq!(events[0]["location"], Value::Null);
  assert_eq!(page["eventsHash"], recompute_events_hash(&page, &events));

  let (page, _) = server.whole_block(4, &chain.block_hash(4), 10);
  assert_eq!(page["scope"]["transfers"], "complete");
}

#[test]
fn a_reorg_reverts_the_journal_and_advances_the_epoch() {
  let chain = Chain::new();
  let rpc = &chain.rpc;
  rpc.mine_blocks(3);

  // Sync first so a savepoint below the fork exists.
  let server = chain.serve();
  let before = server.wait_for_checkpoint(3);
  let database_id = before["databaseId"].as_str().unwrap().to_string();

  let candidate = rpc.broadcast_tx(TransactionTemplate {
    inputs: &[(1, 0, 0)],
    script_sig: envelope(b"text/plain", &[b"4.dogemap"]),
    ..Default::default()
  });
  rpc.broadcast_tx(TransactionTemplate {
    inputs: &[(2, 0, 0)],
    script_sig: envelope(b"text/plain", &[b"5.dogemap"]),
    ..Default::default()
  });
  let forked = rpc.mine_blocks(1).remove(0);
  let forked_hash = forked.header.block_hash().to_string();
  server.wait_for_checkpoint(4);
  let (page, events) = server.whole_block(4, &forked_hash, 1);
  assert_eq!(events.len(), 2);
  let (_, first) = server.block(4, &forked_hash, "&limit=1");
  let cursor = first["nextCursor"].as_str().unwrap().to_string();
  let _ = page;

  assert_eq!(rpc.invalidate_tip(), forked.header.block_hash());
  rpc.mine_blocks(2);
  server.wait_until(|| server.json("/api/v1/dogemap-feed/capabilities").1["reorgEpoch"] == "1");
  let after = server.wait_for_checkpoint(5);
  assert_eq!(after["databaseId"], database_id.as_str());
  assert_eq!(after["reorgEpoch"], "1");
  assert_eq!(after["transferCoverageFromHeight"], "0");

  let query = |hash: &str, epoch: &str, extra: &str| {
    server.json(&format!(
      "/api/v1/dogemap-feed/blocks/4?blockHash={hash}&databaseId={database_id}&reorgEpoch={epoch}{extra}"
    ))
  };
  // Everything issued before the rollback is refused.
  let (status, value) = query(&forked_hash, "0", "");
  assert_error(status, &value, 409, "snapshot_replaced");
  let (status, value) = query(&forked_hash, "1", "");
  assert_error(status, &value, 409, "snapshot_replaced");
  let replacement = chain.block_hash(4);
  let (status, value) = query(&replacement, "1", &format!("&cursor={cursor}"));
  assert_error(status, &value, 409, "snapshot_replaced");

  // The replacement block has neither the creations nor their journal rows.
  let (page, events) = server.whole_block(4, &replacement, 10);
  assert_eq!(page["creationCount"], "0");
  assert_eq!(page["scope"]["transfers"], "complete");
  assert!(events.is_empty());
  let (_, locations) = server.json(&format!("/api/v1/dogemap-feed/locations?ids={candidate}i0"));
  assert_eq!(locations["locations"][0]["found"], false);
}
