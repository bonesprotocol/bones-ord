use {
  super::*,
  crate::relics::{
    BalanceDiff, MintTerms, Pool, PoolSwap, PriceModel, Relic, RelicError, RelicId, SpacedRelic,
  },
  bitcoin::ScriptHash,
};

impl Entry for Relic {
  type Value = u128;

  fn load(value: Self::Value) -> Self {
    Self(value)
  }

  fn store(self) -> Self::Value {
    self.0
  }
}

impl Entry for RelicId {
  type Value = RelicIdValue;

  fn load((block, tx): Self::Value) -> Self {
    Self { block, tx }
  }

  fn store(self) -> Self::Value {
    (self.block, self.tx)
  }
}

pub type SpacedRelicValue = (u128, u32);

impl Entry for SpacedRelic {
  type Value = SpacedRelicValue;

  fn load(value: Self::Value) -> Self {
    SpacedRelic::new(Relic(value.0), value.1)
  }

  fn store(self) -> Self::Value {
    (self.relic.0, self.spacers)
  }
}

#[derive(Debug, Hash, Eq, PartialEq, PartialOrd, Ord, Copy, Clone, Serialize, Deserialize)]
pub struct RelicOwner(pub ScriptHash);

impl Default for RelicOwner {
  fn default() -> Self {
    Self(ScriptHash::all_zeros())
  }
}

pub type RelicOwnerValue = [u8; 20];

impl Entry for RelicOwner {
  type Value = RelicOwnerValue;

  fn load(value: Self::Value) -> Self {
    match ScriptHash::from_slice(&value) {
      Ok(script_hash) => Self(script_hash),
      Err(_) => {
        eprintln!("Error: Failed to create ScriptHash from bytes");
        Self(ScriptHash::all_zeros())
      }
    }
  }

  fn store(self) -> Self::Value {
    let bytes = self.0.as_ref();
    let mut array = [0u8; 20];
    array.copy_from_slice(bytes);
    array
  }
}

#[derive(Debug, Default, PartialEq, Copy, Clone, Serialize, Deserialize)]
pub struct RelicState {
  pub burned: u128,
  pub mints: u128,
  pub subsidy: u128,
  pub subsidy_remaining: u128,
  pub subsidy_locked: bool,
}

pub type RelicStateValue = (u128, u128, u128, u128, bool);

impl Entry for RelicState {
  type Value = RelicStateValue;

  fn load((burned, mints, subsidy, subsidy_remaining, subsidy_locked): Self::Value) -> Self {
    Self {
      burned,
      mints,
      subsidy,
      subsidy_remaining,
      subsidy_locked,
    }
  }

  fn store(self) -> Self::Value {
    (
      self.burned,
      self.mints,
      self.subsidy,
      self.subsidy_remaining,
      self.subsidy_locked,
    )
  }
}

#[derive(Debug, PartialEq, Copy, Clone, Serialize, Deserialize)]
pub struct RelicEntry {
  pub block: u64,
  pub enshrining: Txid,
  pub number: u64,
  pub spaced_relic: SpacedRelic,
  pub symbol: Option<char>,
  pub owner_sequence_number: Option<u32>,
  pub mint_terms: Option<MintTerms>,
  pub state: RelicState,
  pub pool: Option<Pool>,
  pub timestamp: u64,
  pub turbo: bool,
}

impl RelicEntry {
  pub fn mintable(&self, base_balance: u128) -> Result<(u128, u128), RelicError> {
    let Some(terms) = self.mint_terms else {
      return Err(RelicError::Unmintable);
    };

    let cap = terms.cap.unwrap_or_default();

    if self.state.mints >= cap {
      return Err(RelicError::MintCap(cap));
    }

    let price = terms
      .compute_price(self.state.mints)
      .ok_or(RelicError::PriceComputationError)?;

    if base_balance < price {
      return Err(RelicError::MintInsufficientBalance(price));
    }

    Ok((terms.amount.unwrap_or_default(), price))
  }

  pub fn swap(
    &self,
    swap: PoolSwap,
    balance: Option<u128>,
    height: u64,
  ) -> Result<BalanceDiff, RelicError> {
    // fail the swap if pool does not exist (yet)
    let Some(pool) = self.pool else {
      return Err(RelicError::SwapNotAvailable);
    };

    // fail of the minimum swap height is not reached yet
    if let Some(swap_height) = self.mint_terms.and_then(|terms| terms.swap_height) {
      if height < swap_height {
        return Err(RelicError::SwapHeightNotReached(swap_height));
      }
    }

    match pool.calculate(swap) {
      Ok(diff) => {
        if let Some(balance) = balance {
          if diff.input > balance {
            return Err(RelicError::SwapInsufficientBalance(diff.input));
          }
        }
        Ok(diff)
      }
      Err(cause) => Err(RelicError::SwapFailed(cause)),
    }
  }

  /// max supply of this token: maximum amount of tokens that can be minted plus
  /// the additional amount that is created for the pool after minting is complete
  /// and the total subsidy
  pub fn max_supply(&self) -> u128 {
    self.state.subsidy
      + self
        .mint_terms
        .map(|terms| {
          terms.amount.unwrap_or_default() * terms.cap.unwrap_or_default()
            + terms.seed.unwrap_or_default()
        })
        .unwrap_or_default()
  }

  /// circulating supply of tokens: either minted or swapped out of the pool minus burned
  pub fn circulating_supply(&self) -> u128 {
    let amount = self
      .mint_terms
      .and_then(|terms| terms.amount)
      .unwrap_or_default();
    let seed = self
      .mint_terms
      .and_then(|terms| terms.seed)
      .unwrap_or_default();
    let pool_quote_supply = self.pool.map(|pool| pool.quote_supply).unwrap_or(seed);
    self.state.mints * amount + self.state.subsidy - self.state.subsidy_remaining + seed
      - pool_quote_supply
      - self.state.burned
  }

  pub fn locked_base_supply(&self) -> u128 {
    if let Some(pool) = self.pool {
      pool.base_supply
    } else if let Some(terms) = self.mint_terms {
      match terms.price {
        Some(PriceModel::Fixed(fixed)) => self.state.mints * fixed,
        Some(PriceModel::Formula { .. }) => {
          let mut total: u128 = 0;
          for x in 0..self.state.mints {
            total = total.saturating_add(terms.compute_price(x).unwrap_or(0));
          }
          total
        }
        None => 0,
      }
    } else {
      0
    }
  }
}

type MintTermsValue = (
  Option<u128>, // amount
  Option<u128>, // cap
  Option<u128>, // stored price:
  //   - Some(n) with n != 0 represents PriceModel::Fixed(n)
  //   - Some(0) indicates formula pricing (with a, b, c below)
  Option<u128>, // formula_a (for formula pricing)
  Option<u128>, // formula_b (for formula pricing)
  Option<u128>, // formula_c (for formula pricing)
  Option<u128>, // seed
  Option<u64>,  // swap_height
  Option<bool>, // unmintable
);

impl Entry for MintTerms {
  type Value = MintTermsValue;

  fn load((
      amount,
      cap,
      price_type,
      price_fixed_or_a,
      formula_b,
      formula_c,
      seed,
      swap_height,
      unmintable,
    ): Self::Value) -> Self {
    let price = match price_type {
      Some(1) => price_fixed_or_a.map(|p| PriceModel::Fixed(p)),
      Some(2) => {
        if let (Some(a), Some(b), Some(c)) = (price_fixed_or_a, formula_b, formula_c) {
          Some(PriceModel::Formula { a, b, c })
        } else {
          None
        }
      }
      _ => None,
    };
    Self {
      amount,
      cap,
      price,
      seed,
      swap_height,
      unmintable,
    }
  }

  fn store(self) -> Self::Value {
    let (price_type, price_fixed_or_a, formula_b, formula_c) = match self.price {
      Some(PriceModel::Fixed(p)) => (Some(1), Some(p), None, None),
      Some(PriceModel::Formula { a, b, c }) => (Some(2), Some(a), Some(b), Some(c)),
      None => (None, None, None, None),
    };
    (
      self.amount,
      self.cap,
      price_type,
      price_fixed_or_a,
      formula_b,
      formula_c,
      self.seed,
      self.swap_height,
      self.unmintable,
    )
  }
}

pub type PoolValue = (u128, u128, u8);

impl Entry for Pool {
  type Value = PoolValue;

  fn load((base_supply, quote_supply, fee_percentage): Self::Value) -> Self {
    Self {
      base_supply,
      quote_supply,
      fee_percentage,
    }
  }

  fn store(self) -> Self::Value {
    (self.base_supply, self.quote_supply, self.fee_percentage)
  }
}

pub type RelicEntryValue = (
  u64,                    // block
  (u128, u128),           // enshrining
  u64,                    // number
  SpacedRelicValue,       // spaced_relic
  Option<char>,           // symbol
  Option<u32>,            // owner sequence number
  Option<MintTermsValue>, // mint_terms
  RelicStateValue,        // state
  Option<PoolValue>,      // pool
  u64,                    // timestamp
  bool,                   // turbo
);

impl Default for RelicEntry {
  fn default() -> Self {
    Self {
      block: 0,
      enshrining: Txid::all_zeros(),
      number: 0,
      spaced_relic: SpacedRelic::default(),
      symbol: None,
      owner_sequence_number: None,
      mint_terms: None,
      state: RelicState::default(),
      pool: None,
      timestamp: 0,
      turbo: false,
    }
  }
}

impl Entry for RelicEntry {
  type Value = RelicEntryValue;

  fn load(
    (
      block,
      enshrining,
      number,
      spaced_relic,
      symbol,
      owner_sequence_number,
      mint_terms,
      state,
      pool,
      timestamp,
      turbo,
    ): RelicEntryValue,
  ) -> Self {
    Self {
      block,
      enshrining: {
        let low = enshrining.0.to_le_bytes();
        let high = enshrining.1.to_le_bytes();
        let bytes: Vec<u8> = [low, high].concat();
        Txid::from_slice(bytes.as_slice()).unwrap_or(Txid::all_zeros())
      },
      number,
      spaced_relic: SpacedRelic::load(spaced_relic),
      symbol,
      owner_sequence_number,
      mint_terms: mint_terms.map(MintTerms::load),
      state: RelicState::load(state),
      pool: pool.map(Pool::load),
      timestamp,
      turbo,
    }
  }

  fn store(self) -> Self::Value {
    (
      self.block,
      {
        let bytes_vec = self.enshrining.to_vec();
        let bytes: [u8; 32] = match bytes_vec.len() {
          32 => {
            let mut array = [0; 32];
            array.copy_from_slice(&bytes_vec);
            array
          }
          _ => panic!("Vector length is not 32"),
        };
        (
          u128::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
          ]),
          u128::from_le_bytes([
            bytes[16], bytes[17], bytes[18], bytes[19], bytes[20], bytes[21], bytes[22], bytes[23],
            bytes[24], bytes[25], bytes[26], bytes[27], bytes[28], bytes[29], bytes[30], bytes[31],
          ]),
        )
      },
      self.number,
      self.spaced_relic.store(),
      self.symbol,
      self.owner_sequence_number,
      self.mint_terms.map(|terms| terms.store()),
      self.state.store(),
      self.pool.map(|pool| pool.store()),
      self.timestamp,
      self.turbo,
    )
  }
}

pub type RelicIdValue = (u64, u32);

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn relic_entry() {
    let txid_bytes = [
      0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E,
      0x0F, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D,
      0x1E, 0x1F,
    ];
    let txid = Txid::from_slice(&txid_bytes).expect("Slice must be of correct length");
    let entry = RelicEntry {
      block: 12,
      enshrining: txid,
      number: 6,
      spaced_relic: SpacedRelic {
        relic: Relic(7),
        spacers: 8,
      },
      symbol: Some('a'),
      owner_sequence_number: Some(123),
      mint_terms: Some(MintTerms {
        amount: Some(4),
        cap: Some(1),
        price: Some(8),
        seed: Some(22),
        swap_height: Some(400_000),
        unmintable: false,
      }),
      state: RelicState {
        burned: 33,
        mints: 44,
        subsidy: 55,
        subsidy_remaining: 66,
        subsidy_locked: true,
      },
      pool: Some(Pool {
        base_supply: 321,
        quote_supply: 123,
        fee_percentage: 13,
      }),
      timestamp: 10,
      turbo: true,
    };

    let value = (
      12,
      (
        0x0F0E0D0C0B0A09080706050403020100,
        0x1F1E1D1C1B1A19181716151413121110,
      ),
      6,
      (7, 8),
      Some('a'),
      Some(123),
      Some((Some(4), Some(1), Some(8), Some(22), Some(400_000))),
      (33, 44, 55, 66, true),
      Some((321, 123, 13)),
      10,
      true,
    );

    assert_eq!(entry.store(), value);
    assert_eq!(RelicEntry::load(value), entry);
  }

  #[test]
  fn relic_id_entry() {
    assert_eq!(RelicId { block: 1, tx: 2 }.store(), (1, 2),);
    assert_eq!(RelicId { block: 1, tx: 2 }, RelicId::load((1, 2)),);
  }

  #[test]
  fn mintable_default() {
    assert_eq!(
      RelicEntry::default().mintable(0),
      Err(RelicError::Unmintable)
    );
  }
}
