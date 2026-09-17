//! End-to-end coverage of the retained DRC-20 operation decisions.
//!
//! Every test indexes real regtest blocks served by `test-bitcoincore-rpc`
//! through the production `ord` binary and reads the verdicts back over
//! `/api/v1/drc20/operations`, so the ledger, the decision table and the
//! HTTP contract are proved together rather than by serialization alone.

use {
  bitcoin::{blockdata::script::Builder, hashes::Hash, Network, Script, Txid},
  executable_path::executable_path,
  redb::{Database, TableDefinition},
  serde_json::Value,
  std::{
    fs,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
  },
  tempfile::TempDir,
  test_bitcoincore_rpc::{Handle, TransactionTemplate},
};

/// Key in `STATISTIC_TO_COUNT`, from `Statistic` declaration order.
const STATISTIC_DRC20_DECISIONS_FROM_HEIGHT: u64 = 12;

const STATISTIC_TO_COUNT: TableDefinition<u64, u64> = TableDefinition::new("STATISTIC_TO_COUNT");
const DRC20_OPERATION_DECISIONS: TableDefinition<&[u8; 68], &[u8]> =
  TableDefinition::new("DRC20_OPERATION_DECISIONS");

/// A single-piece Dogecoin inscription in the script_sig layout the fork's
/// parser reads: `ord`, piece count, content type, piece index, body.
fn inscription_script(content_type: &str, body: &str) -> Script {
  Builder::new()
    .push_slice(b"ord")
    .push_int(1)
    .push_slice(content_type.as_bytes())
    .push_int(0)
    .push_slice(body.as_bytes())
    .into_script()
}

fn drc20(body: &str) -> Script {
  assert!(body.len() >= 40, "the parser ignores bodies under 40 bytes");
  inscription_script("text/plain;charset=utf-8", body)
}

fn inscribe(rpc: &Handle, input: (usize, usize, usize), body: &str) -> Txid {
  rpc.broadcast_tx(TransactionTemplate {
    inputs: &[input],
    script_sig: drc20(body),
    ..Default::default()
  })
}

/// The binary reads the Dogecoin subsidy schedule from these files at
/// startup, exactly as the production units point it at them.
fn repository_file(name: &str) -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(name)
}

struct Chain {
  rpc: Handle,
  tempdir: TempDir,
  cookie: PathBuf,
  /// Select the chain with the `--regtest` shorthand, as the deployment units
  /// do, instead of `--chain=regtest`.
  shorthand_flag: bool,
}

impl Chain {
  fn new() -> Self {
    let rpc = test_bitcoincore_rpc::builder()
      .network(Network::Regtest)
      .build();
    let tempdir = TempDir::new().unwrap();
    let cookie = tempdir.path().join("cookie");
    fs::write(&cookie, "user:password").unwrap();
    Self {
      rpc,
      tempdir,
      cookie,
      shorthand_flag: false,
    }
  }

  fn ord(&self) -> Command {
    let mut command = Command::new(executable_path("ord"));
    command
      .arg(if self.shorthand_flag {
        "--regtest"
      } else {
        "--chain=regtest"
      })
      .arg("--rpc-url")
      .arg(self.rpc.url())
      .arg("--data-dir")
      .arg(self.tempdir.path())
      .arg("--cookie-file")
      .arg(&self.cookie)
      .arg("--index-drc20")
      .arg("--index-transactions")
      .env("ORD_INTEGRATION_TEST", "1")
      .env("SUBSIDIES_PATH", repository_file("subsidies.json"))
      .env("STARTING_SATS_PATH", repository_file("starting_sats.json"))
      .stdout(Stdio::null())
      .stderr(Stdio::null());
    command
  }

  fn index_once(&self) {
    let status = self.ord().arg("index").status().unwrap();
    assert!(status.success(), "ord index failed");
  }

  fn index_path(&self) -> PathBuf {
    self.tempdir.path().join("regtest").join("index.redb")
  }

  fn serve(&self) -> Server {
    let port = TcpListener::bind("127.0.0.1:0")
      .unwrap()
      .local_addr()
      .unwrap()
      .port();
    let child = self
      .ord()
      .arg("server")
      .arg("--address")
      .arg("127.0.0.1")
      .arg("--http-port")
      .arg(port.to_string())
      .spawn()
      .unwrap();
    let server = Server { child, port };
    server.wait_until(|| server.get("/status").is_some());
    server
  }
}

struct Server {
  child: Child,
  port: u16,
}

impl Server {
  fn get(&self, path: &str) -> Option<(u16, String)> {
    let response = reqwest::blocking::get(format!("http://127.0.0.1:{}{path}", self.port)).ok()?;
    let status = response.status().as_u16();
    Some((status, response.text().ok()?))
  }

  fn wait_until(&self, condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while !condition() {
      assert!(
        Instant::now() < deadline,
        "server did not reach the expected state"
      );
      thread::sleep(Duration::from_millis(100));
    }
  }

  /// The index thread polls every five seconds, so a state change on the
  /// node takes up to that long to become visible.
  fn wait_for_block_count(&self, expected: u64) {
    self.wait_until(|| {
      self
        .get("/block-count")
        .and_then(|(_, text)| text.parse::<u64>().ok())
        == Some(expected)
    });
  }

  fn json(&self, path: &str) -> (u16, Value) {
    let (status, text) = self.get(path).unwrap();
    let value = serde_json::from_str(&text).unwrap_or(Value::String(text));
    (status, value)
  }

  fn decision(&self, txid: Txid) -> Value {
    let (status, value) = self.json(&format!("/api/v1/drc20/operations/{txid}i0"));
    assert_eq!(status, 200, "{value}");
    value
  }
}

impl Drop for Server {
  fn drop(&mut self) {
    self.child.kill().unwrap();
  }
}

fn block_hash(block: &bitcoin::Block) -> String {
  block.header.block_hash().to_string()
}

fn assert_record_shape(record: &Value) {
  let object = record.as_object().unwrap();
  let mut keys = object.keys().cloned().collect::<Vec<_>>();
  keys.sort();
  assert_eq!(
    keys,
    [
      "amount",
      "checkpoint",
      "coverage",
      "index",
      "inscriptionId",
      "operation",
      "reason",
      "reorgEpoch",
      "ruleset",
      "tick",
      "txid",
      "verdict",
    ]
  );
  assert_eq!(record["ruleset"], "drc20-v1");
}

#[test]
fn ledger_verdicts_are_retained_per_operation_with_their_block() {
  let chain = Chain::new();
  let rpc = &chain.rpc;
  rpc.mine_blocks(5);

  // Block 6: a valid deployment.
  let deploy = inscribe(
    rpc,
    (1, 0, 0),
    r#"{"p":"drc-20","op":"deploy","tick":"abcd","max":"1000","lim":"10","dec":"0"}"#,
  );
  let deploy_block = rpc.mine_blocks(1).remove(0);

  // Block 7: a mint over the limit, a mint of an undeployed ticker, a valid
  // mint, and a duplicate deployment, each in its own transaction.
  let over_limit = inscribe(
    rpc,
    (2, 0, 0),
    r#"{"p":"drc-20","op":"mint","tick":"abcd","amt":"11","note":"over-limit"}"#,
  );
  let undeployed = inscribe(
    rpc,
    (3, 0, 0),
    r#"{"p":"drc-20","op":"mint","tick":"zzzz","amt":"1","note":"undeployed"}"#,
  );
  let mint = inscribe(
    rpc,
    (4, 0, 0),
    r#"{"p":"drc-20","op":"mint","tick":"abcd","amt":"10","note":"valid-mint"}"#,
  );
  let duplicate = inscribe(
    rpc,
    (5, 0, 0),
    r#"{"p":"drc-20","op":"deploy","tick":"abcd","max":"5000","lim":"50","dec":"0"}"#,
  );
  let block_7 = rpc.mine_blocks(1).remove(0);

  // Block 8: an inscribe-transfer beyond the minted balance and one within it.
  let insufficient = inscribe(
    rpc,
    (6, 0, 0),
    r#"{"p":"drc-20","op":"transfer","tick":"abcd","amt":"100","note":"too-much"}"#,
  );
  let transferable = inscribe(
    rpc,
    (7, 0, 0),
    r#"{"p":"drc-20","op":"transfer","tick":"abcd","amt":"4","note":"within-balance"}"#,
  );
  let block_8 = rpc.mine_blocks(1).remove(0);

  // Block 9: a plain inscription that is not a DRC-20 operation at all.
  let plain = rpc.broadcast_tx(TransactionTemplate {
    inputs: &[(8, 0, 0)],
    script_sig: inscription_script("text/plain;charset=utf-8", "hello, not a token operation"),
    ..Default::default()
  });
  rpc.mine_blocks(1);

  let server = chain.serve();
  server.wait_for_block_count(10);

  let record = server.decision(deploy);
  assert_record_shape(&record);
  assert_eq!(record["verdict"], "accepted");
  assert_eq!(record["operation"], "deploy");
  assert_eq!(record["tick"], "abcd");
  assert_eq!(record["amount"], Value::Null);
  assert_eq!(record["reason"], Value::Null);
  assert_eq!(record["index"], 0);
  assert_eq!(record["txid"], deploy.to_string());
  assert_eq!(record["inscriptionId"], format!("{deploy}i0"));
  assert_eq!(record["checkpoint"]["height"], 6);
  assert_eq!(record["checkpoint"]["blockHash"], block_hash(&deploy_block));
  assert_eq!(record["reorgEpoch"], 0);
  assert_eq!(record["coverage"]["decisionsFromHeight"], 0);
  assert_eq!(record["coverage"]["indexedHeight"], 9);

  let record = server.decision(over_limit);
  assert_eq!(record["verdict"], "rejected");
  assert_eq!(record["operation"], "mint");
  assert_eq!(record["reason"], "amount exceed limit: 11");
  assert_eq!(record["amount"], Value::Null);
  assert_eq!(record["checkpoint"]["blockHash"], block_hash(&block_7));

  let record = server.decision(undeployed);
  assert_eq!(record["verdict"], "rejected");
  assert_eq!(record["tick"], "zzzz");
  assert_eq!(record["reason"], "tick: zzzz not found");

  let record = server.decision(mint);
  assert_eq!(record["verdict"], "accepted");
  assert_eq!(record["operation"], "mint");
  assert_eq!(record["amount"], "10");

  let record = server.decision(duplicate);
  assert_eq!(record["verdict"], "rejected");
  assert_eq!(record["operation"], "deploy");
  assert_eq!(record["reason"], "tick: abcd has been existed");

  let record = server.decision(insufficient);
  assert_eq!(record["verdict"], "rejected");
  assert_eq!(record["operation"], "inscribe-transfer");
  assert_eq!(record["reason"], "insufficient balance: 10 100");
  assert_eq!(record["checkpoint"]["blockHash"], block_hash(&block_8));

  let record = server.decision(transferable);
  assert_eq!(record["verdict"], "accepted");
  assert_eq!(record["operation"], "inscribe-transfer");
  assert_eq!(record["amount"], "4");

  // Indexed, inside coverage, but carrying no DRC-20 operation: the index
  // evaluated the block and found nothing to decide. Not a rejection.
  let record = server.decision(plain);
  assert_record_shape(&record);
  assert_eq!(record["verdict"], "not-evaluated");
  assert_eq!(record["reason"], "not-a-drc20-operation");
  assert_eq!(record["operation"], Value::Null);
  assert_eq!(record["checkpoint"]["height"], 9);

  // An inscription the index has never seen is a 404, never a verdict.
  let unknown = Txid::all_zeros();
  let (status, _) = server.json(&format!("/api/v1/drc20/operations/{unknown}i0"));
  assert_eq!(status, 404);

  // The ledger agrees with the verdicts: 10 minted, and exactly the accepted
  // inscribe-transfer is outstanding.
  let (status, token) = server.json("/api/v1/drc20/tokens/abcd");
  assert_eq!(status, 200);
  assert_eq!(token["token"]["minted_atomic"], "10");
  let (status, inventory) = server.json("/api/v1/drc20/transferables");
  assert_eq!(status, 200);
  let transferables = inventory["transferables"].as_array().unwrap();
  assert_eq!(transferables.len(), 1);
  assert_eq!(
    transferables[0]["transfer_inscription_id"],
    format!("{transferable}i0")
  );
  assert_eq!(transferables[0]["amount_atomic"], "4");
}

#[test]
fn every_operation_in_one_transaction_is_a_separate_decision() {
  let chain = Chain::new();
  let rpc = &chain.rpc;
  rpc.mine_blocks(4);

  inscribe(
    rpc,
    (1, 0, 0),
    r#"{"p":"drc-20","op":"deploy","tick":"abcd","max":"1000","lim":"100","dec":"0"}"#,
  );
  rpc.mine_blocks(1);
  inscribe(
    rpc,
    (2, 0, 0),
    r#"{"p":"drc-20","op":"mint","tick":"abcd","amt":"100","note":"valid-mint"}"#,
  );
  rpc.mine_blocks(1);
  let first = inscribe(
    rpc,
    (3, 0, 0),
    r#"{"p":"drc-20","op":"transfer","tick":"abcd","amt":"30","note":"first-reserve"}"#,
  );
  let second = inscribe(
    rpc,
    (4, 0, 0),
    r#"{"p":"drc-20","op":"transfer","tick":"abcd","amt":"50","note":"second-reserve"}"#,
  );
  rpc.mine_blocks(1);

  // Block 8: one transaction spends both inscribe-transfer inscriptions, so
  // it carries two transfer operations on two different inscriptions.
  let spend = rpc.broadcast_tx(TransactionTemplate {
    inputs: &[(7, 1, 0), (7, 2, 0)],
    ..Default::default()
  });
  let spend_block = rpc.mine_blocks(1).remove(0);

  let server = chain.serve();
  server.wait_for_block_count(9);

  let (status, listing) = server.json(&format!("/api/v1/drc20/operations?txid={spend}"));
  assert_eq!(status, 200, "{listing}");
  assert_eq!(listing["txid"], spend.to_string());
  assert_eq!(listing["coverage"]["decisionsFromHeight"], 0);
  assert_eq!(listing["coverage"]["indexedHeight"], 8);
  let decisions = listing["decisions"].as_array().unwrap();
  assert_eq!(decisions.len(), 2);
  let mut seen = decisions
    .iter()
    .map(|record| {
      assert_record_shape(record);
      assert_eq!(record["txid"], spend.to_string());
      assert_eq!(record["operation"], "transfer");
      assert_eq!(record["verdict"], "accepted");
      assert_eq!(record["checkpoint"]["blockHash"], block_hash(&spend_block));
      (
        record["inscriptionId"].as_str().unwrap().to_string(),
        record["amount"].as_str().unwrap().to_string(),
      )
    })
    .collect::<Vec<_>>();
  seen.sort();
  let mut expected = vec![
    (format!("{first}i0"), "30".to_string()),
    (format!("{second}i0"), "50".to_string()),
  ];
  expected.sort();
  assert_eq!(seen, expected);

  // The inscribe-transfer verdicts stay addressable by inscription id and are
  // not overwritten by the later transfer of the same inscription.
  let record = server.decision(first);
  assert_eq!(record["operation"], "inscribe-transfer");
  assert_eq!(record["txid"], first.to_string());
  let (_, listing) = server.json(&format!("/api/v1/drc20/operations?txid={first}"));
  assert_eq!(listing["decisions"].as_array().unwrap().len(), 1);

  // A transaction with no DRC-20 operation lists nothing but still names
  // the coverage the empty answer was read under.
  let coinbase = spend_block.txdata[0].txid();
  let (status, listing) = server.json(&format!("/api/v1/drc20/operations?txid={coinbase}"));
  assert_eq!(status, 200);
  assert_eq!(listing["decisions"].as_array().unwrap().len(), 0);
  assert_eq!(listing["coverage"]["indexedHeight"], 8);

  let (status, _) = server.json("/api/v1/drc20/operations");
  assert_eq!(status, 400);
  let (status, _) = server.json("/api/v1/drc20/operations?txid=nope");
  assert_eq!(status, 400);
}

/// A database indexed by a binary without the decision table gains the
/// feature without a rebuild: it starts recording at the next block and
/// reports everything before that as not evaluated.
#[test]
fn history_before_decision_coverage_is_not_evaluated_rather_than_inferred() {
  let chain = Chain::new();
  let rpc = &chain.rpc;
  rpc.mine_blocks(2);
  let deploy = inscribe(
    rpc,
    (1, 0, 0),
    r#"{"p":"drc-20","op":"deploy","tick":"abcd","max":"1000","lim":"10","dec":"0"}"#,
  );
  let deploy_block = rpc.mine_blocks(1).remove(0);
  chain.index_once();

  // Make the database look like one written before this feature existed:
  // no decision table and no coverage start. The ledger rows stay.
  {
    let database = Database::open(chain.index_path()).unwrap();
    let wtx = database.begin_write().unwrap();
    assert!(wtx.delete_table(DRC20_OPERATION_DECISIONS).unwrap());
    assert!(wtx
      .open_table(STATISTIC_TO_COUNT)
      .unwrap()
      .remove(&STATISTIC_DRC20_DECISIONS_FROM_HEIGHT)
      .unwrap()
      .is_some());
    wtx.commit().unwrap();
  }

  let mint = inscribe(
    rpc,
    (2, 0, 0),
    r#"{"p":"drc-20","op":"mint","tick":"abcd","amt":"10","note":"valid-mint"}"#,
  );
  let mint_block = rpc.mine_blocks(1).remove(0);

  let server = chain.serve();
  server.wait_for_block_count(5);

  let (status, capabilities) = server.json("/api/v1/capabilities");
  assert_eq!(status, 200);
  assert_eq!(capabilities["drc20"], true);
  assert_eq!(capabilities["drc20Decisions"], true);
  assert_eq!(capabilities["drc20DecisionsFromHeight"], 4);

  let record = server.decision(deploy);
  assert_record_shape(&record);
  assert_eq!(record["verdict"], "not-evaluated");
  assert_eq!(record["reason"], "outside-decision-coverage");
  assert_eq!(record["operation"], Value::Null);
  assert_eq!(record["tick"], Value::Null);
  assert_eq!(record["checkpoint"]["height"], 3);
  assert_eq!(record["checkpoint"]["blockHash"], block_hash(&deploy_block));
  assert_eq!(record["coverage"]["decisionsFromHeight"], 4);
  assert_eq!(record["coverage"]["indexedHeight"], 4);

  // The deployment was applied to the ledger by the older run, so the mint
  // in the first covered block is evaluated against it and retained.
  let record = server.decision(mint);
  assert_eq!(record["verdict"], "accepted");
  assert_eq!(record["checkpoint"]["height"], 4);
  assert_eq!(record["checkpoint"]["blockHash"], block_hash(&mint_block));
}

#[test]
fn reorg_rolls_decisions_back_with_the_ledger_and_reindexing_restores_them() {
  let chain = Chain::new();
  let rpc = &chain.rpc;
  rpc.mine_blocks(2);
  let deploy = inscribe(
    rpc,
    (1, 0, 0),
    r#"{"p":"drc-20","op":"deploy","tick":"abcd","max":"1000","lim":"10","dec":"0"}"#,
  );
  let deploy_block = rpc.mine_blocks(1).remove(0);

  // Sync the deployment first, so the index holds a savepoint below the
  // fork point that the rollback can restore.
  let server = chain.serve();
  server.wait_for_block_count(4);

  let mint_template = |body: &'static str| TransactionTemplate {
    inputs: &[(2, 0, 0)],
    script_sig: drc20(body),
    ..Default::default()
  };
  let mint = rpc.broadcast_tx(mint_template(
    r#"{"p":"drc-20","op":"mint","tick":"abcd","amt":"10","note":"valid-mint"}"#,
  ));
  let mint_block = rpc.mine_blocks(1).remove(0);
  server.wait_for_block_count(5);
  let before = server.decision(mint);
  assert_eq!(before["verdict"], "accepted");
  assert_eq!(before["checkpoint"]["height"], 4);
  assert_eq!(before["checkpoint"]["blockHash"], block_hash(&mint_block));
  assert_eq!(before["reorgEpoch"], 0);
  let (_, token) = server.json("/api/v1/drc20/tokens/abcd");
  assert_eq!(token["token"]["minted_atomic"], "10");

  // Replace the mint's block with two empty ones. The mint is gone from the
  // chain, so its verdict and its ledger effect must both disappear.
  let orphaned = rpc.invalidate_tip();
  assert_eq!(orphaned, mint_block.header.block_hash());
  rpc.mine_blocks(2);
  server.wait_for_block_count(6);
  server.wait_until(|| {
    server
      .get(&format!("/api/v1/drc20/operations/{mint}i0"))
      .map(|(status, _)| status)
      == Some(404)
  });
  let (_, token) = server.json("/api/v1/drc20/tokens/abcd");
  assert_eq!(token["token"]["minted_atomic"], "0");

  // The deployment sits below the fork point and survived inside the
  // restored savepoint: same block, same verdict, and still the epoch it was
  // decided under, because it was not re-evaluated.
  let record = server.decision(deploy);
  assert_eq!(record["verdict"], "accepted");
  assert_eq!(record["checkpoint"]["height"], 3);
  assert_eq!(record["checkpoint"]["blockHash"], block_hash(&deploy_block));
  assert_eq!(record["reorgEpoch"], 0);
  let (_, capabilities) = server.json("/api/v1/capabilities");
  assert_eq!(capabilities["drc20DecisionsFromHeight"], 0);

  // The same transaction confirms again in a new block and is decided again
  // against the restored ledger.
  let again = rpc.broadcast_tx(mint_template(
    r#"{"p":"drc-20","op":"mint","tick":"abcd","amt":"10","note":"valid-mint"}"#,
  ));
  assert_eq!(again, mint);
  let new_block = rpc.mine_blocks(1).remove(0);
  server.wait_for_block_count(7);
  let after = server.decision(mint);
  assert_eq!(after["verdict"], "accepted");
  assert_eq!(after["amount"], "10");
  assert_eq!(after["checkpoint"]["height"], 6);
  assert_eq!(after["checkpoint"]["blockHash"], block_hash(&new_block));
  assert_eq!(after["reorgEpoch"], 1);
  let (_, token) = server.json("/api/v1/drc20/tokens/abcd");
  assert_eq!(token["token"]["minted_atomic"], "10");

  // A consumer that cached the pre-reorg record can tell it apart from the
  // re-derived one by its epoch alone.
  assert_ne!(before["reorgEpoch"], after["reorgEpoch"]);
}

#[test]
fn capabilities_advertise_retained_decisions_and_their_coverage_start() {
  let chain = Chain::new();
  let server = chain.serve();
  // Before the first block is indexed there is no coverage to report.
  let (status, capabilities) = server.json("/api/v1/capabilities");
  if status == 200 {
    assert_eq!(capabilities["drc20Decisions"], true);
  }

  chain.rpc.mine_blocks(2);
  server.wait_for_block_count(3);
  let (status, capabilities) = server.json("/api/v1/capabilities");
  assert_eq!(status, 200);
  assert_eq!(capabilities["chain"], "dogecoin");
  assert_eq!(capabilities["network"], "regtest");
  assert_eq!(capabilities["drc20"], true);
  assert_eq!(capabilities["transactions"], true);
  assert_eq!(capabilities["drc20Decisions"], true);
  assert_eq!(capabilities["drc20DecisionsFromHeight"], 0);
}

#[test]
fn a_database_without_the_drc20_index_reports_decisions_as_disabled() {
  let chain = Chain::new();
  let rpc = &chain.rpc;
  rpc.mine_blocks(1);
  let deploy = inscribe(
    rpc,
    (1, 0, 0),
    r#"{"p":"drc-20","op":"deploy","tick":"abcd","max":"1000","lim":"10","dec":"0"}"#,
  );
  let deploy_block = rpc.mine_blocks(1).remove(0);

  let port = TcpListener::bind("127.0.0.1:0")
    .unwrap()
    .local_addr()
    .unwrap()
    .port();
  let child = Command::new(executable_path("ord"))
    .arg("--chain=regtest")
    .arg("--rpc-url")
    .arg(rpc.url())
    .arg("--data-dir")
    .arg(chain.tempdir.path())
    .arg("--cookie-file")
    .arg(&chain.cookie)
    .arg("--index-transactions")
    .arg("server")
    .arg("--address")
    .arg("127.0.0.1")
    .arg("--http-port")
    .arg(port.to_string())
    .env("ORD_INTEGRATION_TEST", "1")
    .env("SUBSIDIES_PATH", repository_file("subsidies.json"))
    .env("STARTING_SATS_PATH", repository_file("starting_sats.json"))
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .spawn()
    .unwrap();
  let server = Server { child, port };
  server.wait_until(|| server.get("/status").is_some());
  server.wait_for_block_count(3);

  let (_, capabilities) = server.json("/api/v1/capabilities");
  assert_eq!(capabilities["drc20"], false);
  assert_eq!(capabilities["drc20Decisions"], false);
  assert_eq!(capabilities["drc20DecisionsFromHeight"], Value::Null);

  let record = server.decision(deploy);
  assert_record_shape(&record);
  assert_eq!(record["verdict"], "not-evaluated");
  assert_eq!(record["reason"], "drc20-index-disabled");
  assert_eq!(record["checkpoint"]["height"], 2);
  assert_eq!(record["checkpoint"]["blockHash"], block_hash(&deploy_block));
  assert_eq!(record["coverage"]["decisionsFromHeight"], Value::Null);

  let (status, _) = server.json(&format!("/api/v1/drc20/operations?txid={deploy}"));
  assert_eq!(status, 400);
}

/// The ledger keys holders by address string. On regtest and testnet that
/// string must carry the prefix of the indexed chain (m/n), not the mainnet D
/// prefix: the explorer validates every address against the network of the
/// request and would otherwise reject the deployer and every holder.
#[test]
fn holder_addresses_carry_the_prefix_of_the_indexed_chain() {
  let mut chain = Chain::new();
  chain.shorthand_flag = true;
  let rpc = &chain.rpc;
  rpc.mine_blocks(3);

  let holder = Script::new_p2pkh(&bitcoin::PubkeyHash::from_slice(&[0x11; 20]).unwrap());
  let expected = bitcoin::Address::from_script(&holder, Network::Regtest)
    .unwrap()
    .to_string();
  assert!(
    expected.starts_with('m') || expected.starts_with('n'),
    "{expected}"
  );

  let deploy = rpc.broadcast_tx(TransactionTemplate {
    inputs: &[(1, 0, 0)],
    script_sig: drc20(
      r#"{"p":"drc-20","op":"deploy","tick":"abcd","max":"1000","lim":"10","dec":"0"}"#,
    ),
    output_script: holder.clone(),
    ..Default::default()
  });
  rpc.mine_blocks(1);
  let mint = rpc.broadcast_tx(TransactionTemplate {
    inputs: &[(2, 0, 0)],
    script_sig: drc20(r#"{"p":"drc-20","op":"mint","tick":"abcd","amt":"10","note":"valid-mint"}"#),
    output_script: holder,
    ..Default::default()
  });
  rpc.mine_blocks(1);

  let server = chain.serve();
  server.wait_for_block_count(6);
  assert_eq!(server.decision(deploy)["verdict"], "accepted");
  assert_eq!(server.decision(mint)["verdict"], "accepted");

  let (status, token) = server.json("/api/v1/drc20/tokens/abcd");
  assert_eq!(status, 200, "{token}");
  assert_eq!(token["token"]["deployed_by"], expected, "{token}");

  let (status, holders) = server.json("/api/v1/drc20/tokens/abcd/holders");
  assert_eq!(status, 200, "{holders}");
  let holders = holders["holders"].as_array().unwrap();
  assert_eq!(holders.len(), 1, "{holders:?}");
  assert_eq!(holders[0]["address"], expected);
  assert_eq!(holders[0]["overall_atomic"], "10");

  // The same address, queried with its own prefix, answers the balance.
  let (status, balance) = server.json(&format!("/drc20/balance/{expected}"));
  assert_eq!(status, 200, "{balance}");
}

/// A transfer inscription spent to another address moves the balance on
/// regtest exactly as it does on mainnet: the owner recorded at inscribe time
/// is matched at spend time even though the stored owner round-trips through
/// an address string whose version byte the parser tags as testnet.
#[test]
fn a_transfer_spent_to_another_address_moves_the_balance_on_regtest() {
  let mut chain = Chain::new();
  chain.shorthand_flag = true;
  let rpc = &chain.rpc;
  rpc.mine_blocks(3);

  let holder = Script::new_p2pkh(&bitcoin::PubkeyHash::from_slice(&[0x11; 20]).unwrap());
  let other = Script::new_p2pkh(&bitcoin::PubkeyHash::from_slice(&[0x22; 20]).unwrap());
  let holder_address = bitcoin::Address::from_script(&holder, Network::Regtest)
    .unwrap()
    .to_string();
  let other_address = bitcoin::Address::from_script(&other, Network::Regtest)
    .unwrap()
    .to_string();

  rpc.broadcast_tx(TransactionTemplate {
    inputs: &[(1, 0, 0)],
    script_sig: drc20(
      r#"{"p":"drc-20","op":"deploy","tick":"abcd","max":"1000","lim":"10","dec":"0"}"#,
    ),
    output_script: holder.clone(),
    ..Default::default()
  });
  rpc.mine_blocks(1);
  rpc.broadcast_tx(TransactionTemplate {
    inputs: &[(2, 0, 0)],
    script_sig: drc20(r#"{"p":"drc-20","op":"mint","tick":"abcd","amt":"10","note":"valid-mint"}"#),
    output_script: holder.clone(),
    ..Default::default()
  });
  rpc.mine_blocks(1);
  // Block 6: inscribe a transfer of 4 to the holder's own output.
  let inscribe_transfer = rpc.broadcast_tx(TransactionTemplate {
    inputs: &[(3, 0, 0)],
    script_sig: drc20(
      r#"{"p":"drc-20","op":"transfer","tick":"abcd","amt":"4","note":"to-spend"}"#,
    ),
    output_script: holder,
    ..Default::default()
  });
  rpc.mine_blocks(1);
  // Block 7: spend the transfer inscription's output to the other address.
  let spend = rpc.broadcast_tx(TransactionTemplate {
    inputs: &[(6, 1, 0)],
    output_script: other,
    ..Default::default()
  });
  rpc.mine_blocks(1);

  let server = chain.serve();
  server.wait_for_block_count(8);
  assert_eq!(server.decision(inscribe_transfer)["verdict"], "accepted");
  let (status, decisions) = server.json(&format!("/api/v1/drc20/operations?txid={spend}"));
  assert_eq!(status, 200, "{decisions}");
  let decision = &decisions["decisions"][0];
  assert_eq!(decision["operation"], "transfer", "{decisions}");
  assert_eq!(decision["verdict"], "accepted", "{decisions}");

  let (_, holders) = server.json("/api/v1/drc20/tokens/abcd/holders");
  let holders = holders["holders"].as_array().unwrap();
  let balance = |address: &str| {
    holders
      .iter()
      .find(|h| h["address"] == address)
      .map(|h| h["overall_atomic"].clone())
  };
  assert_eq!(
    balance(&holder_address),
    Some(Value::from("6")),
    "{holders:?}"
  );
  assert_eq!(
    balance(&other_address),
    Some(Value::from("4")),
    "{holders:?}"
  );
}
