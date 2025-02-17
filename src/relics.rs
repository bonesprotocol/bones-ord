use super::*;

use {
  bitcoin::Transaction,
  serde::{Deserialize, Serialize},
  std::{
    collections::{HashMap, VecDeque},
    fmt::{self, Debug, Display, Formatter},
    str::FromStr,
  },
};

pub use {
  artifact::RelicArtifact, cenotaph::RelicCenotaph, enshrining::BoostTerms, enshrining::Enshrining,
  enshrining::MintTerms, enshrining::PriceModel, flaw::RelicFlaw, keepsake::Keepsake, pile::Pile,
  pool::*, relic::Relic, relic_error::RelicError, relic_id::RelicId as SyndicateId,
  relic_id::RelicId, spaced_relic::SpacedRelic, summoning::Summoning, swap::Swap,
  transfer::Transfer,
};

pub const RELIC_ID: RelicId = RelicId { block: 1, tx: 0 };
pub const RELIC_NAME: &str = "BONE";

pub const BONESTONES_INSCRIPTION_ID: &str =
  "babc46e7095a90c814d4c161b1d9d47f921c566ea93ad483d78741cc27c07debi0";
pub const BONESTONES_END_BLOCK: u32 = 5444000;
pub const BONESTONES_START_BLOCK: u32 = 5431819;

#[cfg(test)]
fn default<T: Default>() -> T {
  Default::default()
}

pub mod artifact;
pub mod cenotaph;
pub mod enshrining;
pub mod flaw;
pub mod keepsake;
pub mod manifest;
pub mod pile;
pub mod pool;
pub mod relic;
pub mod relic_error;
pub mod relic_id;
pub mod spaced_relic;
pub mod summoning;
pub mod swap;
pub mod transfer;
pub mod varint;
