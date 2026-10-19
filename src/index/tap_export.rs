//! TAP KV tables and the tap-writer-ordtap-v1 export layer.
//! Ported from Trac-Systems/ord-tap b8f6ea3 src/index.rs (CC0); see docs/TAP-DOGE-ENGINE-PLAN.md.

use super::*;
use crate::index::updater::inscription_updater::{tap_js_json_stringify_value, tap_js_preprocess_json_for_serde};
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, BufWriter, Write};

// Generic bytes->bytes key/value store for TAP protocol state
pub(crate) const TAP_KV: TableDefinition<&[u8], &[u8]> = TableDefinition::new("TAP_KV");
// Read-only fallback for export deltas written before sidecar delta files.
pub(crate) const TAP_EXPORT_DELTAS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("TAP_EXPORT_DELTAS");
// Non-consensus TAP export coverage metadata.
pub(crate) const TAP_EXPORT_METADATA: TableDefinition<&[u8], &[u8]> = TableDefinition::new("TAP_EXPORT_METADATA");
// Non-consensus rolling TAP export state digests by block.
pub(crate) const TAP_EXPORT_BLOCK_STATES: TableDefinition<&[u8], &[u8]> = TableDefinition::new("TAP_EXPORT_BLOCK_STATES");

const TAP_EXPORT_ENABLED_FROM_HEIGHT: &[u8] = b"export_enabled_from_height";
pub(crate) const TAP_EXPORT_COVERAGE_TIP: &[u8] = b"export_coverage_tip";
const TAP_EXPORT_ROLLING_ENABLED_FROM_HEIGHT: &[u8] = b"rolling_enabled_from_height";
pub(crate) const TAP_EXPORT_ROLLING_STATE_TIP: &[u8] = b"rolling_state_tip";
pub(crate) const TAP_EXPORT_ROLLING_STATE_ROW_COUNT: &[u8] = b"rolling_state_row_count";
pub(crate) const TAP_EXPORT_ROLLING_STATE_DIGEST: &[u8] = b"rolling_state_digest";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TapExportSnapshotRow {
  pub key: String,
  pub value: String,
  pub value_kind: String,
  pub source_encoding: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TapExportSnapshot {
  pub source_height: u32,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub source_digest: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub source_row_count: Option<u64>,
  pub rows: Vec<TapExportSnapshotRow>,
  pub next_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TapExportDeltaRecord {
  pub height: u32,
  pub sequence: u64,
  pub op: String,
  pub key: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub value: Option<String>,
  pub row_hash: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub block_hash: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub parent_block_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TapExportDeltaPage {
  pub rows: Vec<TapExportDeltaRecord>,
  pub next: Option<(u32, u64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TapExportSnapshotOpen {
  pub snapshot_id: String,
  pub source_height: u32,
  pub source_block_hash: Option<String>,
  pub row_count: u64,
  pub state_digest: String,
  pub limit_rows_max: usize,
  pub limit_bytes_max: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TapExportSnapshotRead {
  pub snapshot_id: String,
  pub source_height: u32,
  pub rows: Vec<TapExportSnapshotRow>,
  pub next_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TapExportRetentionStatus {
  pub watermark: u32,
  pub earliest_retained_block: Option<u32>,
  pub latest_retained_block: Option<u32>,
  pub latest_sequence: Option<u64>,
  pub delta_rows: u64,
  pub delta_bytes: u64,
  pub export_enabled_from_height: Option<u32>,
  pub export_coverage_tip: Option<u32>,
  pub rolling_enabled_from_height: Option<u32>,
  pub rolling_state_tip: Option<u32>,
  pub rolling_state_row_count: Option<u64>,
  pub rolling_state_digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TapExportStateDigest {
  pub source_height: u32,
  pub row_count: u64,
  pub state_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TapExportBlockDigest {
  pub height: u32,
  pub block_hash: Option<String>,
  pub parent_block_hash: Option<String>,
  pub delta_rows: u64,
  pub delta_digest: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub rolling_state_row_count: Option<u64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub rolling_state_digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct TapExportRollingState {
  pub row_count: u64,
  pub state_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct TapExportBlockState {
  pub height: u32,
  pub rolling_state_row_count: u64,
  pub rolling_state_digest: String,
}

#[derive(Debug, Clone, Default)]
struct TapExportValueDetails {
  value: String,
  value_kind: String,
  source_encoding: String,
}

fn tap_export_string_value_kind(value: &str) -> String {
  if value.is_empty() {
    "empty-marker".to_string()
  } else if is_number_string(value) {
    "number-string".to_string()
  } else {
    "string".to_string()
  }
}

fn tap_export_raw_value_kind(value: &str) -> String {
  if value.is_empty() {
    return "empty-marker".to_string();
  }
  if is_number_string(value) {
    return "number-string".to_string();
  }
  if let Ok(json) = serde_json::from_str::<serde_json::Value>(value) {
    return tap_export_json_value_kind(&json);
  }
  tap_export_string_value_kind(value)
}

fn tap_export_json_value_kind(value: &serde_json::Value) -> String {
  match value {
    serde_json::Value::Object(_) => "json-object".to_string(),
    serde_json::Value::Array(_) => "json-array".to_string(),
    serde_json::Value::Number(_) => "json-number".to_string(),
    serde_json::Value::String(value) => tap_export_string_value_kind(value),
    serde_json::Value::Bool(_) => "json-bool".to_string(),
    serde_json::Value::Null => "json-null".to_string(),
  }
}

fn is_number_string(value: &str) -> bool {
  let value = value.strip_prefix('-').unwrap_or(value);
  !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}

impl Index {
  pub(crate) fn tap_export_delta_dir(&self) -> PathBuf {
    self.path.with_extension("tap-export-deltas")
  }

  fn tap_export_delta_file_path(&self, height: u32) -> PathBuf {
    updater::inscription_updater::TapDeltaBatch::delta_file_path(
      &self.tap_export_delta_dir(),
      height,
    )
  }

  // --- TAP KV helpers for server ---
  pub(crate) fn tap_decode_string_bytes(bytes: &[u8]) -> Option<String> {
    ciborium::de::from_reader::<String, _>(std::io::Cursor::new(bytes))
      .ok()
      .or_else(|| {
        let raw = std::str::from_utf8(bytes).ok()?;
        serde_json::from_str::<String>(&tap_js_preprocess_json_for_serde(raw))
          .ok()
          .or_else(|| Some(raw.to_string()))
      })
  }

  pub(crate) fn tap_export_value_string(bytes: &[u8]) -> Option<String> {
    Self::tap_export_value_details(bytes).map(|details| details.value)
  }

  fn tap_export_value_details(bytes: &[u8]) -> Option<TapExportValueDetails> {
    if let Ok(value) = ciborium::de::from_reader::<String, _>(std::io::Cursor::new(bytes)) {
      return Some(TapExportValueDetails {
        value_kind: tap_export_string_value_kind(&value),
        value,
        source_encoding: "cbor".to_string(),
      });
    }

    if let Ok(value) =
      ciborium::de::from_reader::<serde_json::Value, _>(std::io::Cursor::new(bytes))
    {
      return Some(TapExportValueDetails {
        value: tap_js_json_stringify_value(&value),
        value_kind: tap_export_json_value_kind(&value),
        source_encoding: "cbor".to_string(),
      });
    }

    let raw = std::str::from_utf8(bytes).ok()?;
    let value = serde_json::from_str::<String>(&tap_js_preprocess_json_for_serde(raw))
      .ok()
      .unwrap_or_else(|| raw.to_string());
    Some(TapExportValueDetails {
      value_kind: tap_export_raw_value_kind(&value),
      value,
      source_encoding: "utf8".to_string(),
    })
  }

  fn tap_export_digest_pair(hasher: &mut Sha256, key: &[u8], value: &[u8]) {
    hasher.update((key.len() as u64).to_be_bytes());
    hasher.update(key);
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
  }

  fn tap_export_digest_pair_bytes(key: &[u8], value: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    Self::tap_export_digest_pair(&mut hasher, key, value);
    hasher.finalize().into()
  }

  fn tap_export_digest_hex(hasher: Sha256) -> String {
    format!("{:x}", hasher.finalize())
  }

  fn tap_export_rolling_zero_digest() -> String {
    "0".repeat(64)
  }

  fn tap_export_rolling_hex_to_bytes(value: &str) -> Result<[u8; 32]> {
    ensure!(
      value.len() == 64,
      "invalid TAP export rolling digest length: {}",
      value.len()
    );
    let mut out = [0u8; 32];
    hex::decode_to_slice(value, &mut out)?;
    Ok(out)
  }

  fn tap_export_rolling_add(mut state: [u8; 32], value: [u8; 32]) -> [u8; 32] {
    let mut carry = 0u16;
    for i in (0..32).rev() {
      let sum = state[i] as u16 + value[i] as u16 + carry;
      state[i] = (sum & 0xff) as u8;
      carry = sum >> 8;
    }
    state
  }

  fn tap_export_rolling_sub(mut state: [u8; 32], value: [u8; 32]) -> [u8; 32] {
    let mut borrow = 0i16;
    for i in (0..32).rev() {
      let diff = state[i] as i16 - value[i] as i16 - borrow;
      if diff < 0 {
        state[i] = (diff + 256) as u8;
        borrow = 1;
      } else {
        state[i] = diff as u8;
        borrow = 0;
      }
    }
    state
  }

  pub(crate) fn tap_export_rolling_state_apply(
    state: &mut TapExportRollingState,
    key: &str,
    old_value: Option<&str>,
    new_value: Option<&str>,
  ) -> Result {
    let mut digest = Self::tap_export_rolling_hex_to_bytes(&state.state_digest)?;

    if let Some(old_value) = old_value {
      let contribution = Self::tap_export_digest_pair_bytes(key.as_bytes(), old_value.as_bytes());
      digest = Self::tap_export_rolling_sub(digest, contribution);
    }

    if let Some(new_value) = new_value {
      let contribution = Self::tap_export_digest_pair_bytes(key.as_bytes(), new_value.as_bytes());
      digest = Self::tap_export_rolling_add(digest, contribution);
    }

    match (old_value.is_some(), new_value.is_some()) {
      (false, true) => state.row_count = state.row_count.saturating_add(1),
      (true, false) => {
        ensure!(
          state.row_count > 0,
          "TAP export rolling row count underflow"
        );
        state.row_count -= 1;
      }
      _ => {}
    }

    state.state_digest = hex::encode(digest);
    Ok(())
  }

  pub fn tap_get_string(&self, key: &str) -> Result<Option<String>> {
    let rtx = self.begin_read()?;
    let table = rtx.0.open_table(TAP_KV)?;
    Ok(
      table
        .get(key.as_bytes())?
        .map(|v| Self::tap_decode_string_bytes(v.value()).unwrap_or_default()),
    )
  }

  pub fn tap_get_raw(&self, key: &str) -> Result<Option<Vec<u8>>> {
    let rtx = self.begin_read()?;
    let table = rtx.0.open_table(TAP_KV)?;
    Ok(table.get(key.as_bytes())?.map(|v| v.value().to_vec()))
  }

  #[cfg(test)]
  pub(crate) fn tap_test_put_raw_rows<I, K, V>(&self, rows: I) -> Result<()>
  where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<str>,
    V: AsRef<str>,
  {
    let tx = self.begin_write()?;
    {
      let mut table = tx.open_table(TAP_KV)?;
      for (key, value) in rows {
        table.insert(key.as_ref().as_bytes(), value.as_ref().as_bytes())?;
      }
    }
    tx.commit()?;
    Ok(())
  }

  pub fn tap_get_length(&self, length_key: &str) -> Result<u64> {
    Ok(
      self
        .tap_get_string(length_key)?
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0),
    )
  }

  fn tap_export_metadata_get_u32(
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    key: &[u8],
  ) -> Result<Option<u32>> {
    table
      .get(key)?
      .map(|value| {
        std::str::from_utf8(value.value())?
          .parse::<u32>()
          .map_err(Into::into)
      })
      .transpose()
  }

  fn tap_export_metadata_get_u64(
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    key: &[u8],
  ) -> Result<Option<u64>> {
    table
      .get(key)?
      .map(|value| {
        std::str::from_utf8(value.value())?
          .parse::<u64>()
          .map_err(Into::into)
      })
      .transpose()
  }

  fn tap_export_metadata_get_string(
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    key: &[u8],
  ) -> Result<Option<String>> {
    table
      .get(key)?
      .map(|value| Ok(std::str::from_utf8(value.value())?.to_string()))
      .transpose()
  }

  pub(crate) fn tap_export_metadata_put_u32(
    table: &mut Table<'_, &'static [u8], &'static [u8]>,
    key: &'static [u8],
    value: u32,
  ) -> Result {
    let value = value.to_string();
    table.insert(key, value.as_bytes())?;
    Ok(())
  }

  pub(crate) fn tap_export_metadata_put_u64(
    table: &mut Table<'_, &'static [u8], &'static [u8]>,
    key: &'static [u8],
    value: u64,
  ) -> Result {
    let value = value.to_string();
    table.insert(key, value.as_bytes())?;
    Ok(())
  }

  pub(crate) fn tap_export_metadata_put_string(
    table: &mut Table<'_, &'static [u8], &'static [u8]>,
    key: &'static [u8],
    value: &str,
  ) -> Result {
    table.insert(key, value.as_bytes())?;
    Ok(())
  }

  pub(crate) fn ensure_tap_export_coverage_metadata(
    table: &mut Table<'_, &'static [u8], &'static [u8]>,
    next_uncovered_height: u32,
  ) -> Result {
    let enabled_from = Self::tap_export_metadata_get_u32(table, TAP_EXPORT_ENABLED_FROM_HEIGHT)?;
    let coverage_tip = Self::tap_export_metadata_get_u32(table, TAP_EXPORT_COVERAGE_TIP)?;

    if next_uncovered_height == 0 {
      if enabled_from.is_none() {
        Self::tap_export_metadata_put_u32(table, TAP_EXPORT_ENABLED_FROM_HEIGHT, 0)?;
      }
      table.remove(TAP_EXPORT_COVERAGE_TIP)?;
      return Ok(());
    }

    let covered_tip = next_uncovered_height - 1;

    if enabled_from.is_none() {
      Self::tap_export_metadata_put_u32(
        table,
        TAP_EXPORT_ENABLED_FROM_HEIGHT,
        next_uncovered_height,
      )?;
    }

    if coverage_tip.map_or(true, |tip| tip < covered_tip) {
      Self::tap_export_metadata_put_u32(table, TAP_EXPORT_COVERAGE_TIP, covered_tip)?;
    } else if coverage_tip.is_some_and(|tip| tip > covered_tip) {
      Self::tap_export_metadata_put_u32(table, TAP_EXPORT_COVERAGE_TIP, covered_tip)?;
    }

    Ok(())
  }

  pub(crate) fn tap_export_rolling_state_from_table(
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
  ) -> Result<Option<TapExportRollingState>> {
    let Some(row_count) =
      Self::tap_export_metadata_get_u64(table, TAP_EXPORT_ROLLING_STATE_ROW_COUNT)?
    else {
      return Ok(None);
    };
    let Some(state_digest) =
      Self::tap_export_metadata_get_string(table, TAP_EXPORT_ROLLING_STATE_DIGEST)?
    else {
      return Ok(None);
    };
    Self::tap_export_rolling_hex_to_bytes(&state_digest)?;
    Ok(Some(TapExportRollingState {
      row_count,
      state_digest,
    }))
  }

  #[cfg(test)]
  fn compute_tap_export_rolling_state(
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
  ) -> Result<TapExportRollingState> {
    let mut state = TapExportRollingState {
      row_count: 0,
      state_digest: Self::tap_export_rolling_zero_digest(),
    };
    for result in table.iter()? {
      let (key, value) = result?;
      let value = Self::tap_export_value_string(value.value()).ok_or_else(|| {
        anyhow!(
          "failed to decode TAP export value for key `{}`",
          String::from_utf8_lossy(key.value())
        )
      })?;
      Self::tap_export_rolling_state_apply(
        &mut state,
        std::str::from_utf8(key.value())?,
        None,
        Some(&value),
      )?;
    }
    Ok(state)
  }

  pub(crate) fn ensure_tap_export_rolling_metadata(
    tap_kv: &impl ReadableTable<&'static [u8], &'static [u8]>,
    metadata: &mut Table<'_, &'static [u8], &'static [u8]>,
    block_states: &mut Table<'_, &'static [u8], &'static [u8]>,
    next_uncovered_height: u32,
  ) -> Result<TapExportRollingState> {
    let rolling_tip = Self::tap_export_metadata_get_u32(metadata, TAP_EXPORT_ROLLING_STATE_TIP)?;
    let rolling_state = Self::tap_export_rolling_state_from_table(metadata)?;

    if next_uncovered_height == 0 {
      let state = TapExportRollingState {
        row_count: 0,
        state_digest: Self::tap_export_rolling_zero_digest(),
      };
      if Self::tap_export_metadata_get_u32(metadata, TAP_EXPORT_ROLLING_ENABLED_FROM_HEIGHT)?
        .is_none()
      {
        Self::tap_export_metadata_put_u32(metadata, TAP_EXPORT_ROLLING_ENABLED_FROM_HEIGHT, 0)?;
      }
      metadata.remove(TAP_EXPORT_ROLLING_STATE_TIP)?;
      Self::tap_export_metadata_put_u64(
        metadata,
        TAP_EXPORT_ROLLING_STATE_ROW_COUNT,
        state.row_count,
      )?;
      Self::tap_export_metadata_put_string(
        metadata,
        TAP_EXPORT_ROLLING_STATE_DIGEST,
        &state.state_digest,
      )?;
      return Ok(state);
    }

    let covered_tip = next_uncovered_height - 1;

    let Some(state) = rolling_state else {
      let tap_rows = tap_kv.len()?;
      ensure!(
        tap_rows == 0,
        "TAP export rolling state cannot be enabled on an existing non-empty TAP index without a fresh export start; disable rolling export or use a fresh index/export path"
      );
      let state = TapExportRollingState {
        row_count: 0,
        state_digest: Self::tap_export_rolling_zero_digest(),
      };
      if Self::tap_export_metadata_get_u32(metadata, TAP_EXPORT_ROLLING_ENABLED_FROM_HEIGHT)?
        .is_none()
      {
        Self::tap_export_metadata_put_u32(
          metadata,
          TAP_EXPORT_ROLLING_ENABLED_FROM_HEIGHT,
          next_uncovered_height,
        )?;
      }
      Self::tap_export_metadata_put_u32(metadata, TAP_EXPORT_ROLLING_STATE_TIP, covered_tip)?;
      Self::tap_export_metadata_put_u64(
        metadata,
        TAP_EXPORT_ROLLING_STATE_ROW_COUNT,
        state.row_count,
      )?;
      Self::tap_export_metadata_put_string(
        metadata,
        TAP_EXPORT_ROLLING_STATE_DIGEST,
        &state.state_digest,
      )?;
      return Ok(state);
    };

    ensure!(
      rolling_tip == Some(covered_tip),
      "TAP export rolling state tip mismatch: expected covered tip {covered_tip}, found {:?}; disable rolling export or use a fresh index/export path",
      rolling_tip
    );

    let block_state = TapExportBlockState {
      height: covered_tip,
      rolling_state_row_count: state.row_count,
      rolling_state_digest: state.state_digest.clone(),
    };
    let key = format!("{covered_tip:010}");
    let value = serde_json::to_vec(&block_state)?;
    block_states.insert(key.as_bytes(), value.as_slice())?;

    Ok(state)
  }

  pub(crate) fn ensure_tap_writer_export_coverage_start(&self) -> Result {
    let block_count = self.block_count()?;
    let tx = self.begin_write()?;
    {
      let mut tap_kv = self.tap_writer_export_rolling_state
        .then(|| tx.open_table(TAP_KV))
        .transpose()?;
      let mut table = tx.open_table(TAP_EXPORT_METADATA)?;
      Self::ensure_tap_export_coverage_metadata(&mut table, block_count)?;
      if let Some(tap_kv) = tap_kv.as_mut() {
        let mut block_states = tx.open_table(TAP_EXPORT_BLOCK_STATES)?;
        Self::ensure_tap_export_rolling_metadata(
          tap_kv,
          &mut table,
          &mut block_states,
          block_count,
        )?;
      }
    }
    tx.commit()?;
    Ok(())
  }

  pub fn tap_list_strings(
    &self,
    length_key: &str,
    iterator_key: &str,
    offset: u64,
    max: u64,
  ) -> Result<Vec<String>> {
    let rtx = self.begin_read()?;
    let table = rtx.0.open_table(TAP_KV)?;
    let length = self.tap_get_length(length_key)?;
    let mut out = Vec::new();
    let end = std::cmp::min(length, offset.saturating_add(max));
    for i in offset..end {
      if let Some(v) = table.get(format!("{}/{}", iterator_key, i).as_bytes())? {
        let s = Self::tap_decode_string_bytes(v.value()).unwrap_or_default();
        out.push(s);
      }
    }
    Ok(out)
  }

  pub(crate) fn tap_export_snapshot(
    &self,
    after_key: Option<&str>,
    limit: usize,
    limit_bytes: Option<usize>,
  ) -> Result<TapExportSnapshot> {
    let rtx = self.begin_read()?;
    let source_height = rtx.block_count()?;
    let table = rtx.0.open_table(TAP_KV)?;
    let mut rows = Vec::new();
    let mut next_key = None;
    let limit = limit.clamp(1, 50_000);
    let limit_bytes = limit_bytes.map(|limit| limit.clamp(1024, 64 * 1024 * 1024));
    let mut bytes = 0usize;
    let after_key = after_key.unwrap_or("");

    for result in table.range(after_key.as_bytes()..)? {
      let (key, value) = result?;
      let key = std::str::from_utf8(key.value())?.to_string();
      if !after_key.is_empty() && key.as_str() <= after_key {
        continue;
      }
      let value = Self::tap_export_value_details(value.value())
        .ok_or_else(|| anyhow!("failed to decode TAP export value for key `{key}`"))?;
      let row_bytes = key
        .len()
        .saturating_add(value.value.len())
        .saturating_add(64);
      if let Some(limit_bytes) = limit_bytes {
        if !rows.is_empty() && bytes.saturating_add(row_bytes) > limit_bytes {
          break;
        }
      }
      bytes = bytes.saturating_add(row_bytes);
      rows.push(TapExportSnapshotRow {
        key: key.clone(),
        value: value.value,
        value_kind: value.value_kind,
        source_encoding: value.source_encoding,
      });
      next_key = Some(key);
      if rows.len() >= limit {
        break;
      }
    }

    Ok(TapExportSnapshot {
      source_height,
      source_digest: None,
      source_row_count: None,
      rows,
      next_key,
    })
  }

  pub(crate) fn tap_export_snapshot_open(&self) -> Result<TapExportSnapshotOpen> {
    let digest = self.tap_export_state_digest()?;
    let rtx = self.begin_read()?;
    let source_block_hash = digest
      .source_height
      .checked_sub(1)
      .map(|height| rtx.block_hash(Some(height)))
      .transpose()?
      .flatten()
      .map(|hash| hash.to_string());
    let snapshot_tip = source_block_hash.as_deref().unwrap_or("genesis");
    Ok(TapExportSnapshotOpen {
      snapshot_id: format!(
        "{}:{}:{}",
        digest.source_height, snapshot_tip, digest.state_digest
      ),
      source_height: digest.source_height,
      source_block_hash,
      row_count: digest.row_count,
      state_digest: digest.state_digest,
      limit_rows_max: 50_000,
      limit_bytes_max: 64 * 1024 * 1024,
    })
  }

  pub(crate) fn tap_export_snapshot_read(
    &self,
    snapshot_id: &str,
    after_key: Option<&str>,
    limit_rows: usize,
    limit_bytes: Option<usize>,
  ) -> Result<TapExportSnapshotRead> {
    let mut id_parts = snapshot_id.splitn(3, ':');
    let Some(source_height) = id_parts.next() else {
      bail!("invalid tap export snapshot id");
    };
    let Some(source_block_hash) = id_parts.next() else {
      bail!("invalid tap export snapshot id");
    };
    let Some(_source_digest) = id_parts.next() else {
      bail!("invalid tap export snapshot id");
    };
    let source_height = source_height.parse::<u32>()?;
    let rtx = self.begin_read()?;
    let current_height = rtx.block_count()?;
    ensure!(
      current_height == source_height,
      "tap export snapshot expired or source state changed"
    );
    let current_block_hash = source_height
      .checked_sub(1)
      .map(|height| rtx.block_hash(Some(height)))
      .transpose()?
      .flatten()
      .map(|hash| hash.to_string());
    ensure!(
      current_block_hash.as_deref().unwrap_or("genesis") == source_block_hash,
      "tap export snapshot expired or source state changed"
    );
    drop(rtx);
    let page = self.tap_export_snapshot(after_key, limit_rows, limit_bytes)?;
    Ok(TapExportSnapshotRead {
      snapshot_id: snapshot_id.to_string(),
      source_height,
      rows: page.rows,
      next_key: page.next_key,
    })
  }

  pub(crate) fn tap_export_state_digest(&self) -> Result<TapExportStateDigest> {
    let rtx = self.begin_read()?;
    let source_height = rtx.block_count()?;
    let table = rtx.0.open_table(TAP_KV)?;
    let mut hasher = Sha256::new();
    let mut row_count = 0;
    for result in table.iter()? {
      let (key, value) = result?;
      let value = Self::tap_export_value_string(value.value()).ok_or_else(|| {
        anyhow!(
          "failed to decode TAP export value for key `{}`",
          String::from_utf8_lossy(key.value())
        )
      })?;
      Self::tap_export_digest_pair(&mut hasher, key.value(), value.as_bytes());
      row_count += 1;
    }

    Ok(TapExportStateDigest {
      source_height,
      row_count,
      state_digest: Self::tap_export_digest_hex(hasher),
    })
  }

  pub(crate) fn tap_export_retention_status(&self) -> Result<TapExportRetentionStatus> {
    let rtx = self.begin_read()?;
    let watermark = rtx.block_count()?.saturating_sub(1);
    let (
      export_enabled_from_height,
      export_coverage_tip,
      rolling_enabled_from_height,
      rolling_state_tip,
      rolling_state_row_count,
      rolling_state_digest,
    ) = match rtx.0.open_table(TAP_EXPORT_METADATA) {
      Ok(table) => (
        Self::tap_export_metadata_get_u32(&table, TAP_EXPORT_ENABLED_FROM_HEIGHT)?,
        Self::tap_export_metadata_get_u32(&table, TAP_EXPORT_COVERAGE_TIP)?,
        Self::tap_export_metadata_get_u32(&table, TAP_EXPORT_ROLLING_ENABLED_FROM_HEIGHT)?,
        Self::tap_export_metadata_get_u32(&table, TAP_EXPORT_ROLLING_STATE_TIP)?,
        Self::tap_export_metadata_get_u64(&table, TAP_EXPORT_ROLLING_STATE_ROW_COUNT)?,
        Self::tap_export_metadata_get_string(&table, TAP_EXPORT_ROLLING_STATE_DIGEST)?,
      ),
      Err(redb::TableError::TableDoesNotExist(_)) => (None, None, None, None, None, None),
      Err(err) => return Err(err.into()),
    };
    let mut earliest_retained_block = None;
    let mut latest_retained_block = None;
    let mut latest_nonempty_block = None;
    let mut latest_sequence = None;
    // Sidecar delta files make retention checks bounded and avoid scanning every
    // delta row on each mirror poll. Exact per-block row counts are exposed by
    // block-digest.
    let delta_rows = 0;
    let mut delta_bytes = 0u64;
    let retained_tip = export_coverage_tip.unwrap_or(watermark).min(watermark);

    if let Ok(entries) = std::fs::read_dir(self.tap_export_delta_dir()) {
      for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let Some(height) = updater::inscription_updater::TapDeltaBatch::delta_file_height(&path)
        else {
          continue;
        };
        if height > retained_tip {
          continue;
        }
        let len = entry.metadata()?.len();
        delta_bytes = delta_bytes.saturating_add(len);
        earliest_retained_block = Some(earliest_retained_block.unwrap_or(height).min(height));
        latest_retained_block = Some(latest_retained_block.unwrap_or(height).max(height));
        if len > 0 {
          latest_nonempty_block = Some(latest_nonempty_block.unwrap_or(height).max(height));
        }
      }
    }

    if let Some(height) = latest_nonempty_block {
      latest_sequence = self.tap_export_sidecar_last_delta_sequence_for_block(height)?;
    }

    Ok(TapExportRetentionStatus {
      watermark,
      earliest_retained_block,
      latest_retained_block,
      latest_sequence,
      delta_rows,
      delta_bytes,
      export_enabled_from_height,
      export_coverage_tip,
      rolling_enabled_from_height,
      rolling_state_tip,
      rolling_state_row_count,
      rolling_state_digest,
    })
  }

  fn tap_export_sidecar_last_delta_sequence_for_block(&self, height: u32) -> Result<Option<u64>> {
    let path = self.tap_export_delta_file_path(height);
    if !path.exists() {
      return Ok(None);
    }

    let file = std::fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut latest_sequence = None;

    for line in reader.lines() {
      let line = line?;
      if line.trim().is_empty() {
        continue;
      }
      let row: TapExportDeltaRecord = serde_json::from_str(&line)?;
      ensure!(
        row.height == height,
        "tap export delta file for block {height} contains row for block {}",
        row.height
      );
      latest_sequence = Some(row.sequence);
    }

    Ok(latest_sequence)
  }

  fn tap_export_nonempty_delta_heights(&self, from_block: u32, to_block: u32) -> Result<Vec<u32>> {
    let mut heights = Vec::new();
    let Ok(entries) = std::fs::read_dir(self.tap_export_delta_dir()) else {
      return Ok(heights);
    };

    for entry in entries {
      let entry = entry?;
      let path = entry.path();
      let Some(height) = updater::inscription_updater::TapDeltaBatch::delta_file_height(&path)
      else {
        continue;
      };
      if height >= from_block && height <= to_block && entry.metadata()?.len() > 0 {
        heights.push(height);
      }
    }

    heights.sort_unstable();
    Ok(heights)
  }

  fn tap_export_sidecar_delta_rows_for_block(
    &self,
    height: u32,
  ) -> Result<Vec<TapExportDeltaRecord>> {
    let path = self.tap_export_delta_file_path(height);
    if !path.exists() {
      return Ok(Vec::new());
    }

    let file = std::fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut rows = Vec::new();

    for line in reader.lines() {
      let line = line?;
      if line.trim().is_empty() {
        continue;
      }
      let row: TapExportDeltaRecord = serde_json::from_str(&line)?;
      ensure!(
        row.height == height,
        "tap export delta file for block {height} contains row for block {}",
        row.height
      );
      rows.push(row);
    }

    Ok(rows)
  }

  fn tap_export_redb_delta_rows_for_block(
    rtx: &rtx::Rtx,
    height: u32,
  ) -> Result<Vec<TapExportDeltaRecord>> {
    let table = match rtx.0.open_table(TAP_EXPORT_DELTAS) {
      Ok(table) => table,
      Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
      Err(err) => return Err(err.into()),
    };
    let start_key = format!("{height:010}/");
    let mut rows = Vec::new();

    for result in table.range(start_key.as_bytes()..)? {
      let (_key, value) = result?;
      let row: TapExportDeltaRecord = serde_json::from_slice(value.value())?;
      if row.height != height {
        break;
      }
      rows.push(row);
    }

    Ok(rows)
  }

  fn tap_export_redb_delta_page(
    rtx: &rtx::Rtx,
    from_block: u32,
    from_sequence: u64,
    to_block: u32,
    limit: usize,
    limit_bytes: Option<usize>,
  ) -> Result<TapExportDeltaPage> {
    let table = match rtx.0.open_table(TAP_EXPORT_DELTAS) {
      Ok(table) => table,
      Err(redb::TableError::TableDoesNotExist(_)) => {
        return Ok(TapExportDeltaPage {
          rows: Vec::new(),
          next: None,
        });
      }
      Err(err) => return Err(err.into()),
    };
    let start_key = format!("{from_block:010}/{from_sequence:020}");
    let mut bytes = 0usize;
    let mut rows = Vec::new();
    let mut next = None;

    for result in table.range(start_key.as_bytes()..)? {
      let (_key, value) = result?;
      let mut row: TapExportDeltaRecord = serde_json::from_slice(value.value())?;
      if row.height > to_block {
        break;
      }
      row.block_hash = rtx
        .block_hash(Some(row.height))?
        .map(|hash| hash.to_string());
      row.parent_block_hash = row
        .height
        .checked_sub(1)
        .map(|height| rtx.block_hash(Some(height)))
        .transpose()?
        .flatten()
        .map(|hash| hash.to_string());
      let row_bytes = value.value().len().saturating_add(64);
      if let Some(limit_bytes) = limit_bytes {
        if !rows.is_empty() && bytes.saturating_add(row_bytes) > limit_bytes {
          break;
        }
      }
      bytes = bytes.saturating_add(row_bytes);
      next = Some((row.height, row.sequence.saturating_add(1)));
      rows.push(row);
      if rows.len() >= limit {
        break;
      }
    }

    Ok(TapExportDeltaPage { rows, next })
  }

  pub(crate) fn tap_export_block_digest(&self, height: u32) -> Result<TapExportBlockDigest> {
    let rtx = self.begin_read()?;
    let rolling_block_state = match rtx.0.open_table(TAP_EXPORT_BLOCK_STATES) {
      Ok(table) => {
        let key = format!("{height:010}");
        table
          .get(key.as_bytes())?
          .map(|value| serde_json::from_slice::<TapExportBlockState>(value.value()))
          .transpose()?
      }
      Err(redb::TableError::TableDoesNotExist(_)) => None,
      Err(err) => return Err(err.into()),
    };
    let mut hasher = Sha256::new();
    let mut delta_rows = 0;

    let mut block_rows = self.tap_export_sidecar_delta_rows_for_block(height)?;
    if block_rows.is_empty() {
      block_rows = Self::tap_export_redb_delta_rows_for_block(&rtx, height)?;
    }

    for row in block_rows {
      hasher.update(row.row_hash.as_bytes());
      hasher.update(b"\n");
      delta_rows += 1;
    }

    Ok(TapExportBlockDigest {
      height,
      block_hash: rtx.block_hash(Some(height))?.map(|hash| hash.to_string()),
      parent_block_hash: height
        .checked_sub(1)
        .map(|height| rtx.block_hash(Some(height)))
        .transpose()?
        .flatten()
        .map(|hash| hash.to_string()),
      delta_rows,
      delta_digest: Self::tap_export_digest_hex(hasher),
      rolling_state_row_count: rolling_block_state
        .as_ref()
        .map(|state| state.rolling_state_row_count),
      rolling_state_digest: rolling_block_state
        .as_ref()
        .map(|state| state.rolling_state_digest.clone()),
    })
  }

  pub(crate) fn tap_export_deltas(
    &self,
    from_block: u32,
    from_sequence: u64,
    limit: usize,
    limit_bytes: Option<usize>,
  ) -> Result<TapExportDeltaPage> {
    let rtx = self.begin_read()?;
    let covered_tip = match rtx.0.open_table(TAP_EXPORT_METADATA) {
      Ok(table) => Self::tap_export_metadata_get_u32(&table, TAP_EXPORT_COVERAGE_TIP)?,
      Err(redb::TableError::TableDoesNotExist(_)) => None,
      Err(err) => return Err(err.into()),
    };
    let Some(covered_tip) = covered_tip else {
      return Ok(TapExportDeltaPage {
        rows: Vec::new(),
        next: None,
      });
    };
    let to_block = covered_tip.min(rtx.block_count()?.saturating_sub(1));
    if from_block > to_block {
      return Ok(TapExportDeltaPage {
        rows: Vec::new(),
        next: None,
      });
    }
    let limit = limit.clamp(1, 100_000);
    let limit_bytes = limit_bytes.map(|limit| limit.clamp(1024, 64 * 1024 * 1024));
    let mut bytes = 0usize;
    let mut rows = Vec::new();
    let mut next = None;

    let redb_page = Self::tap_export_redb_delta_page(
      &rtx,
      from_block,
      from_sequence,
      to_block,
      limit,
      limit_bytes,
    )?;
    if !redb_page.rows.is_empty() {
      return Ok(redb_page);
    }

    for height in self.tap_export_nonempty_delta_heights(from_block, to_block)? {
      let block_hash = rtx.block_hash(Some(height))?.map(|hash| hash.to_string());
      let parent_block_hash = height
        .checked_sub(1)
        .map(|height| rtx.block_hash(Some(height)))
        .transpose()?
        .flatten()
        .map(|hash| hash.to_string());

      for mut row in self.tap_export_sidecar_delta_rows_for_block(height)? {
        if height == from_block && row.sequence < from_sequence {
          continue;
        }

        row.block_hash = block_hash.clone();
        row.parent_block_hash = parent_block_hash.clone();
        let row_bytes = serde_json::to_vec(&row)?.len().saturating_add(1);
        if let Some(limit_bytes) = limit_bytes {
          if !rows.is_empty() && bytes.saturating_add(row_bytes) > limit_bytes {
            return Ok(TapExportDeltaPage { rows, next });
          }
        }
        bytes = bytes.saturating_add(row_bytes);
        next = Some((row.height, row.sequence.saturating_add(1)));
        rows.push(row);
        if rows.len() >= limit {
          return Ok(TapExportDeltaPage { rows, next });
        }
      }
    }

    Ok(TapExportDeltaPage { rows, next })
  }
}
