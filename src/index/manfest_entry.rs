use super::*;

// Define the manifest entry value type.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct ManifestEntry {
  pub left_parent: Option<u128>,
  pub right_parent: Option<u128>,
  pub number: u64,
  /// sequence number of the inscription which has the content of the manifest
  pub inscription_number: u32,
  pub title: Option<String>,
}

// Storage alias.
pub type ManifestEntryValue = (Option<u128>, Option<u128>, u64, u32, Option<String>);

impl Entry for ManifestEntry {
  type Value = ManifestEntryValue;

  fn load(value: Self::Value) -> Self {
    let (left_parent, right_parent, number, inscription_number, title) = value;
    Self {
      left_parent,
      right_parent,
      number,
      inscription_number,
      title,
    }
  }

  fn store(self) -> Self::Value {
    (
      self.left_parent,
      self.right_parent,
      self.number,
      self.inscription_number,
      self.title,
    )
  }
}

pub type ManifestedMinterValue = (RelicOwnerValue, u64, u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestedMinter {
  pub owner: RelicOwner,
  pub manifest: RelicId,
}

impl Entry for ManifestedMinter {
  type Value = ManifestedMinterValue;

  fn load((owner_bytes, block, tx): Self::Value) -> Self {
    Self {
      owner: RelicOwner::load(owner_bytes),
      manifest: RelicId::load((block, tx)),
    }
  }

  fn store(self) -> Self::Value {
    (self.owner.store(), self.manifest.block, self.manifest.tx)
  }
}
