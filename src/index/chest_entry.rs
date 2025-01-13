use super::*;

#[derive(Debug, Default, PartialEq, Copy, Clone, Serialize, Deserialize)]
pub struct ChestEntry {
  pub sequence_number: u32,
  pub syndicate: SyndicateId,
  pub created_block: u64,
  pub amount: u128,
}

pub type ChestEntryValue = (
  u32,              // sequence_number
  SyndicateIdValue, // syndicate
  u64,              // created_block
  u128,             // amount
);

impl Entry for ChestEntry {
  type Value = ChestEntryValue;

  fn load((sequence_number, syndicate, created_block, amount): Self::Value) -> Self {
    Self {
      sequence_number,
      syndicate: SyndicateId::load(syndicate),
      created_block,
      amount,
    }
  }

  fn store(self) -> Self::Value {
    (
      self.sequence_number,
      self.syndicate.store(),
      self.created_block,
      self.amount,
    )
  }
}
