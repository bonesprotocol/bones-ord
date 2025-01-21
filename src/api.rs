use {
  crate::{
    charm::Charm,
    inscription_id::InscriptionId,
    relics::{SpacedRelic, SyndicateId},
    sat::Sat,
    sat_point::SatPoint,
  },
  serde::{Deserialize, Serialize},
};

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct RelicInscription {
  pub is_bonestone: bool,
  // if this Inscription has sealed a Relic ticker
  #[serde(rename = "bone_claimed")]
  pub relic_sealed: Option<SpacedRelic>,
  // if the sealed Relic ticker has already been enshrined
  #[serde(rename = "bone_deployed")]
  pub relic_enshrined: bool,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct Inscription {
  pub address: Option<String>,
  pub charms: Vec<Charm>,
  pub child_count: u64,
  pub children: Vec<InscriptionId>,
  pub content_length: Option<usize>,
  pub content_type: Option<String>,
  pub effective_content_type: Option<String>,
  pub fee: u64,
  pub height: u32,
  pub id: InscriptionId,
  pub next: Option<InscriptionId>,
  pub number: u64,
  pub parents: Vec<InscriptionId>,
  pub previous: Option<InscriptionId>,
  // if this Inscription has sealed a Relic ticker
  #[serde(rename = "bone_claimed")]
  pub relic_sealed: Option<SpacedRelic>,
  // if the sealed Relic ticker has already been enshrined
  #[serde(rename = "bone_deployed")]
  pub relic_enshrined: bool,
  pub syndicate: Option<SyndicateId>,
  pub chest: bool,
  pub sat: Option<Sat>,
  pub satpoint: SatPoint,
  pub timestamp: i64,
  pub value: Option<u64>,
}
