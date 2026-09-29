use {
  super::*,
  clap::builder::styling::{AnsiColor, Effects, Styles},
};
use crate::subcommand::SubcommandResult;

#[derive(Debug, Parser)]
#[command(
version,
styles = Styles::styled()
.header(AnsiColor::Green.on_default() | Effects::BOLD)
.usage(AnsiColor::Green.on_default() | Effects::BOLD)
.literal(AnsiColor::Blue.on_default() | Effects::BOLD)
.placeholder(AnsiColor::Cyan.on_default()))
]
pub(crate) struct Arguments {
  #[command(flatten)]
  pub(crate) options: Options,
  #[command(subcommand)]
  pub(crate) subcommand: Subcommand,
}
impl Arguments {
  pub(crate) fn run(self) -> SubcommandResult {
    // Dogecoin Core 1.14.9 defines mainnet, testnet and regtest only. A
    // signet flag would select Bitcoin parameters no Dogecoin node serves.
    if self.options.chain() == Chain::Signet {
      bail!("signet does not exist for Dogecoin; use --chain mainnet, testnet or regtest");
    }
    self.subcommand.run(self.options)
  }
}
