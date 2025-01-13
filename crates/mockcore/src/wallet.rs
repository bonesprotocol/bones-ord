use super::*;
use bitcoin::schnorr::TapTweak;
use bitcoin::secp256k1::Signature;
use bitcoin::util::sighash;
use bitcoin::util::sighash::SighashCache;

#[derive(Debug)]
pub struct Wallet {
  address_indices: HashMap<Address, u32>,
  master_key: ExtendedPrivKey,
  network: Network,
  next_index: u32,
  secp: Secp256k1<secp256k1::All>,
  derivation_path: DerivationPath,
}

impl Wallet {
  pub fn new(network: Network) -> Self {
    let derivation_path = DerivationPath::master()
      .child(ChildNumber::Hardened { index: 86 })
      .child(ChildNumber::Hardened { index: 0 })
      .child(ChildNumber::Hardened { index: 0 })
      .child(ChildNumber::Normal { index: 0 });

    Self {
      address_indices: HashMap::new(),
      master_key: ExtendedPrivKey::new_master(network, &[]).unwrap(),
      network,
      next_index: 0,
      secp: Secp256k1::new(),
      derivation_path,
    }
  }

  pub fn new_address(&mut self) -> Address {
    let address = {
      let derived_key = self
        .master_key
        .derive_priv(
          &self.secp,
          &self.derivation_path.child(ChildNumber::Normal {
            index: self.next_index,
          }),
        )
        .unwrap();

      let keypair = derived_key.to_keypair(&self.secp);
      let (internal_key, _parity) = XOnlyPublicKey::from_keypair(&keypair);

      let script = Script::new_v1_p2tr(&self.secp, internal_key, None);

      Address::from_script(&script, self.network).unwrap()
    };

    self
      .address_indices
      .insert(address.clone(), self.next_index);
    self.next_index += 1;

    address
  }
}
