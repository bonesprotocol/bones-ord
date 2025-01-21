use {
  super::*,
  crate::{
    charm::Charm,
    relics::{SpacedRelic, SyndicateId},
    sat::Sat,
    sat_point::SatPoint,
  },
  ciborium::Value,
  serde::{Deserialize, Serialize},
};

#[derive(Boilerplate, Default, Deserialize, Serialize)]
pub(crate) struct InscriptionHtml {
  pub(crate) chain: Chain,
  pub(crate) genesis_fee: u64,
  pub(crate) genesis_height: u32,
  pub(crate) inscription: Inscription,
  pub(crate) inscription_id: InscriptionId,
  pub(crate) inscription_number: u64,
  pub(crate) next: Option<InscriptionId>,
  pub(crate) output: TxOut,
  pub(crate) previous: Option<InscriptionId>,
  pub(crate) sat: Option<Sat>,
  pub(crate) satpoint: SatPoint,
  pub(crate) timestamp: DateTime<Utc>,
  #[serde(rename = "bone_claimed")]
  pub(crate) relic_sealed: Option<SpacedRelic>,
  #[serde(rename = "bone_deployed")]
  pub(crate) relic_enshrined: bool,
  pub(crate) syndicate: Option<SyndicateId>,
  pub(crate) charms: u16,
  pub(crate) child_count: u64,
  pub(crate) children: Vec<InscriptionId>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct InscriptionDecodedHtml {
  pub(crate) chain: Chain,
  pub(crate) genesis_fee: u64,
  pub(crate) genesis_height: u32,
  pub(crate) inscription: InscriptionDecoded,
  pub(crate) inscription_id: InscriptionId,
  pub(crate) inscription_number: u64,
  pub(crate) next: Option<InscriptionId>,
  pub(crate) output: Option<TxOut>,
  pub(crate) previous: Option<InscriptionId>,
  pub(crate) sat: Option<Sat>,
  pub(crate) satpoint: SatPoint,
  pub(crate) timestamp: DateTime<Utc>,
  #[serde(rename = "bone_claimed")]
  pub(crate) relic_sealed: Option<SpacedRelic>,
  #[serde(rename = "bone_deployed")]
  pub(crate) relic_enshrined: bool,
  pub(crate) syndicate: Option<SyndicateId>,
  pub(crate) charms: Vec<String>,
  pub(crate) child_count: u64,
  pub(crate) children: Vec<InscriptionId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub(crate) struct InscriptionDecoded {
  pub(crate) body: Option<Vec<u8>>,
  pub(crate) content_type: Option<String>,
  pub(crate) delegate: Option<InscriptionId>,
  pub(crate) metadata: Option<Value>,
  pub(crate) parents: Vec<InscriptionId>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct RelicShibescriptionJson {
  pub(crate) is_bonestone: bool,
  #[serde(rename = "bone_claimed")]
  pub(crate) relic_sealed: Option<SpacedRelic>,
  #[serde(rename = "bone_deployed")]
  pub(crate) relic_enshrined: bool,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct ShibescriptionJson {
  pub(crate) chain: Chain,
  pub(crate) genesis_fee: u64,
  pub(crate) genesis_height: u32,
  pub(crate) inscription: Inscription,
  pub(crate) inscription_id: InscriptionId,
  pub(crate) inscription_number: u64,
  pub(crate) next: Option<InscriptionId>,
  pub(crate) output: Option<TxOut>,
  pub(crate) address: Option<String>,
  pub(crate) previous: Option<InscriptionId>,
  pub(crate) sat: Option<Sat>,
  pub(crate) satpoint: SatPoint,
  pub(crate) timestamp: DateTime<Utc>,
  #[serde(rename = "bone_claimed")]
  pub(crate) relic_sealed: Option<SpacedRelic>,
  #[serde(rename = "bone_deployed")]
  pub(crate) relic_enshrined: bool,
  pub(crate) syndicate: Option<SyndicateId>,
  pub(crate) charms: Vec<String>,
  pub(crate) child_count: u64,
  pub(crate) children: Vec<InscriptionId>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct InscriptionJson {
  pub tx_id: String,
  pub vout: u32,
  pub content: Option<Vec<u8>>,
  pub content_length: Option<usize>,
  pub content_type: Option<String>,
  pub genesis_height: u32,
  pub inscription_id: InscriptionId,
  pub inscription_number: u64,
  pub timestamp: u32,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct InscriptionByAddressJson {
  pub utxo: Utxo,
  pub content: Option<String>,
  pub content_length: Option<usize>,
  pub content_type: Option<String>,
  pub genesis_height: u32,
  pub inscription_id: InscriptionId,
  pub inscription_number: u64,
  pub timestamp: u32,
  pub offset: u64,
}

impl PageContent for InscriptionHtml {
  fn title(&self) -> String {
    format!("Shibescription {}", self.inscription_number)
  }

  fn preview_image_url(&self) -> Option<Trusted<String>> {
    Some(Trusted(format!("/content/{}", self.inscription_id)))
  }
}
