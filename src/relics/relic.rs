use super::*;

#[derive(
  Default,
  Debug,
  Hash,
  PartialEq,
  Copy,
  Clone,
  PartialOrd,
  Ord,
  Eq,
  DeserializeFromStr,
  SerializeDisplay,
)]
pub struct Relic(pub u128);

impl Relic {
  pub fn n(self) -> u128 {
    self.0
  }

  pub fn first_relic_height(network: Network) -> u32 {
    match network {
      Network::Bitcoin => 850000,
      Network::Regtest => 0,
      Network::Signet => 0,
      Network::Testnet => 2800000,
      _ => 0,
    }
  }

  pub fn length(self) -> u32 {
    // self.to_string().len() as u32
    let mut len = 0;
    let mut n = self.0;
    if n == u128::MAX {
      return 28;
    }
    n += 1;
    while n > 0 {
      n = (n - 1) / 26;
      len += 1;
    }
    len
  }

  /// Sealing fee based on the length of this Relic.
  /// - 1 letter = 210,000
  /// - 2 letters = 21,000
  /// - 3 letters = 2,100
  /// - 4-6 letters = 500
  /// - 7-12 letters = 10
  /// - 13+ letters = 1
  pub fn sealing_fee(self) -> u128 {
    let x: u128 = match self.length() {
      0 => unreachable!(),
      1 => 210_000,
      2 => 21_000,
      3 => 2_100,
      4..=6 => 500,
      7..=12 => 10,
      13.. => 1,
    };
    x * 10u128.pow(Enshrining::DIVISIBILITY.into())
  }

  pub fn commitment(self) -> Vec<u8> {
    let bytes = self.0.to_le_bytes();

    let mut end = bytes.len();

    while end > 0 && bytes[end - 1] == 0 {
      end -= 1;
    }

    bytes[..end].into()
  }
}

impl Display for Relic {
  fn fmt(&self, f: &mut Formatter) -> fmt::Result {
    let mut n = self.0;
    if n == u128::MAX {
      return write!(f, "BCGDENLQRQWDSLRUGSNLBTMFIJAV");
    }

    n += 1;
    let mut symbol = String::new();
    while n > 0 {
      symbol.push(
        "ABCDEFGHIJKLMNOPQRSTUVWXYZ"
          .chars()
          .nth(((n - 1) % 26) as usize)
          .unwrap(),
      );
      n = (n - 1) / 26;
    }

    for c in symbol.chars().rev() {
      write!(f, "{c}")?;
    }

    Ok(())
  }
}

impl FromStr for Relic {
  type Err = Error;

  fn from_str(s: &str) -> Result<Self, Error> {
    let mut x = 0u128;
    for (i, c) in s.chars().enumerate() {
      if i > 0 {
        x = x.checked_add(1).ok_or(Error::Range)?;
      }
      x = x.checked_mul(26).ok_or(Error::Range)?;
      match c {
        'A'..='Z' => {
          x = x.checked_add(c as u128 - 'A' as u128).ok_or(Error::Range)?;
        }
        _ => return Err(Error::Character(c)),
      }
    }
    Ok(Relic(x))
  }
}

#[derive(Debug, PartialEq)]
pub enum Error {
  Character(char),
  Range,
}

impl Display for Error {
  fn fmt(&self, f: &mut Formatter) -> fmt::Result {
    match self {
      Self::Character(c) => write!(f, "invalid character `{c}`"),
      Self::Range => write!(f, "name out of range"),
    }
  }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::relics::spaced_relic::SpacedRelic;

  #[test]
  fn base_token() {
    let base = SpacedRelic::from_str(RELIC_NAME).unwrap();
    println!("base token: {base} {}", base.relic.n());
    assert_eq!(
      base.relic.n(),
      45660,
      "unexpected value for base token \"BONE\""
    );
  }

  #[test]
  fn length() {
    fn case(str: &str, expected: u32) {
      let relic = str.parse::<Relic>().unwrap();
      let actual = relic.length();
      assert_eq!(
        actual, expected,
        "unexpected length of Relic {}({}): expected {expected}, got {actual}",
        str, relic.0
      );
    }

    case("A", 1);
    case("B", 1);
    case("C", 1);
    case("A", 1);
    case("Z", 1);
    case("AA", 2);
    case("AZ", 2);
    case("ZA", 2);
    case("ZZ", 2);
    case("AAAAA", 5);
    case("ZZZZZ", 5);
    case("ANCIENTRELIC", 12);
    case("BCGDENLQRQWDSLRUGSNLBTMFIJAV", 28);
  }

  #[test]
  fn round_trip() {
    fn case(n: u128, s: &str) {
      assert_eq!(Relic(n).to_string(), s);
      assert_eq!(s.parse::<Relic>().unwrap(), Relic(n));
    }

    case(0, "A");
    case(1, "B");
    case(2, "C");
    case(3, "D");
    case(4, "E");
    case(5, "F");
    case(6, "G");
    case(7, "H");
    case(8, "I");
    case(9, "J");
    case(10, "K");
    case(11, "L");
    case(12, "M");
    case(13, "N");
    case(14, "O");
    case(15, "P");
    case(16, "Q");
    case(17, "R");
    case(18, "S");
    case(19, "T");
    case(20, "U");
    case(21, "V");
    case(22, "W");
    case(23, "X");
    case(24, "Y");
    case(25, "Z");
    case(26, "AA");
    case(27, "AB");
    case(51, "AZ");
    case(52, "BA");
    case(u128::MAX - 2, "BCGDENLQRQWDSLRUGSNLBTMFIJAT");
    case(u128::MAX - 1, "BCGDENLQRQWDSLRUGSNLBTMFIJAU");
    case(u128::MAX, "BCGDENLQRQWDSLRUGSNLBTMFIJAV");
  }

  #[test]
  fn sealing_fee() {
    fn case(fee: u128, ticker: &str) {
      assert_eq!(ticker.parse::<Relic>().unwrap().sealing_fee(), fee);
    }

    case(210000_00000000, "A");
    case(210000_00000000, "X");
    case(210000_00000000, "Z");
    case(2100_00000000, "ABC");
    case(2100_00000000, "BTC");
    case(500_00000000, "YOLO");
    case(500_00000000, "QWERTZ");
    case(10_00000000, "INTEGER");
    case(1_00000000, "THIRTEENLETTA");
    case(1_00000000, "THIRTEENLETTER");
  }

  fn serde() {
    let rune = Relic(0);
    let json = "\"A\"";
    assert_eq!(serde_json::to_string(&rune).unwrap(), json);
    assert_eq!(serde_json::from_str::<Relic>(json).unwrap(), rune);
  }

  #[test]
  fn commitment() {
    #[track_caller]
    fn case(rune: u128, bytes: &[u8]) {
      assert_eq!(Relic(rune).commitment(), bytes);
    }

    case(0, &[]);
    case(1, &[1]);
    case(255, &[255]);
    case(256, &[0, 1]);
    case(65535, &[255, 255]);
    case(65536, &[0, 0, 1]);
    case(u128::MAX, &[255; 16]);
  }
}
