use bitcoin::blockdata::constants::MAX_SCRIPT_ELEMENT_SIZE;
use {
  super::*,
  enshrining::{BoostTerms, MultiMint, PriceModel},
  flag::Flag,
  manifest::Manifest,
  message::Message,
  tag::Tag,
};

mod flag;
mod message;
mod tag;

/// Relic protocol message
#[derive(Default, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct Keepsake {
  /// allocation of Relics to outputs
  pub transfers: Vec<Transfer>,
  /// output number to receive unallocated Relics,
  /// if not specified the first non-OP_RETURN output is used
  pub pointer: Option<u32>,
  /// if set any tokens claimable by the script of the given output will be allocated
  /// note: the script on the given output must match the "owner" output of the enshrining
  pub claim: Option<u32>,
  /// seal a Relic Ticker
  pub sealing: bool,
  /// enshrine a previously sealed Relic
  pub enshrining: Option<Enshrining>,
  /// create a manifest
  pub manifest: Option<Manifest>,
  /// mint given Relic
  pub mint: Option<RelicId>,
  /// multi mint (also unmint) given Relic
  pub multi_mint: Option<MultiMint>,
  // unmint
  pub unmint: Option<RelicId>,
  /// execute token swap
  pub swap: Option<Swap>,
  /// summon a Syndicate
  pub summoning: Option<Summoning>,
  /// encase Relics into a non-fungible container
  pub encasing: Option<SyndicateId>,
  /// release a Chest
  pub release: bool,
}

#[derive(Debug, PartialEq)]
enum Payload {
  Valid(Vec<u8>),
  Invalid(RelicFlaw),
}
impl Keepsake {
  /// Runes use 13, Relics use 14
  pub const MAGIC_NUMBER: opcodes::All = opcodes::all::OP_PUSHNUM_14;
  pub const COMMIT_CONFIRMATIONS: u16 = 6;

  pub fn decipher(transaction: &Transaction) -> Option<RelicArtifact> {
    let payload = match Keepsake::payload(transaction) {
      Some(Payload::Valid(payload)) => payload,
      Some(Payload::Invalid(flaw)) => {
        return Some(RelicArtifact::Cenotaph(RelicCenotaph { flaw: Some(flaw) }));
      }
      None => return None,
    };

    let Ok(integers) = Keepsake::integers(&payload) else {
      return Some(RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::Varint),
      }));
    };

    let Message {
      mut flaw,
      transfers,
      mut fields,
    } = Message::from_integers(transaction, &integers);

    let mut flags = Tag::Flags
      .take(&mut fields, |[flags]| Some(flags))
      .unwrap_or_default();

    let get_non_zero = |tag: Tag, fields: &mut HashMap<u128, VecDeque<u128>>| -> Option<u128> {
      tag.take(fields, |[value]| (value > 0).then_some(value))
    };

    let get_output_option = |tag: Tag, fields: &mut HashMap<u128, VecDeque<u128>>| -> Option<u32> {
      tag.take(fields, |[value]| {
        let value = u32::try_from(value).ok()?;
        (u64::from(value) < u64::try_from(transaction.output.len()).unwrap()).then_some(value)
      })
    };

    let get_relic_id = |tag: Tag, fields: &mut HashMap<u128, VecDeque<u128>>| -> Option<RelicId> {
      tag.take(fields, |[block, tx]| {
        RelicId::new(block.try_into().ok()?, tx.try_into().ok()?)
      })
    };

    let sealing = Flag::Sealing.take(&mut flags);
    let release = Flag::Release.take(&mut flags);

    let manifest = Flag::Manifest.take(&mut flags).then(|| Manifest {
      // Try to take the left/right parent values from the fields.
      left_parent: Tag::LeftParent.take(&mut fields, |[val]| Some(val)),
      right_parent: Tag::RightParent.take(&mut fields, |[val]| Some(val)),
    });

    let enshrining = Flag::Enshrining.take(&mut flags).then(|| Enshrining {
      boost_terms: Flag::BoostTerms.take(&mut flags).then(|| BoostTerms {
        rare_chance: Tag::RareChance.take(&mut fields, |[val]| u32::try_from(val).ok()),
        rare_multiplier: Tag::RareMultiplier.take(&mut fields, |[val]| u16::try_from(val).ok()),
        ultra_rare_chance: Tag::UltraRareChance.take(&mut fields, |[val]| u32::try_from(val).ok()),
        ultra_rare_multiplier: Tag::UltraRareMultiplier
          .take(&mut fields, |[val]| u16::try_from(val).ok()),
      }),
      symbol: Tag::Symbol.take(&mut fields, |[symbol]| {
        char::from_u32(u32::try_from(symbol).ok()?)
      }),
      subsidy: get_non_zero(Tag::Subsidy, &mut fields),
      mint_terms: Flag::MintTerms.take(&mut flags).then(|| MintTerms {
        amount: Tag::Amount.take(&mut fields, |[amount]| Some(amount)),
        block_cap: Tag::BlockCap.take(&mut fields, |[val]| u32::try_from(val).ok()),
        cap: Tag::Cap.take(&mut fields, |[cap]| Some(cap)),
        manifest: Tag::Manifest.take(&mut fields, |[block, tx]| {
          RelicId::new(block.try_into().ok()?, tx.try_into().ok()?)
        }),
        max_per_tx: Tag::MaxPerTx.take(&mut fields, |[val]| u8::try_from(val).ok()),
        max_unmints: Tag::MaxUnmints.take(&mut fields, |[val]| u32::try_from(val).ok()),
        price: Tag::Price
          .take(&mut fields, |values: [u128; 1]| {
            Some(PriceModel::Fixed(values[0]))
          })
          .or_else(|| {
            Tag::Price.take(&mut fields, |values: [u128; 4]| {
              if values[0] == 0 {
                Some(PriceModel::Formula {
                  a: values[1],
                  b: values[2],
                  c: values[3],
                })
              } else {
                None
              }
            })
          }),
        seed: get_non_zero(Tag::Seed, &mut fields),
        swap_height: Tag::SwapHeight.take(&mut fields, |[height]| u64::try_from(height).ok()),
      }),
      turbo: Flag::Turbo.take(&mut flags),
    });

    if manifest.is_some() && enshrining.is_some() {
      flaw.get_or_insert(RelicFlaw::EnshriningAndManifest);
    }

    let mint = get_relic_id(Tag::Mint, &mut fields);
    let unmint = get_relic_id(Tag::Unmint, &mut fields);

    let multi_mint = if let Some(is_unmint) = if Flag::MultiMint.take(&mut flags) {
      Some(false)
    } else if Flag::MultiUnmint.take(&mut flags) {
      Some(true)
    } else {
      None
    } {
      let count = Tag::MultiMintCount.take(&mut fields, |[val]| u8::try_from(val).ok())?;
      let base_limit = Tag::MultiMintBaseLimit.take(&mut fields, |[val]| Some(val))?;
      let relic = Tag::MultiMintRelic.take(&mut fields, |[block, tx]| {
        RelicId::new(block.try_into().ok()?, tx.try_into().ok()?)
      })?;
      Some(MultiMint {
        count,
        base_limit,
        relic,
        is_unmint,
      })
    } else {
      None
    };

    let swap = Flag::Swap.take(&mut flags).then(|| Swap {
      input: get_relic_id(Tag::SwapInput, &mut fields),
      output: get_relic_id(Tag::SwapOutput, &mut fields),
      input_amount: get_non_zero(Tag::SwapInputAmount, &mut fields),
      output_amount: get_non_zero(Tag::SwapOutputAmount, &mut fields),
      is_exact_input: Flag::SwapExactInput.take(&mut flags),
    });

    let summoning = Flag::Summoning.take(&mut flags).then(|| Summoning {
      treasure: Tag::Treasure.take(&mut fields, |[block, tx]| {
        RelicId::new(block.try_into().ok()?, tx.try_into().ok()?)
      }),
      height: (
        Tag::HeightStart.take(&mut fields, |[start_height]| {
          u64::try_from(start_height).ok()
        }),
        Tag::HeightEnd.take(&mut fields, |[start_height]| {
          u64::try_from(start_height).ok()
        }),
      ),
      cap: Tag::SyndicateCap.take(&mut fields, |[cap]| u32::try_from(cap).ok()),
      quota: Tag::Quota.take(&mut fields, |[quota]| Some(quota)),
      royalty: Tag::Royalty.take(&mut fields, |[royalty]| Some(royalty)),
      gated: Flag::Gated.take(&mut flags),
      lock: Tag::Lock.take(&mut fields, |[lock]| u64::try_from(lock).ok()),
      reward: Tag::Reward.take(&mut fields, |[reward]| Some(reward)),
      lock_subsidy: Flag::LockSubsidy.take(&mut flags),
      turbo: Flag::Turbo.take(&mut flags),
    });

    let encasing = get_relic_id(Tag::Syndicate, &mut fields);
    let pointer = get_output_option(Tag::Pointer, &mut fields);
    let claim = get_output_option(Tag::Claim, &mut fields);

    // Check if both enshrining and summoning are present
    if enshrining.is_some() && summoning.is_some() {
      flaw.get_or_insert(RelicFlaw::EnshriningAndSummoning);
    }

    // check for overflows or if mint terms are given, but the cap is zero
    if let Some(enshrining) = enshrining {
      let terms_valid = enshrining.mint_terms.as_ref().map_or(false, |terms| {
        // Require a nonzero cap.
        if let Some(cap) = terms.cap {
          if cap == 0 {
            return false;
          }
          // Check that the total mint supply (cap × amount) does not overflow
          if let Some(amount) = terms.amount {
            if cap.checked_mul(amount).is_none() {
              return false;
            }
          }
          // If max_per_tx is set, check that (max_per_tx as u128) × amount doesn't overflow.
          if let Some(max_tx) = terms.max_per_tx {
            if let Some(amount) = terms.amount {
              if (max_tx as u128).checked_mul(amount).is_none() {
                return false;
              }
            }
          }
          // If block_cap is set, check that block_cap × amount doesn't overflow.
          if let Some(max_block) = terms.block_cap {
            if let Some(amount) = terms.amount {
              if (max_block as u128).checked_mul(amount).is_none() {
                return false;
              }
            }
          }
          match terms.price {
            Some(PriceModel::Fixed(price)) => {
              // For fixed pricing, check multiplication doesn't overflow.
              cap.checked_mul(price).is_some()
            }
            Some(PriceModel::Formula { a, b, c }) => {
              // For formula pricing:
              //   • c must be nonzero (avoid division by zero)
              //   • a >= b / c (avoid underflow at x=0)
              //   • cap must not exceed 1,000,000
              c > 0 && (b / c) <= a && cap <= 1_000_000
            }
            None => false,
          }
        } else {
          false
        }
      });
      let boost_valid = enshrining.boost_terms.map_or(true, |boost| {
        let rare_valid = if boost.rare_chance.is_some() || boost.rare_multiplier.is_some() {
          if let (Some(rc), Some(rm)) = (boost.rare_chance, boost.rare_multiplier) {
            rc != 0 && rm > 1
          } else {
            false
          }
        } else {
          true
        };
        let ultra_valid =
          if boost.ultra_rare_chance.is_some() || boost.ultra_rare_multiplier.is_some() {
            if let (Some(urc), Some(urm)) = (boost.ultra_rare_chance, boost.ultra_rare_multiplier) {
              urc != 0 && urm > 1
            } else {
              false
            }
          } else {
            true
          };
        let multiplier_valid = enshrining
          .mint_terms
          .as_ref()
          .and_then(|terms| terms.amount)
          .map_or(true, |amount| {
            let rare_mul_valid = boost
              .rare_multiplier
              .map_or(true, |rm| amount.checked_mul(rm as u128).is_some());
            let ultra_mul_valid = boost
              .ultra_rare_multiplier
              .map_or(true, |urm| amount.checked_mul(urm as u128).is_some());
            rare_mul_valid && ultra_mul_valid
          });
        rare_valid && ultra_valid && multiplier_valid
      });
      if !boost_valid || !terms_valid || enshrining.max_supply().is_none() {
        flaw.get_or_insert(RelicFlaw::InvalidEnshrining);
      }
    }

    // the base token must not be minted the usual way,
    // instead it is minted by burning eligible inscriptions
    if mint.map(|id| id == RELIC_ID).unwrap_or_default() {
      flaw.get_or_insert(RelicFlaw::InvalidBaseTokenMint);
    }

    // base token is not unmintable but check for extra security here
    if unmint.map(|id| id == RELIC_ID).unwrap_or_default() {
      flaw.get_or_insert(RelicFlaw::InvalidBaseTokenUnmint);
    }

    // Additionally, base token must not be multi minted.
    if multi_mint
      .as_ref()
      .map(|m| m.relic == RELIC_ID)
      .unwrap_or(false)
    {
      flaw.get_or_insert(RelicFlaw::InvalidBaseTokenMint);
    }

    // make sure to not swap from and to the same token
    if swap
      .map(|swap| swap.input.unwrap_or(RELIC_ID) == swap.output.unwrap_or(RELIC_ID))
      .unwrap_or_default()
    {
      flaw.get_or_insert(RelicFlaw::InvalidSwap);
    }

    if flags != 0 {
      flaw.get_or_insert(RelicFlaw::UnrecognizedFlag);
    }

    if fields.keys().any(|tag| tag % 2 == 0) {
      flaw.get_or_insert(RelicFlaw::UnrecognizedEvenTag);
    }

    if let Some(flaw) = flaw {
      return Some(RelicArtifact::Cenotaph(RelicCenotaph { flaw: Some(flaw) }));
    }

    Some(RelicArtifact::Keepsake(Self {
      transfers,
      pointer,
      claim,
      sealing,
      enshrining,
      manifest,
      mint,
      multi_mint,
      unmint,
      swap,
      summoning,
      encasing,
      release,
    }))
  }

  fn encipher_internal(&self) -> Vec<u8> {
    let mut payload = Vec::new();
    let mut flags = 0;

    if self.sealing {
      Flag::Sealing.set(&mut flags);
    }

    if self.release {
      Flag::Release.set(&mut flags);
    }

    if let Some(manifest) = self.manifest {
      Flag::Manifest.set(&mut flags);
      Tag::LeftParent.encode_option(manifest.left_parent, &mut payload);
      Tag::RightParent.encode_option(manifest.right_parent, &mut payload);
    }

    if let Some(enshrining) = self.enshrining {
      Flag::Enshrining.set(&mut flags);

      if enshrining.turbo {
        Flag::Turbo.set(&mut flags);
      }

      Tag::Symbol.encode_option(enshrining.symbol, &mut payload);
      Tag::Subsidy.encode_option(enshrining.subsidy, &mut payload);

      if let Some(boost) = enshrining.boost_terms {
        Flag::BoostTerms.set(&mut flags);
        Tag::RareChance.encode_option(boost.rare_chance, &mut payload);
        Tag::RareMultiplier.encode_option(boost.rare_multiplier, &mut payload);
        Tag::UltraRareChance.encode_option(boost.ultra_rare_chance, &mut payload);
        Tag::UltraRareMultiplier.encode_option(boost.ultra_rare_multiplier, &mut payload);
      }

      if let Some(terms) = enshrining.mint_terms {
        Flag::MintTerms.set(&mut flags);
        Tag::Amount.encode_option(terms.amount, &mut payload);
        Tag::BlockCap.encode_option(terms.block_cap, &mut payload);
        Tag::MaxPerTx.encode_option(terms.max_per_tx, &mut payload);
        Tag::Cap.encode_option(terms.cap, &mut payload);
        if let Some(price_model) = terms.price {
          match price_model {
            PriceModel::Fixed(price) => {
              // Legacy fixed price: encode as a single integer.
              Tag::Price.encode([price], &mut payload);
            }
            PriceModel::Formula { a, b, c } => {
              // New formula: encode a marker (0) then a, b, and c.
              Tag::Price.encode([0, a, b, c], &mut payload);
            }
          }
        }
        Tag::Seed.encode_option(terms.seed, &mut payload);
        Tag::SwapHeight.encode_option(terms.swap_height, &mut payload);
      }
    }

    if let Some(RelicId { block, tx }) = self.mint {
      Tag::Mint.encode([block.into(), tx.into()], &mut payload);
    }

    if let Some(multi) = self.multi_mint {
      if multi.is_unmint {
        Flag::MultiUnmint.set(&mut flags);
      } else {
        Flag::MultiMint.set(&mut flags);
      }
      Tag::MultiMintCount.encode([multi.count as u128], &mut payload);
      Tag::MultiMintBaseLimit.encode([multi.base_limit], &mut payload);
      Tag::MultiMintRelic.encode(
        [multi.relic.block.into(), multi.relic.tx.into()],
        &mut payload,
      );
    }

    if let Some(swap) = &self.swap {
      Flag::Swap.set(&mut flags);

      if swap.is_exact_input {
        Flag::SwapExactInput.set(&mut flags);
      }

      if let Some(RelicId { block, tx }) = swap.input {
        Tag::SwapInput.encode([block.into(), tx.into()], &mut payload);
      }
      if let Some(RelicId { block, tx }) = swap.output {
        Tag::SwapOutput.encode([block.into(), tx.into()], &mut payload);
      }
      Tag::SwapInputAmount.encode_option(swap.input_amount, &mut payload);
      Tag::SwapOutputAmount.encode_option(swap.output_amount, &mut payload);
    }

    if let Some(summoning) = self.summoning {
      Flag::Summoning.set(&mut flags);

      if summoning.gated {
        Flag::Gated.set(&mut flags);
      }

      if summoning.lock_subsidy {
        Flag::LockSubsidy.set(&mut flags);
      }

      if summoning.turbo {
        Flag::Turbo.set(&mut flags);
      }

      if let Some(RelicId { block, tx }) = summoning.treasure {
        Tag::Treasure.encode([block.into(), tx.into()], &mut payload);
      }
      Tag::SyndicateCap.encode_option(summoning.cap, &mut payload);
      Tag::Lock.encode_option(summoning.lock, &mut payload);
      Tag::HeightStart.encode_option(summoning.height.0, &mut payload);
      Tag::HeightEnd.encode_option(summoning.height.1, &mut payload);
      Tag::Quota.encode_option(summoning.quota, &mut payload);
      Tag::Royalty.encode_option(summoning.royalty, &mut payload);
      Tag::Reward.encode_option(summoning.reward, &mut payload);
    }

    if let Some(SyndicateId { block, tx }) = self.encasing {
      Tag::Syndicate.encode([block.into(), tx.into()], &mut payload);
    }

    if flags != 0 {
      Tag::Flags.encode([flags], &mut payload);
    }

    Tag::Pointer.encode_option(self.pointer, &mut payload);
    Tag::Claim.encode_option(self.claim, &mut payload);

    if !self.transfers.is_empty() {
      varint::encode_to_vec(Tag::Body.into(), &mut payload);

      let mut transfers = self.transfers.clone();
      transfers.sort_by_key(|transfer| transfer.id);

      let mut previous = RelicId::default();
      for transfer in transfers {
        let (block, tx) = previous.delta(transfer.id).unwrap();
        varint::encode_to_vec(block, &mut payload);
        varint::encode_to_vec(tx, &mut payload);
        varint::encode_to_vec(transfer.amount, &mut payload);
        varint::encode_to_vec(transfer.output.into(), &mut payload);
        previous = transfer.id;
      }
    }
    payload
  }

  pub fn encipher(&self) -> Script {
    let mut builder = script::Builder::new()
      .push_opcode(opcodes::all::OP_RETURN)
      .push_opcode(Keepsake::MAGIC_NUMBER);

    for chunk in self.encipher_internal().chunks(MAX_SCRIPT_ELEMENT_SIZE) {
      builder = builder.push_slice(chunk);
    }

    builder.into_script()
  }

  fn payload(transaction: &Transaction) -> Option<Payload> {
    // search transaction outputs for payload
    for output in &transaction.output {
      let mut instructions = output.script_pubkey.instructions();

      // payload starts with OP_RETURN
      if instructions.next() != Some(Ok(Instruction::Op(opcodes::all::OP_RETURN))) {
        continue;
      }

      // followed by the protocol identifier, ignoring errors, since OP_RETURN
      // scripts may be invalid
      if instructions.next() != Some(Ok(Instruction::Op(Keepsake::MAGIC_NUMBER))) {
        continue;
      }

      // construct the payload by concatenating remaining data pushes
      let mut payload = Vec::new();

      for result in instructions {
        match result {
          Ok(Instruction::PushBytes(push)) => {
            payload.extend_from_slice(push);
          }
          Ok(Instruction::Op(_)) => {
            return Some(Payload::Invalid(RelicFlaw::Opcode));
          }
          Err(_) => {
            return Some(Payload::Invalid(RelicFlaw::InvalidScript));
          }
        }
      }

      return Some(Payload::Valid(payload));
    }

    None
  }

  fn integers(payload: &[u8]) -> Result<Vec<u128>, varint::Error> {
    let mut integers = Vec::new();
    let mut i = 0;

    while i < payload.len() {
      let (integer, length) = varint::decode(&payload[i..])?;
      integers.push(integer);
      i += length;
    }

    Ok(integers)
  }
}

#[cfg(test)]
mod tests {
  use {
    super::*,
    bitcoin::{blockdata::locktime::PackedLockTime, OutPoint, Sequence, TxIn, TxOut, Witness},
    pretty_assertions::assert_eq,
  };

  pub(crate) fn relic_id(tx: u32) -> RelicId {
    RelicId { block: 1, tx }
  }

  fn decipher(integers: &[u128]) -> RelicArtifact {
    let payload = payload(integers);

    let payload = payload.as_slice().try_into().unwrap();

    Keepsake::decipher(&Transaction {
      input: Vec::new(),
      output: vec![TxOut {
        script_pubkey: script::Builder::new()
          .push_opcode(opcodes::all::OP_RETURN)
          .push_opcode(Keepsake::MAGIC_NUMBER)
          .push_slice(payload)
          .into_script(),
        value: 0,
      }],
      lock_time: PackedLockTime::ZERO,
      version: 2,
    })
    .unwrap()
  }

  fn payload(integers: &[u128]) -> Vec<u8> {
    let mut payload = Vec::new();

    for integer in integers {
      payload.extend(varint::encode(*integer));
    }

    payload
  }

  #[test]
  fn decipher_returns_none_if_first_opcode_is_malformed() {
    assert_eq!(
      Keepsake::decipher(&Transaction {
        input: Vec::new(),
        output: vec![TxOut {
          script_pubkey: Script::from(vec![opcodes::all::OP_PUSHBYTES_4.to_u8()]),
          value: 0,
        }],
        lock_time: PackedLockTime::ZERO,
        version: 2,
      }),
      None,
    );
  }

  #[test]
  fn deciphering_transaction_with_no_outputs_returns_none() {
    assert_eq!(
      Keepsake::decipher(&Transaction {
        input: Vec::new(),
        output: Vec::new(),
        lock_time: PackedLockTime::ZERO,
        version: 2,
      }),
      None,
    );
  }

  #[test]
  fn deciphering_transaction_with_non_op_return_output_returns_none() {
    assert_eq!(
      Keepsake::decipher(&Transaction {
        input: Vec::new(),
        output: vec![TxOut {
          script_pubkey: script::Builder::new().push_slice(&[]).into_script(),
          value: 0
        }],
        lock_time: PackedLockTime::ZERO,
        version: 2,
      }),
      None,
    );
  }

  #[test]
  fn deciphering_transaction_with_bare_op_return_returns_none() {
    assert_eq!(
      Keepsake::decipher(&Transaction {
        input: Vec::new(),
        output: vec![TxOut {
          script_pubkey: script::Builder::new()
            .push_opcode(opcodes::all::OP_RETURN)
            .into_script(),
          value: 0
        }],
        lock_time: PackedLockTime::ZERO,
        version: 2,
      }),
      None,
    );
  }

  #[test]
  fn deciphering_transaction_with_non_matching_op_return_returns_none() {
    assert_eq!(
      Keepsake::decipher(&Transaction {
        input: Vec::new(),
        output: vec![TxOut {
          script_pubkey: script::Builder::new()
            .push_opcode(opcodes::all::OP_RETURN)
            .push_slice(b"FOOO")
            .into_script(),
          value: 0
        }],
        lock_time: PackedLockTime::ZERO,
        version: 2,
      }),
      None,
    );
  }

  #[test]
  fn deciphering_valid_runestone_with_invalid_script_postfix_returns_invalid_payload() {
    let mut script_pubkey = script::Builder::new()
      .push_opcode(opcodes::all::OP_RETURN)
      .push_opcode(Keepsake::MAGIC_NUMBER)
      .into_script()
      .into_bytes();

    script_pubkey.push(opcodes::all::OP_PUSHBYTES_4.to_u8());

    assert_eq!(
      Keepsake::payload(&Transaction {
        input: Vec::new(),
        output: vec![TxOut {
          script_pubkey: Script::from(script_pubkey),
          value: 0,
        }],
        lock_time: PackedLockTime::ZERO,
        version: 2,
      }),
      Some(Payload::Invalid(RelicFlaw::InvalidScript))
    );
  }

  #[test]
  fn deciphering_runestone_with_truncated_varint_succeeds() {
    Keepsake::decipher(&Transaction {
      input: Vec::new(),
      output: vec![TxOut {
        script_pubkey: script::Builder::new()
          .push_opcode(opcodes::all::OP_RETURN)
          .push_opcode(Keepsake::MAGIC_NUMBER)
          .push_slice(&[128])
          .into_script(),
        value: 0,
      }],
      lock_time: PackedLockTime::ZERO,
      version: 2,
    })
    .unwrap();
  }

  #[test]
  fn outputs_with_non_pushdata_opcodes_are_cenotaph() {
    assert_eq!(
      Keepsake::decipher(&Transaction {
        input: Vec::new(),
        output: vec![
          TxOut {
            script_pubkey: script::Builder::new()
              .push_opcode(opcodes::all::OP_RETURN)
              .push_opcode(Keepsake::MAGIC_NUMBER)
              .push_opcode(opcodes::all::OP_VERIFY)
              .push_slice(&[0])
              .push_slice(varint::encode(1).as_slice().try_into().unwrap())
              .push_slice(varint::encode(1).as_slice().try_into().unwrap())
              .push_slice(&[2, 0])
              .into_script(),
            value: 0,
          },
          TxOut {
            script_pubkey: script::Builder::new()
              .push_opcode(opcodes::all::OP_RETURN)
              .push_opcode(Keepsake::MAGIC_NUMBER)
              .push_slice(&[0])
              .push_slice(varint::encode(1).as_slice().try_into().unwrap())
              .push_slice(varint::encode(2).as_slice().try_into().unwrap())
              .push_slice(&[3, 0])
              .into_script(),
            value: 0,
          },
        ],
        lock_time: PackedLockTime::ZERO,
        version: 2,
      })
      .unwrap(),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::Opcode),
      }),
    );
  }

  #[test]
  fn pushnum_opcodes_in_runestone_produce_cenotaph() {
    assert_eq!(
      Keepsake::decipher(&Transaction {
        input: Vec::new(),
        output: vec![TxOut {
          script_pubkey: script::Builder::new()
            .push_opcode(opcodes::all::OP_RETURN)
            .push_opcode(Keepsake::MAGIC_NUMBER)
            .push_opcode(opcodes::all::OP_PUSHNUM_1)
            .into_script(),
          value: 0,
        },],
        lock_time: PackedLockTime::ZERO,
        version: 2,
      })
      .unwrap(),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::Opcode),
      }),
    );
  }

  #[test]
  fn deciphering_empty_runestone_is_successful() {
    assert_eq!(
      Keepsake::decipher(&Transaction {
        input: Vec::new(),
        output: vec![TxOut {
          script_pubkey: script::Builder::new()
            .push_opcode(opcodes::all::OP_RETURN)
            .push_opcode(Keepsake::MAGIC_NUMBER)
            .into_script(),
          value: 0
        }],
        lock_time: PackedLockTime::ZERO,
        version: 2,
      })
      .unwrap(),
      RelicArtifact::Keepsake(Keepsake::default()),
    );
  }

  #[test]
  fn invalid_input_scripts_are_skipped_when_searching_for_runestone() {
    let payload = payload(&[Tag::Pointer.into(), 1]);

    let payload = payload.as_slice().try_into().unwrap();

    let script_pubkey = vec![
      opcodes::all::OP_RETURN.to_u8(),
      opcodes::all::OP_PUSHBYTES_9.to_u8(),
      Keepsake::MAGIC_NUMBER.to_u8(),
      opcodes::all::OP_PUSHBYTES_4.to_u8(),
    ];

    assert_eq!(
      Keepsake::decipher(&Transaction {
        input: Vec::new(),
        output: vec![
          TxOut {
            script_pubkey: Script::from(script_pubkey),
            value: 0,
          },
          TxOut {
            script_pubkey: script::Builder::new()
              .push_opcode(opcodes::all::OP_RETURN)
              .push_opcode(Keepsake::MAGIC_NUMBER)
              .push_slice(payload)
              .into_script(),
            value: 0,
          },
        ],
        lock_time: PackedLockTime::ZERO,
        version: 2,
      })
      .unwrap(),
      RelicArtifact::Keepsake(Keepsake {
        pointer: Some(1),
        ..default()
      }),
    );
  }

  #[test]
  fn deciphering_non_empty_runestone_is_successful() {
    assert_eq!(
      decipher(&[Tag::Body.into(), 1, 1, 2, 0]),
      RelicArtifact::Keepsake(Keepsake {
        transfers: vec![Transfer {
          id: relic_id(1),
          amount: 2,
          output: 0,
        }],
        ..default()
      }),
    );
  }

  #[test]
  fn decipher_enshrining() {
    assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Enshrining.mask(),
        Tag::Body.into(),
        1,
        1,
        2,
        0,
      ]),
      RelicArtifact::Keepsake(Keepsake {
        transfers: vec![Transfer {
          id: relic_id(1),
          amount: 2,
          output: 0,
        }],
        enshrining: Some(Enshrining::default()),
        ..default()
      }),
    );
  }

  #[test]
  fn decipher_etching_with_rune() {
    assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Enshrining.mask(),
        Tag::Symbol.into(),
        'R'.into(),
        Tag::Body.into(),
        1,
        1,
        2,
        0,
      ]),
      RelicArtifact::Keepsake(Keepsake {
        transfers: vec![Transfer {
          id: relic_id(1),
          amount: 2,
          output: 0,
        }],
        enshrining: Some(Enshrining {
          symbol: Some('R'),
          ..default()
        }),
        ..default()
      }),
    );
  }

  #[test]
  fn turbo_flag_without_etching_flag_produces_cenotaph() {
    assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Turbo.mask(),
        Tag::Body.into(),
        0,
        0,
        0,
        0,
      ]),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::UnrecognizedFlag),
      }),
    );
  }

  #[test]
  fn recognized_fields_without_flag_produces_cenotaph() {
    #[track_caller]
    fn case(integers: &[u128]) {
      assert_eq!(
        decipher(integers),
        RelicArtifact::Cenotaph(RelicCenotaph {
          flaw: Some(RelicFlaw::UnrecognizedEvenTag),
        }),
      );
    }

    case(&[Tag::Seed.into(), 0]);
    case(&[Tag::Amount.into(), 0]);
    case(&[Tag::Cap.into(), 0]);
    case(&[Tag::Price.into(), 0]);
    case(&[Tag::Mint.into(), 0]);
    case(&[Tag::SwapInput.into(), 0]);
    case(&[Tag::SwapOutput.into(), 0]);
    case(&[Tag::SwapInputAmount.into(), 0]);
    case(&[Tag::SwapOutputAmount.into(), 0]);

    // case(&[Tag::Flags.into(), Flag::Enshrining.into(), Tag::Cap.into(), 0]);
    // case(&[
    //   Tag::Flags.into(),
    //   Flag::Enshrining.into(),
    //   Tag::Amount.into(),
    //   0,
    // ]);
    // case(&[
    //   Tag::Flags.into(),
    //   Flag::Enshrining.into(),
    //   Tag::OffsetStart.into(),
    //   0,
    // ]);
    // case(&[
    //   Tag::Flags.into(),
    //   Flag::Enshrining.into(),
    //   Tag::OffsetEnd.into(),
    //   0,
    // ]);
    // case(&[
    //   Tag::Flags.into(),
    //   Flag::Enshrining.into(),
    //   Tag::HeightStart.into(),
    //   0,
    // ]);
    // case(&[
    //   Tag::Flags.into(),
    //   Flag::Enshrining.into(),
    //   Tag::HeightEnd.into(),
    //   0,
    // ]);
  }

  // #[test]
  // fn decipher_etching_with_term() {
  //   assert_eq!(
  //     decipher(&[
  //       Tag::Flags.into(),
  //       Flag::Enshrining.mask() | Flag::Terms.mask(),
  //       Tag::OffsetEnd.into(),
  //       4,
  //       Tag::Body.into(),
  //       1,
  //       1,
  //       2,
  //       0
  //     ]),
  //     Artifact::Keepsake(Keepsake {
  //       transfers: vec![Transfer {
  //         id: relic_id(1),
  //         amount: 2,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         terms: Some(Terms {
  //           offset: (None, Some(4)),
  //           ..default()
  //         }),
  //         ..default()
  //       }),
  //       ..default()
  //     }),
  //   );
  // }

  // #[test]
  // fn decipher_etching_with_amount() {
  //   assert_eq!(
  //     decipher(&[
  //       Tag::Flags.into(),
  //       Flag::Enshrining.mask() | Flag::Terms.mask(),
  //       Tag::Amount.into(),
  //       4,
  //       Tag::Body.into(),
  //       1,
  //       1,
  //       2,
  //       0
  //     ]),
  //     Artifact::Keepsake(Keepsake {
  //       transfers: vec![Transfer {
  //         id: relic_id(1),
  //         amount: 2,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         terms: Some(Terms {
  //           amount: Some(4),
  //           ..default()
  //         }),
  //         ..default()
  //       }),
  //       ..default()
  //     }),
  //   );
  // }

  #[test]
  fn invalid_varint_produces_cenotaph() {
    assert_eq!(
      Keepsake::decipher(&Transaction {
        input: Vec::new(),
        output: vec![TxOut {
          script_pubkey: script::Builder::new()
            .push_opcode(opcodes::all::OP_RETURN)
            .push_opcode(Keepsake::MAGIC_NUMBER)
            .push_slice(&[128])
            .into_script(),
          value: 0,
        }],
        lock_time: PackedLockTime::ZERO,
        version: 2,
      })
      .unwrap(),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::Varint),
      }),
    );
  }

  #[test]
  fn duplicate_even_tags_produce_cenotaph() {
    assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Enshrining.mask(),
        Tag::Seed.into(),
        4,
        Tag::Seed.into(),
        5,
        Tag::Body.into(),
        1,
        1,
        2,
        0,
      ]),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::UnrecognizedEvenTag),
      }),
    );
  }

  #[test]
  fn duplicate_odd_tags_are_ignored() {
    assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Enshrining.mask(),
        Tag::Symbol.into(),
        'a'.into(),
        Tag::Symbol.into(),
        'b'.into(),
        Tag::Body.into(),
        1,
        1,
        2,
        0,
      ]),
      RelicArtifact::Keepsake(Keepsake {
        transfers: vec![Transfer {
          id: relic_id(1),
          amount: 2,
          output: 0,
        }],
        enshrining: Some(Enshrining {
          symbol: Some('a'),
          ..default()
        }),
        ..default()
      })
    );
  }

  #[test]
  fn unrecognized_odd_tag_is_ignored() {
    assert_eq!(
      decipher(&[Tag::Nop.into(), 100, Tag::Body.into(), 1, 1, 2, 0]),
      RelicArtifact::Keepsake(Keepsake {
        transfers: vec![Transfer {
          id: relic_id(1),
          amount: 2,
          output: 0,
        }],
        ..default()
      }),
    );
  }

  #[test]
  fn runestone_with_unrecognized_even_tag_is_cenotaph() {
    assert_eq!(
      decipher(&[Tag::Cenotaph.into(), 0, Tag::Body.into(), 1, 1, 2, 0]),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::UnrecognizedEvenTag),
      }),
    );
  }

  #[test]
  fn runestone_with_unrecognized_flag_is_cenotaph() {
    assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Cenotaph.mask(),
        Tag::Body.into(),
        1,
        1,
        2,
        0
      ]),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::UnrecognizedFlag),
      }),
    );
  }

  #[test]
  fn runestone_with_edict_id_with_zero_block_and_nonzero_tx_is_cenotaph() {
    assert_eq!(
      decipher(&[Tag::Body.into(), 0, 1, 2, 0, 0, 0]),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::TransferRelicId),
      }),
    );
  }

  #[test]
  fn runestone_with_overflowing_edict_id_delta_is_cenotaph() {
    assert_eq!(
      decipher(&[Tag::Body.into(), 1, 0, 0, 0, u64::MAX.into(), 0, 0, 0]),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::TransferRelicId),
      }),
    );

    assert_eq!(
      decipher(&[Tag::Body.into(), 1, 1, 0, 0, 0, u64::MAX.into(), 0, 0,]),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::TransferRelicId),
      }),
    );
  }

  #[test]
  fn runestone_with_output_over_max_is_cenotaph() {
    assert_eq!(
      decipher(&[Tag::Body.into(), 1, 1, 2, 2, 0, 0]),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::TransferOutput),
      }),
    );
  }

  #[test]
  fn tag_with_no_value_is_cenotaph() {
    assert_eq!(
      decipher(&[Tag::Flags.into(), 1, Tag::Flags.into()]),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::TruncatedField),
      }),
    );
  }

  #[test]
  fn trailing_integers_in_body_is_cenotaph() {
    let mut integers = vec![Tag::Body.into(), 1, 1, 2, 0];

    for i in 0..4 {
      assert_eq!(
        decipher(&integers),
        if i == 0 {
          RelicArtifact::Keepsake(Keepsake {
            transfers: vec![Transfer {
              id: relic_id(1),
              amount: 2,
              output: 0,
            }],
            ..default()
          })
        } else {
          RelicArtifact::Cenotaph(RelicCenotaph {
            flaw: Some(RelicFlaw::TrailingIntegers),
          })
        }
      );

      integers.push(0);
    }
  }

  #[test]
  fn decipher_etching_with_supply() {
    assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Enshrining.mask(),
        Tag::Subsidy.into(),
        1234,
        Tag::Body.into(),
        1,
        1,
        2,
        0,
      ]),
      RelicArtifact::Keepsake(Keepsake {
        transfers: vec![Transfer {
          id: relic_id(1),
          amount: 2,
          output: 0,
        }],
        enshrining: Some(Enshrining {
          subsidy: Some(1234),
          ..default()
        }),
        ..default()
      }),
    );
  }

  // #[test]
  // fn divisibility_above_max_is_ignored() {
  //   assert_eq!(
  //     decipher(&[
  //       Tag::Flags.into(),
  //       Flag::Enshrining.mask(),
  //       Tag::Relic.into(),
  //       4,
  //       Tag::Divisibility.into(),
  //       (Enshrining::MAX_DIVISIBILITY + 1).into(),
  //       Tag::Body.into(),
  //       1,
  //       1,
  //       2,
  //       0,
  //     ]),
  //     RelicArtifact::Keepsake(Keepsake {
  //       transfers: vec![Transfer {
  //         id: relic_id(1),
  //         amount: 2,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(4)),
  //         ..default()
  //       }),
  //       ..default()
  //     }),
  //   );
  // }

  #[test]
  fn symbol_above_max_is_ignored() {
    assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Enshrining.mask(),
        Tag::Symbol.into(),
        u128::from(u32::from(char::MAX) + 1),
        Tag::Body.into(),
        1,
        1,
        2,
        0,
      ]),
      RelicArtifact::Keepsake(Keepsake {
        transfers: vec![Transfer {
          id: relic_id(1),
          amount: 2,
          output: 0,
        }],
        enshrining: Some(Enshrining::default()),
        ..default()
      }),
    );
  }

  #[test]
  fn decipher_etching_with_symbol() {
    assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Enshrining.mask(),
        Tag::Symbol.into(),
        'a'.into(),
        Tag::Body.into(),
        1,
        1,
        2,
        0,
      ]),
      RelicArtifact::Keepsake(Keepsake {
        transfers: vec![Transfer {
          id: relic_id(1),
          amount: 2,
          output: 0,
        }],
        enshrining: Some(Enshrining {
          symbol: Some('a'),
          ..default()
        }),
        ..default()
      }),
    );
  }

  #[test]
  fn decipher_etching_with_all_etching_tags() {
    assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Sealing.mask()
          | Flag::Enshrining.mask()
          | Flag::MintTerms.mask()
          | Flag::Swap.mask()
          | Flag::SwapExactInput.mask()
          | Flag::Release.mask()
          | Flag::Turbo.mask(),
        Tag::Symbol.into(),
        'a'.into(),
        Tag::Subsidy.into(),
        400,
        Tag::Amount.into(),
        100,
        Tag::Cap.into(),
        100_000,
        Tag::Price.into(),
        321,
        Tag::Seed.into(),
        300,
        Tag::SwapHeight.into(),
        400_000,
        Tag::Mint.into(),
        1,
        Tag::Mint.into(),
        5,
        Tag::SwapInput.into(),
        1,
        Tag::SwapInput.into(),
        42,
        Tag::SwapOutput.into(),
        1,
        Tag::SwapOutput.into(),
        43,
        Tag::SwapInputAmount.into(),
        123,
        Tag::SwapOutputAmount.into(),
        456,
        Tag::Pointer.into(),
        0,
        Tag::Claim.into(),
        0,
        Tag::Body.into(),
        1,
        1,
        2,
        0,
      ]),
      RelicArtifact::Keepsake(Keepsake {
        transfers: vec![Transfer {
          id: relic_id(1),
          amount: 2,
          output: 0,
        }],
        sealing: true,
        enshrining: Some(Enshrining {
          symbol: Some('a'),
          subsidy: Some(400),
          mint_terms: Some(MintTerms {
            amount: Some(100),
            cap: Some(100_000),
            price: Some(PriceModel::Fixed(321)),
            seed: Some(300),
            swap_height: Some(400_000),
          }),
          turbo: true,
        }),
        mint: Some(relic_id(5)),
        swap: Some(Swap {
          input: Some(relic_id(42)),
          output: Some(relic_id(43)),
          input_amount: Some(123),
          output_amount: Some(456),
          is_exact_input: true,
        }),
        summoning: None,
        encasing: None,
        release: true,
        pointer: Some(0),
        claim: Some(0),
      }),
    );
  }

  #[test]
  fn recognized_even_etching_fields_produce_cenotaph_if_etching_flag_is_not_set() {
    assert_eq!(
      decipher(&[Tag::Seed.into(), 4]),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::UnrecognizedEvenTag),
      }),
    );
  }

  #[test]
  fn decipher_etching_with_min_height_and_symbol() {
    assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Enshrining.mask(),
        Tag::Subsidy.into(),
        1234,
        Tag::Symbol.into(),
        'a'.into(),
        Tag::Body.into(),
        1,
        1,
        2,
        0,
      ]),
      RelicArtifact::Keepsake(Keepsake {
        transfers: vec![Transfer {
          id: relic_id(1),
          amount: 2,
          output: 0,
        }],
        enshrining: Some(Enshrining {
          subsidy: Some(1234),
          symbol: Some('a'),
          ..default()
        }),
        ..default()
      }),
    );
  }

  #[test]
  fn tag_values_are_not_parsed_as_tags() {
    assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Enshrining.mask(),
        Tag::Symbol.into(),
        Tag::Body.into(),
        Tag::Body.into(),
        1,
        1,
        2,
        0,
      ]),
      RelicArtifact::Keepsake(Keepsake {
        transfers: vec![Transfer {
          id: relic_id(1),
          amount: 2,
          output: 0,
        }],
        enshrining: Some(Enshrining {
          symbol: Some(0.into()),
          ..default()
        }),
        ..default()
      }),
    );
  }

  #[test]
  fn runestone_may_contain_multiple_edicts() {
    assert_eq!(
      decipher(&[Tag::Body.into(), 1, 1, 2, 0, 0, 3, 5, 0]),
      RelicArtifact::Keepsake(Keepsake {
        transfers: vec![
          Transfer {
            id: relic_id(1),
            amount: 2,
            output: 0,
          },
          Transfer {
            id: relic_id(4),
            amount: 5,
            output: 0,
          },
        ],
        ..default()
      }),
    );
  }

  #[test]
  fn runestones_with_invalid_rune_id_blocks_are_cenotaph() {
    assert_eq!(
      decipher(&[Tag::Body.into(), 1, 1, 2, 0, u128::MAX, 1, 0, 0]),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::TransferRelicId),
      }),
    );
  }

  #[test]
  fn runestones_with_invalid_rune_id_txs_are_cenotaph() {
    assert_eq!(
      decipher(&[Tag::Body.into(), 1, 1, 2, 0, 1, u128::MAX, 0, 0]),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::TransferRelicId),
      }),
    );
  }

  #[test]
  fn keepsakes_recognize_syndicates() {
    assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Summoning.mask(),
        Tag::Treasure.into(),
        1,
        Tag::Treasure.into(),
        100,
      ]),
      RelicArtifact::Keepsake(Keepsake {
        summoning: Some(Summoning {
          treasure: Some(relic_id(100)),
          turbo: false,
          ..default()
        }),
        ..default()
      }),
    );
  }

  #[test]
  fn keepsakes_recognize_minimal_syndicates() {
    assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Summoning.mask(),
        Tag::Treasure.into(),
        1,
        Tag::Treasure.into(),
        100,
      ]),
      RelicArtifact::Keepsake(Keepsake {
        summoning: Some(Summoning {
          treasure: Some(relic_id(100)),
          turbo: false,
          ..default()
        }),
        ..default()
      }),
    );
  }

  #[test]
  fn keepsake_with_enshrining_and_summoning_is_cenotaph() {
    assert_eq!(
      decipher(&[
        Tag::Flags.into(),
        Flag::Enshrining.mask() | Flag::Summoning.mask(),
        Tag::Treasure.into(),
        1,
        Tag::Treasure.into(),
        100,
      ]),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::EnshriningAndSummoning),
      }),
    );
  }

  #[test]
  fn keepsakes_recognize_chests() {
    assert_eq!(
      decipher(&[Tag::Syndicate.into(), 1, Tag::Syndicate.into(), 100,]),
      RelicArtifact::Keepsake(Keepsake {
        encasing: Some(relic_id(100)),
        ..default()
      }),
    );
  }

  #[test]
  fn payload_pushes_are_concatenated() {
    assert_eq!(
      Keepsake::decipher(&Transaction {
        input: Vec::new(),
        output: vec![TxOut {
          script_pubkey: script::Builder::new()
            .push_opcode(opcodes::all::OP_RETURN)
            .push_opcode(Keepsake::MAGIC_NUMBER)
            .push_slice(varint::encode(Tag::Flags.into()).as_slice())
            .push_slice(varint::encode(Flag::Enshrining.mask()).as_slice())
            .push_slice(varint::encode(Tag::Subsidy.into()).as_slice())
            .push_slice(varint::encode(5).as_slice())
            .push_slice(varint::encode(Tag::Body.into()).as_slice())
            .push_slice(varint::encode(1).as_slice())
            .push_slice(varint::encode(1).as_slice())
            .push_slice(varint::encode(2).as_slice())
            .push_slice(varint::encode(0).as_slice())
            .into_script(),
          value: 0
        }],
        lock_time: PackedLockTime::ZERO,
        version: 2,
      })
      .unwrap(),
      RelicArtifact::Keepsake(Keepsake {
        transfers: vec![Transfer {
          id: relic_id(1),
          amount: 2,
          output: 0,
        }],
        enshrining: Some(Enshrining {
          subsidy: Some(5),
          ..default()
        }),
        ..default()
      }),
    );
  }

  #[test]
  fn runestone_may_be_in_second_output() {
    let payload = payload(&[0, 1, 1, 2, 0]);

    let payload = payload.as_slice().try_into().unwrap();

    assert_eq!(
      Keepsake::decipher(&Transaction {
        input: Vec::new(),
        output: vec![
          TxOut {
            script_pubkey: Script::new(),
            value: 0,
          },
          TxOut {
            script_pubkey: script::Builder::new()
              .push_opcode(opcodes::all::OP_RETURN)
              .push_opcode(Keepsake::MAGIC_NUMBER)
              .push_slice(payload)
              .into_script(),
            value: 0
          }
        ],
        lock_time: PackedLockTime::ZERO,
        version: 2,
      })
      .unwrap(),
      RelicArtifact::Keepsake(Keepsake {
        transfers: vec![Transfer {
          id: relic_id(1),
          amount: 2,
          output: 0,
        }],
        ..default()
      }),
    );
  }

  #[test]
  fn runestone_may_be_after_non_matching_op_return() {
    let payload = payload(&[0, 1, 1, 2, 0]);

    let payload = payload.as_slice().try_into().unwrap();

    assert_eq!(
      Keepsake::decipher(&Transaction {
        input: Vec::new(),
        output: vec![
          TxOut {
            script_pubkey: script::Builder::new()
              .push_opcode(opcodes::all::OP_RETURN)
              .push_slice(b"FOO")
              .into_script(),
            value: 0,
          },
          TxOut {
            script_pubkey: script::Builder::new()
              .push_opcode(opcodes::all::OP_RETURN)
              .push_opcode(Keepsake::MAGIC_NUMBER)
              .push_slice(payload)
              .into_script(),
            value: 0
          }
        ],
        lock_time: PackedLockTime::ZERO,
        version: 2,
      })
      .unwrap(),
      RelicArtifact::Keepsake(Keepsake {
        transfers: vec![Transfer {
          id: relic_id(1),
          amount: 2,
          output: 0,
        }],
        ..default()
      })
    );
  }

  #[test]
  fn enshrining_size() {
    #[track_caller]
    fn case(transfers: Vec<Transfer>, enshrining: Option<Enshrining>, size: usize) {
      assert_eq!(
        Keepsake {
          transfers,
          enshrining,
          ..default()
        }
        .encipher()
        .len(),
        size
      );
    }

    case(Vec::new(), None, 2);

    case(Vec::new(), Some(Enshrining::default()), 5);

    case(
      Vec::new(),
      Some(Enshrining {
        subsidy: Some(12),
        ..default()
      }),
      7,
    );

    case(
      Vec::new(),
      Some(Enshrining {
        symbol: Some('\u{10FFFF}'),
        subsidy: Some(300),
        mint_terms: Some(MintTerms {
          amount: Some(100),
          cap: Some(100_000),
          price: Some(PriceModel::Fixed(321)),
          seed: Some(200),
          swap_height: Some(400_000),
        }),
        turbo: true,
      }),
      28,
    );

    case(
      Vec::new(),
      Some(Enshrining {
        subsidy: Some(u128::MAX),
        ..default()
      }),
      25,
    );

    case(
      vec![Transfer {
        amount: 0,
        id: RelicId { block: 0, tx: 0 },
        output: 0,
      }],
      Some(Enshrining {
        subsidy: Some(12),
        ..default()
      }),
      12,
    );

    case(
      vec![Transfer {
        amount: u128::MAX,
        id: RelicId { block: 0, tx: 0 },
        output: 0,
      }],
      Some(Enshrining {
        subsidy: Some(12),
        ..default()
      }),
      30,
    );

    case(
      vec![Transfer {
        amount: 0,
        id: RelicId {
          block: 1_000_000,
          tx: u32::MAX,
        },
        output: 0,
      }],
      None,
      14,
    );

    case(
      vec![Transfer {
        amount: u128::MAX,
        id: RelicId {
          block: 1_000_000,
          tx: u32::MAX,
        },
        output: 0,
      }],
      None,
      32,
    );

    case(
      vec![
        Transfer {
          amount: u128::MAX,
          id: RelicId {
            block: 1_000_000,
            tx: u32::MAX,
          },
          output: 0,
        },
        Transfer {
          amount: u128::MAX,
          id: RelicId {
            block: 1_000_000,
            tx: u32::MAX,
          },
          output: 0,
        },
      ],
      None,
      54,
    );

    case(
      vec![
        Transfer {
          amount: u128::MAX,
          id: RelicId {
            block: 1_000_000,
            tx: u32::MAX,
          },
          output: 0,
        },
        Transfer {
          amount: u128::MAX,
          id: RelicId {
            block: 1_000_000,
            tx: u32::MAX,
          },
          output: 0,
        },
        Transfer {
          amount: u128::MAX,
          id: RelicId {
            block: 1_000_000,
            tx: u32::MAX,
          },
          output: 0,
        },
      ],
      None,
      76,
    );

    case(
      vec![
        Transfer {
          amount: u64::MAX.into(),
          id: RelicId {
            block: 1_000_000,
            tx: u32::MAX,
          },
          output: 0,
        };
        4
      ],
      None,
      62,
    );

    case(
      vec![
        Transfer {
          amount: u64::MAX.into(),
          id: RelicId {
            block: 1_000_000,
            tx: u32::MAX,
          },
          output: 0,
        };
        5
      ],
      None,
      75,
    );

    case(
      vec![
        Transfer {
          amount: u64::MAX.into(),
          id: RelicId {
            block: 0,
            tx: u32::MAX,
          },
          output: 0,
        };
        5
      ],
      None,
      73,
    );

    case(
      vec![
        Transfer {
          amount: 1_000_000_000_000_000_000,
          id: RelicId {
            block: 1_000_000,
            tx: u32::MAX,
          },
          output: 0,
        };
        5
      ],
      None,
      70,
    );
  }

  #[test]
  fn summoning_size() {
    #[track_caller]
    fn case(transfers: Vec<Transfer>, summoning: Option<Summoning>, size: usize) {
      assert_eq!(
        Keepsake {
          transfers,
          summoning,
          ..default()
        }
        .encipher()
        .len(),
        size
      );
    }

    case(
      vec![],
      Some(Summoning {
        treasure: Some(relic_id(1)),
        gated: true,
        cap: Some(12312),
        lock: Some(10000),
        height: (Some(450_000), Some(550_000)),
        quota: Some(1_000_000_000),
        royalty: Some(1_000_000_000),
        reward: Some(1_000_000_000),
        lock_subsidy: true,
        turbo: true,
      }),
      42,
    );

    case(
      vec![
        Transfer {
          amount: 1_000_000_000,
          id: RelicId {
            block: 1_000_000,
            tx: 1000,
          },
          output: 12,
        };
        3
      ],
      Some(Summoning {
        treasure: Some(relic_id(1)),
        gated: true,
        cap: Some(1_000_000),
        lock: Some(100_000),
        height: (Some(450_000), Some(550_000)),
        quota: Some(10_000_000_000),
        royalty: Some(1_000_000_000),
        reward: Some(10_000_000),
        lock_subsidy: true,
        turbo: true,
      }),
      71,
    );
  }

  // #[test]
  // fn etching_with_term_greater_than_maximum_is_still_an_etching() {
  //   assert_eq!(
  //     decipher(&[
  //       Tag::Flags.into(),
  //       Flag::Enshrining.mask(),
  //       Tag::OffsetEnd.into(),
  //       u128::from(u64::MAX) + 1,
  //     ]),
  //     Artifact::Cenotaph(Cenotaph {
  //       flaw: Some(Flaw::UnrecognizedEvenTag),
  //     }),
  //   );
  // }

  #[test]
  fn encipher() {
    #[track_caller]
    fn case(keepsake: Keepsake, expected: &[u128]) {
      let script_pubkey = keepsake.encipher();

      let transaction = Transaction {
        input: Vec::new(),
        output: vec![TxOut {
          script_pubkey,
          value: 0,
        }],
        lock_time: PackedLockTime::ZERO,
        version: 2,
      };

      let Payload::Valid(payload) = Keepsake::payload(&transaction).unwrap() else {
        panic!("invalid payload")
      };

      assert_eq!(Keepsake::integers(&payload).unwrap(), expected);

      let keepsake = {
        let mut transfers = keepsake.transfers;
        transfers.sort_by_key(|edict| edict.id);
        Keepsake {
          transfers,
          ..keepsake
        }
      };

      assert_eq!(
        Keepsake::decipher(&transaction).unwrap(),
        RelicArtifact::Keepsake(keepsake),
      );
    }

    case(Keepsake::default(), &[]);

    case(
      Keepsake {
        transfers: vec![
          Transfer {
            id: RelicId::new(2, 3).unwrap(),
            amount: 1,
            output: 0,
          },
          Transfer {
            id: RelicId::new(5, 6).unwrap(),
            amount: 4,
            output: 1,
          },
        ],
        sealing: true,
        enshrining: Some(Enshrining {
          symbol: Some('@'),
          subsidy: Some(300),
          mint_terms: Some(MintTerms {
            amount: Some(100),
            cap: Some(100_000),
            price: Some(PriceModel::Fixed(321)),
            seed: Some(200),
            swap_height: Some(400_000),
          }),
          turbo: true,
        }),
        mint: Some(relic_id(5)),
        swap: Some(Swap {
          input: Some(relic_id(42)),
          output: Some(relic_id(43)),
          input_amount: Some(123),
          output_amount: Some(456),
          is_exact_input: true,
        }),
        summoning: None,
        encasing: None,
        release: false,
        pointer: Some(0),
        claim: Some(0),
      },
      &[
        Tag::Symbol.into(),
        '@'.into(),
        Tag::Subsidy.into(),
        300,
        Tag::Amount.into(),
        100,
        Tag::Cap.into(),
        100_000,
        Tag::Price.into(),
        123,
        Tag::Seed.into(),
        200,
        Tag::SwapHeight.into(),
        400_000,
        Tag::Mint.into(),
        1,
        Tag::Mint.into(),
        5,
        Tag::SwapInput.into(),
        1,
        Tag::SwapInput.into(),
        42,
        Tag::SwapOutput.into(),
        1,
        Tag::SwapOutput.into(),
        43,
        Tag::SwapInputAmount.into(),
        123,
        Tag::SwapOutputAmount.into(),
        456,
        Tag::Flags.into(),
        Flag::Sealing.mask()
          | Flag::Enshrining.mask()
          | Flag::MintTerms.mask()
          | Flag::Swap.mask()
          | Flag::SwapExactInput.mask()
          | Flag::Turbo.mask(),
        Tag::Pointer.into(),
        0,
        Tag::Claim.into(),
        0,
        Tag::Body.into(),
        2,
        3,
        1,
        0,
        3,
        6,
        4,
        1,
      ],
    );

    case(
      Keepsake {
        transfers: vec![
          Transfer {
            id: RelicId::new(2, 3).unwrap(),
            amount: 1,
            output: 0,
          },
          Transfer {
            id: RelicId::new(5, 6).unwrap(),
            amount: 4,
            output: 1,
          },
        ],
        sealing: false,
        enshrining: None,
        mint: None,
        swap: None,
        summoning: Some(Summoning {
          treasure: Some(relic_id(20)),
          gated: true,
          cap: Some(21),
          lock: Some(22),
          height: (Some(23), Some(24)),
          quota: Some(25),
          royalty: Some(26),
          reward: Some(27),
          lock_subsidy: true,
          turbo: true,
        }),
        encasing: Some(relic_id(30)),
        release: true,
        pointer: Some(0),
        claim: Some(0),
      },
      &[
        Tag::Treasure.into(),
        1,
        Tag::Treasure.into(),
        20,
        Tag::SyndicateCap.into(),
        21,
        Tag::Lock.into(),
        22,
        Tag::HeightStart.into(),
        23,
        Tag::HeightEnd.into(),
        24,
        Tag::Quota.into(),
        25,
        Tag::Royalty.into(),
        26,
        Tag::Reward.into(),
        27,
        Tag::Syndicate.into(),
        1,
        Tag::Syndicate.into(),
        30,
        Tag::Flags.into(),
        Flag::Summoning.mask()
          | Flag::Gated.mask()
          | Flag::LockSubsidy.mask()
          | Flag::Release.mask()
          | Flag::Turbo.mask(),
        Tag::Pointer.into(),
        0,
        Tag::Claim.into(),
        0,
        Tag::Body.into(),
        2,
        3,
        1,
        0,
        3,
        6,
        4,
        1,
      ],
    );

    case(
      Keepsake {
        enshrining: Some(Enshrining {
          symbol: None,
          subsidy: Some(3),
          mint_terms: None,
          turbo: false,
        }),
        ..default()
      },
      &[
        Tag::Subsidy.into(),
        3,
        Tag::Flags.into(),
        Flag::Enshrining.mask(),
      ],
    );

    case(
      Keepsake {
        enshrining: Some(Enshrining {
          symbol: None,
          subsidy: None,
          mint_terms: None,
          turbo: false,
        }),
        ..default()
      },
      &[Tag::Flags.into(), Flag::Enshrining.mask()],
    );
  }

  #[test]
  fn runestone_payload_is_chunked() {
    let script = Keepsake {
      transfers: vec![
        Transfer {
          id: RelicId::default(),
          amount: 0,
          output: 0,
        };
        129
      ],
      ..default()
    }
    .encipher();

    assert_eq!(script.instructions().count(), 3);

    let script = Keepsake {
      transfers: vec![
        Transfer {
          id: RelicId::default(),
          amount: 0,
          output: 0,
        };
        130
      ],
      ..default()
    }
    .encipher();

    assert_eq!(script.instructions().count(), 4);
  }

  #[test]
  fn edict_output_greater_than_32_max_produces_cenotaph() {
    assert_eq!(
      decipher(&[Tag::Body.into(), 1, 1, 1, u128::from(u32::MAX) + 1, 0, 0]),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::TransferOutput),
      }),
    );
  }

  #[test]
  fn partial_swap_produces_cenotaph() {
    assert_eq!(
      decipher(&[Tag::SwapInput.into(), 1]),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::UnrecognizedEvenTag),
      }),
    );
  }

  #[test]
  fn invalid_swap_produces_cenotaph() {
    assert_eq!(
      decipher(&[Tag::SwapInput.into(), 0, Tag::SwapInput.into(), 1]),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::UnrecognizedEvenTag),
      }),
    );
  }

  // #[test]
  // fn invalid_deadline_produces_cenotaph() {
  //   assert_eq!(
  //     decipher(&[Tag::OffsetEnd.into(), u128::MAX]),
  //     Artifact::Cenotaph(Cenotaph {
  //       flaw: Some(Flaw::UnrecognizedEvenTag),
  //     }),
  //   );
  // }

  #[test]
  fn invalid_default_output_produces_cenotaph() {
    assert_eq!(
      decipher(&[Tag::Pointer.into(), 1]),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::UnrecognizedEvenTag),
      }),
    );
    assert_eq!(
      decipher(&[Tag::Pointer.into(), u128::MAX]),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::UnrecognizedEvenTag),
      }),
    );
  }

  // #[test]
  // fn invalid_divisibility_does_not_produce_cenotaph() {
  //   assert_eq!(
  //     decipher(&[Tag::Divisibility.into(), u128::MAX]),
  //     RelicArtifact::Keepsake(default()),
  //   );
  // }

  // #[test]
  // fn min_and_max_runes_are_not_cenotaphs() {
  //   assert_eq!(
  //     decipher(&[
  //       Tag::Flags.into(),
  //       Flag::Enshrining.into(),
  //       Tag::Relic.into(),
  //       0
  //     ]),
  //     RelicArtifact::Keepsake(Keepsake {
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(0)),
  //         ..default()
  //       }),
  //       ..default()
  //     }),
  //   );
  //   assert_eq!(
  //     decipher(&[
  //       Tag::Flags.into(),
  //       Flag::Enshrining.into(),
  //       Tag::Relic.into(),
  //       u128::MAX
  //     ]),
  //     RelicArtifact::Keepsake(Keepsake {
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(u128::MAX)),
  //         ..default()
  //       }),
  //       ..default()
  //     }),
  //   );
  // }

  // #[test]
  // fn invalid_spacers_does_not_produce_cenotaph() {
  //   assert_eq!(
  //     decipher(&[Tag::Spacers.into(), u128::MAX]),
  //     RelicArtifact::Keepsake(default()),
  //   );
  // }

  #[test]
  fn invalid_symbol_does_not_produce_cenotaph() {
    assert_eq!(
      decipher(&[Tag::Symbol.into(), u128::MAX]),
      RelicArtifact::Keepsake(default()),
    );
  }

  // #[test]
  // fn invalid_term_produces_cenotaph() {
  //   assert_eq!(
  //     decipher(&[Tag::OffsetEnd.into(), u128::MAX]),
  //     Artifact::Cenotaph(Cenotaph {
  //       flaw: Some(Flaw::UnrecognizedEvenTag),
  //     }),
  //   );
  // }

  // #[test]
  // fn invalid_supply_produces_cenotaph() {
  //   assert_eq!(
  //     decipher(&[
  //       Tag::Flags.into(),
  //       Flag::Enshrining.mask() | Flag::Terms.mask(),
  //       Tag::Cap.into(),
  //       1,
  //       Tag::Amount.into(),
  //       u128::MAX
  //     ]),
  //     Artifact::Keepsake(Keepsake {
  //       enshrining: Some(Enshrining {
  //         terms: Some(Terms {
  //           cap: Some(1),
  //           amount: Some(u128::MAX),
  //           height: (None, None),
  //           offset: (None, None),
  //         }),
  //         ..default()
  //       }),
  //       ..default()
  //     }),
  //   );
  //
  //   assert_eq!(
  //     decipher(&[
  //       Tag::Flags.into(),
  //       Flag::Enshrining.mask() | Flag::Terms.mask(),
  //       Tag::Cap.into(),
  //       2,
  //       Tag::Amount.into(),
  //       u128::MAX
  //     ]),
  //     Artifact::Cenotaph(Cenotaph {
  //       flaw: Some(Flaw::SupplyOverflow),
  //     }),
  //   );
  //
  //   assert_eq!(
  //     decipher(&[
  //       Tag::Flags.into(),
  //       Flag::Enshrining.mask() | Flag::Terms.mask(),
  //       Tag::Cap.into(),
  //       2,
  //       Tag::Amount.into(),
  //       u128::MAX / 2 + 1
  //     ]),
  //     Artifact::Cenotaph(Cenotaph {
  //       flaw: Some(Flaw::SupplyOverflow),
  //     }),
  //   );
  //
  //   assert_eq!(
  //     decipher(&[
  //       Tag::Flags.into(),
  //       Flag::Enshrining.mask() | Flag::Terms.mask(),
  //       Tag::Premine.into(),
  //       1,
  //       Tag::Cap.into(),
  //       1,
  //       Tag::Amount.into(),
  //       u128::MAX
  //     ]),
  //     Artifact::Cenotaph(Cenotaph {
  //       flaw: Some(Flaw::SupplyOverflow),
  //     }),
  //   );
  // }

  #[test]
  fn invalid_scripts_in_op_returns_without_magic_number_are_ignored() {
    assert_eq!(
      Keepsake::decipher(&Transaction {
        version: 2,
        lock_time: PackedLockTime::ZERO,
        input: vec![TxIn {
          previous_output: OutPoint::null(),
          script_sig: Script::new(),
          sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
          witness: Witness::new(),
        }],
        output: vec![TxOut {
          script_pubkey: Script::from(vec![
            opcodes::all::OP_RETURN.to_u8(),
            opcodes::all::OP_PUSHBYTES_4.to_u8(),
          ]),
          value: 0,
        }],
      }),
      None
    );

    assert_eq!(
      Keepsake::decipher(&Transaction {
        version: 2,
        lock_time: PackedLockTime::ZERO,
        input: vec![TxIn {
          previous_output: OutPoint::null(),
          script_sig: Script::new(),
          sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
          witness: Witness::new(),
        }],
        output: vec![
          TxOut {
            script_pubkey: Script::from(vec![
              opcodes::all::OP_RETURN.to_u8(),
              opcodes::all::OP_PUSHBYTES_4.to_u8(),
            ]),
            value: 0,
          },
          TxOut {
            script_pubkey: Keepsake::default().encipher(),
            value: 0,
          }
        ],
      })
      .unwrap(),
      RelicArtifact::Keepsake(Keepsake::default()),
    );
  }

  #[test]
  fn invalid_scripts_in_op_returns_with_magic_number_produce_cenotaph() {
    assert_eq!(
      Keepsake::decipher(&Transaction {
        version: 2,
        lock_time: PackedLockTime::ZERO,
        input: vec![TxIn {
          previous_output: OutPoint::null(),
          script_sig: Script::new(),
          sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
          witness: Witness::new(),
        }],
        output: vec![TxOut {
          script_pubkey: Script::from(vec![
            opcodes::all::OP_RETURN.to_u8(),
            Keepsake::MAGIC_NUMBER.to_u8(),
            opcodes::all::OP_PUSHBYTES_4.to_u8(),
          ]),
          value: 0,
        }],
      })
      .unwrap(),
      RelicArtifact::Cenotaph(RelicCenotaph {
        flaw: Some(RelicFlaw::InvalidScript),
      }),
    );
  }

  #[test]
  fn all_pushdata_opcodes_are_valid() {
    for i in 0..79 {
      let mut script_pubkey = Vec::new();

      script_pubkey.push(opcodes::all::OP_RETURN.to_u8());
      script_pubkey.push(Keepsake::MAGIC_NUMBER.to_u8());
      script_pubkey.push(i);

      match i {
        0..=75 => {
          for j in 0..i {
            script_pubkey.push(if j % 2 == 0 { 1 } else { 0 });
          }

          if i % 2 == 1 {
            script_pubkey.push(1);
            script_pubkey.push(1);
          }
        }
        76 => {
          script_pubkey.push(0);
        }
        77 => {
          script_pubkey.push(0);
          script_pubkey.push(0);
        }
        78 => {
          script_pubkey.push(0);
          script_pubkey.push(0);
          script_pubkey.push(0);
          script_pubkey.push(0);
        }
        _ => unreachable!(),
      }

      assert_eq!(
        Keepsake::decipher(&Transaction {
          version: 2,
          lock_time: PackedLockTime::ZERO,
          input: default(),
          output: vec![TxOut {
            script_pubkey: script_pubkey.into(),
            value: 0,
          },],
        })
        .unwrap(),
        RelicArtifact::Keepsake(Keepsake::default()),
      );
    }
  }

  #[test]
  fn all_non_pushdata_opcodes_are_invalid() {
    for i in 79..=u8::MAX {
      assert_eq!(
        Keepsake::decipher(&Transaction {
          version: 2,
          lock_time: PackedLockTime::ZERO,
          input: default(),
          output: vec![TxOut {
            script_pubkey: vec![
              opcodes::all::OP_RETURN.to_u8(),
              Keepsake::MAGIC_NUMBER.to_u8(),
              i
            ]
            .into(),
            value: 0,
          },],
        })
        .unwrap(),
        RelicArtifact::Cenotaph(RelicCenotaph {
          flaw: Some(RelicFlaw::Opcode),
        }),
      );
    }
  }
}
