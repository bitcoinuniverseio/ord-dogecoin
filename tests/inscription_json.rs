//! End-to-end coverage of the JSON inscription detail.
//!
//! The explorer overlay reads `GET /inscription/:id` with
//! `Accept: application/json` exactly as it does against upstream `ord`.
//! Every test indexes real regtest blocks served by `test-bitcoincore-rpc`
//! through the production `ord` binary, so the content negotiation, the
//! field layout and the `/api/v1` alias are proved over HTTP.

use {
  bitcoin::{blockdata::script::Builder, hashes::Hash, Network, Script, Txid},
  executable_path::executable_path,
  reqwest::{blocking::Client, header},
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

fn inscribe(rpc: &Handle, input: (usize, usize, usize), body: &str) -> Txid {
  rpc.broadcast_tx(TransactionTemplate {
    inputs: &[input],
    script_sig: inscription_script("text/plain;charset=utf-8", body),
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
    }
  }

  fn serve(&self) -> Server {
    let port = TcpListener::bind("127.0.0.1:0")
      .unwrap()
      .local_addr()
      .unwrap()
      .port();
    let child = Command::new(executable_path("ord"))
      .arg("--chain=regtest")
      .arg("--rpc-url")
      .arg(self.rpc.url())
      .arg("--data-dir")
      .arg(self.tempdir.path())
      .arg("--cookie-file")
      .arg(&self.cookie)
      .arg("--index-transactions")
      .env("ORD_INTEGRATION_TEST", "1")
      .env("SUBSIDIES_PATH", repository_file("subsidies.json"))
      .env("STARTING_SATS_PATH", repository_file("starting_sats.json"))
      .stdout(Stdio::null())
      .stderr(Stdio::null())
      .arg("server")
      .arg("--address")
      .arg("127.0.0.1")
      .arg("--http-port")
      .arg(port.to_string())
      .spawn()
      .unwrap();
    let server = Server { child, port };
    server.wait_until(|| server.get("/status", None).is_some());
    server
  }
}

struct Server {
  child: Child,
  port: u16,
}

impl Server {
  fn get(&self, path: &str, accept: Option<&str>) -> Option<(u16, Option<String>, String)> {
    let mut request = Client::new().get(format!("http://127.0.0.1:{}{path}", self.port));
    if let Some(accept) = accept {
      request = request.header(header::ACCEPT, accept);
    }
    let response = request.send().ok()?;
    let status = response.status().as_u16();
    let content_type = response
      .headers()
      .get(header::CONTENT_TYPE)
      .and_then(|value| value.to_str().ok())
      .map(str::to_owned);
    Some((status, content_type, response.text().ok()?))
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
        .get("/block-count", None)
        .and_then(|(_, _, text)| text.parse::<u64>().ok())
        == Some(expected)
    });
  }

  /// A request the explorer overlay makes: `Accept: application/json`.
  fn json(&self, path: &str) -> (u16, Option<String>, Value) {
    let (status, content_type, text) = self.get(path, Some("application/json")).unwrap();
    let value = serde_json::from_str(&text).unwrap_or(Value::String(text));
    (status, content_type, value)
  }
}

impl Drop for Server {
  fn drop(&mut self) {
    self.child.kill().unwrap();
  }
}

const NOT_FOUND: &str = r#"{"error":"inscription not found"}"#;

/// Every upstream field is present with the type the overlay parses.
fn assert_detail_shape(detail: &Value) {
  let object = detail.as_object().unwrap();
  let mut keys = object.keys().cloned().collect::<Vec<_>>();
  keys.sort();
  assert_eq!(
    keys,
    [
      "address",
      "chain",
      "charms",
      "child_count",
      "content_length",
      "content_type",
      "fee",
      "genesis_transaction",
      "height",
      "id",
      "metaprotocol",
      "network",
      "next",
      "number",
      "output",
      "parents",
      "previous",
      "rune",
      "sat",
      "satpoint",
      "timestamp",
      "value",
    ]
  );
  assert!(detail["id"].is_string());
  assert!(detail["number"].is_u64());
  assert!(detail["address"].is_null() || detail["address"].is_string());
  assert!(detail["content_type"].is_null() || detail["content_type"].is_string());
  assert!(detail["content_length"].is_null() || detail["content_length"].is_u64());
  assert!(detail["height"].is_u64());
  assert!(detail["fee"].is_u64());
  assert!(detail["value"].is_u64());
  assert!(detail["sat"].is_null() || detail["sat"].is_u64());
  assert!(detail["satpoint"].is_string());
  assert!(detail["output"].is_string());
  assert!(detail["genesis_transaction"].is_string());
  assert!(detail["timestamp"].is_u64());
  assert!(detail["charms"].is_array());
  assert!(detail["parents"].is_array());
  assert!(detail["child_count"].is_u64());
  assert!(detail["rune"].is_null() || detail["rune"].is_string());
  assert!(detail["metaprotocol"].is_null() || detail["metaprotocol"].is_string());
  assert!(detail["previous"].is_null() || detail["previous"].is_string());
  assert!(detail["next"].is_null() || detail["next"].is_string());
}

#[test]
fn accept_json_answers_the_upstream_inscription_detail() {
  let chain = Chain::new();
  let rpc = &chain.rpc;
  rpc.mine_blocks(2);

  let first = inscribe(rpc, (1, 0, 0), "hello from the first doginal");
  let genesis_block = rpc.mine_blocks(1).remove(0);
  let second = inscribe(rpc, (2, 0, 0), "hello from the second doginal");
  rpc.mine_blocks(1);

  let server = chain.serve();
  server.wait_for_block_count(5);

  let first_id = format!("{first}i0");
  let second_id = format!("{second}i0");

  let (status, content_type, detail) = server.json(&format!("/inscription/{first_id}"));
  assert_eq!(status, 200, "{detail}");
  assert_eq!(content_type.as_deref(), Some("application/json"));
  assert_detail_shape(&detail);

  assert_eq!(detail["chain"], "dogecoin");
  assert_eq!(detail["network"], "regtest");
  assert_eq!(detail["id"], first_id);
  assert_eq!(detail["number"], 0);
  // The regtest template pays to an empty script, which is not an address.
  assert_eq!(detail["address"], Value::Null);
  assert_eq!(detail["content_type"], "text/plain;charset=utf-8");
  assert_eq!(
    detail["content_length"],
    "hello from the first doginal".len()
  );
  assert_eq!(detail["height"], 3);
  assert_eq!(detail["fee"], 0);
  assert_eq!(detail["value"], 50 * 100_000_000u64);
  // No --index-sats on this server.
  assert_eq!(detail["sat"], Value::Null);
  assert_eq!(detail["satpoint"], format!("{first}:0:0"));
  assert_eq!(detail["output"], format!("{first}:0"));
  assert_eq!(detail["genesis_transaction"], first.to_string());
  assert_eq!(detail["timestamp"], genesis_block.header.time);
  assert_eq!(detail["charms"], Value::Array(Vec::new()));
  assert_eq!(detail["parents"], Value::Array(Vec::new()));
  assert_eq!(detail["child_count"], 0);
  assert_eq!(detail["rune"], Value::Null);
  assert_eq!(detail["metaprotocol"], Value::Null);
  assert_eq!(detail["previous"], Value::Null);
  assert_eq!(detail["next"], second_id);

  // The legacy alias negotiates the same document.
  let (status, _, alias) = server.json(&format!("/shibescription/{first_id}"));
  assert_eq!(status, 200, "{alias}");
  assert_eq!(alias, detail);

  // The second inscription links back to the first.
  let (status, _, second_detail) = server.json(&format!("/inscription/{second_id}"));
  assert_eq!(status, 200, "{second_detail}");
  assert_eq!(second_detail["number"], 1);
  assert_eq!(second_detail["height"], 4);
  assert_eq!(second_detail["previous"], first_id);
  assert_eq!(second_detail["next"], Value::Null);
}

#[test]
fn browsers_still_receive_html_without_the_accept_header() {
  let chain = Chain::new();
  let rpc = &chain.rpc;
  rpc.mine_blocks(1);
  let txid = inscribe(rpc, (1, 0, 0), "hello from a browser-facing doginal");
  rpc.mine_blocks(1);

  let server = chain.serve();
  server.wait_for_block_count(3);

  for path in [
    format!("/inscription/{txid}i0"),
    format!("/shibescription/{txid}i0"),
  ] {
    let (status, content_type, body) = server.get(&path, None).unwrap();
    assert_eq!(status, 200, "{body}");
    assert_eq!(content_type.as_deref(), Some("text/html;charset=utf-8"));
    assert!(body.contains("<title>Shibescription 0</title>"), "{body}");
    assert!(serde_json::from_str::<Value>(&body).is_err());

    // A browser's Accept header does not name JSON either.
    let (status, content_type, _) = server
      .get(&path, Some("text/html,application/xhtml+xml,*/*;q=0.8"))
      .unwrap();
    assert_eq!(status, 200);
    assert_eq!(content_type.as_deref(), Some("text/html;charset=utf-8"));
  }
}

#[test]
fn unknown_inscriptions_are_a_json_not_found_under_accept_json() {
  let chain = Chain::new();
  chain.rpc.mine_blocks(1);
  let server = chain.serve();
  server.wait_for_block_count(2);

  let unknown = format!("{}i0", "ab".repeat(32));
  for path in [
    format!("/inscription/{unknown}"),
    format!("/shibescription/{unknown}"),
    format!("/api/v1/inscriptions/{unknown}"),
    "/inscription/not-an-inscription-id".to_string(),
    "/api/v1/inscriptions/not-an-inscription-id".to_string(),
  ] {
    let (status, content_type, body) = server.get(&path, Some("application/json")).unwrap();
    assert_eq!(status, 404, "{path}: {body}");
    assert_eq!(content_type.as_deref(), Some("application/json"), "{path}");
    assert_eq!(body, NOT_FOUND, "{path}");
  }

  // Without the header the HTML route keeps its plain-text not-found.
  let (status, _, body) = server
    .get(&format!("/inscription/{unknown}"), None)
    .unwrap();
  assert_eq!(status, 404);
  assert_eq!(body, format!("inscription {unknown} not found"));
}

#[test]
fn the_api_v1_alias_equals_the_negotiated_body() {
  let chain = Chain::new();
  let rpc = &chain.rpc;
  rpc.mine_blocks(1);
  let txid = inscribe(rpc, (1, 0, 0), "hello from the api alias doginal");
  rpc.mine_blocks(1);

  let server = chain.serve();
  server.wait_for_block_count(3);

  let id = format!("{txid}i0");
  let (status, _, negotiated) = server.json(&format!("/inscription/{id}"));
  assert_eq!(status, 200, "{negotiated}");
  assert_detail_shape(&negotiated);

  // The alias answers JSON with or without the Accept header.
  for accept in [None, Some("application/json"), Some("*/*")] {
    let (status, content_type, body) = server
      .get(&format!("/api/v1/inscriptions/{id}"), accept)
      .unwrap();
    assert_eq!(status, 200, "{body}");
    assert_eq!(content_type.as_deref(), Some("application/json"));
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap(), negotiated);
  }
}

/// The explorer's outpoint enrichment reads `GET /output/:outpoint` with
/// `Accept: application/json` as it does against upstream `ord`: the
/// inscriptions on the output, its spent state, value, script and address.
#[test]
fn accept_json_answers_the_upstream_output_detail() {
  let chain = Chain::new();
  let rpc = &chain.rpc;
  rpc.mine_blocks(2);

  let holder = Script::new_p2pkh(&bitcoin::PubkeyHash::from_slice(&[0x33; 20]).unwrap());
  let inscribed = rpc.broadcast_tx(TransactionTemplate {
    inputs: &[(1, 0, 0)],
    script_sig: inscription_script("text/plain;charset=utf-8", "hello from an output"),
    output_script: holder.clone(),
    ..Default::default()
  });
  rpc.mine_blocks(1);
  // Block 4 spends the inscribed output onwards; block 5 has an untouched one.
  let spend = rpc.broadcast_tx(TransactionTemplate {
    inputs: &[(3, 1, 0)],
    output_script: holder.clone(),
    ..Default::default()
  });
  rpc.mine_blocks(1);

  let server = chain.serve();
  server.wait_for_block_count(5);

  let inscription_id = format!("{inscribed}i0");
  let expected_address = bitcoin::Address::from_script(&holder, Network::Regtest)
    .unwrap()
    .to_string();

  // The spent genesis output: no inscription is on it any more.
  let (status, content_type, detail) = server.json(&format!("/output/{inscribed}:0"));
  assert_eq!(status, 200, "{detail}");
  assert_eq!(content_type.as_deref(), Some("application/json"));
  assert_eq!(detail["outpoint"], format!("{inscribed}:0"));
  assert_eq!(detail["spent"], true);
  assert_eq!(detail["indexed"], true);
  assert_eq!(detail["transaction"], inscribed.to_string());
  assert_eq!(detail["address"], expected_address);
  assert_eq!(detail["inscriptions"], Value::Array(Vec::new()));
  assert_eq!(detail["runes"], serde_json::json!({}));
  assert_eq!(detail["sat_ranges"], Value::Null);
  assert!(detail["value"].is_u64(), "{detail}");
  assert!(
    detail["script_pubkey"]
      .as_str()
      .unwrap()
      .starts_with("OP_DUP OP_HASH160"),
    "{detail}"
  );
  assert_eq!(detail["chain"], "dogecoin");
  assert_eq!(detail["network"], "regtest");

  // The current output carries the inscription and is unspent.
  let (status, _, current) = server.json(&format!("/output/{spend}:0"));
  assert_eq!(status, 200, "{current}");
  assert_eq!(current["spent"], false);
  assert_eq!(current["inscriptions"], serde_json::json!([inscription_id]));
  assert_eq!(current["address"], expected_address);

  // The /api/v1 alias equals the negotiated body.
  let (status, _, alias) = server.json(&format!("/api/v1/outputs/{spend}:0"));
  assert_eq!(status, 200);
  assert_eq!(alias, current);

  // Unknown or malformed outpoints are a JSON 404, never HTML.
  for path in [
    format!("/output/{}:0", "f".repeat(64)),
    "/output/not-an-outpoint".to_string(),
    format!("/api/v1/outputs/{}:7", "f".repeat(64)),
  ] {
    let (status, content_type, body) = server.json(&path);
    assert_eq!(status, 404, "{path}: {body}");
    assert_eq!(content_type.as_deref(), Some("application/json"));
    assert_eq!(body, serde_json::json!({ "error": "output not found" }));
  }

  // Browsers keep the HTML page.
  let (status, content_type, body) = server.get(&format!("/output/{spend}:0"), None).unwrap();
  assert_eq!(status, 200, "{body}");
  assert_eq!(content_type.as_deref(), Some("text/html;charset=utf-8"));
  assert!(body.contains("<title>Output"), "{body}");
}

/// The explorer's outpoint enrichment reads `/status` and `/blockhash` the
/// way upstream `ord` answers them: the index availability flags and the
/// indexed height as JSON under `Accept: application/json`, the tip hash as
/// bare text. The plain-text `/status` answer for probes is unchanged.
#[test]
fn status_and_blockhash_answer_the_upstream_layout() {
  let chain = Chain::new();
  let rpc = &chain.rpc;
  let blocks = rpc.mine_blocks(3);
  let tip = blocks.last().unwrap().header.block_hash().to_string();

  let server = chain.serve();
  server.wait_for_block_count(4);

  let (status, content_type, body) = server.json("/status");
  assert_eq!(status, 200, "{body}");
  assert_eq!(content_type.as_deref(), Some("application/json"));
  assert_eq!(body["chain"], "dogecoin");
  assert_eq!(body["network"], "regtest");
  assert_eq!(body["height"], 3);
  assert_eq!(body["inscription_index"], true);
  assert_eq!(body["address_index"], true);
  assert_eq!(body["rune_index"], false);
  assert_eq!(body["sat_index"], false);
  assert_eq!(body["transaction_index"], true);
  // This harness starts ord without --index-drc20, so the flag is reported off.
  assert_eq!(body["drc20_index"], false);
  assert_eq!(body["unrecoverably_reorged"], false);

  let (status, _, text) = server.get("/status", None).unwrap();
  assert_eq!(status, 200);
  assert_eq!(text, "OK");

  let (status, _, hash) = server.get("/blockhash", None).unwrap();
  assert_eq!(status, 200);
  assert_eq!(hash, tip);
  let (status, _, genesis) = server.get("/blockhash/0", None).unwrap();
  assert_eq!(status, 200);
  assert_eq!(genesis.len(), 64);
  let (status, _, _) = server.get("/blockhash/99", None).unwrap();
  assert_eq!(status, 404);
}

/// The batch output route answers JSON with the JSON content type. The
/// explorer's client refuses a JSON body labelled text/plain, which is what
/// this route used to send, so every Dogecoin holding was out of coverage.
#[test]
fn the_batch_output_route_is_labelled_json() {
  let chain = Chain::new();
  let rpc = &chain.rpc;
  rpc.mine_blocks(1);
  let txid = inscribe(rpc, (1, 0, 0), "hello from a batched output");
  rpc.mine_blocks(1);

  let server = chain.serve();
  server.wait_for_block_count(3);

  let (status, content_type, body) = server.get(&format!("/outputs/{txid}:0"), None).unwrap();
  assert_eq!(status, 200, "{body}");
  assert_eq!(content_type.as_deref(), Some("application/json"));
  let outputs: Value = serde_json::from_str(&body).unwrap();
  assert_eq!(outputs[0]["transaction"], txid.to_string());
  assert_eq!(
    outputs[0]["inscriptions"],
    serde_json::json!([format!("{txid}i0")])
  );

  let (status, content_type, _) = server.get("/blocks/0/2", None).unwrap();
  assert_eq!(status, 200);
  assert_eq!(content_type.as_deref(), Some("application/json"));
}
