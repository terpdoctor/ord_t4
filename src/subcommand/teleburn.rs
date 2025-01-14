use {super::*, crate::index::entry::Entry};
use base58::ToBase58;

#[derive(Debug, Parser)]
pub(crate) struct Teleburn {
@@ -8,11 +9,15 @@ pub(crate) struct Teleburn {
#[derive(Debug, PartialEq, Serialize)]
pub struct Output {
  ethereum: EthereumTeleburnAddress,
  solana: SolanaTeleburnAddress,
}

#[derive(Debug, PartialEq)]
struct EthereumTeleburnAddress([u8; 20]);

#[derive(Debug, PartialEq)]
struct SolanaTeleburnAddress([u8; 32]);

impl Serialize for EthereumTeleburnAddress {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
@@ -34,11 +39,29 @@ impl Display for EthereumTeleburnAddress {
  }
}

impl Serialize for SolanaTeleburnAddress {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    serializer.collect_str(self)
  }
}

impl Display for SolanaTeleburnAddress {
  fn fmt(&self, f: &mut Formatter) -> fmt::Result {
    write!(f, "{}", self.0.to_base58())?;

    Ok(())
  }
}

impl Teleburn {
  pub(crate) fn run(self) -> Result {
    let digest = bitcoin::hashes::sha256::Hash::hash(&self.recipient.store());
    print_json(Output {
      ethereum: EthereumTeleburnAddress(digest[0..20].try_into().unwrap()),
      solana: SolanaTeleburnAddress(digest[0..32].try_into().unwrap()),
    })?;
    Ok(())
  }
