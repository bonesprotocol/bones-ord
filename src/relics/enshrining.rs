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

/// Allows minting of tokens for a fixed price until the total supply was minted.
/// Afterward, the liquidity pool is immediately opened with the total RELIC collected during minting and the Relics seed supply.
/// If the Relic never mints out, no pool is created and the collected RELIC are locked.
#[derive(Default, Serialize, Deserialize, Debug, PartialEq, Copy, Clone, Eq)]
pub struct MintTerms {
  /// amount of quote tokens minted per mint
  pub amount: Option<u128>,
  /// maximum number of mints allowed
  pub cap: Option<u128>,
  /// price per mint in RELIC
  /// note: must be set, except for RELIC, which does not have a price
  pub price: Option<u128>,
  /// initial supply of quote tokens when the liquidity pool is created
  /// the typical case would be to set this to amount*cap
  pub seed: Option<u128>,
  /// minimum block height for swaps
  pub swap_height: Option<u64>,
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

  pub fn total_mint_value(&self) -> Option<u128> {
    let cap = self
      .mint_terms
      .and_then(|terms| terms.cap)
      .unwrap_or_default();
    let price = self
      .mint_terms
      .and_then(|terms| terms.price)
      .unwrap_or_default();
    cap.checked_mul(price)
  }
}
