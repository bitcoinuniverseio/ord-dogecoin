use {super::*, updater::BlockData};

// hack
// hack end

#[derive(Debug, PartialEq)]
pub(crate) enum ReorgError {
  Recoverable { height: u32, depth: u32 },
  Unrecoverable,
}

impl fmt::Display for ReorgError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      ReorgError::Recoverable { height, depth } => {
        write!(f, "{depth} block deep reorg detected at height {height}")
      }
      ReorgError::Unrecoverable => write!(f, "unrecoverable reorg detected"),
    }
  }
}

impl std::error::Error for ReorgError {}

const MAX_SAVEPOINTS: u32 = 5;
const SAVEPOINT_INTERVAL: u32 = 10;
const CHAIN_TIP_DISTANCE: u32 = 25;

pub(crate) struct Reorg {}

impl Reorg {
  pub(crate) fn detect_reorg(block: &BlockData, height: u32, index: &Index) -> Result {
    let bitcoind_prev_blockhash = block.header.prev_blockhash;

    match index.block_hash(height.checked_sub(1))? {
      Some(index_prev_blockhash) if index_prev_blockhash == bitcoind_prev_blockhash => Ok(()),
      Some(index_prev_blockhash) if index_prev_blockhash != bitcoind_prev_blockhash => {

        let max_recoverable_reorg_depth =
          (MAX_SAVEPOINTS - 1) * SAVEPOINT_INTERVAL + height % SAVEPOINT_INTERVAL;

        for depth in 1..max_recoverable_reorg_depth {
          let index_block_hash = index.block_hash(height.checked_sub(depth))?;
          let bitcoind_block_hash = index
            .client()
            .get_block_hash(u64::from(height.saturating_sub(depth)))
            .into_option()?;

          if index_block_hash == bitcoind_block_hash {
            return Err(anyhow!(ReorgError::Recoverable { height, depth }));
          }
        }

        Err(anyhow!(ReorgError::Unrecoverable))
      }
      _ => Ok(()),
    }
  }

  /// Restore the oldest savepoint. Every table, the Dogemap feed journal and
  /// its journal range included, returns to the savepoint's contents, so the
  /// feed serves the rolled-back chain exactly as it was then; the new value
  /// of `Statistic::Reorgs` is the feed's `reorgEpoch` and invalidates every
  /// earlier cursor. The feed's database id and creation coverage start are
  /// the generation, not chain state, so they are carried across the restore.
  pub(crate) fn handle_reorg(index: &Index, height: u32, depth: u32) -> Result {
    log::info!("rolling back database after reorg of depth {depth} at height {height}");

    // Read from a separate read transaction: redb stages the root of every
    // table opened in a write transaction when the handle is dropped, and
    // restore_savepoint does not discard staged roots, so a table opened in
    // `wtx` before the restore would keep its pre-restore contents.
    let feed_generation = dogemap_feed::preserve_meta(&index.database.begin_read()?)?;

    let mut wtx = index.begin_write()?;

    // Read the rollback counter before the restore replaces every table with
    // the savepoint's contents, so the counter keeps growing across rollbacks
    // instead of being reset to whatever the savepoint held.
    let reorgs_before = wtx
      .open_table(STATISTIC_TO_COUNT)?
      .get(&Statistic::Reorgs.key())?
      .map(|value| value.value())
      .unwrap_or(0);

    // No savepoint, or one taken above the fork, cannot roll back this reorg:
    // report it as unrecoverable (aborting `wtx` discards the restore) instead
    // of panicking the index thread or restoring the same savepoint forever.
    let Some(oldest) = wtx.list_persistent_savepoints()?.min() else {
      log::warn!("no savepoint to roll back a reorg of depth {depth} at height {height}");
      wtx.abort()?;
      return Err(anyhow!(ReorgError::Unrecoverable));
    };

    wtx.restore_savepoint(&wtx.get_persistent_savepoint(oldest)?)?;

    let restored_block_count = wtx
      .open_table(HEIGHT_TO_BLOCK_HASH)?
      .range(0..)?
      .next_back()
      .transpose()?
      .map(|(height, _hash)| height.value() + 1)
      .unwrap_or(0);
    let first_replaced_height = height.saturating_sub(depth) + 1;
    if restored_block_count > first_replaced_height {
      log::warn!(
        "oldest savepoint (block count {restored_block_count}) is above the fork of a reorg of depth {depth} at height {height}"
      );
      wtx.abort()?;
      return Err(anyhow!(ReorgError::Unrecoverable));
    }

    wtx
      .open_table(STATISTIC_TO_COUNT)?
      .insert(&Statistic::Reorgs.key(), &(reorgs_before + 1))?;
    dogemap_feed::reinstate_meta(&wtx, feed_generation)?;

    Index::increment_statistic(&wtx, Statistic::Commits, 1)?;
    wtx.commit()?;

    log::info!(
      "successfully rolled back database to height {}",
      index.block_count()?
    );

    Ok(())
  }

  /// Whether the updater must commit after indexing up to block count
  /// `height`, so that `update_savepoints` can take a savepoint there.
  /// `starting_height` is the node's block count plus one when the update
  /// began.
  pub(crate) fn savepoint_due(height: u32, starting_height: u32) -> bool {
    height >= SAVEPOINT_INTERVAL
      && height % SAVEPOINT_INTERVAL == 0
      && starting_height.saturating_sub(height) <= CHAIN_TIP_DISTANCE
  }

  pub(crate) fn update_savepoints(index: &Index, height: u32) -> Result {
    if (height < SAVEPOINT_INTERVAL || height % SAVEPOINT_INTERVAL == 0)
      && u32::try_from(
      index
          .client()
          .get_block_count()?
        )
        .unwrap()
        .saturating_sub(height)
      <= CHAIN_TIP_DISTANCE
    {
      let wtx = index.begin_write()?;

      let savepoints = wtx.list_persistent_savepoints()?.collect::<Vec<u64>>();

      if savepoints.len() >= usize::try_from(MAX_SAVEPOINTS).unwrap() {
        wtx.delete_persistent_savepoint(savepoints.into_iter().min().unwrap())?;
      }

      Index::increment_statistic(&wtx, Statistic::Commits, 1)?;
      wtx.commit()?;

      let wtx = index.begin_write()?;

      log::info!("creating savepoint at height {}", height);
      wtx.persistent_savepoint()?;

      Index::increment_statistic(&wtx, Statistic::Commits, 1)?;
      wtx.commit()?;
    }

    Ok(())
  }
}
