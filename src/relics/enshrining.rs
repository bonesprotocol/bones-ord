use super::*;

#[derive(Default, Serialize, Deserialize, Debug, PartialEq, Copy, Clone, Eq)]
pub struct Enshrining {
  /// symbol attached to this Relic
  pub symbol: Option<char>,
  /// supply of quote tokens available for Syndicate rewards
  pub subsidy: Option<u128>,
  /// mint parameters
  pub mint_terms: Option<MintTerms>,
  /// opt-in to future protocol changes
  pub turbo: bool,
}

#[derive(Default, Serialize, Deserialize, Debug, PartialEq, Copy, Clone, Eq)]
pub struct MultiMint {
  /// Number of mints to perform (always positive).
  pub count: u32,
  /// When minting, the maximum base token to spend; when unminting, the minimum base token to receive.
  pub base_limit: u128,
  /// True if this operation is an unmint (i.e. a revert), false for a mint.
  pub is_unmint: bool,
  /// The Relic ID to mint or unmint.
  pub relic: RelicId,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Copy, Clone, Eq)]
#[serde(untagged)]
pub enum PriceModel {
  // Legacy: a fixed price as a number.
  Fixed(u128),
  // New: formula pricing.
  Formula { a: u128, b: u128, c: u128 },
}

impl PriceModel {
  pub fn compute_price(&self, x: u128) -> Option<u128> {
    match *self {
      PriceModel::Fixed(price) => Some(price),
      PriceModel::Formula { a, b, c } => {
        let denom = c.checked_add(x)?;
        Some(a.saturating_sub(b / denom))
      }
    }
  }

  /// Computes the total price for `count` mints starting at mint index `start`.
  pub fn compute_total_price(&self, start: u128, count: u32) -> Option<u128> {
    match *self {
      PriceModel::Fixed(price) => price.checked_mul(count as u128),
      PriceModel::Formula { .. } => {
        let mut total = 0u128;
        for i in 0..count {
          let x = start.checked_add(i as u128)?;
          total = total.checked_add(self.compute_price(x)?)?;
        }
        Some(total)
      }
    }
  }
}

/// Allows minting of tokens for a fixed price until the total supply was minted.
/// Afterward, the liquidity pool is immediately opened with the total RELIC collected during minting and the Relics seed supply.
/// If the Relic never mints out, no pool is created and the collected RELIC are locked.
#[derive(Default, Serialize, Deserialize, Debug, PartialEq, Copy, Clone, Eq)]
pub struct MintTerms {
  /// amount of quote tokens minted per mint
  pub amount: Option<u128>,
  /// maximum number of mints allowed
  pub cap: Option<u128>,
  /// Maximum number of mints allowed in one block
  pub max_per_block: Option<u16>,
  /// Maximum number of mints allowed in one transaction
  pub max_per_tx: Option<u32>,
  /// Only if set, tokens can be unminted (until max_unmints reached)
  pub max_unmints: Option<u32>,
  /// note: must be set, except for RELIC, which does not have a price
  pub price: Option<PriceModel>,
  /// initial supply of quote tokens when the liquidity pool is created
  /// the typical case would be to set this to amount*cap
  pub seed: Option<u128>,
  /// minimum block height for swaps
  pub swap_height: Option<u64>,
}

impl MintTerms {
  // Compute price using the current mint count (x)
  pub fn compute_price(&self, x: u128) -> Option<u128> {
    self.price.and_then(|p| p.compute_price(x))
  }

  /// Computes the total price for `count` mints starting at mint index `start`.
  pub fn compute_total_price(&self, start: u128, count: u32) -> Option<u128> {
    self.price.and_then(|p| p.compute_total_price(start, count))
  }
}

impl Enshrining {
  /// All Relics come with the same divisibility
  pub const DIVISIBILITY: u8 = 8;
  pub const MAX_SPACERS: u32 = 0b00000111_11111111_11111111_11111111;

  pub fn max_supply(&self) -> Option<u128> {
    let subsidy = self.subsidy.unwrap_or_default();
    let amount = self
      .mint_terms
      .and_then(|terms| terms.amount)
      .unwrap_or_default();
    let cap = self
      .mint_terms
      .and_then(|terms| terms.cap)
      .unwrap_or_default();
    let seed = self
      .mint_terms
      .and_then(|terms| terms.seed)
      .unwrap_or_default();
    subsidy
      .checked_add(seed)?
      .checked_add(cap.checked_mul(amount)?)
  }
}
