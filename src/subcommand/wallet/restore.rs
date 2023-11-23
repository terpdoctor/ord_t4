use super::*;

#[derive(Debug, Parser)]
pub(crate) struct Restore {
  #[arg(help = "Restore wallet from <MNEMONIC>")]
  mnemonic: Mnemonic,
  #[arg(
    long,
    default_value = "",
    help = "Use <PASSPHRASE> when deriving wallet"
  )]
  pub(crate) passphrase: String,
  #[arg(long, value_enum, default_value="bech32m")]
  pub(crate) address_type: AddressType,
  #[arg(long, help = "Restore from an ordinalswallet seed phrase. This will break most things, but might be useful rarely.")]
  pub(crate) ordinalswallet: bool,
}

impl Restore {
  pub(crate) fn run(self, options: Options) -> SubcommandResult {
    initialize_wallet(&options, self.mnemonic.to_seed(self.passphrase), self.address_type, self.ordinalswallet)?;
    Ok(Box::new(Empty {}))
  }
}
