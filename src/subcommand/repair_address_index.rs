use super::*;

#[derive(Debug, Parser)]
pub(crate) struct RepairAddressIndex {
  #[arg(long, default_value_t = 1_000)]
  address_batch_size: usize,
  #[arg(long)]
  compact: bool,
}

impl RepairAddressIndex {
  pub(crate) fn run(self, options: Options) -> SubcommandResult {
    let mut index = Index::open(&options)?;
    Ok(Box::new(
      index.repair_address_index(self.address_batch_size, self.compact)?,
    ))
  }
}
