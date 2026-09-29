use {
  super::*,
  bitcoin::hashes::{sha256d},
};

pub(crate) struct Rtx(pub(crate) redb::ReadTransaction);

// IMPLEMENTATION-HANDOFF [P-01] FEED-SNAPSHOT; P-C01/P-C02/P-C05, P-F01/P-F05.
// 1. Extend this existing read-transaction owner with P-02 feed table readers:
//    block identity/parent, coverage/watermark, databaseId, epoch, dense ordered
//    events and body metadata. Pass &self throughout so table reads share root.
// 2. Read Statistic::Reorgs from the same transaction; it already survives
//    savepoint rollback. Combine it with databaseId/profile/schema, because a
//    replaced/rebuilt database can otherwise reuse numeric epochs and cursors.
// 3. Read no historical location from a current-state table. Validate manifest
//    cardinality/order/digest and return explicit unavailable/corrupt errors.
// 4. Test concurrent writer snapshots and restored manifests in PROPOSED
//    dogemap-feed-contract; redb 2.6.3 source P-S03 supports shared transaction
//    roots. Do not hold database transactions across remote HTTP calls.
//    Full contract/commands/rollback: docs/preparation-dogemap/work-packages.md.
impl Rtx {
  pub(crate) fn height(&self) -> Result<Option<Height>> {
    Ok(
      self
        .0
        .open_table(HEIGHT_TO_BLOCK_HASH)?
        .range(0..)?
        .rev()
        .next()
        .map(|result| {
          result.map(|(height, _hash)| Height(height.value()))
        })
        .transpose()? // Converts Option<Result<T, E>> to Result<Option<T>, E>
    )
  }

  pub(crate) fn block_count(&self) -> Result<u32> {
    Ok(
      self
        .0
        .open_table(HEIGHT_TO_BLOCK_HASH)?
        .range(0..)?
        .rev()
        .next()
        .map(|result| {
          result.map(|(height, _hash)| height.value() + 1)
        })
        .transpose()?  // Converts Option<Result<T, E>> to Result<Option<T>, E> and propagates error if any
        .unwrap_or(0),
    )
  }


  pub(crate) fn block_hash(&self, height: Option<u32>) -> Result<Option<BlockHash>> {
    let height_to_block_header = self.0.open_table(HEIGHT_TO_BLOCK_HASH)?;

    Ok(
      match height {
        Some(height) => height_to_block_header.get(height)?
          .map(|header| {
            let block_hash_value = header.value().clone();
            let sha256d_hash = sha256d::Hash::from_slice(&block_hash_value)
              .expect("Invalid block hash");
            BlockHash::from(sha256d_hash)
          }),
        None => height_to_block_header
          .range(0..)?
          .next_back()
          .transpose()?
          .map(|(_height, header)| {
            let block_hash_value = header.value().clone();
            let sha256d_hash = sha256d::Hash::from_slice(&block_hash_value)
              .expect("Invalid block hash");
            BlockHash::from(sha256d_hash)
          }),
      }
    )
  }
}
