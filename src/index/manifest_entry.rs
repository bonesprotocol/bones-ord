use super::*;
use bitcoin::ScriptHash;

pub type ManifestIdValue = (u64, u32);

impl Entry for ManifestId {
  type Value = ManifestIdValue;

  fn load((block, tx): Self::Value) -> Self {
    Self { block, tx }
  }

  fn store(self) -> Self::Value {
    (self.block, self.tx)
  }
}

#[derive(Debug, Hash, Eq, PartialEq, PartialOrd, Ord, Copy, Clone, Serialize, Deserialize)]
pub struct Minter(pub ScriptHash);

impl Default for Minter {
  fn default() -> Self {
    Self(ScriptHash::all_zeros())
  }
}

pub type MinterValue = [u8; 20];

impl Entry for Minter {
  type Value = MinterValue;

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

// Define the manifest entry value type.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct ManifestEntry {
  pub left_parent: Option<ManifestId>,
  pub right_parent: Option<ManifestId>,
  pub number: u64,
  /// sequence number of the inscription which has the content of the manifest
  pub inscription_number: u32,
  pub title: Option<String>,
}

// Storage alias.
pub type ManifestEntryValue = (Option<ManifestIdValue>, Option<ManifestIdValue>, u64, u32, Option<String>);

impl Entry for ManifestEntry {
  type Value = ManifestEntryValue;

  fn load(value: Self::Value) -> Self {
    let (left_parent, right_parent, number, inscription_number, title) = value;
    Self {
      left_parent: left_parent.map(ManifestId::load),
      right_parent: left_parent.map(ManifestId::load),
      number,
      inscription_number,
      title,
    }
  }

  fn store(self) -> Self::Value {
    (
      self.left_parent.map(ManifestId::store),
      self.right_parent.map(ManifestId::store),
      self.number,
      self.inscription_number,
      self.title,
    )
  }
}

pub type ManifestedMinterValue = (MinterValue, u64, u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestedMinter {
  pub minter: Minter,
  pub manifest: ManifestId,
}

impl Entry for ManifestedMinter {
  type Value = ManifestedMinterValue;

  fn load((minter_bytes, block, tx): Self::Value) -> Self {
    Self {
      minter: Minter::load(minter_bytes),
      manifest: ManifestId::load((block, tx)),
    }
  }

  fn store(self) -> Self::Value {
    (self.minter.store(), self.manifest.block, self.manifest.tx)
  }
}
