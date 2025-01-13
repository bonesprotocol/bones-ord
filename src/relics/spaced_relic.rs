use super::*;
use ciborium::Value;

#[derive(
  Debug,
  Copy,
  Clone,
  PartialEq,
  Ord,
  PartialOrd,
  Eq,
  Default,
  Hash,
  DeserializeFromStr,
  SerializeDisplay,
)]
pub struct SpacedRelic {
  pub relic: Relic,
  pub spacers: u32,
}

impl SpacedRelic {
  pub const METADATA_KEY: &'static str = "BONE";

  pub fn new(relic: Relic, spacers: u32) -> Self {
    Self { relic, spacers }
  }

  pub fn from_metadata(metadata: Value) -> Option<Self> {
    for (key, value) in metadata.as_map()? {
      if key.as_text() != Some(Self::METADATA_KEY) {
        continue;
      }
      return SpacedRelic::from_str(value.as_text()?).ok();
    }
    None
  }

  pub fn to_metadata(&self) -> Value {
    Value::Map(vec![(
      Value::Text(Self::METADATA_KEY.into()),
      Value::Text(self.to_string()),
    )])
  }

  pub fn to_metadata_yaml(&self) -> serde_yaml::Value {
    let mut mapping = serde_yaml::Mapping::new();
    mapping.insert(
      serde_yaml::Value::String(Self::METADATA_KEY.into()),
      serde_yaml::Value::String(self.to_string()),
    );
    serde_yaml::Value::Mapping(mapping)
  }
}

impl FromStr for SpacedRelic {
  type Err = Error;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let mut relic = String::new();
    let mut spacers = 0u32;

    for c in s.chars() {
      match c {
        'A'..='Z' => relic.push(c),
        '.' | '•' => {
          let flag = 1 << relic.len().checked_sub(1).ok_or(Error::LeadingSpacer)?;
          if spacers & flag != 0 {
            return Err(Error::DoubleSpacer);
          }
          spacers |= flag;
        }
        _ => return Err(Error::Character(c)),
      }
    }

    if 32 - spacers.leading_zeros() >= relic.len().try_into().unwrap() {
      return Err(Error::TrailingSpacer);
    }

    Ok(SpacedRelic {
      relic: relic.parse().map_err(Error::Relic)?,
      spacers,
    })
  }
}

impl Display for SpacedRelic {
  fn fmt(&self, f: &mut Formatter) -> fmt::Result {
    let relic = self.relic.to_string();

    for (i, c) in relic.chars().enumerate() {
      write!(f, "{c}")?;

      if i < relic.len() - 1 && self.spacers & 1 << i != 0 {
        write!(f, "•")?;
      }
    }

    Ok(())
  }
}

#[derive(Debug, PartialEq)]
pub enum Error {
  LeadingSpacer,
  TrailingSpacer,
  DoubleSpacer,
  Character(char),
  Relic(relic::Error),
}

impl Display for Error {
  fn fmt(&self, f: &mut Formatter) -> fmt::Result {
    match self {
      Self::Character(c) => write!(f, "invalid character `{c}`"),
      Self::DoubleSpacer => write!(f, "double spacer"),
      Self::LeadingSpacer => write!(f, "leading spacer"),
      Self::TrailingSpacer => write!(f, "trailing spacer"),
      Self::Relic(err) => write!(f, "{err}"),
    }
  }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn display() {
    assert_eq!("A.B".parse::<SpacedRelic>().unwrap().to_string(), "A•B");
    assert_eq!("A.B.C".parse::<SpacedRelic>().unwrap().to_string(), "A•B•C");
    assert_eq!(
      SpacedRelic {
        relic: Relic(0),
        spacers: 1
      }
      .to_string(),
      "A"
    );
  }

  #[test]
  fn from_str() {
    #[track_caller]
    fn case(s: &str, relic: &str, spacers: u32) {
      assert_eq!(
        s.parse::<SpacedRelic>().unwrap(),
        SpacedRelic {
          relic: relic.parse().unwrap(),
          spacers
        },
      );
    }

    assert_eq!(
      ".A".parse::<SpacedRelic>().unwrap_err(),
      Error::LeadingSpacer,
    );

    assert_eq!(
      "A..B".parse::<SpacedRelic>().unwrap_err(),
      Error::DoubleSpacer,
    );

    assert_eq!(
      "A.".parse::<SpacedRelic>().unwrap_err(),
      Error::TrailingSpacer,
    );

    assert_eq!(
      "Ax".parse::<SpacedRelic>().unwrap_err(),
      Error::Character('x')
    );

    case("A.B", "AB", 0b1);
    case("A.B.C", "ABC", 0b11);
    case("A•B", "AB", 0b1);
    case("A•B•C", "ABC", 0b11);
    case("A•BC", "ABC", 0b1);
  }

  #[test]
  fn serde() {
    let spaced_relic = SpacedRelic {
      relic: Relic(26),
      spacers: 1,
    };
    let json = "\"A•A\"";
    assert_eq!(serde_json::to_string(&spaced_relic).unwrap(), json);
    assert_eq!(
      serde_json::from_str::<SpacedRelic>(json).unwrap(),
      spaced_relic
    );
  }
}
