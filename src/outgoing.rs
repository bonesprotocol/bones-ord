use super::*;
use crate::relics::SpacedRelic;
use crate::sat_point::SatPoint;

#[derive(Debug, PartialEq, Clone, DeserializeFromStr, SerializeDisplay)]
pub(crate) enum Outgoing {
  Amount(Amount),
  InscriptionId(InscriptionId),
  Relic {
    decimal: Decimal,
    relic: SpacedRelic,
  },
  SatPoint(SatPoint),
}

impl Display for Outgoing {
  fn fmt(&self, f: &mut Formatter) -> fmt::Result {
    match self {
      Self::Amount(amount) => write!(f, "{}", amount.to_string().to_lowercase()),
      Self::InscriptionId(inscription_id) => inscription_id.fmt(f),
      Self::Relic { decimal, relic } => write!(f, "{decimal}:{relic}"),
      Self::SatPoint(satpoint) => satpoint.fmt(f),
    }
  }
}

impl FromStr for Outgoing {
  type Err = Error;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    lazy_static! {
      static ref SATPOINT: Regex = Regex::new(r"^[[:xdigit:]]{64}:\d+:\d+$").unwrap();
      static ref INSCRIPTION_ID: Regex = Regex::new(r"^[[:xdigit:]]{64}i\d+$").unwrap();
      static ref AMOUNT: Regex = Regex::new(
        r"(?x)
        ^
        (
          \d+
          |
          \.\d+
          |
          \d+\.\d+
        )
        \ ?
        (bit|btc|cbtc|mbtc|msat|nbtc|pbtc|sat|satoshi|ubtc)
        (s)?
        $
        "
      )
      .unwrap();
      static ref RELIC: Regex = Regex::new(
        r"(?x)
        ^
        (
          \d+
          |
          \.\d+
          |
          \d+\.\d+
        )
        \s*:\s*
        (
          [A-Z•.]+
        )
        $
        "
      )
      .unwrap();
    }

    Ok(if SATPOINT.is_match(s) {
      Self::SatPoint(s.parse()?)
    } else if INSCRIPTION_ID.is_match(s) {
      Self::InscriptionId(s.parse()?)
    } else if AMOUNT.is_match(s) {
      Self::Amount(s.parse()?)
    } else if let Some(captures) = RELIC.captures(s) {
      Self::Relic {
        decimal: captures[1].parse()?,
        relic: captures[2].parse()?,
      }
    } else {
      bail!("unrecognized outgoing: {s}");
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use bitcoin::blockdata::constants::COIN_VALUE;

  #[test]
  fn from_str() {
    #[track_caller]
    fn case(s: &str, outgoing: Outgoing) {
      assert_eq!(s.parse::<Outgoing>().unwrap(), outgoing);
    }

    // case("nvtdijuwxlp", Outgoing::Sat("nvtdijuwxlp".parse().unwrap()));
    // case("a", Outgoing::Sat("a".parse().unwrap()));

    case(
      "0000000000000000000000000000000000000000000000000000000000000000i0",
      Outgoing::InscriptionId(
        "0000000000000000000000000000000000000000000000000000000000000000i0"
          .parse()
          .unwrap(),
      ),
    );

    case(
      "0000000000000000000000000000000000000000000000000000000000000000:0:0",
      Outgoing::SatPoint(
        "0000000000000000000000000000000000000000000000000000000000000000:0:0"
          .parse()
          .unwrap(),
      ),
    );

    case("0 btc", Outgoing::Amount("0 btc".parse().unwrap()));
    // case("0btc", Outgoing::Amount("0 btc".parse().unwrap()));
    // case("0.0btc", Outgoing::Amount("0 btc".parse().unwrap()));
    // case(".0btc", Outgoing::Amount("0 btc".parse().unwrap()));

    case(
      "0  : XYZ",
      Outgoing::Relic {
        relic: "XYZ".parse().unwrap(),
        decimal: "0".parse().unwrap(),
      },
    );

    case(
      "0:XYZ",
      Outgoing::Relic {
        relic: "XYZ".parse().unwrap(),
        decimal: "0".parse().unwrap(),
      },
    );

    case(
      "0.0:XYZ",
      Outgoing::Relic {
        relic: "XYZ".parse().unwrap(),
        decimal: "0.0".parse().unwrap(),
      },
    );

    case(
      ".0:XYZ",
      Outgoing::Relic {
        relic: "XYZ".parse().unwrap(),
        decimal: ".0".parse().unwrap(),
      },
    );

    case(
      "1.1:XYZ",
      Outgoing::Relic {
        relic: "XYZ".parse().unwrap(),
        decimal: "1.1".parse().unwrap(),
      },
    );

    case(
      "1.1:X.Y.Z",
      Outgoing::Relic {
        relic: "X.Y.Z".parse().unwrap(),
        decimal: "1.1".parse().unwrap(),
      },
    );
  }

  #[test]
  fn roundtrip() {
    #[track_caller]
    fn case(s: &str, outgoing: Outgoing) {
      assert_eq!(s.parse::<Outgoing>().unwrap(), outgoing);
      assert_eq!(s, outgoing.to_string());
    }

    // case("nvtdijuwxlp", Outgoing::Sat("nvtdijuwxlp".parse().unwrap()));
    // case("a", Outgoing::Sat("a".parse().unwrap()));

    case(
      "0000000000000000000000000000000000000000000000000000000000000000i0",
      Outgoing::InscriptionId(
        "0000000000000000000000000000000000000000000000000000000000000000i0"
          .parse()
          .unwrap(),
      ),
    );

    case(
      "0000000000000000000000000000000000000000000000000000000000000000:0:0",
      Outgoing::SatPoint(
        "0000000000000000000000000000000000000000000000000000000000000000:0:0"
          .parse()
          .unwrap(),
      ),
    );

    case("0 btc", Outgoing::Amount("0 btc".parse().unwrap()));
    case(
      "1.21231237 btc",
      Outgoing::Amount("1.21231237 btc".parse().unwrap()),
    );

    case(
      "0:XY•Z",
      Outgoing::Relic {
        relic: "XY•Z".parse().unwrap(),
        decimal: "0".parse().unwrap(),
      },
    );

    case(
      "1.1:XYZ",
      Outgoing::Relic {
        relic: "XYZ".parse().unwrap(),
        decimal: "1.1".parse().unwrap(),
      },
    );
  }

  #[test]
  fn serde() {
    #[track_caller]
    fn case(s: &str, j: &str, o: Outgoing) {
      assert_eq!(s.parse::<Outgoing>().unwrap(), o);
      assert_eq!(serde_json::to_string(&o).unwrap(), j);
      assert_eq!(serde_json::from_str::<Outgoing>(j).unwrap(), o);
    }

    // case(
    //   "nvtdijuwxlp",
    //   "\"nvtdijuwxlp\"",
    //   Outgoing::Sat("nvtdijuwxlp".parse().unwrap()),
    // );
    // case("a", "\"a\"", Outgoing::Sat("a".parse().unwrap()));

    case(
      "0000000000000000000000000000000000000000000000000000000000000000i0",
      "\"0000000000000000000000000000000000000000000000000000000000000000i0\"",
      Outgoing::InscriptionId(
        "0000000000000000000000000000000000000000000000000000000000000000i0"
          .parse()
          .unwrap(),
      ),
    );

    case(
      "0000000000000000000000000000000000000000000000000000000000000000:0:0",
      "\"0000000000000000000000000000000000000000000000000000000000000000:0:0\"",
      Outgoing::SatPoint(
        "0000000000000000000000000000000000000000000000000000000000000000:0:0"
          .parse()
          .unwrap(),
      ),
    );

    case(
      "3 btc",
      "\"3 btc\"",
      Outgoing::Amount(Amount::from_sat(3 * COIN_VALUE)),
    );

    case(
      "6.66:HELL.MONEY",
      "\"6.66:HELL•MONEY\"",
      Outgoing::Relic {
        relic: "HELL•MONEY".parse().unwrap(),
        decimal: "6.66".parse().unwrap(),
      },
    );
  }
}
