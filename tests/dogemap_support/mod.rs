//! Shared harness for the Dogemap feed and parser suites: the production
//! `ord` binary indexes regtest blocks served by `test-bitcoincore-rpc`, and
//! the tests read the feed over HTTP, as in tests/drc20_decisions.rs.

#![allow(dead_code)]

use {
  bitcoin::{blockdata::script::Builder, Script},
  executable_path::executable_path,
  serde_json::Value,
  std::{
    collections::BTreeMap,
    fs,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
  },
  tempfile::TempDir,
  test_bitcoincore_rpc::Handle,
};

/// Coinbase value the test node mines by default (50 DOGE).
pub const DEFAULT_SUBSIDY: u64 = 50 * 100_000_000;

/// The genesis coinbase of the regtest chain pays 88 DOGE.
pub const GENESIS_SUBSIDY: u64 = 8_800_000_000;

/// A single- or multi-piece Dogecoin inscription envelope in the scriptSig
/// layout the parser reads: `ord`, piece count, content type, then
/// (countdown, data) pairs.
pub fn envelope(content_type: &[u8], pieces: &[&[u8]]) -> Script {
  let mut builder = Builder::new()
    .push_slice(b"ord")
    .push_int(pieces.len() as i64)
    .push_slice(content_type);
  for (i, piece) in pieces.iter().enumerate() {
    builder = builder
      .push_int((pieces.len() - i - 1) as i64)
      .push_slice(piece);
  }
  builder.into_script()
}

/// The first part of a multipart envelope: header plus the first pieces of a
/// `total`-piece countdown.
pub fn envelope_head(content_type: &[u8], total: usize, pieces: &[&[u8]]) -> Script {
  let mut builder = Builder::new()
    .push_slice(b"ord")
    .push_int(total as i64)
    .push_slice(content_type);
  for (i, piece) in pieces.iter().enumerate() {
    builder = builder.push_int((total - i - 1) as i64).push_slice(piece);
  }
  builder.into_script()
}

/// A continuation part carrying pieces `next, next-1, ...`.
pub fn envelope_tail(next: usize, pieces: &[&[u8]]) -> Script {
  let mut builder = Builder::new();
  for (i, piece) in pieces.iter().enumerate() {
    builder = builder.push_int((next - i) as i64).push_slice(piece);
  }
  builder.into_script()
}

pub fn repository_file(name: &str) -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(name)
}

pub struct Chain {
  pub rpc: Handle,
  pub tempdir: TempDir,
  cookie: PathBuf,
  subsidies: PathBuf,
  pub args: Vec<String>,
}

impl Chain {
  /// A regtest chain whose subsidy schedule matches what the test node
  /// mines: 88 DOGE at genesis, `DEFAULT_SUBSIDY` afterwards, except the
  /// heights in `overrides`.
  pub fn with_subsidies(overrides: &[(u32, u64)]) -> Self {
    let rpc = test_bitcoincore_rpc::builder()
      .network(bitcoin::Network::Regtest)
      .build();
    let tempdir = TempDir::new().unwrap();
    let cookie = tempdir.path().join("cookie");
    fs::write(&cookie, "user:password").unwrap();

    let mut epochs = BTreeMap::new();
    for height in 0..500u32 {
      epochs.insert(
        height.to_string(),
        if height == 0 {
          GENESIS_SUBSIDY
        } else {
          DEFAULT_SUBSIDY
        },
      );
    }
    for (height, subsidy) in overrides {
      epochs.insert(height.to_string(), *subsidy);
    }
    let subsidies = tempdir.path().join("subsidies.json");
    fs::write(
      &subsidies,
      serde_json::to_string(&serde_json::json!({ "epochs": epochs })).unwrap(),
    )
    .unwrap();

    Self {
      rpc,
      tempdir,
      cookie,
      subsidies,
      args: Vec::new(),
    }
  }

  pub fn new() -> Self {
    Self::with_subsidies(&[])
  }

  pub fn ord(&self) -> Command {
    let mut command = Command::new(executable_path("ord"));
    command
      .arg("--chain=regtest")
      .arg("--rpc-url")
      .arg(self.rpc.url())
      .arg("--data-dir")
      .arg(self.tempdir.path())
      .arg("--cookie-file")
      .arg(&self.cookie)
      .args(&self.args)
      .env("ORD_INTEGRATION_TEST", "1")
      .env("SUBSIDIES_PATH", &self.subsidies)
      .env("STARTING_SATS_PATH", repository_file("starting_sats.json"))
      .stdout(Stdio::null())
      .stderr(Stdio::null());
    // Set DOGEMAP_TEST_LOG=<file> to append the binary's info log there.
    if let Some(path) = std::env::var_os("DOGEMAP_TEST_LOG") {
      let log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap();
      command.env("RUST_LOG", "info").stderr(log);
    }
    command
  }

  pub fn index_path(&self) -> PathBuf {
    self.tempdir.path().join("regtest").join("index.redb")
  }

  pub fn index_once(&self) {
    let status = self.ord().arg("index").status().unwrap();
    assert!(status.success(), "ord index failed");
  }

  pub fn serve(&self) -> Server {
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

  /// The active-chain hash at `height`, from the node.
  pub fn block_hash(&self, height: u64) -> String {
    let request = serde_json::json!({
      "jsonrpc": "2.0",
      "id": 0,
      "method": "getblockhash",
      "params": [height],
    });
    let response = reqwest::blocking::Client::new()
      .post(self.rpc.url())
      .header("content-type", "application/json")
      .body(request.to_string())
      .send()
      .unwrap()
      .text()
      .unwrap();
    let response: Value = serde_json::from_str(&response).unwrap();
    response["result"].as_str().unwrap().to_string()
  }
}

pub struct Server {
  child: Child,
  pub port: u16,
}

impl Server {
  pub fn get(&self, path: &str) -> Option<(u16, String)> {
    let response = reqwest::blocking::get(format!("http://127.0.0.1:{}{path}", self.port)).ok()?;
    let status = response.status().as_u16();
    Some((status, response.text().ok()?))
  }

  pub fn wait_until(&self, condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(90);
    while !condition() {
      assert!(
        Instant::now() < deadline,
        "server did not reach the expected state"
      );
      thread::sleep(Duration::from_millis(100));
    }
  }

  pub fn wait_for_block_count(&self, expected: u64) {
    self.wait_until(|| {
      self
        .get("/block-count")
        .and_then(|(_, text)| text.parse::<u64>().ok())
        == Some(expected)
    });
  }

  pub fn json(&self, path: &str) -> (u16, Value) {
    let (status, text) = self.get(path).unwrap();
    let value = serde_json::from_str(&text).unwrap_or(Value::String(text));
    (status, value)
  }

  pub fn capabilities(&self) -> Value {
    let (status, value) = self.json("/api/v1/dogemap-feed/capabilities");
    assert_eq!(status, 200, "{value}");
    value
  }

  /// Wait until the feed reports `height` as its checkpoint.
  pub fn wait_for_checkpoint(&self, height: u64) -> Value {
    self.wait_until(|| {
      self.json("/api/v1/dogemap-feed/capabilities").1["indexedCheckpoint"]["height"]
        .as_str()
        .and_then(|value| value.parse::<u64>().ok())
        == Some(height)
    });
    self.capabilities()
  }

  /// `/blocks/{height}` for the current identity; `extra` is appended to the
  /// query string.
  pub fn block(&self, height: u64, block_hash: &str, extra: &str) -> (u16, Value) {
    let capabilities = self.capabilities();
    self.json(&format!(
      "/api/v1/dogemap-feed/blocks/{height}?blockHash={block_hash}&databaseId={}&reorgEpoch={}{extra}",
      capabilities["databaseId"].as_str().unwrap(),
      capabilities["reorgEpoch"].as_str().unwrap(),
    ))
  }

  /// Every page of a block, checking page invariants on the way.
  pub fn whole_block(&self, height: u64, block_hash: &str, limit: usize) -> (Value, Vec<Value>) {
    let (status, first) = self.block(height, block_hash, &format!("&limit={limit}"));
    assert_eq!(status, 200, "{first}");
    let mut events = first["events"].as_array().unwrap().clone();
    let mut page = first.clone();
    while !page["complete"].as_bool().unwrap() {
      let cursor = page["nextCursor"].as_str().unwrap().to_string();
      let (status, next) = self.block(
        height,
        block_hash,
        &format!("&limit={limit}&cursor={cursor}"),
      );
      assert_eq!(status, 200, "{next}");
      assert_eq!(next["eventsHash"], first["eventsHash"]);
      assert_eq!(next["totalEvents"], first["totalEvents"]);
      events.extend(next["events"].as_array().unwrap().iter().cloned());
      page = next;
    }
    assert_eq!(page["nextCursor"], Value::Null);
    assert_eq!(
      events.len().to_string(),
      first["totalEvents"].as_str().unwrap()
    );
    for (ordinal, event) in events.iter().enumerate() {
      assert_eq!(event["eventOrdinal"], ordinal.to_string());
    }
    (first, events)
  }
}

impl Drop for Server {
  fn drop(&mut self) {
    self.child.kill().unwrap();
    self.child.wait().unwrap();
  }
}

/// The error document every refusal carries.
pub fn assert_error(status: u16, value: &Value, expected_status: u16, code: &str) {
  assert_eq!(status, expected_status, "{value}");
  assert_eq!(value["schemaVersion"], "dogemap-feed-error-v1", "{value}");
  assert_eq!(value["error"]["code"], code, "{value}");
  assert!(value["error"]["message"].is_string(), "{value}");
  let correlation = value["correlationId"].as_str().unwrap();
  assert_eq!(correlation.len(), 32, "{value}");
}
