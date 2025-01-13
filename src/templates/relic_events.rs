use super::*;
use crate::index::event::Event;
use crate::relics::spaced_relic::SpacedRelic;

#[derive(Boilerplate, Debug, PartialEq, Serialize, Deserialize)]
pub struct RelicEventsHtml {
  #[serde(rename = "spaced_bone")]
  pub spaced_relic: SpacedRelic,
  pub events: Vec<Event>,
}

impl PageContent for RelicEventsHtml {
  fn title(&self) -> String {
    format!("Relic Events {}", self.spaced_relic)
  }
}
