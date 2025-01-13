use super::*;

#[derive(Default, Serialize, Deserialize, Debug, PartialEq, Copy, Clone, Eq)]
pub struct Summoning {
  /// ID of the relic the syndicate is for
  /// note: defaults to RELIC if omitted
  pub treasure: Option<RelicId>,
  /// from which block to which block chests can be created
  pub height: (Option<u64>, Option<u64>),
  /// max number of chests that can exist at the same time
  pub cap: Option<u32>,
  /// how many relics needed per chest (exact)
  /// note: this is not optional
  pub quota: Option<u128>,
  /// royalty to be paid in RELIC (to the syndicate inscription owner)
  /// a flat fee paid to the owner for every chest created
  pub royalty: Option<u128>,
  /// if this is set, only owner of the Syndicate inscription can chest
  pub gated: bool,
  /// how many blocks the relics should be locked in the chest, no withdrawal possible before
  pub lock: Option<u64>,
  /// rewards that are paid by having relics wrapped, measured in Relics per Chest per block
  /// these are taken from the subsidy supply available on the Relic
  /// note: only the owner of the Relic can summon Syndicates with a reward
  pub reward: Option<u128>,
  /// kill switch to deny any further Syndicates with reward
  pub lock_subsidy: bool,
  /// opt-in to future protocol changes
  pub turbo: bool,
}
