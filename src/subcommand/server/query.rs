use super::*;
use crate::{relics::RelicId, relics::SpacedRelic, relics::SyndicateId};

pub(super) enum Block {
  Height(u32),
  Hash(BlockHash),
}

impl FromStr for Block {
  type Err = Error;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    Ok(if s.len() == 64 {
      Self::Hash(s.parse()?)
    } else {
      Self::Height(s.parse()?)
    })
  }
}

#[derive(Copy, Clone, Debug)]
pub(crate) enum Inscription {
  Id(InscriptionId),
  Number(u64),
}

impl FromStr for Inscription {
  type Err = Error;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    Ok(if s.contains('i') {
      Self::Id(s.parse()?)
    } else {
      Self::Number(s.parse()?)
    })
  }
}

impl Display for Inscription {
  fn fmt(&self, f: &mut Formatter) -> fmt::Result {
    match self {
      Self::Id(id) => write!(f, "{id}"),
      Self::Number(number) => write!(f, "{number}"),
    }
  }
}

#[derive(Debug)]
pub(super) enum Relic {
  Spaced(SpacedRelic),
  Id(RelicId),
  Number(u64),
}

impl FromStr for Relic {
  type Err = Error;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    if s.contains(':') {
      Ok(Self::Id(s.parse()?))
    } else if re::RUNE_NUMBER.is_match(s) {
      Ok(Self::Number(s.parse()?))
    } else {
      Ok(Self::Spaced(s.parse()?))
    }
  }
}

#[derive(Debug)]
pub(super) enum Syndicate {
  Inscription(InscriptionId),
  Id(SyndicateId),
  // TODO: add name based query when implemented
}

impl FromStr for crate::subcommand::server::query::Syndicate {
  type Err = Error;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    if re::INSCRIPTION_ID.is_match(s) {
      Ok(Self::Inscription(s.parse()?))
    } else {
      Ok(Self::Id(s.parse()?))
    }
  }
}
