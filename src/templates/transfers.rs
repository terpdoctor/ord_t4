use super::*;

#[derive(Boilerplate)]
pub(crate) struct TransfersHtml {
  height: Height,
  data: Vec<(InscriptionId, Vec<(String, SatPoint, String, SatPoint)>)>,
}

impl TransfersHtml {
  pub(crate) fn new(
    height: Height,
    data: Vec<(InscriptionId, Vec<(String, SatPoint, String, SatPoint)>)>,
  ) -> Self {
    Self {
      height,
      data,
    }
  }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct TransferJson {
  from_address: String,
  from_satpoint: SatPoint,
  to_address: String,
  to_satpoint: SatPoint,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct TransfersJson {
  inscriptionid: InscriptionId,
  transfers: Vec<TransferJson>,
}

impl TransfersJson {
  pub(crate) fn new(
    data: (InscriptionId, Vec<(String, SatPoint, String, SatPoint)>),
  ) -> Self {
    Self {
      inscriptionid: data.0,
      transfers: data.1.iter().map(|transfer| TransferJson {
        from_address: transfer.0.clone(),
        from_satpoint: transfer.1,
        to_address: transfer.2.clone(),
        to_satpoint: transfer.3,
      }).collect()
    }
  }
}

impl PageContent for TransfersHtml {
  fn title(&self) -> String {
    format!("Transfers for block {}", self.height)
  }
}
