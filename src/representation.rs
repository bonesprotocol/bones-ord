use {super::*, regex::RegexSet};

#[derive(Debug, Copy, Clone)]
pub(crate) enum Representation {
  Address,
  Decimal,
  Hash,
  InscriptionId,
  Integer,
  OutPoint,
  Relic,
  SatPoint,
}

impl Representation {
  const fn pattern(self) -> (Self, &'static str) {
    (
      self,
      match self {
        Self::Address => r"^(bc|BC|tb|TB|bcrt|BCRT)1.*$",
        Self::Decimal => r"^.*\..*$",
        Self::Hash => r"^[[:xdigit:]]{64}$",
        Self::InscriptionId => r"^[[:xdigit:]]{64}i\d+$",
        Self::Integer => r"^[0-9]*$",
        Self::OutPoint => r"^[[:xdigit:]]{64}:\d+$",
        Self::Relic => r"^[A-Z•.]+$",
        Self::SatPoint => r"^[[:xdigit:]]{64}:\d+:\d+$",
      },
    )
  }
}

impl FromStr for Representation {
  type Err = Error;

  fn from_str(s: &str) -> Result<Self> {
    if let Some(i) = REGEX_SET.matches(s).into_iter().next() {
      Ok(PATTERNS[i].0)
    } else {
      Err(anyhow!("unrecognized object"))
    }
  }
}

const PATTERNS: &[(Representation, &str)] = &[
  Representation::Address.pattern(),
  Representation::Decimal.pattern(),
  Representation::Hash.pattern(),
  Representation::InscriptionId.pattern(),
  Representation::Integer.pattern(),
  Representation::OutPoint.pattern(),
  Representation::Relic.pattern(),
  Representation::SatPoint.pattern(),
];

lazy_static! {
  static ref REGEX_SET: RegexSet =
    RegexSet::new(PATTERNS.iter().map(|(_representation, pattern)| pattern),).unwrap();
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn all_patterns_are_anchored() {
    assert!(PATTERNS
      .iter()
      .all(|(_representation, pattern)| pattern.starts_with('^') && pattern.ends_with('$')));
  }
}
