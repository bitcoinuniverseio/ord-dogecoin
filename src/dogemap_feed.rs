//! Wire contract of the `dogemap-feed-v1` provider feed (Dogemap contract v1,
//! section 2): identity constants, the candidate prefilter, the eventsHash
//! projection and its RFC 8785 serialization, the opaque block cursor and the
//! strict request parameter grammar.
//!
//! Everything here is pure so the golden vector in
//! `docs/contract/eventsHash-golden-v1.json` and the integration tests can
//! exercise exactly the code the server runs. Storage lives in
//! `index/dogemap_feed.rs`, HTTP in `subcommand/server/dogemap_feed.rs`.

use {
  bitcoin::hashes::{sha256, Hash},
  serde_json::{Map, Value},
  std::fmt::Write as _,
};

pub const FEED_VERSION: &str = "dogemap-feed-v1";
pub const PARSER_PROFILE: &str = "doginals-trac-1.0.2-compat-v1";
pub const ORDER_PROFILE: &str = "ord-dogecoin-inscription-number-v1";
pub const CANDIDATE_FILTER: &str = "dogemap-candidate-prefilter-v1";
pub const DATABASE_SCHEMA: &str = "6";
pub const EVENTS_HASH_ALGORITHM: &str = "sha256-rfc8785-dogemap-feed-v1";
pub const BODY_RANGE_POLICY: &str = "base64-chunks-v1";

pub const CAPABILITIES_SCHEMA: &str = "dogemap-feed-capabilities-v1";
pub const BLOCK_SCHEMA: &str = "dogemap-feed-block-v1";
pub const BODY_SCHEMA: &str = "dogemap-feed-body-v1";
pub const LOCATIONS_SCHEMA: &str = "dogemap-feed-locations-v1";
pub const ERROR_SCHEMA: &str = "dogemap-feed-error-v1";

pub const MAX_PAGE_LIMIT: usize = 500;
pub const DEFAULT_PAGE_LIMIT: usize = 100;
pub const MAX_INLINE_BODY_BYTES: usize = 4096;
pub const MAX_BODY_CHUNK_BYTES: usize = 65536;
pub const MAX_LOCATION_IDS: usize = 100;

/// Largest body the candidate prefilter admits.
pub const PREFILTER_MAX_BODY_BYTES: usize = 64;

/// The feed is ready only while the node tip is at most this many blocks
/// ahead of the indexed checkpoint.
pub const MAX_READY_LAG_BLOCKS: u64 = 3;

/// Networks of Dogecoin Core v1.14.9 and their genesis block hashes. Signet
/// does not exist for Dogecoin and is rejected at startup.
pub const NETWORKS: [(&str, &str); 3] = [
  (
    "mainnet",
    "1a91e3dace36e2be3bf030a65679fe821aa1d6ef92e7c9902eb318182c355691",
  ),
  (
    "testnet",
    "bb0a78264637406b6360aad926284d544d7049f45189db5664f3c4d07350559e",
  ),
  (
    "regtest",
    "3d2160a3b5dc4a9d62e7e66a295f70313ac808440ef7400d6c0772171ce973a5",
  ),
];

pub fn genesis_hash(network: &str) -> Option<&'static str> {
  NETWORKS
    .iter()
    .find(|(name, _)| *name == network)
    .map(|(_, hash)| *hash)
}

/// `dogemap-candidate-prefilter-v1`: the intrinsic body is 1 to 64 bytes long
/// and contains the ASCII bytes `.dogemap`, compared case-insensitively.
/// Every body the `D.dogemap` grammar accepts has at most 10 + 8 bytes and
/// contains `.dogemap`, so the filter is a superset of the grammar.
pub fn candidate_prefilter(body: &[u8]) -> bool {
  (1..=PREFILTER_MAX_BODY_BYTES).contains(&body.len())
    && body
      .windows(b".dogemap".len())
      .any(|window| window.eq_ignore_ascii_case(b".dogemap"))
}

/// Lowercase hex SHA-256.
pub fn sha256_hex(bytes: &[u8]) -> String {
  hex::encode(sha256::Hash::hash(bytes).into_inner())
}

/// Why a value cannot be serialized by the RFC 8785 subset the feed uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rfc8785Error {
  /// Numbers never appear in hashed feed objects; every chain integer is a
  /// decimal string, so a number is a projection bug rather than data.
  Number,
}

impl std::fmt::Display for Rfc8785Error {
  fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
    match self {
      Self::Number => write!(f, "JSON numbers are outside the eventsHash projection"),
    }
  }
}

impl std::error::Error for Rfc8785Error {}

/// RFC 8785 serialization of a value made only of strings, nulls, booleans,
/// arrays and objects. For that subset the scheme reduces to: no insignificant
/// whitespace, object members sorted by the UTF-16 code units of their names,
/// and ECMAScript `JSON.stringify` string escaping (`\"`, `\\`, `\b`, `\f`,
/// `\n`, `\r`, `\t`, other controls below U+0020 as lowercase `\u00xx`,
/// everything else literal UTF-8).
pub fn rfc8785(value: &Value) -> Result<String, Rfc8785Error> {
  let mut out = String::new();
  write_value(&mut out, value)?;
  Ok(out)
}

fn write_value(out: &mut String, value: &Value) -> Result<(), Rfc8785Error> {
  match value {
    Value::Null => out.push_str("null"),
    Value::Bool(true) => out.push_str("true"),
    Value::Bool(false) => out.push_str("false"),
    Value::Number(_) => return Err(Rfc8785Error::Number),
    Value::String(string) => write_string(out, string),
    Value::Array(items) => {
      out.push('[');
      for (i, item) in items.iter().enumerate() {
        if i > 0 {
          out.push(',');
        }
        write_value(out, item)?;
      }
      out.push(']');
    }
    Value::Object(members) => {
      let mut keys = members.keys().collect::<Vec<&String>>();
      keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
      out.push('{');
      for (i, key) in keys.into_iter().enumerate() {
        if i > 0 {
          out.push(',');
        }
        write_string(out, key);
        out.push(':');
        write_value(out, &members[key])?;
      }
      out.push('}');
    }
  }
  Ok(())
}

fn write_string(out: &mut String, string: &str) {
  out.push('"');
  for c in string.chars() {
    match c {
      '"' => out.push_str("\\\""),
      '\\' => out.push_str("\\\\"),
      '\u{08}' => out.push_str("\\b"),
      '\u{0c}' => out.push_str("\\f"),
      '\n' => out.push_str("\\n"),
      '\r' => out.push_str("\\r"),
      '\t' => out.push_str("\\t"),
      c if u32::from(c) < 0x20 => {
        write!(out, "\\u{:04x}", u32::from(c)).expect("writing to a String cannot fail");
      }
      c => out.push(c),
    }
  }
  out.push('"');
}

/// The block-level fields the eventsHash binds besides the events.
pub struct EventsHashHeader<'a> {
  pub network: &'a str,
  pub genesis_hash: &'a str,
  pub height: u32,
  pub block_hash: &'a str,
  pub parent_hash: Option<&'a str>,
  /// `complete` or `not-journaled`.
  pub transfers_scope: &'a str,
  pub creation_count: u64,
}

/// An event as it enters the hash: `rawBody.bytes` and `rawBody.bodyRef` are
/// transport, so they are removed; every other field, nulls included, stays.
pub fn project_event(event: &Value) -> Value {
  let mut event = event.clone();
  if let Some(Value::Object(raw_body)) = event.get_mut("rawBody") {
    raw_body.remove("bytes");
    raw_body.remove("bodyRef");
  }
  event
}

/// The exact object whose RFC 8785 serialization is hashed into eventsHash.
pub fn events_hash_input(header: &EventsHashHeader, events: &[Value]) -> Value {
  let mut scope = Map::new();
  scope.insert("creations".into(), Value::from(CANDIDATE_FILTER));
  scope.insert("transfers".into(), Value::from(header.transfers_scope));

  let mut input = Map::new();
  input.insert("feedVersion".into(), Value::from(FEED_VERSION));
  input.insert("parserProfile".into(), Value::from(PARSER_PROFILE));
  input.insert("orderProfile".into(), Value::from(ORDER_PROFILE));
  input.insert("candidateFilter".into(), Value::from(CANDIDATE_FILTER));
  input.insert("network".into(), Value::from(header.network));
  input.insert("genesisHash".into(), Value::from(header.genesis_hash));
  input.insert("height".into(), Value::from(header.height.to_string()));
  input.insert("blockHash".into(), Value::from(header.block_hash));
  input.insert(
    "parentHash".into(),
    header.parent_hash.map(Value::from).unwrap_or(Value::Null),
  );
  input.insert("scope".into(), Value::Object(scope));
  input.insert(
    "creationCount".into(),
    Value::from(header.creation_count.to_string()),
  );
  input.insert(
    "events".into(),
    Value::Array(events.iter().map(project_event).collect()),
  );
  Value::Object(input)
}

/// `sha256-rfc8785-dogemap-feed-v1` over the complete ordered block.
pub fn events_hash(header: &EventsHashHeader, events: &[Value]) -> Result<String, Rfc8785Error> {
  Ok(sha256_hex(
    rfc8785(&events_hash_input(header, events))?.as_bytes(),
  ))
}

/// Position inside one block's event list, bound to everything that makes the
/// list valid: database generation, reorg epoch, height, block hash and the
/// block's eventsHash. Encoded as unpadded base64url of a fixed 97-byte layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockCursor {
  pub database_id: [u8; 16],
  pub reorg_epoch: u64,
  pub height: u32,
  pub block_hash: [u8; 32],
  pub events_hash: [u8; 32],
  pub next_ordinal: u32,
}

const CURSOR_VERSION: u8 = 1;
const CURSOR_BYTES: usize = 1 + 16 + 8 + 4 + 32 + 32 + 4;
const CURSOR_CHARS: usize = (CURSOR_BYTES * 4).div_ceil(3);

impl BlockCursor {
  pub fn encode(&self) -> String {
    let mut bytes = Vec::with_capacity(CURSOR_BYTES);
    bytes.push(CURSOR_VERSION);
    bytes.extend_from_slice(&self.database_id);
    bytes.extend_from_slice(&self.reorg_epoch.to_be_bytes());
    bytes.extend_from_slice(&self.height.to_be_bytes());
    bytes.extend_from_slice(&self.block_hash);
    bytes.extend_from_slice(&self.events_hash);
    bytes.extend_from_slice(&self.next_ordinal.to_be_bytes());
    base64::encode_config(bytes, base64::URL_SAFE_NO_PAD)
  }

  /// `None` for anything that is not a cursor this feed version issued.
  pub fn decode(cursor: &str) -> Option<Self> {
    if cursor.len() != CURSOR_CHARS
      || !cursor
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
      return None;
    }
    let bytes = base64::decode_config(cursor, base64::URL_SAFE_NO_PAD).ok()?;
    if bytes.len() != CURSOR_BYTES || bytes[0] != CURSOR_VERSION {
      return None;
    }
    // Reject non-zero padding bits so every cursor has one spelling.
    if base64::encode_config(&bytes, base64::URL_SAFE_NO_PAD) != cursor {
      return None;
    }
    let mut at = 1;
    let mut take = |n: usize| {
      let slice = &bytes[at..at + n];
      at += n;
      slice
    };
    Some(Self {
      database_id: take(16).try_into().ok()?,
      reorg_epoch: u64::from_be_bytes(take(8).try_into().ok()?),
      height: u32::from_be_bytes(take(4).try_into().ok()?),
      block_hash: take(32).try_into().ok()?,
      events_hash: take(32).try_into().ok()?,
      next_ordinal: u32::from_be_bytes(take(4).try_into().ok()?),
    })
  }
}

/// `^(0|[1-9][0-9]*)$` as a `u64`.
pub fn parse_decimal_u64(s: &str) -> Option<u64> {
  let valid = match s.as_bytes() {
    [b'0'] => true,
    [first, rest @ ..] => {
      (b'1'..=b'9').contains(first) && rest.iter().all(u8::is_ascii_digit) && s.len() <= 20
    }
    [] => false,
  };
  if valid {
    s.parse().ok()
  } else {
    None
  }
}

/// `^(0|[1-9][0-9]*)$` as a `u32`.
pub fn parse_decimal_u32(s: &str) -> Option<u32> {
  parse_decimal_u64(s).and_then(|n| u32::try_from(n).ok())
}

/// Exactly `len` lowercase hex digits.
pub fn is_lower_hex(s: &str, len: usize) -> bool {
  s.len() == len && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// `<64 lowercase hex>i<index>` with the index in `^(0|[1-9][0-9]*)$` and u32
/// range. Returns the txid hex and the index.
pub fn parse_inscription_id(s: &str) -> Option<(&str, u32)> {
  let (txid, index) = s.split_once('i')?;
  if !is_lower_hex(txid, 64) {
    return None;
  }
  Some((txid, parse_decimal_u32(index)?))
}

/// Typed feed errors and their HTTP status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedErrorCode {
  InvalidRequest,
  InvalidCursor,
  UnknownInscription,
  SnapshotReplaced,
  CoverageUnavailable,
  ProviderRecovering,
  ContentUnavailable,
  NodeUnavailable,
}

impl FeedErrorCode {
  pub fn as_str(self) -> &'static str {
    match self {
      Self::InvalidRequest => "invalid_request",
      Self::InvalidCursor => "invalid_cursor",
      Self::UnknownInscription => "unknown_inscription",
      Self::SnapshotReplaced => "snapshot_replaced",
      Self::CoverageUnavailable => "coverage_unavailable",
      Self::ProviderRecovering => "provider_recovering",
      Self::ContentUnavailable => "content_unavailable",
      Self::NodeUnavailable => "node_unavailable",
    }
  }

  pub fn http_status(self) -> u16 {
    match self {
      Self::InvalidRequest | Self::InvalidCursor => 400,
      Self::UnknownInscription => 404,
      Self::SnapshotReplaced => 409,
      Self::CoverageUnavailable
      | Self::ProviderRecovering
      | Self::ContentUnavailable
      | Self::NodeUnavailable => 503,
    }
  }
}

/// Read-only view of the provider's inscription parser for the parser
/// compatibility suite: the compat profile that indexes, and the Core-correct
/// push decoding used only by `ord dogemap-pushdata-audit`.
pub mod parser {
  use {
    crate::inscription::{Inscription, ParsedInscription},
    bitcoin::{Script, Transaction},
  };

  #[derive(Debug, Clone, PartialEq, Eq)]
  pub enum ParseOutcome {
    None,
    Partial,
    Complete {
      content_type: Option<Vec<u8>>,
      body: Option<Vec<u8>>,
      delegate: Option<Vec<u8>>,
    },
  }

  impl ParseOutcome {
    pub fn kind(&self) -> &'static str {
      match self {
        Self::None => "none",
        Self::Partial => "partial",
        Self::Complete { .. } => "complete",
      }
    }

    pub fn body(&self) -> Option<&[u8]> {
      match self {
        Self::Complete { body, .. } => body.as_deref(),
        _ => None,
      }
    }
  }

  impl From<ParsedInscription> for ParseOutcome {
    fn from(parsed: ParsedInscription) -> Self {
      match parsed {
        ParsedInscription::None => Self::None,
        ParsedInscription::Partial => Self::Partial,
        ParsedInscription::Complete(inscription) => Self::Complete {
          content_type: inscription.content_type,
          body: inscription.body,
          delegate: inscription.delegate,
        },
      }
    }
  }

  /// Exactly what the indexer runs on a stored reveal chain.
  pub fn compat_transactions(txs: &[Transaction]) -> ParseOutcome {
    Inscription::from_transactions(txs.to_vec()).into()
  }

  pub fn compat_script_sigs(scripts: &[Script]) -> ParseOutcome {
    Inscription::parse_script_sigs_compat(scripts).into()
  }

  pub fn strict_script_sigs(scripts: &[Script]) -> ParseOutcome {
    Inscription::parse_script_sigs_strict(scripts).into()
  }

  pub fn compat_pushes(script: &Script) -> Option<Vec<Vec<u8>>> {
    Inscription::decode_pushes_compat(script)
  }

  pub fn strict_pushes(script: &Script) -> Option<Vec<Vec<u8>>> {
    Inscription::decode_pushes_strict(script)
  }

  /// The first transaction's own outcome and the body bytes it contributed,
  /// which the feed uses to skip assembling multipart bodies that are
  /// already longer than the prefilter admits.
  pub fn first_part(tx: &Transaction) -> (ParseOutcome, usize) {
    let (parsed, prefix) = Inscription::parse_first_part(tx);
    (parsed.into(), prefix)
  }
}
