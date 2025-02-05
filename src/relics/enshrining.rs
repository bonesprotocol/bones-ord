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
  /// note: must be set, except for RELIC, which does not have a price
  pub price: Option<PriceModel>,
  /// initial supply of quote tokens when the liquidity pool is created
  /// the typical case would be to set this to amount*cap
  pub seed: Option<u128>,
  /// minimum block height for swaps
  pub swap_height: Option<u64>,
  /// If true, minted tokens can be reverted by users.
  pub unmintable: Option<bool>,
}

impl MintTerms {
  // Compute price using the current mint count (x)
  pub fn compute_price(&self, x: u128) -> Option<u128> {
    self.price.and_then(|p| p.compute_price(x))
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
