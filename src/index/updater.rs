use bitcoincore_rpc::bitcoin::BlockHeader;

use {
  self::{dune_updater::DuneUpdater, inscription_updater::InscriptionUpdater},
  futures::future::try_join_all,
  std::sync::mpsc,
  super::{*, fetcher::Fetcher},
  tokio::sync::mpsc::{error::TryRecvError, Receiver, Sender},
};

use crate::drc20::BlockContext;
use crate::index::updater::drc20_updater::Drc20Updater;
use crate::sat::Sat;
use crate::sat_point::SatPoint;

mod drc20_updater;
mod dune_updater;
pub(crate) mod inscription_updater;

/// Headers fetched per batched RPC call below the first inscription height.
const HEADER_BATCH: u32 = 1000;
/// Heights per JSON-RPC batch; small enough to finish within the client timeout.
const HEADER_CHUNK: u32 = 100;
/// Upper bound on concurrent header batches and block fetches (one RPC connection each; the
/// actual count is --nr-parallel-requests). A loaded node answers each call in seconds, so
/// throughput scales with requests in flight.
const HEADER_FETCH_CONNECTIONS: usize = 64;

/// Blocks per write transaction while only headers are indexed.
const HEADER_ONLY_COMMIT_INTERVAL: u32 = 25_000;

pub(crate) struct BlockData {
  pub(crate) header: BlockHeader,
  pub(crate) txdata: Vec<(Transaction, Txid)>,
}

impl From<Block> for BlockData {
  fn from(block: Block) -> Self {
    BlockData {
      header: block.header,
      txdata: block
        .txdata
        .into_iter()
        .map(|transaction| {
          let txid = transaction.txid();
          (transaction, txid)
        })
        .collect(),
    }
  }
}

#[derive(Clone)]
pub(crate) struct Updater<'index> {
  range_cache: HashMap<OutPointValue, Vec<u8>>,
  height: u32,
  index: &'index Index,
  sat_ranges_since_flush: u64,
  outputs_cached: u64,
  outputs_inserted_since_flush: u64,
  outputs_traversed: u64,
  /// Lean replay lookahead: input values fetched ahead of the blocks waiting
  /// in the fetch queue, so a block rarely waits for a node round trip.
  #[cfg(feature = "tap")]
  tap_lookahead: Arc<Mutex<HashMap<OutPoint, u64>>>,
  #[cfg(feature = "tap")]
  tap_lookahead_sender: Option<std::sync::mpsc::Sender<Vec<OutPoint>>>,
}

impl<'index> Updater<'_> {
  pub(crate) fn new(index: &'index Index) -> Result<Updater<'index>> {
    Ok(Updater {
      range_cache: HashMap::new(),
      height: index.block_count()?,
      index,
      sat_ranges_since_flush: 0,
      outputs_cached: 0,
      outputs_inserted_since_flush: 0,
      outputs_traversed: 0,
      #[cfg(feature = "tap")]
      tap_lookahead: Arc::new(Mutex::new(HashMap::new())),
      #[cfg(feature = "tap")]
      tap_lookahead_sender: None,
    })
  }

  pub(crate) fn update_index(&mut self) -> Result {
    let mut wtx = self.index.begin_write()?;
    let starting_height = u32::try_from(self.index.client().get_block_count()?).unwrap() + 1;
    #[cfg(feature = "tap")]
    Self::tap_lean_durability(&mut wtx, self.index, self.height, starting_height);

    wtx
      .open_table(WRITE_TRANSACTION_STARTING_BLOCK_COUNT_TO_TIMESTAMP)?
      .insert(
        &self.height,
        &SystemTime::now()
          .duration_since(SystemTime::UNIX_EPOCH)
          .map(|duration| duration.as_millis())
          .unwrap_or(0),
      )?;

    let mut progress_bar = if cfg!(test)
      || log_enabled!(log::Level::Info)
      || starting_height <= self.height as u32
      || integration_test()
    {
      None
    } else {
      let progress_bar = ProgressBar::new(starting_height.into());
      progress_bar.set_position(self.height.into());
      progress_bar.set_style(
        ProgressStyle::with_template("[indexing blocks] {wide_bar} {pos}/{len}").unwrap(),
      );
      Some(progress_bar)
    };

    let rx = Self::fetch_blocks_from(self.index, self.height, self.index.index_sats)?;

    let (mut outpoint_sender, mut value_receiver) = Self::spawn_fetcher(self.index)?;

    #[cfg(feature = "tap")]
    if self.index.tap_lean && self.tap_lookahead_sender.is_none() {
      self.tap_lookahead_sender = Some(Self::spawn_tap_lookahead(
        self.index,
        self.tap_lookahead.clone(),
      )?);
    }

    let mut uncommitted = 0;
    let mut value_cache = HashMap::new();
    let mut ahead: std::collections::VecDeque<BlockData> = std::collections::VecDeque::new();
    #[cfg(feature = "tap")]
    let mut scanned_ahead = 0usize;
    loop {
      let block = match ahead.pop_front() {
        Some(block) => {
          #[cfg(feature = "tap")]
          {
            scanned_ahead = scanned_ahead.saturating_sub(1);
          }
          block
        }
        None => match rx.recv() {
          Ok(block) => block,
          Err(_) => break,
        },
      };
      #[cfg(feature = "tap")]
      if self.tap_lookahead_sender.is_some() {
        while ahead.len() < 32 {
          match rx.try_recv() {
            Ok(next) => ahead.push_back(next),
            Err(_) => break,
          }
        }
        if scanned_ahead < ahead.len() {
          self.tap_lookahead_scan(&mut wtx, &block, &ahead, scanned_ahead, &value_cache)?;
          scanned_ahead = ahead.len();
        }
      }
      self.index_block(
        self.index,
        &mut outpoint_sender,
        &mut value_receiver,
        &mut wtx,
        block,
        &mut value_cache,
      )?;

      if let Some(progress_bar) = &mut progress_bar {
        progress_bar.inc(1);

        if progress_bar.position() > progress_bar.length().unwrap() {
          if let Ok(count) = self.index.client().get_block_count() {
            progress_bar.set_length(count + 1);
          } else {
            log::warn!("Failed to fetch latest block height");
          }
        }
      }

      uncommitted += 1;

      // Header-only blocks below the first inscription height write a few
      // bytes each, so committing them every 1000 blocks spends most of a
      // fresh index's time in fsync. They commit every
      // HEADER_ONLY_COMMIT_INTERVAL blocks; savepoints are only taken near the
      // node tip, where blocks carry inscriptions and commit every 1000.
      #[cfg(feature = "tap")]
      let lean = self.index.tap_lean;
      #[cfg(not(feature = "tap"))]
      let lean = false;
      let commit_interval =
        if !self.index.index_sats && self.height < self.index.first_inscription_height {
          HEADER_ONLY_COMMIT_INTERVAL
        } else if lean {
          // Each commit rewrites every page it touches and redb reuses the
          // freed pages only a commit or two later, so the file carries about
          // two commits of rewritten pages as free space. Lean replay commits
          // every 100 blocks to keep that headroom small.
          100
        } else {
          1000
        };

      // Also commit at every savepoint height near the node tip: savepoints
      // are only taken on a commit at such a height, and without them a
      // reorg right after catching up has nothing to roll back to.
      if uncommitted >= commit_interval || Reorg::savepoint_due(self.height, starting_height) {
        self.commit(wtx, value_cache)?;
        value_cache = HashMap::new();
        uncommitted = 0;
        wtx = self.index.begin_write()?;
        #[cfg(feature = "tap")]
        Self::tap_lean_durability(&mut wtx, self.index, self.height, starting_height);
        let height = wtx
          .open_table(HEIGHT_TO_BLOCK_HASH)?
          .range(0..)?
          .next_back()
          .and_then(|result| result.ok())
          .map(|(height, _hash)| height.value() + 1)
          .unwrap_or(0);
        if height != self.height {
          // another update has run between committing and beginning the new
          // write transaction
          break;
        }
        wtx
          .open_table(WRITE_TRANSACTION_STARTING_BLOCK_COUNT_TO_TIMESTAMP)?
          .insert(
            &self.height,
            &SystemTime::now()
              .duration_since(SystemTime::UNIX_EPOCH)
              .map(|duration| duration.as_millis())
              .unwrap_or(0),
          )?;
      }

      if SHUTTING_DOWN.load(atomic::Ordering::Relaxed) {
        break;
      }
    }

    if uncommitted > 0 {
      self.commit(wtx, value_cache)?;
    }

    if let Some(progress_bar) = &mut progress_bar {
      progress_bar.finish_and_clear();
    }

    Ok(())
  }

  /// Lean TAP replay far from the node tip commits with eventual durability:
  /// the OS orders the page writes instead of a synchronous flush per commit,
  /// which dominated replay time on HDD. A crash can lose the last commits
  /// but leaves a consistent database; near the tip, commits stay immediate
  /// so reorg savepoints are durable.
  #[cfg(feature = "tap")]
  fn tap_lean_durability(
    wtx: &mut WriteTransaction,
    index: &Index,
    height: u32,
    starting_height: u32,
  ) {
    if index.tap_lean && starting_height.saturating_sub(height) > 100 {
      wtx.set_durability(redb::Durability::Eventual);
    }
  }

  fn fetch_blocks_from(
    index: &Index,
    mut height: u32,
    index_sats: bool,
  ) -> Result<mpsc::Receiver<BlockData>> {
    let (tx, rx) = mpsc::sync_channel(32);

    let height_limit = index.height_limit;

    let client =
      Client::new(&index.rpc_url, index.auth.clone()).context("failed to connect to RPC URL")?;

    // One connection per concurrent header chunk: a node answers a JSON-RPC
    // batch serially, so a single 1000-call batch can outlast the client's
    // 15 s timeout on a loaded node, while chunks on separate connections
    // run on the node's RPC worker threads in parallel.
    #[allow(unused_mut)]
    let mut header_clients = (0..index.nr_parallel_requests.clamp(1, HEADER_FETCH_CONNECTIONS))
      .map(|_| Client::new(&index.rpc_url, index.auth.clone()))
      .collect::<std::result::Result<Vec<Client>, _>>()
      .context("failed to connect to RPC URL")?;
    // A second node, interleaved so each fetch window spreads across both.
    #[cfg(feature = "tap")]
    if let Some((url, auth)) = &index.tap_extra_rpc {
      let extra = (0..header_clients.len())
        .map(|_| Client::new(url, auth.clone()))
        .collect::<std::result::Result<Vec<Client>, _>>()
        .context("failed to connect to the extra RPC URL")?;
      header_clients = header_clients
        .into_iter()
        .zip(extra)
        .flat_map(|(primary, extra)| [primary, extra])
        .collect();
    }

    let first_inscription_height = index.first_inscription_height;

    thread::spawn(move || loop {
      if let Some(height_limit) = height_limit {
        if height >= height_limit {
          break;
        }
      }

      // Below the first inscription height only headers are indexed. Fetch
      // them in batches: two RPC round trips per block made a fresh index of
      // a long chain (Dogecoin testnet has tens of millions of blocks) take a
      // day. Anything the batch cannot answer, such as heights past the node
      // tip, falls through to the one-block path with its retries.
      if !index_sats && height < first_inscription_height {
        let end = first_inscription_height
          .min(height.saturating_add(HEADER_BATCH))
          .min(height_limit.unwrap_or(u32::MAX));
        match Self::get_headers_parallel(&header_clients, height, end) {
          Ok(Some(headers)) => {
            for header in headers {
              if let Err(err) = tx.send(BlockData {
                header,
                txdata: Vec::new(),
              }) {
                log::info!("Block receiver disconnected: {err}");
                return;
              }
              height += 1;
            }
            continue;
          }
          Ok(None) => {}
          Err(err) => log::warn!("batched header fetch from {height} failed: {err}"),
        }
      }

      // Full blocks: fetch a window of heights concurrently, one connection
      // each, and hand them over in height order. A loaded node takes 0.2-2 s
      // per getblock, so one-at-a-time fetching left the indexer idle.
      if (index_sats || height >= first_inscription_height) && header_clients.len() > 1 {
        let window = u32::try_from(header_clients.len()).unwrap_or(1);
        let window = height_limit.map_or(window, |limit| window.min(limit.saturating_sub(height)));
        let mut results = Vec::with_capacity(window as usize);
        thread::scope(|scope| {
          let handles = (0..window)
            .zip(&header_clients)
            .map(|(i, client)| {
              scope.spawn(move || {
                Self::get_block_with_retries(client, height + i, index_sats, first_inscription_height)
              })
            })
            .collect::<Vec<_>>();
          for handle in handles {
            results.push(
              handle
                .join()
                .unwrap_or_else(|_| Err(anyhow!("block fetch thread panicked"))),
            );
          }
        });
        let mut stop = false;
        for result in results {
          match result {
            Ok(Some(block)) => {
              if let Err(err) = tx.send(block.into()) {
                log::info!("Block receiver disconnected: {err}");
                return;
              }
              height += 1;
            }
            Ok(None) => {
              stop = true;
              break;
            }
            Err(err) => {
              log::error!("failed to fetch block {height}: {err}");
              stop = true;
              break;
            }
          }
        }
        if stop {
          break;
        }
        continue;
      }

      match Self::get_block_with_retries(&client, height, index_sats, first_inscription_height) {
        Ok(Some(block)) => {
          if let Err(err) = tx.send(block.into()) {
            log::info!("Block receiver disconnected: {err}");
            break;
          }
          height += 1;
        }
        Ok(None) => break,
        Err(err) => {
          log::error!("failed to fetch block {height}: {err}");
          break;
        }
      }
    });

    Ok(rx)
  }

  /// Headers of `start..end`, fetched as `HEADER_CHUNK`-sized batches spread
  /// over `clients` concurrently and returned in height order, or `None` when
  /// any chunk cannot be answered completely.
  fn get_headers_parallel(
    clients: &[Client],
    start: u32,
    end: u32,
  ) -> Result<Option<Vec<BlockHeader>>> {
    let chunks = (start..end)
      .step_by(HEADER_CHUNK as usize)
      .map(|chunk_start| (chunk_start, end.min(chunk_start + HEADER_CHUNK)))
      .collect::<Vec<_>>();
    let mut results: Vec<Option<Result<Option<Vec<BlockHeader>>>>> =
      (0..chunks.len()).map(|_| None).collect();
    for (wave, wave_results) in chunks
      .chunks(clients.len())
      .zip(results.chunks_mut(clients.len()))
    {
      thread::scope(|scope| {
        let handles = wave
          .iter()
          .zip(clients)
          .map(|(&(chunk_start, chunk_end), client)| {
            scope.spawn(move || Self::get_headers_batch(client, chunk_start, chunk_end))
          })
          .collect::<Vec<_>>();
        for (slot, handle) in wave_results.iter_mut().zip(handles) {
          *slot = Some(
            handle
              .join()
              .unwrap_or_else(|_| Err(anyhow!("header fetch thread panicked"))),
          );
        }
      });
    }
    let mut headers = Vec::with_capacity((end - start) as usize);
    for result in results {
      match result.expect("every chunk is fetched")? {
        Some(chunk) => headers.extend(chunk),
        None => return Ok(None),
      }
    }
    Ok(Some(headers))
  }

  /// Headers of `start..end` in two batched calls, or `None` when the node
  /// cannot answer every height (the batch then yields to the one-block path).
  fn get_headers_batch(client: &Client, start: u32, end: u32) -> Result<Option<Vec<BlockHeader>>> {
    use serde_json::value::{to_raw_value, RawValue};

    if start >= end {
      return Ok(None);
    }
    let rpc = client.get_jsonrpc_client();

    let params = (start..end)
      .map(|height| Ok(vec![to_raw_value(&height)?]))
      .collect::<Result<Vec<Vec<Box<RawValue>>>>>()?;
    let requests = params
      .iter()
      .map(|params| rpc.build_request("getblockhash", params))
      .collect::<Vec<_>>();
    let mut hashes = Vec::with_capacity(requests.len());
    for response in rpc.send_batch(&requests)? {
      let Some(Ok(hash)) = response.map(|response| response.result::<String>()) else {
        return Ok(None);
      };
      hashes.push(hash);
    }

    let params = hashes
      .iter()
      .map(|hash| Ok(vec![to_raw_value(hash)?, to_raw_value(&false)?]))
      .collect::<Result<Vec<Vec<Box<RawValue>>>>>()?;
    let requests = params
      .iter()
      .map(|params| rpc.build_request("getblockheader", params))
      .collect::<Vec<_>>();
    let mut headers = Vec::with_capacity(requests.len());
    for (response, hash) in rpc.send_batch(&requests)?.into_iter().zip(&hashes) {
      let Some(Ok(header)) = response.map(|response| response.result::<String>()) else {
        return Ok(None);
      };
      // Merge-mined headers are followed by their AuxPoW; like the RPC
      // client's own header decoding, read the 80-byte header and ignore it.
      let (header, _) =
        consensus::encode::deserialize_partial::<BlockHeader>(&hex::decode(header)?)?;
      ensure!(
        header.block_hash().to_string() == *hash,
        "node returned header {} for block {hash}",
        header.block_hash()
      );
      headers.push(header);
    }
    Ok(Some(headers))
  }

  fn get_block_with_retries(
    client: &Client,
    height: u32,
    index_sats: bool,
    first_inscription_height: u32,
  ) -> Result<Option<Block>> {
    let mut errors = 0;
    loop {
      match client
        .get_block_hash(height.into())
        .into_option()
        .and_then(|option| {
          option
            .map(|hash| {
              if index_sats || height >= first_inscription_height {
                Ok(client.get_block(&hash)?)
              } else {
                Ok(Block {
                  header: client.get_block_header(&hash)?,
                  txdata: Vec::new(),
                })
              }
            })
            .transpose()
        }) {
        Err(err) => {
          if cfg!(test) {
            return Err(err);
          }

          errors += 1;
          let seconds = 1 << errors;
          log::warn!("failed to fetch block {height}, retrying in {seconds}s: {err}");

          if seconds > 120 {
            log::error!("would sleep for more than 120s, giving up");
            return Err(err);
          }

          thread::sleep(Duration::from_secs(seconds));
        }
        Ok(result) => return Ok(result),
      }
    }
  }

  /// Background fetcher for the lean lookahead: values land in `map` keyed by
  /// outpoint, so a speculative or late answer can never be consumed out of
  /// order (the per-block fetcher returns values strictly in request order).
  #[cfg(feature = "tap")]
  fn spawn_tap_lookahead(
    index: &Index,
    map: Arc<Mutex<HashMap<OutPoint, u64>>>,
  ) -> Result<std::sync::mpsc::Sender<Vec<OutPoint>>> {
    let fetcher = Fetcher::new(&index.rpc_url, index.auth.clone())?;
    // A quarter of the request budget: block and per-block value fetches
    // already hold most connections, and a syncing node refuses new ones.
    let parallel = (index.nr_parallel_requests / 4).max(1);
    let (sender, receiver) = std::sync::mpsc::channel::<Vec<OutPoint>>();
    thread::spawn(move || {
      let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(err) => {
          log::warn!("tap lookahead disabled: {err}");
          return;
        }
      };
      while let Ok(mut outpoints) = receiver.recv() {
        while outpoints.len() < 20_000 {
          match receiver.try_recv() {
            Ok(more) => outpoints.extend(more),
            Err(_) => break,
          }
        }
        let chunk = outpoints.len() / parallel + 1;
        let result = runtime.block_on(try_join_all(outpoints.chunks(chunk).map(|chunk| {
          fetcher.get_transactions(chunk.iter().map(|outpoint| outpoint.txid).collect())
        })));
        match result {
          Ok(txs) => {
            let mut map = map.lock().unwrap();
            if map.len() > 2_000_000 {
              map.clear();
            }
            for (outpoint, tx) in outpoints.iter().zip(txs.iter().flatten()) {
              if let Some(output) = usize::try_from(outpoint.vout)
                .ok()
                .and_then(|vout| tx.output.get(vout))
              {
                map.insert(*outpoint, output.value);
              }
            }
          }
          Err(err) => log::warn!("tap lookahead fetch failed: {err}"),
        }
      }
    });
    Ok(sender)
  }

  /// Requests, ahead of time, the input values the queued blocks will need:
  /// inputs of transactions that move a currently known inscription or may
  /// inscribe, whose values are neither stored nor created within the queue.
  #[cfg(feature = "tap")]
  fn tap_lookahead_scan(
    &self,
    wtx: &mut WriteTransaction,
    current: &BlockData,
    ahead: &std::collections::VecDeque<BlockData>,
    from: usize,
    value_cache: &HashMap<OutPoint, OutPointMapValue>,
  ) -> Result {
    let Some(sender) = &self.tap_lookahead_sender else {
      return Ok(());
    };
    let queued: HashSet<Txid> = std::iter::once(current)
      .chain(ahead.iter())
      .flat_map(|block| block.txdata.iter().map(|(_, txid)| *txid))
      .collect();
    let satpoints = wtx.open_table(SATPOINT_TO_INSCRIPTION_ID)?;
    let partials = wtx.open_table(PARTIAL_TXID_TO_INSCRIPTION_TXIDS)?;
    let values = wtx.open_table(OUTPOINT_TO_VALUE)?;
    let known = self.tap_lookahead.lock().unwrap().len();
    let mut wanted = Vec::new();
    for block in ahead.iter().skip(from) {
      for (tx, _) in block.txdata.iter().skip(1) {
        let mut relevant = false;
        for input in &tx.input {
          if Index::inscriptions_on_output(&satpoints, input.previous_output)?
            .next()
            .is_some()
          {
            relevant = true;
            break;
          }
        }
        if !relevant && !InscriptionUpdater::tap_lean_may_inscribe(&partials, tx)? {
          continue;
        }
        for input in &tx.input {
          let outpoint = input.previous_output;
          if outpoint.is_null()
            || queued.contains(&outpoint.txid)
            || value_cache.contains_key(&outpoint)
            || values.get(&outpoint.store())?.is_some()
          {
            continue;
          }
          wanted.push(outpoint);
        }
      }
    }
    if !wanted.is_empty() && known < 2_000_000 {
      let _ = sender.send(wanted);
    }
    Ok(())
  }

  fn spawn_fetcher(index: &Index) -> Result<(Sender<OutPoint>, Receiver<u64>)> {
    let fetcher = Fetcher::new(&index.rpc_url, index.auth.clone())?;

    // Not sure if any block has more than 20k inputs, but none so far after first inscription block
    const CHANNEL_BUFFER_SIZE: usize = 20_000;
    let (outpoint_sender, mut outpoint_receiver) =
      tokio::sync::mpsc::channel::<OutPoint>(CHANNEL_BUFFER_SIZE);
    let (value_sender, value_receiver) = tokio::sync::mpsc::channel::<u64>(CHANNEL_BUFFER_SIZE);

    // Batch 2048 missing inputs at a time. Arbitrarily chosen for now, maybe higher or lower can be faster?
    // Did rudimentary benchmarks with 1024 and 4096 and time was roughly the same.
    const BATCH_SIZE: usize = 2048 * 10;
    // Keep in mind that default rpcworkqueue in dogecoind is 16, meaning more than 16 concurrent requests will be rejected.
    // Since we are already requesting blocks on a separate thread, and we don't want to break if anything
    // else runs a request, we need to keep this a bit lower as configured.
    let parallel_requests = index.nr_parallel_requests;

    std::thread::spawn(move || {
      let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
      rt.block_on(async move {
        loop {
          let Some(outpoint) = outpoint_receiver.recv().await else {
            log::debug!("Outpoint channel closed");
            return;
          };
          // There's no try_iter on tokio::sync::mpsc::Receiver like std::sync::mpsc::Receiver.
          // So we just loop until BATCH_SIZE doing try_recv until it returns None.
          let mut outpoints = vec![outpoint];
          for _ in 0..BATCH_SIZE - 1 {
            let Ok(outpoint) = outpoint_receiver.try_recv() else {
              break;
            };
            outpoints.push(outpoint);
          }
          // Break outpoints into chunks for parallel requests
          let chunk_size = (outpoints.len() / parallel_requests) + 1;
          let mut futs = Vec::with_capacity(parallel_requests);
          for chunk in outpoints.chunks(chunk_size) {
            let txids = chunk.iter().map(|outpoint| outpoint.txid).collect();
            let fut = fetcher.get_transactions(txids);
            futs.push(fut);
          }
          let txs = match try_join_all(futs).await {
            Ok(txs) => txs,
            Err(e) => {
              log::error!("Couldn't receive txs {e}");
              return;
            }
          };
          // Send all tx output values back in order
          for (i, tx) in txs.iter().flatten().enumerate() {
            let Ok(_) = value_sender
              .send(tx.output[usize::try_from(outpoints[i].vout).unwrap()].value)
              .await
            else {
              log::error!("Value channel closed unexpectedly");
              return;
            };
          }
        }
      })
    });

    Ok((outpoint_sender, value_receiver))
  }

  fn index_block(
    &mut self,
    index: &Index,
    outpoint_sender: &mut Sender<OutPoint>,
    value_receiver: &mut Receiver<u64>,
    wtx: &mut WriteTransaction,
    block: BlockData,
    value_cache: &mut HashMap<OutPoint, OutPointMapValue>,
  ) -> Result<()> {

    Reorg::detect_reorg(&block, self.height, self.index)?;

    let start = Instant::now();
    let mut sat_ranges_written = 0;
    let mut outputs_in_block = 0;

    log::info!(
      "Block {} at {} with {} transactions…",
      self.height,
      timestamp(block.header.time.into()),
      block.txdata.len()
    );

    // If value_receiver still has values something went wrong with the last block
    // Could be an assert, shouldn't recover from this and commit the last block
    let Err(TryRecvError::Empty) = value_receiver.try_recv() else {
      return Err(anyhow!("Previous block did not consume all input values"));
    };

    // A header-only block (below the first inscription height, without the
    // sat index, outside dune indexing) changes nothing but its block hash
    // and the feed journal range; the general path below would open two
    // dozen tables to write that. Genesis takes the general path, so every
    // table exists from the first committed block as before.
    if self.height > 0
      && block.txdata.is_empty()
      && !index.index_sats
      && self.height < index.first_inscription_height
      && !(index.index_dunes && self.height >= index.first_dune_height)
    {
      #[cfg(feature = "tap")]
      let record_feed = !index.tap_lean;
      #[cfg(not(feature = "tap"))]
      let record_feed = true;
      if record_feed {
        dogemap_feed::record_block(wtx, self.height, Vec::new(), index.first_inscription_height)?;
      }
      #[cfg(feature = "tap")]
      {
        let row = InscriptionUpdater::tap_header_row(&block.header, 0)?;
        wtx
          .open_table(TAP_KV)?
          .insert(format!("hdr/{}", self.height).as_bytes(), row.as_slice())?;
      }
      wtx
        .open_table(HEIGHT_TO_BLOCK_HASH)?
        .insert(&self.height, &block.header.block_hash().store())?;
      self.height += 1;
      return Ok(());
    }

    let mut outpoint_to_value = wtx.open_table(OUTPOINT_TO_VALUE)?;
    let mut outpoint_to_address = wtx.open_table(OUTPOINT_TO_ADDRESS)?;
    let mut address_to_outpoint = wtx.open_multimap_table(ADDRESS_TO_OUTPOINT)?;

    let index_inscriptions = self.height >= index.first_inscription_height;

    #[cfg(feature = "tap")]
    let mut tap_prefetched = HashSet::new();

    if index_inscriptions {
      // Lean replay: request input values only for transactions that can move,
      // create or continue an inscription as of the start of this block.
      #[cfg(feature = "tap")]
      let lean_tables = if index.tap_lean {
        Some((
          wtx.open_table(SATPOINT_TO_INSCRIPTION_ID)?,
          wtx.open_table(PARTIAL_TXID_TO_INSCRIPTION_TXIDS)?,
        ))
      } else {
        None
      };

      // Send all missing input outpoints to be fetched right away
      let txids = block
        .txdata
        .iter()
        .map(|(_, txid)| txid)
        .collect::<HashSet<_>>();
      // Relevance also flows forward within the block: a transaction spending
      // an output of an earlier relevant one may receive its inscription, so
      // its other inputs are prefetched too instead of fetched one by one.
      #[cfg(feature = "tap")]
      let mut relevant_in_block: HashSet<Txid> = HashSet::new();
      for (tx, tx_txid) in &block.txdata {
        #[cfg(feature = "tap")]
        if let Some((satpoints, partials)) = &lean_tables {
          let mut relevant = tx
            .input
            .iter()
            .any(|input| relevant_in_block.contains(&input.previous_output.txid));
          for input in &tx.input {
            if relevant {
              break;
            }
            if !input.previous_output.is_null()
              && Index::inscriptions_on_output(satpoints, input.previous_output)?
                .next()
                .is_some()
            {
              relevant = true;
              break;
            }
          }
          if !relevant && !InscriptionUpdater::tap_lean_may_inscribe(partials, tx)? {
            continue;
          }
          relevant_in_block.insert(*tx_txid);
        }
        for input in &tx.input {
          let prev_output = input.previous_output;
          // We don't need coinbase input value
          if prev_output.is_null() {
            continue;
          }
          // We don't need input values from txs earlier in the block, since they'll be added to value_cache
          // when the tx is indexed
          if txids.contains(&prev_output.txid) {
            continue;
          }
          // We don't need input values we already have in our value_cache from earlier blocks
          if value_cache.contains_key(&prev_output) {
            continue;
          }
          // We don't need input values we already have in our outpoint_to_value table from earlier blocks that
          // were committed to db already
          if outpoint_to_value.get(&prev_output.store())?.is_some() {
            continue;
          }
          // A value the lookahead already fetched needs no round trip.
          #[cfg(feature = "tap")]
          if let Some(value) = self.tap_lookahead.lock().unwrap().remove(&prev_output) {
            value_cache.insert(prev_output, (value, [0u8; 34]));
            continue;
          }
          // We don't know the value of this tx input. Send this outpoint to background thread to be fetched
          outpoint_sender.blocking_send(prev_output)?;
          #[cfg(feature = "tap")]
          tap_prefetched.insert(prev_output);
        }
      }
    }

    let mut height_to_block_hash = wtx.open_table(HEIGHT_TO_BLOCK_HASH)?;

    let mut inscription_id_to_inscription_entry =
      wtx.open_table(INSCRIPTION_ID_TO_INSCRIPTION_ENTRY)?;
    let mut inscription_id_to_satpoint = wtx.open_table(INSCRIPTION_ID_TO_SATPOINT)?;
    let mut inscription_id_to_txids = wtx.open_table(INSCRIPTION_ID_TO_TXIDS)?;
    let mut inscription_txid_to_tx = wtx.open_table(INSCRIPTION_TXID_TO_TX)?;
    let mut partial_txid_to_inscription_txids =
      wtx.open_table(PARTIAL_TXID_TO_INSCRIPTION_TXIDS)?;
    let mut inscription_number_to_inscription_id =
      wtx.open_table(INSCRIPTION_NUMBER_TO_INSCRIPTION_ID)?;
    let mut sat_to_inscription_id = wtx.open_table(SAT_TO_INSCRIPTION_ID)?;
    let mut satpoint_to_inscription_id = wtx.open_table(SATPOINT_TO_INSCRIPTION_ID)?;
    let mut statistic_to_count = wtx.open_table(STATISTIC_TO_COUNT)?;
    let mut transaction_id_to_transaction = wtx.open_table(TRANSACTION_ID_TO_TRANSACTION)?;

    let mut drc20_token_info = wtx.open_table(DRC20_TOKEN)?;
    let mut drc20_token_holder = wtx.open_multimap_table(DRC20_TOKEN_HOLDER)?;
    let mut drc20_token_balance = wtx.open_table(DRC20_BALANCES)?;
    let mut drc20_inscribe_transfer = wtx.open_table(DRC20_INSCRIBE_TRANSFER)?;
    let mut drc20_transferable_log = wtx.open_table(DRC20_TRANSFERABLELOG)?;
    let mut drc20_operation_decisions = wtx.open_table(DRC20_OPERATION_DECISIONS)?;

    let mut lost_sats = statistic_to_count
      .get(&Statistic::LostSats.key())?
      .map(|lost_sats| lost_sats.value())
      .unwrap_or(0);

    let feed_journal;

    {
      #[cfg(feature = "tap")]
      let mut tap_kv = wtx.open_table(TAP_KV)?;
      let mut inscription_updater = InscriptionUpdater::new(
        self.height,
        &mut inscription_id_to_satpoint,
        &mut inscription_id_to_txids,
        &mut inscription_txid_to_tx,
        &mut partial_txid_to_inscription_txids,
        value_receiver,
        self.index.index_transactions,
        Vec::new(),
        &mut transaction_id_to_transaction,
        &mut inscription_id_to_inscription_entry,
        lost_sats,
        &mut inscription_number_to_inscription_id,
        &mut outpoint_to_value,
        &mut outpoint_to_address,
        &mut address_to_outpoint,
        &mut sat_to_inscription_id,
        &mut satpoint_to_inscription_id,
        block.header.time,
        value_cache,
        index.chain,
        #[cfg(feature = "tap")]
        &mut tap_kv,
        #[cfg(feature = "tap")]
        index,
      )?;

      #[cfg(feature = "tap")]
      {
        inscription_updater.tap_prefetched = std::mem::take(&mut tap_prefetched);
        let ntx = u32::try_from(block.txdata.len()).unwrap_or(u32::MAX);
        let row = InscriptionUpdater::tap_header_row(&block.header, ntx)?;
        inscription_updater
          .tap_db
          .put(format!("hdr/{}", self.height).as_bytes(), &row);
      }

      if self.index.index_sats {
        let mut sat_to_satpoint = wtx.open_table(SAT_TO_SATPOINT)?;
        let mut outpoint_to_sat_ranges = wtx.open_table(OUTPOINT_TO_SAT_RANGES)?;

        let mut coinbase_inputs = VecDeque::new();

        let h = Height(self.height);
        if h.subsidy() > 0 {
          let start = h.starting_sat();
          coinbase_inputs.push_front((start.n(), (start + h.subsidy()).n()));
          self.sat_ranges_since_flush += 1;
        }

        for (tx_offset, (tx, txid)) in block.txdata.iter().enumerate().skip(1) {
          log::trace!("Indexing transaction {tx_offset}…");

          let mut input_sat_ranges = VecDeque::new();

          for input in &tx.input {
            let key = input.previous_output.store();

            let sat_ranges = match self.range_cache.remove(&key) {
              Some(sat_ranges) => {
                self.outputs_cached += 1;
                sat_ranges
              }
              None => outpoint_to_sat_ranges
                .remove(&key)?
                .ok_or_else(|| {
                  anyhow!("Could not find outpoint {} in index", input.previous_output)
                })?
                .value()
                .to_vec(),
            };

            for chunk in sat_ranges.chunks_exact(11) {
              input_sat_ranges.push_back(SatRange::load(chunk.try_into().unwrap()));
            }
          }

          self.index_transaction_sats(
            tx,
            *txid,
            u32::try_from(tx_offset)?,
            &mut sat_to_satpoint,
            &mut input_sat_ranges,
            &mut sat_ranges_written,
            &mut outputs_in_block,
            &mut inscription_updater,
            index_inscriptions,
          )?;

          coinbase_inputs.extend(input_sat_ranges);
        }

        if let Some((tx, txid)) = block.txdata.first() {
          self.index_transaction_sats(
            tx,
            *txid,
            0,
            &mut sat_to_satpoint,
            &mut coinbase_inputs,
            &mut sat_ranges_written,
            &mut outputs_in_block,
            &mut inscription_updater,
            index_inscriptions,
          )?;
        }

        if !coinbase_inputs.is_empty() {
          let mut lost_sat_ranges = outpoint_to_sat_ranges
            .remove(&OutPoint::null().store())?
            .map(|ranges| ranges.value().to_vec())
            .unwrap_or_default();

          for (start, end) in coinbase_inputs {
            if !Sat(start).is_common() {
              sat_to_satpoint.insert(
                &start,
                &SatPoint {
                  outpoint: OutPoint::null(),
                  offset: lost_sats,
                }
                .store(),
              )?;
            }

            lost_sat_ranges.extend_from_slice(&(start, end).store());

            lost_sats += u64::try_from(end - start).unwrap();
          }

          outpoint_to_sat_ranges.insert(&OutPoint::null().store(), lost_sat_ranges.as_slice())?;
        }
      } else {
        for (tx_index, (tx, txid)) in block
          .txdata
          .iter()
          .enumerate()
          .skip(1)
          .chain(block.txdata.iter().enumerate().take(1))
        {
          lost_sats += inscription_updater.index_transaction_inscriptions(
            tx,
            *txid,
            u32::try_from(tx_index)?,
            None,
          )?;
        }
      }

      feed_journal = std::mem::take(&mut inscription_updater.journal.records);

      // Flush this block's TAP writes inside the block's write transaction.
      #[cfg(feature = "tap")]
      inscription_updater.tap_finalize_block()?;

      if index.index_drc20 && self.height >= index.first_inscription_height {
        let operations = inscription_updater.operations.clone();

        // Decisions are retained from the first block this binary indexes
        // with the table open. Recording the height here, in the block's own
        // write transaction, lets an upgraded database report where its
        // decision coverage starts without a rebuild.
        if statistic_to_count
          .get(&Statistic::Drc20DecisionsFromHeight.key())?
          .is_none()
        {
          statistic_to_count.insert(
            &Statistic::Drc20DecisionsFromHeight.key(),
            &u64::from(self.height),
          )?;
        }
        let reorg_epoch = statistic_to_count
          .get(&Statistic::Reorgs.key())?
          .map(|value| value.value())
          .unwrap_or(0);

        // Create a protocol manager to index the block of drc20 data.
        Drc20Updater::new(
          &mut drc20_token_info,
          &mut drc20_token_holder,
          &mut drc20_token_balance,
          &mut drc20_inscribe_transfer,
          &mut drc20_transferable_log,
          &mut drc20_operation_decisions,
          &inscription_id_to_inscription_entry,
          &mut transaction_id_to_transaction,
          index,
        )?
        .index_block(
          BlockContext {
            network: index.chain.network(),
            blockheight: u64::from(self.height),
            blockhash: block.header.block_hash(),
            blocktime: block.header.time,
            reorg_epoch,
          },
          &block,
          operations,
        )?;
      }

      statistic_to_count.insert(&Statistic::LostSats.key(), &lost_sats)?;
    }

    if index.index_dunes && self.height >= self.index.first_dune_height {
      let mut outpoint_to_dune_balances = wtx.open_table(OUTPOINT_TO_DUNE_BALANCES)?;
      let mut dune_id_to_dune_entry = wtx.open_table(DUNE_ID_TO_DUNE_ENTRY)?;
      let mut dune_to_dune_id = wtx.open_table(DUNE_TO_DUNE_ID)?;
      let mut inscription_id_to_dune = wtx.open_table(INSCRIPTION_ID_TO_DUNE)?;
      let mut dune_updater = DuneUpdater::new(
        self.height,
        &mut outpoint_to_dune_balances,
        &mut dune_id_to_dune_entry,
        &inscription_id_to_inscription_entry,
        &mut inscription_id_to_dune,
        &mut dune_to_dune_id,
        &mut statistic_to_count,
        block.header.time,
        Dune::minimum_at_height(Chain::Mainnet, Height(self.height)),
      )?;
      for (i, (tx, txid)) in block.txdata.iter().enumerate() {
        dune_updater.index_dunes(i, tx, *txid)?;
      }
    }

    // Dogemap feed journal: this block's location records and the journal
    // range, in the same write transaction as the block hash, so a committed
    // block always has its complete journal and a rolled-back one has none.
    // Independent of the DRC-20 and Dunes flags, which only read their own
    // copies of the updater's operations.
    // Lean TAP replay serves no Dogemap feed; its journal is write volume only.
    #[cfg(feature = "tap")]
    let record_feed = !index.tap_lean;
    #[cfg(not(feature = "tap"))]
    let record_feed = true;
    if record_feed {
      dogemap_feed::record_block(
        wtx,
        self.height,
        feed_journal,
        index.first_inscription_height,
      )?;
    }

    height_to_block_hash.insert(&self.height, &block.header.block_hash().store())?;

    self.height += 1;
    self.outputs_traversed += outputs_in_block;

    log::info!(
      "Wrote {sat_ranges_written} sat ranges from {outputs_in_block} outputs in {} ms",
      (Instant::now() - start).as_millis(),
    );

    Ok(())
  }

  fn index_transaction_sats(
    &mut self,
    tx: &Transaction,
    txid: Txid,
    tx_index: u32,
    sat_to_satpoint: &mut Table<u64, &SatPointValue>,
    input_sat_ranges: &mut VecDeque<(u64, u64)>,
    sat_ranges_written: &mut u64,
    outputs_traversed: &mut u64,
    inscription_updater: &mut InscriptionUpdater,
    index_inscriptions: bool,
  ) -> Result {
    if index_inscriptions {
      inscription_updater.index_transaction_inscriptions(
        tx,
        txid,
        tx_index,
        Some(input_sat_ranges),
      )?;
    }

    for (vout, output) in tx.output.iter().enumerate() {
      let outpoint = OutPoint {
        vout: vout.try_into().unwrap(),
        txid,
      };
      let mut sats = Vec::new();

      let mut remaining = output.value;
      while remaining > 0 {
        let range = input_sat_ranges
          .pop_front()
          .ok_or_else(|| anyhow!("insufficient inputs for transaction outputs"))?;

        if !Sat(range.0).is_common() {
          sat_to_satpoint.insert(
            &range.0,
            &SatPoint {
              outpoint,
              offset: output.value - remaining,
            }
            .store(),
          )?;
        }

        let count = u64::try_from(range.1 - range.0).unwrap();

        let assigned = if count > remaining {
          self.sat_ranges_since_flush += 1;
          let middle = range.0 + remaining;
          input_sat_ranges.push_front((middle, range.1));
          (range.0, middle)
        } else {
          range
        };

        sats.extend_from_slice(&assigned.store());

        remaining -= u64::try_from(assigned.1 - assigned.0).unwrap();

        *sat_ranges_written += 1;
      }

      *outputs_traversed += 1;

      self.range_cache.insert(outpoint.store(), sats);
      self.outputs_inserted_since_flush += 1;
    }

    Ok(())
  }

  fn commit(
    &mut self,
    wtx: WriteTransaction,
    value_cache: HashMap<OutPoint, OutPointMapValue>,
  ) -> Result {
    log::info!(
      "Committing at block height {}, {} outputs traversed, {} in map, {} cached",
      self.height,
      self.outputs_traversed,
      self.range_cache.len(),
      self.outputs_cached
    );

    if self.index.index_sats {
      log::info!(
        "Flushing {} entries ({:.1}% resulting from {} insertions) from memory to database",
        self.range_cache.len(),
        self.range_cache.len() as f64 / self.outputs_inserted_since_flush as f64 * 100.,
        self.outputs_inserted_since_flush,
      );

      let mut outpoint_to_sat_ranges = wtx.open_table(OUTPOINT_TO_SAT_RANGES)?;

      for (outpoint, sat_range) in self.range_cache.drain() {
        outpoint_to_sat_ranges.insert(&outpoint, sat_range.as_slice())?;
      }

      self.outputs_inserted_since_flush = 0;
    }

    {
      let mut outpoint_to_value = wtx.open_table(OUTPOINT_TO_VALUE)?;
      let mut outpoint_to_address = wtx.open_table(OUTPOINT_TO_ADDRESS)?;
      let mut address_to_outpoint = wtx.open_multimap_table(ADDRESS_TO_OUTPOINT)?;

      #[cfg(feature = "tap")]
      let address_index = !self.index.tap_lean;
      #[cfg(not(feature = "tap"))]
      let address_index = true;

      for (outpoint, map) in value_cache {
        #[cfg(feature = "tap")]
        if self.index.tap_lean && map.1[0] == 0 {
          continue;
        }
        outpoint_to_value.insert(&outpoint.store(), map.0)?;
        if address_index && map.1 != [0u8; 34] {
          outpoint_to_address.insert(&outpoint.store(), map.1.as_slice())?;
          address_to_outpoint.insert(map.1.as_slice(), &outpoint.store())?;
        }
      }
    }

    Index::increment_statistic(&wtx, Statistic::OutputsTraversed, self.outputs_traversed)?;
    self.outputs_traversed = 0;
    Index::increment_statistic(&wtx, Statistic::SatRanges, self.sat_ranges_since_flush)?;
    self.sat_ranges_since_flush = 0;
    Index::increment_statistic(&wtx, Statistic::Commits, 1)?;

    wtx.commit()?;

    Reorg::update_savepoints(self.index, self.height)?;

    Ok(())
  }
}

/// One-off lean replay maintenance (`--tap-prune`): drops index rows the lean
/// replay never reads, most of them written before lean filtering existed.
/// Stored values are refetched over RPC when needed; content is kept only for
/// TAP-readable inscriptions and unfinished partial chains, matching what lean
/// replay stores today.
#[cfg(feature = "tap")]
pub(crate) fn tap_prune(database: &Database) -> Result {
  use std::collections::HashSet;

  let mut wtx = database.begin_write()?;
  wtx.set_durability(redb::Durability::Immediate);

  {
    let mut tap_kv = wtx.open_table(TAP_KV)?;
    let numbers = wtx.open_table(INSCRIPTION_NUMBER_TO_INSCRIPTION_ID)?;
    let after_last = numbers
      .last()?
      .map(|(number, _)| number.value() + 1)
      .unwrap_or(0);
    let stored = tap_kv
      .get(inscription_updater::TAP_LEAN_NEXT_NUMBER_KEY)?
      .and_then(|value| <[u8; 8]>::try_from(value.value()).ok())
      .map(u64::from_le_bytes)
      .unwrap_or(0);
    let next = stored.max(after_last);
    tap_kv.insert(
      inscription_updater::TAP_LEAN_NEXT_NUMBER_KEY,
      next.to_le_bytes().as_slice(),
    )?;
    log::info!("tap-prune: next inscription number {next} (stored {stored}, table {after_last})");
  }

  let removed_values = wtx.open_table(OUTPOINT_TO_VALUE)?.len()?;
  wtx.delete_table(OUTPOINT_TO_VALUE)?;
  wtx.open_table(OUTPOINT_TO_VALUE)?;
  log::info!("tap-prune: dropped {removed_values} stored output values");

  let mut referenced: HashSet<[u8; 32]> = HashSet::new();
  {
    let partials = wtx.open_table(PARTIAL_TXID_TO_INSCRIPTION_TXIDS)?;
    for row in partials.iter()? {
      let (key, txids) = row?;
      if let Ok(key) = <[u8; 32]>::try_from(key.value()) {
        referenced.insert(key);
      }
      for chunk in txids.value().chunks_exact(32) {
        referenced.insert(chunk.try_into().unwrap());
      }
    }
  }

  let mut drop_ids: Vec<InscriptionIdValue> = Vec::new();
  let mut kept_ids: HashSet<InscriptionIdValue> = HashSet::new();
  {
    let id_to_txids = wtx.open_table(INSCRIPTION_ID_TO_TXIDS)?;
    let txid_to_tx = wtx.open_table(INSCRIPTION_TXID_TO_TX)?;
    for row in id_to_txids.iter()? {
      let (id, txids) = row?;
      let mut txs = Vec::new();
      let mut complete = true;
      for chunk in txids.value().chunks_exact(32) {
        match txid_to_tx.get(chunk)? {
          Some(tx) => txs.push(bitcoin::Transaction::consensus_decode(
            &mut std::io::Cursor::new(tx.value()),
          )?),
          None => {
            complete = false;
            break;
          }
        }
      }
      let keep = complete
        && matches!(
          Inscription::from_transactions(txs),
          ParsedInscription::Complete(inscription)
            if InscriptionUpdater::tap_lean_keeps_content(&inscription)
        );
      if keep {
        kept_ids.insert(*id.value());
        for chunk in txids.value().chunks_exact(32) {
          referenced.insert(chunk.try_into().unwrap());
        }
      } else {
        drop_ids.push(*id.value());
      }
    }
  }
  {
    let mut id_to_txids = wtx.open_table(INSCRIPTION_ID_TO_TXIDS)?;
    for id in &drop_ids {
      id_to_txids.remove(id)?;
    }
  }
  log::info!(
    "tap-prune: kept content of {} inscriptions, dropped {}",
    kept_ids.len(),
    drop_ids.len()
  );
  drop(drop_ids);

  let mut drop_txids: Vec<Vec<u8>> = Vec::new();
  {
    let txid_to_tx = wtx.open_table(INSCRIPTION_TXID_TO_TX)?;
    for row in txid_to_tx.iter()? {
      let (txid, _) = row?;
      let keep = <[u8; 32]>::try_from(txid.value())
        .map(|txid| referenced.contains(&txid))
        .unwrap_or(false);
      if !keep {
        drop_txids.push(txid.value().to_vec());
      }
    }
  }
  {
    let mut txid_to_tx = wtx.open_table(INSCRIPTION_TXID_TO_TX)?;
    for txid in &drop_txids {
      txid_to_tx.remove(txid.as_slice())?;
    }
  }
  log::info!("tap-prune: dropped {} stored transactions", drop_txids.len());
  drop(drop_txids);

  let mut drop_numbers: Vec<u64> = Vec::new();
  {
    let numbers = wtx.open_table(INSCRIPTION_NUMBER_TO_INSCRIPTION_ID)?;
    for row in numbers.iter()? {
      let (number, id) = row?;
      if !kept_ids.contains(id.value()) {
        drop_numbers.push(number.value());
      }
    }
  }
  {
    let mut numbers = wtx.open_table(INSCRIPTION_NUMBER_TO_INSCRIPTION_ID)?;
    for number in &drop_numbers {
      numbers.remove(number)?;
    }
  }
  log::info!("tap-prune: dropped {} inscription numbers", drop_numbers.len());

  wtx.commit()?;
  Ok(())
}

/// One-off lean replay maintenance (`--tap-restore-values <BACKUP>`): copies
/// stored output values back from an earlier copy of the index, so inputs
/// spent after `--tap-prune` do not each cost a node round trip. Values of
/// outputs spent since the backup stay as unused rows.
#[cfg(feature = "tap")]
pub(crate) fn tap_restore_values(database: &Database, backup: &Path) -> Result {
  let backup = Database::builder()
    .set_cache_size(1 << 30)
    .open(backup)
    .with_context(|| format!("failed to open backup index {}", backup.display()))?;
  let rtx = backup.begin_read()?;
  let source = rtx.open_table(OUTPOINT_TO_VALUE)?;
  let mut copied = 0u64;
  let mut batch: Vec<(OutPointValue, u64)> = Vec::with_capacity(1_000_000);
  let mut flush = |batch: &mut Vec<(OutPointValue, u64)>| -> Result {
    let mut wtx = database.begin_write()?;
    wtx.set_durability(redb::Durability::Immediate);
    {
      let mut values = wtx.open_table(OUTPOINT_TO_VALUE)?;
      for (outpoint, value) in batch.iter() {
        values.insert(outpoint, value)?;
      }
    }
    wtx.commit()?;
    batch.clear();
    Ok(())
  };
  for row in source.iter()? {
    let (outpoint, value) = row?;
    batch.push((*outpoint.value(), value.value()));
    copied += 1;
    if batch.len() == 1_000_000 {
      flush(&mut batch)?;
      log::info!("tap-restore-values: {copied} values copied");
    }
  }
  flush(&mut batch)?;
  log::info!("tap-restore-values: done, {copied} values copied");
  Ok(())
}
