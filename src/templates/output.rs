use super::*;
use crate::relics::spaced_relic::SpacedRelic;

#[derive(Boilerplate)]
pub(crate) struct OutputHtml {
  pub(crate) outpoint: OutPoint,
  pub(crate) list: Option<List>,
  pub(crate) chain: Chain,
  pub(crate) output: TxOut,
  pub(crate) inscriptions: Vec<InscriptionId>,
  pub(crate) relics: BTreeMap<SpacedRelic, Pile>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct AddressOutputJson {
  pub(crate) outpoint: Vec<OutPoint>,
}

impl AddressOutputJson {
  pub fn new(outputs: Vec<OutPoint>) -> Self {
    Self { outpoint: outputs }
  }
}

impl PageContent for OutputHtml {
  fn title(&self) -> String {
    format!("Output {}", self.outpoint)
  }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OutputJson {
  pub address: Option<String>,
  pub inscriptions: Vec<InscriptionDecodedHtml>,
  #[serde(rename = "bones")]
  pub relics: BTreeMap<SpacedRelic, Pile>,
  pub script_pubkey: String,
  pub transaction: String,
  pub output: String,
  pub value: u64,
}

impl OutputJson {
  pub fn new(
    chain: Chain,
    inscriptions: Vec<InscriptionDecodedHtml>,
    outpoint: OutPoint,
    output: TxOut,
    relics: BTreeMap<SpacedRelic, Pile>,
  ) -> Self {
    Self {
      address: chain
        .address_from_script(&output.script_pubkey)
        .ok()
        .map(|address| address.to_string()),
      inscriptions,
      relics,
      script_pubkey: output.script_pubkey.asm(),
      transaction: outpoint.txid.to_string(),
      output: outpoint
        .txid
        .to_string()
        .add(':'.to_string().as_str())
        .add(outpoint.vout.to_string().as_str()),
      value: output.value,
    }
  }
}
