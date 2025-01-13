use {super::*, crate::relics::Summoning};

pub type SyndicateIdValue = RelicIdValue;

#[derive(Debug, PartialEq, Copy, Clone, Serialize, Deserialize)]
pub struct SyndicateEntry {
  // transaction that summoned this syndicate
  pub summoning: Txid,
  // sequence number of the inscription that owns this Syndicate
  // and will provide the artwork for the chests
  pub sequence_number: u32,
  /// ID of the relic the syndicate is for
  pub treasure: RelicId,
  /// from which block to which block chests can be created
  pub height: (Option<u64>, Option<u64>),
  /// max number of chests that can exist at the same time
  pub cap: Option<u32>,
  /// how many relics needed per chest (minimum)
  pub quota: u128,
  /// royalty to be paid in RELIC (to the syndicate inscription owner)
  pub royalty: u128,
  /// if this is set, only owner of the Syndicate inscription can chest
  pub gated: bool,
  /// how many blocks the relics should be locked in the chest, no withdrawal possible before
  pub lock: Option<u64>,
  /// rewards that are paid by holding Chests, denominated in Relics per Chest per block
  pub reward: Option<u128>,
  /// opt in for future protocol changes
  pub turbo: bool,
  /// current number of Chests
  pub chests: u32,
}

impl SyndicateEntry {
  pub fn new(summoning: Summoning, sequence_number: u32, txid: Txid) -> Self {
    let Summoning {
      treasure,
      gated,
      cap,
      lock,
      height,
      quota,
      royalty,
      reward,
      lock_subsidy: _lock_subsidy,
      turbo,
    } = summoning;
    Self {
      summoning: txid,
      sequence_number,
      treasure: treasure.unwrap_or(RELIC_ID),
      gated,
      cap,
      lock,
      height,
      quota: quota.unwrap_or_default(),
      royalty: royalty.unwrap_or_default(),
      reward,
      turbo,
      chests: 0,
    }
  }

  pub fn chestable(&self, height: u64) -> Result<u128, RelicError> {
    if let Some(start) = self.height.0 {
      if height < start {
        return Err(RelicError::SyndicateStart(start));
      }
    }

    if let Some(end) = self.height.1 {
      if height >= end {
        return Err(RelicError::SyndicateEnd(end));
      }
    }

    let cap = self.cap.unwrap_or(u32::MAX);
    if self.chests >= cap {
      return Err(RelicError::SyndicateCap(cap));
    }

    Ok(self.quota)
  }
}

pub type SyndicateEntryValue = (
  (u128, u128),               // summoning
  u32,                        // sequence_number
  RelicIdValue,               // treasure
  Option<u32>,                // cap
  Option<u64>,                // lock
  (Option<u64>, Option<u64>), // height
  u128,                       // quota
  u128,                       // royalty
  Option<u128>,               // subsidy
  bool,                       // gated
  bool,                       // turbo
  u32,                        // chests
);

impl Entry for SyndicateEntry {
  type Value = SyndicateEntryValue;

  fn load(
    (
      summoning,
      sequence_number,
      treasure,
      cap,
      lock,
      height,
      quota,
      royalty,
      reward,
      gated,
      turbo,
      chests,
    ): Self::Value,
  ) -> Self {
    Self {
      summoning: {
        let low = summoning.0.to_le_bytes();
        let high = summoning.1.to_le_bytes();
        let bytes: Vec<u8> = [low, high].concat();
        Txid::from_slice(bytes.as_slice()).unwrap_or(Txid::all_zeros())
      },
      sequence_number,
      treasure: RelicId::load(treasure),
      cap,
      lock,
      height,
      quota,
      royalty,
      reward,
      gated,
      turbo,
      chests,
    }
  }

  fn store(self) -> Self::Value {
    (
      {
        let bytes_vec = self.summoning.to_vec();
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
      self.sequence_number,
      self.treasure.store(),
      self.cap,
      self.lock,
      self.height,
      self.quota,
      self.royalty,
      self.reward,
      self.gated,
      self.turbo,
      self.chests,
    )
  }
}
