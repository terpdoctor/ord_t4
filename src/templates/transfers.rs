use super::*;

#[derive(Boilerplate)]
pub(crate) struct TransfersHtml {
  height: Height,
  data: Vec<(InscriptionId, Vec<(String, OutPoint)>)>,
}

impl TransfersHtml {
  pub(crate) fn new(
    height: Height,
    data: Vec<(InscriptionId, Vec<(String, OutPoint)>)>,
  ) -> Self {
    Self {
      height,
      data,
    }
  }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct TransferJson {
  address: String,
  outpoint: OutPoint,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct TransfersJson {
  inscriptionid: InscriptionId,
  transfers: Vec<TransferJson>,
}

impl TransfersJson {
  pub(crate) fn new(
    data: (InscriptionId, Vec<(String, OutPoint)>),
  ) -> Self {
    Self {
      inscriptionid: data.0,
      transfers: data.1.iter().map(|transfer| TransferJson {
        address: transfer.0.clone(),
        outpoint: transfer.1,
      }).collect()
    }
  }
}

impl PageContent for TransfersHtml {
  fn title(&self) -> String {
    format!("Transfers for block {}", self.height)
  }
}
