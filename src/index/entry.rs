use {
  super::*,
  crate::{sat::Sat, sat_point::SatPoint},
};

pub(crate) trait Entry: Sized {
  type Value;

  fn load(value: Self::Value) -> Self;

  fn store(self) -> Self::Value;
}

pub(super) type BlockHashValue = [u8; 32];

impl Entry for BlockHash {
  type Value = BlockHashValue;

  fn load(value: Self::Value) -> Self {
    BlockHash::from_inner(value)
  }

  fn store(self) -> Self::Value {
    self.into_inner()
  }
}

pub(crate) type TxidValue = [u8; 32];

impl Entry for Txid {
  type Value = TxidValue;

  fn load(value: Self::Value) -> Self {
    Txid::from_inner(value)
  }

  fn store(self) -> Self::Value {
    self.into_inner()
  }
}

#[derive(Clone)]
pub(crate) struct InscriptionEntry {
  pub(crate) charms: u16,
  pub(crate) fee: u64,
  pub(crate) height: u32,
  pub(crate) id: InscriptionId,
  pub(crate) inscription_number: u64,
  pub(crate) parents: Vec<u32>,
  pub(crate) sat: Option<Sat>,
  pub(crate) sequence_number: u32,
  pub(crate) timestamp: u32,
}

pub(crate) type InscriptionEntryValue = (
  u16,                // charms
  u64,                // fee
  u32,                // height
  InscriptionIdValue, // inscription id
  u64,                // inscription number
  Vec<u32>,           // parents
  Option<u64>,        // sat
  u32,                // sequence number
  u32,                // timestamp
);

impl Entry for InscriptionEntry {
  type Value = InscriptionEntryValue;

  fn load(
    (charms, fee, height, id, inscription_number, parents, sat, sequence_number, timestamp): InscriptionEntryValue,
  ) -> Self {
    Self {
      charms,
      fee,
      height,
      id: InscriptionId::load(id),
      inscription_number,
      parents,
      sat: sat.map(Sat),
      sequence_number,
      timestamp,
    }
  }

  fn store(self) -> Self::Value {
    (
      self.charms,
      self.fee,
      self.height,
      self.id.store(),
      self.inscription_number,
      self.parents,
      self.sat.map(Sat::n),
      self.sequence_number,
      self.timestamp,
    )
  }
}

pub type InscriptionIdValue = [u8; 36];

impl Entry for InscriptionId {
  type Value = InscriptionIdValue;

  fn load(value: Self::Value) -> Self {
    let (txid, index) = value.split_at(32);
    Self {
      txid: Txid::from_inner(txid.try_into().unwrap()),
      index: u32::from_be_bytes(index.try_into().unwrap()),
    }
  }

  fn store(self) -> Self::Value {
    let mut value = [0; 36];
    let (txid, index) = value.split_at_mut(32);
    txid.copy_from_slice(self.txid.as_inner());
    index.copy_from_slice(&self.index.to_be_bytes());
    value
  }
}

pub(crate) struct OutPointMap {
  pub(crate) value: u64,
  pub(crate) address: [u8; 34],
}

pub(crate) type OutPointMapValue = (u64, [u8; 34]);

impl Entry for OutPointMap {
  type Value = OutPointMapValue;

  fn load(value: Self::Value) -> Self {
    Self {
      value: value.0,
      address: value.1,
    }
  }

  fn store(self) -> Self::Value {
    (self.value, self.address)
  }
}

pub type OutPointValue = [u8; 36];

impl Entry for OutPoint {
  type Value = OutPointValue;

  fn load(value: Self::Value) -> Self {
    Decodable::consensus_decode(&mut io::Cursor::new(value)).unwrap()
  }

  fn store(self) -> Self::Value {
    let mut value = [0; 36];
    self.consensus_encode(&mut value.as_mut_slice()).unwrap();
    value
  }
}

pub(super) type SatPointValue = [u8; 44];

impl Entry for SatPoint {
  type Value = SatPointValue;

  fn load(value: Self::Value) -> Self {
    Decodable::consensus_decode(&mut io::Cursor::new(value)).unwrap()
  }

  fn store(self) -> Self::Value {
    let mut value = [0; 44];
    self.consensus_encode(&mut value.as_mut_slice()).unwrap();
    value
  }
}

pub(super) type SatRange = (u64, u64);

impl Entry for SatRange {
  type Value = [u8; 11];

  fn load([b0, b1, b2, b3, b4, b5, b6, b7, b8, b9, b10]: Self::Value) -> Self {
    let raw_base = u64::from_le_bytes([b0, b1, b2, b3, b4, b5, b6, 0]);

    // 51 bit base
    let base = raw_base & ((1 << 51) - 1);

    let raw_delta = u64::from_le_bytes([b6, b7, b8, b9, b10, 0, 0, 0]);

    // 33 bit delta
    let delta = raw_delta >> 3;

    (base, base + delta)
  }

  fn store(self) -> Self::Value {
    let base = self.0;
    let delta = self.1 - self.0;
    let n = u128::from(base) | u128::from(delta) << 51;
    n.to_le_bytes()[0..11].try_into().unwrap()
  }
}
