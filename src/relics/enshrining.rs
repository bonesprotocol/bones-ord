use super::*;

#[derive(Default, Serialize, Deserialize, Debug, PartialEq, Copy, Clone, Eq)]
pub struct Enshrining {
  /// potential mint boosts
  pub boost_terms: Option<BoostTerms>,
  /// trading fee in bps (10_000 = 100%)
  pub fee: Option<u16>,
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
  pub count: u8,
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
  pub fn compute_total_price(&self, start: u128, count: u8) -> Option<u128> {
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
  /// Maximum number of mints allowed in one block
  pub block_cap: Option<u32>,
  /// maximum number of mints allowed
  /// if mint is boosted, this is only a soft cap
  pub cap: Option<u128>,
  /// if set, only allow minters from manifest (and parent manifests)
  pub manifest: Option<ManifestId>,
  /// Only if set, tokens can be unminted (until max_unmints reached)
  pub max_unmints: Option<u32>,
  /// note: must be set, except for RELIC, which does not have a price
  pub price: Option<PriceModel>,
  /// initial supply of quote tokens when the liquidity pool is created
  /// the typical case would be to set this to amount*cap
  pub seed: Option<u128>,
  /// minimum block height for swaps
  pub swap_height: Option<u64>,
  /// Maximum number of mints allowed in one transaction
  pub tx_cap: Option<u8>,
}

/// If set give people the chance to get boosts (multipliers) on their mints
#[derive(Default, Serialize, Deserialize, Debug, PartialEq, Copy, Clone, Eq)]
pub struct BoostTerms {
  // chance to get a rare mint in ppm
  pub rare_chance: Option<u32>,
  // e.g. if set to 10 -> rare mint = min. 1x mint amount, max 10x mint amount
  pub rare_multiplier_cap: Option<u16>,
  // chance to get an ultra rare mint in ppm
  pub ultra_rare_chance: Option<u32>,
  // e.g. if set to 20 and rare mint set to 10 -> min 10x mint amount, max 20x mint amount
  pub ultra_rare_multiplier_cap: Option<u16>,
}

impl MintTerms {
  // Compute price using the current mint count (x)
  pub fn compute_price(&self, x: u128) -> Option<u128> {
    self.price.and_then(|p| p.compute_price(x))
  }

  /// Computes the total price for `count` mints starting at mint index `start`.
  pub fn compute_total_price(&self, start: u128, count: u8) -> Option<u128> {
    self.price.and_then(|p| p.compute_total_price(start, count))
  }
}

impl Enshrining {
  /// All Relics come with the same divisibility
  pub const DIVISIBILITY: u8 = 8;
  pub const MAX_SPACERS: u32 = 0b00000111_11111111_11111111_11111111;

  pub fn max_supply(&self) -> Option<u128> {
    let subsidy = self.subsidy.unwrap_or_default();
    let amount = self.mint_terms.and_then(|terms| terms.amount).unwrap_or_default();
    let cap = self.mint_terms.and_then(|terms| terms.cap).unwrap_or_default();
    let seed = self.mint_terms.and_then(|terms| terms.seed).unwrap_or_default();

    // If ultra_rare_multiplier_cap is not set, use rare_multiplier_cap; if that's also not set, use 1.
    let max_boost = self
      .boost_terms
      .map(|b| b.ultra_rare_multiplier_cap.unwrap_or(b.rare_multiplier_cap.unwrap_or(1)))
      .unwrap_or(1);

    subsidy
      .checked_add(seed)?
      .checked_add(cap.checked_mul(amount)?.checked_mul(max_boost.into())?)
  }
}
