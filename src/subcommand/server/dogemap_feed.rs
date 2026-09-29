//! HTTP routes of the `dogemap-feed-v1` provider feed. Parameters are parsed
//! strictly here; each handler then runs one `Index::dogemap_feed_*` call on a
//! blocking thread, which reads everything it serves from one redb read
//! transaction (index/dogemap_feed.rs).

use {
  super::*,
  crate::{
    dogemap_feed::{self as wire, BlockCursor, FeedErrorCode},
    index::{FeedBlockCache, FeedBlockRequest, FeedBodyRequest, FeedFailure},
  },
  bitcoin::secp256k1::rand::{self, RngCore},
  serde_json::Value,
};

/// The node tip is read at most this often for readiness.
const NODE_TIP_REFRESH: Duration = Duration::from_secs(10);

#[derive(Default)]
pub(super) struct FeedState {
  node_tip: Mutex<Option<(Instant, Option<u64>)>>,
  blocks: FeedBlockCache,
}

impl FeedState {
  pub(super) fn node_tip(&self, index: &Index) -> Option<u64> {
    let mut cached = self.node_tip.lock().unwrap();
    if let Some((at, tip)) = *cached {
      if at.elapsed() < NODE_TIP_REFRESH {
        return tip;
      }
    }
    let tip = match index.dogemap_feed_rpc().and_then(|rpc| rpc.block_count()) {
      Ok(tip) => Some(tip),
      Err(error) => {
        log::warn!("dogemap feed: node tip unavailable: {error}");
        None
      }
    };
    *cached = Some((Instant::now(), tip));
    tip
  }
}

/// A typed feed error response.
pub(super) struct FeedError {
  code: FeedErrorCode,
  message: String,
}

impl FeedError {
  fn new(code: FeedErrorCode, message: impl Into<String>) -> Self {
    Self {
      code,
      message: message.into(),
    }
  }
}

impl From<FeedFailure> for FeedError {
  fn from(failure: FeedFailure) -> Self {
    match failure {
      FeedFailure::Refused(code, message) => Self::new(code, message),
      FeedFailure::Internal(error) => {
        log::error!("dogemap feed: internal error: {error:#}");
        Self::new(
          FeedErrorCode::ContentUnavailable,
          "the provider could not read its index",
        )
      }
    }
  }
}

impl IntoResponse for FeedError {
  fn into_response(self) -> Response {
    let mut id = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut id);
    let correlation_id = hex::encode(id);
    log::info!(
      "dogemap feed: {} {} [{correlation_id}]",
      self.code.as_str(),
      self.message
    );
    let status =
      StatusCode::from_u16(self.code.http_status()).unwrap_or(StatusCode::SERVICE_UNAVAILABLE);
    (
      status,
      [(header::CACHE_CONTROL, HeaderValue::from_static("no-store"))],
      Json(json!({
        "schemaVersion": wire::ERROR_SCHEMA,
        "error": {
          "code": self.code.as_str(),
          "message": self.message,
        },
        "correlationId": correlation_id,
      })),
    )
      .into_response()
  }
}

type FeedResult = Result<Response, FeedError>;

fn invalid(message: impl Into<String>) -> FeedError {
  FeedError::new(FeedErrorCode::InvalidRequest, message)
}

fn ok(value: Value) -> FeedResult {
  Ok(
    (
      [(header::CACHE_CONTROL, HeaderValue::from_static("no-store"))],
      Json(value),
    )
      .into_response(),
  )
}

async fn blocking<T: Send + 'static>(
  task: impl FnOnce() -> Result<T, FeedError> + Send + 'static,
) -> Result<T, FeedError> {
  task::spawn_blocking(task)
    .await
    .map_err(|error| FeedError::from(FeedFailure::Internal(error.into())))?
}

/// Query parameters arrive as raw strings so every value is checked against
/// the contract grammar instead of a permissive integer parser.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BlockQuery {
  #[serde(rename = "blockHash")]
  block_hash: Option<String>,
  #[serde(rename = "databaseId")]
  database_id: Option<String>,
  #[serde(rename = "reorgEpoch")]
  reorg_epoch: Option<String>,
  cursor: Option<String>,
  limit: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BodyQuery {
  #[serde(rename = "blockHash")]
  block_hash: Option<String>,
  #[serde(rename = "databaseId")]
  database_id: Option<String>,
  #[serde(rename = "reorgEpoch")]
  reorg_epoch: Option<String>,
  offset: Option<String>,
  length: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LocationsQuery {
  ids: Option<String>,
}

struct Identity {
  block_hash: String,
  database_id: String,
  reorg_epoch: u64,
}

fn identity(
  block_hash: Option<String>,
  database_id: Option<String>,
  reorg_epoch: Option<String>,
) -> Result<Identity, FeedError> {
  let block_hash = block_hash.ok_or_else(|| invalid("blockHash is required"))?;
  if !wire::is_lower_hex(&block_hash, 64) {
    return Err(invalid("blockHash must be 64 lowercase hex digits"));
  }
  let database_id = database_id.ok_or_else(|| invalid("databaseId is required"))?;
  if !wire::is_lower_hex(&database_id, 32) {
    return Err(invalid("databaseId must be 32 lowercase hex digits"));
  }
  let reorg_epoch = reorg_epoch
    .as_deref()
    .ok_or_else(|| invalid("reorgEpoch is required"))
    .and_then(|epoch| {
      wire::parse_decimal_u64(epoch).ok_or_else(|| invalid("reorgEpoch must be a decimal string"))
    })?;
  Ok(Identity {
    block_hash,
    database_id,
    reorg_epoch,
  })
}

fn bounded(
  value: Option<&str>,
  name: &str,
  default: usize,
  max: usize,
) -> Result<usize, FeedError> {
  match value {
    None => Ok(default),
    Some(value) => wire::parse_decimal_u64(value)
      .and_then(|value| usize::try_from(value).ok())
      .filter(|value| (1..=max).contains(value))
      .ok_or_else(|| invalid(format!("{name} must be an integer from 1 to {max}"))),
  }
}

fn inscription_id(value: &str) -> Result<InscriptionId, FeedError> {
  if wire::parse_inscription_id(value).is_none() {
    return Err(invalid(format!(
      "{value:?} is not an inscription id (<64 lowercase hex>i<index>)"
    )));
  }
  value
    .parse()
    .map_err(|_| invalid(format!("{value:?} is not an inscription id")))
}

impl Server {
  pub(super) async fn dogemap_feed_capabilities(
    Extension(index): Extension<Arc<Index>>,
    Extension(state): Extension<Arc<FeedState>>,
  ) -> FeedResult {
    let value = blocking(move || {
      let node_tip = state.node_tip(&index);
      index
        .dogemap_feed_capabilities(node_tip)
        .map_err(|error| FeedError::from(FeedFailure::Internal(error)))
    })
    .await?;
    ok(value)
  }

  pub(super) async fn dogemap_feed_block(
    Extension(index): Extension<Arc<Index>>,
    Extension(state): Extension<Arc<FeedState>>,
    Path(height): Path<String>,
    query: Result<Query<BlockQuery>, axum::extract::rejection::QueryRejection>,
  ) -> FeedResult {
    let Query(query) = query.map_err(|rejection| invalid(rejection.body_text()))?;
    let height =
      wire::parse_decimal_u32(&height).ok_or_else(|| invalid("height must be a decimal u32"))?;
    let identity = identity(query.block_hash, query.database_id, query.reorg_epoch)?;
    let cursor = match query.cursor.as_deref() {
      None => None,
      Some(cursor) => Some(BlockCursor::decode(cursor).ok_or_else(|| {
        FeedError::new(FeedErrorCode::InvalidCursor, "cursor is not a feed cursor")
      })?),
    };
    let limit = bounded(
      query.limit.as_deref(),
      "limit",
      wire::DEFAULT_PAGE_LIMIT,
      wire::MAX_PAGE_LIMIT,
    )?;
    let request = FeedBlockRequest {
      height,
      block_hash: identity.block_hash,
      database_id: identity.database_id,
      reorg_epoch: identity.reorg_epoch,
      cursor,
      limit,
    };
    let value = blocking(move || {
      index
        .dogemap_feed_block(&request, &state.blocks)
        .map_err(FeedError::from)
    })
    .await?;
    ok(value)
  }

  pub(super) async fn dogemap_feed_body(
    Extension(index): Extension<Arc<Index>>,
    Path(id): Path<String>,
    query: Result<Query<BodyQuery>, axum::extract::rejection::QueryRejection>,
  ) -> FeedResult {
    let Query(query) = query.map_err(|rejection| invalid(rejection.body_text()))?;
    let inscription_id = inscription_id(&id)?;
    let identity = identity(query.block_hash, query.database_id, query.reorg_epoch)?;
    let offset = match query.offset.as_deref() {
      None => 0,
      Some(offset) => wire::parse_decimal_u64(offset)
        .and_then(|offset| usize::try_from(offset).ok())
        .ok_or_else(|| invalid("offset must be a decimal string"))?,
    };
    let length = bounded(
      query.length.as_deref(),
      "length",
      wire::MAX_BODY_CHUNK_BYTES,
      wire::MAX_BODY_CHUNK_BYTES,
    )?;
    let request = FeedBodyRequest {
      inscription_id,
      block_hash: identity.block_hash,
      database_id: identity.database_id,
      reorg_epoch: identity.reorg_epoch,
      offset,
      length,
    };
    let value =
      blocking(move || index.dogemap_feed_body(&request).map_err(FeedError::from)).await?;
    ok(value)
  }

  pub(super) async fn dogemap_feed_locations(
    Extension(index): Extension<Arc<Index>>,
    query: Result<Query<LocationsQuery>, axum::extract::rejection::QueryRejection>,
  ) -> FeedResult {
    let Query(query) = query.map_err(|rejection| invalid(rejection.body_text()))?;
    let ids = query.ids.ok_or_else(|| invalid("ids is required"))?;
    let ids = ids.split(',').collect::<Vec<&str>>();
    if ids.len() > wire::MAX_LOCATION_IDS {
      return Err(invalid(format!(
        "at most {} ids per request",
        wire::MAX_LOCATION_IDS
      )));
    }
    let ids = ids
      .into_iter()
      .map(inscription_id)
      .collect::<Result<Vec<InscriptionId>, FeedError>>()?;
    let value =
      blocking(move || index.dogemap_feed_locations(&ids).map_err(FeedError::from)).await?;
    ok(value)
  }
}
