use {
  super::*,
  crate::{
    charm::Charm,
    index::{
      chest_entry::ChestEntry,
      event::{EventEmitter, EventInfo, RelicOperation},
      lot::Lot,
      relics_entry::{RelicEntry, RelicOwner, RelicState},
      syndicate_entry::SyndicateEntry,
      updater::relics_balance::RelicsBalance,
    },
    relics::{
      BalanceDiff, Enshrining, Keepsake, Pool, PoolSwap, PriceModel, RelicArtifact, RelicError, SpacedRelic,
      Summoning, Swap, SwapDirection, RELIC_ID,
    },
  },
};

pub(super) struct RelicUpdater<'a, 'tx, 'index, 'emitter> {
  pub(super) block_time: u32,
  pub(super) burned: HashMap<RelicId, Lot>,
  pub(super) claimable: HashMap<RelicOwner, u128>,
  pub(super) unsafe_txids: HashSet<Txid>,
  pub(super) index: &'index Index,
  pub(super) height: u32,
  pub(super) first_relic_syndicate_height: u32,
  pub(super) id_to_entry: &'a mut Table<'tx, RelicIdValue, RelicEntryValue>,
  pub(super) id_to_syndicate: &'a mut Table<'tx, SyndicateIdValue, SyndicateEntryValue>,
  pub(super) inscription_id_to_sequence_number: &'a Table<'tx, &'static InscriptionIdValue, u32>,
  pub(super) mints_in_block: HashMap<RelicId, u16>,
  pub(super) outpoint_to_balances: &'a mut Table<'tx, &'static OutPointValue, &'static [u8]>,
  pub(super) relic_owner_to_claimable: &'a mut Table<'tx, &'static RelicOwnerValue, u128>,
  pub(super) relic_to_id: &'a mut Table<'tx, u128, RelicIdValue>,
  pub(super) relics: u64,
  pub(super) statistic_to_count: &'a mut Table<'tx, u64, u64>,
  pub(super) transaction_id_to_relic: &'a mut Table<'tx, &'static TxidValue, u128>,
  pub(super) satpoint_to_sequence_number: &'a MultimapTable<'tx, &'static SatPointValue, u32>,
  pub(super) sequence_number_to_inscription_entry: &'a Table<'tx, u32, InscriptionEntryValue>,
  pub(super) sequence_number_to_satpoint: &'a Table<'tx, u32, &'static SatPointValue>,
  pub(super) sequence_number_to_spaced_relic: &'a mut Table<'tx, u32, SpacedRelicValue>,
  pub(super) sequence_number_to_syndicate: &'a mut Table<'tx, u32, SyndicateIdValue>,
  pub(super) sequence_number_to_chest: &'a mut Table<'tx, u32, ChestEntryValue>,
  pub(super) syndicate_to_chest_sequence_number: &'a mut MultimapTable<'tx, SyndicateIdValue, u32>,
  pub(super) relic_to_sequence_number: &'a mut Table<'tx, u128, u32>,
  pub(super) event_emitter: &'a mut EventEmitter<'emitter, 'tx>,
  pub(super) inscription_id_to_txids: &'a Table<'tx, &'static InscriptionIdValue, &'static [u8]>,
  pub(super) inscription_txid_to_tx: &'a Table<'tx, &'static [u8], &'static [u8]>,
  pub(super) sequence_number_to_bonestone_block_height: &'a mut Table<'tx, u32, u32>,
}

impl<'a, 'tx, 'index, 'emitter> RelicUpdater<'a, 'tx, 'index, 'emitter> {
  pub(super) fn index_relics(&mut self, tx_index: u32, tx: &Transaction, txid: Txid) -> Result<()> {
    let artifact = Keepsake::decipher(tx);

    let mut balances = RelicsBalance::new(
      tx,
      &self.unsafe_txids,
      self.outpoint_to_balances,
      self.index,
    )?;

    if let Some(amount) = self.mint_base_token(txid, tx)? {
      balances.add_safe(RELIC_ID, amount);
    }

    if let Some(RelicArtifact::Keepsake(keepsake)) = &artifact {
      if keepsake.sealing {
        match self.seal(tx, txid, balances.get(RELIC_ID))? {
          Ok(sealing_fee) => {
            // burn sealing fee in RELIC
            balances.remove(RELIC_ID, sealing_fee);
            balances.burn(RELIC_ID, sealing_fee);
          }
          Err(error) => {
            eprintln!("Sealing error: {error}");
            self.event_emitter.emit(
              txid,
              EventInfo::RelicError {
                operation: RelicOperation::Seal,
                error,
              },
            )?;
          }
        }
      }

      let enshrined_relic = if let Some(enshrining) = keepsake.enshrining {
        match self.enshrine_relic(tx, txid, tx_index, enshrining)? {
          Ok(id) => Some(id),
          Err(error) => {
            eprintln!("Enshrine error: {error}");
            self.event_emitter.emit(
              txid,
              EventInfo::RelicError {
                operation: RelicOperation::Enshrine,
                error,
              },
            )?;
            None
          }
        }
      } else {
        None
      };

      if let Some(id) = keepsake.mint {
        let id = if id == RelicId::default() { enshrined_relic } else { Some(id) };
        if let Some(id) = id {
          match self.mint(txid, id, balances.get(RELIC_ID))? {
            Ok((amount, price)) => {
              balances.remove(RELIC_ID, price);
              balances.add(id, amount);
            }
            Err(error) => {
              eprintln!("Mint error: {error}");
              self.event_emitter.emit(
                txid,
                EventInfo::RelicError {
                  operation: RelicOperation::Mint,
                  error,
                },
              )?;
            }
          }
        }
      }

      if let Some(id) = keepsake.unmint.filter(|id| *id != RelicId::default()) {
        if enshrined_relic.is_some() {
          eprintln!("Unmint error: Unmint not allowed in transaction with enshrined relic");
          self.event_emitter.emit(
            txid,
            EventInfo::RelicError {
              operation: RelicOperation::Unmint,
              error: RelicError::UnmintNotAllowed,
            },
          )?;
        } else {
          // Pass the balance of the minted token (id) instead of the base token.
          match self.unmint(txid, id, balances.get(id))? {
            Ok((amount, price)) => {
              // Remove the minted token from the caller’s balance and refund base tokens.
              balances.remove(id, amount);
              balances.add(RELIC_ID, price);
            }
            Err(error) => {
              eprintln!("Unmint error: {error}");
              self.event_emitter.emit(
                txid,
                EventInfo::RelicError {
                  operation: RelicOperation::Unmint,
                  error,
                },
              )?;
            }
          }
        }
      }

      if let Some(swap) = &keepsake.swap {
        let input = swap.input.unwrap_or(RELIC_ID);
        let output = swap.output.unwrap_or(RELIC_ID);
        // note: use safe balance here for Sandwich protection:
        // this will prevent swapping the same Relics twice within a block
        match self.swap(txid, swap, input, output, balances.get_safe(input))? {
          Ok((input_amount, output_amount, fees)) => {
            balances.remove_safe(input, Lot(input_amount));
            balances.add(output, Lot(output_amount));
            for (owner, fee) in fees {
              if let Some(owner) = owner {
                // add fees to the claimable amount of the owner
                *self.claimable.entry(owner).or_default() += fee;
              } else {
                // burn fees if there is no owner
                balances.burn(RELIC_ID, Lot(fee));
              }
            }
          }
          Err(error) => {
            eprintln!("Swap error: {error}");
            self.event_emitter.emit(
              txid,
              EventInfo::RelicError {
                operation: RelicOperation::Swap,
                error,
              },
            )?;
          }
        }
      }

      if let Some(multi) = keepsake.multi_mint {
        // Use enshrined relic if multi.relic is default
        let id = if multi.relic == RelicId::default() {
          enshrined_relic
        } else {
          Some(multi.relic)
        };
        if let Some(id) = id {
          if multi.is_unmint {
            // Unmint not allowed if an enshrined relic is present
            if enshrined_relic.is_some() {
              eprintln!("Unmint error: Unmint not allowed in transaction with enshrined relic");
              self.event_emitter.emit(
                txid,
                EventInfo::RelicError {
                  operation: RelicOperation::Unmint,
                  error: RelicError::UnmintNotAllowed,
                },
              )?;
            } else {
              // Call multi_unmint for multiple unmint operations
              match self.multi_unmint(txid, id, balances.get(id), multi.count, multi.base_limit)? {
                Ok(lots) => {
                  let (total_relic, total_base) = lots.iter().fold(
                    (0u128, 0u128),
                    |(acc_r, acc_b), (Lot(amount), Lot(price))| (acc_r + amount, acc_b + price)
                  );
                  // Remove the unminted tokens from `id`'s balance and refund base tokens to RELIC balance.
                  balances.remove(id, Lot(total_relic));
                  balances.add(RELIC_ID, Lot(total_base));
                  self.event_emitter.emit(
                    txid,
                    EventInfo::RelicMultiMinted {
                      relic_id: id,
                      amount: total_relic,
                      num_mints: multi.count,
                      base_limit: multi.base_limit,
                    },
                  )?;
                }
                Err(error) => {
                  eprintln!("MultiUnmint error: {error}");
                  self.event_emitter.emit(
                    txid,
                    EventInfo::RelicError {
                      operation: RelicOperation::MultiUnmint,
                      error,
                    },
                  )?;
                }
              }
            }
          } else {
            // Mint operation
            match self.multi_mint(txid, id, balances.get(RELIC_ID), multi.count, multi.base_limit)? {
              Ok(lots) => {
                let (total_relic, total_base) = lots.iter().fold(
                  (0u128, 0u128),
                  |(acc_r, acc_b), (Lot(amount), Lot(price))| (acc_r + amount, acc_b + price)
                );
                balances.remove(RELIC_ID, Lot(total_base));
                balances.add(id, Lot(total_relic));
                self.event_emitter.emit(
                  txid,
                  EventInfo::RelicMultiMinted {
                    relic_id: id,
                    amount: total_relic,
                    num_mints: multi.count,
                    base_limit: multi.base_limit,
                  },
                )?;
              }
              Err(error) => {
                eprintln!("MultiMint error: {error}");
                self.event_emitter.emit(
                  txid,
                  EventInfo::RelicError {
                    operation: RelicOperation::MultiMint,
                    error,
                  },
                )?;
              }
            }
          }
        }
      }

      if self.height >= self.first_relic_syndicate_height {
        if let Some(summoning) = &keepsake.summoning {
          match self.summon_syndicate(txid, tx_index, summoning)? {
            Ok(_syndicate_id) => {
              // TODO: charge a Summoning fee
            }
            Err(error) => {
              eprintln!("Syndicate summon error: {error}");
              self.event_emitter.emit(
                txid,
                EventInfo::RelicError {
                  operation: RelicOperation::Summon,
                  error,
                },
              )?;
            }
          }
        }

        if let Some(syndicate_id) = &keepsake.encasing {
          match self.encase_chest(txid, *syndicate_id, &balances)? {
            Ok((id, quota, owner, royalty)) => {
              // lock Chest quota
              balances.remove(id, Lot(quota));
              // pay royalty to Syndicate owner
              if let Some(owner) = owner {
                balances.remove(RELIC_ID, Lot(royalty));
                *self.claimable.entry(owner).or_default() += royalty;
              }
            }
            Err(error) => {
              eprintln!("Chest encase error: {error}");
              self.event_emitter.emit(
                txid,
                EventInfo::RelicError {
                  operation: RelicOperation::Encase,
                  error,
                },
              )?;
            }
          }
        }

        if keepsake.release {
          match self.release_chest(txid, tx)? {
            Ok((relic_id, amount)) => {
              // payout Relics from the Chest
              balances.add(relic_id, Lot(amount));
            }
            Err(error) => {
              eprintln!("Chest release error: {error}");
              self.event_emitter.emit(
                txid,
                EventInfo::RelicError {
                  operation: RelicOperation::Release,
                  error,
                },
              )?;
            }
          }
        }
      }

      if let Some(claim) = keepsake.claim {
        let claim = usize::try_from(claim).unwrap();
        // values greater than the number of outputs should never be produced by the parser
        assert!(claim < tx.output.len());
        let owner = RelicOwner(tx.output[claim].script_pubkey.script_hash());
        if let Some(amount) = self.claim(txid, owner)? {
          // handle fee collection: assign all fees claimable by the given owner
          balances.allocate(claim, RELIC_ID, amount);
        } else {
          eprintln!("Claim error: no balance to claim");
          self.event_emitter.emit(
            txid,
            EventInfo::RelicError {
              operation: RelicOperation::Claim,
              error: RelicError::NoClaimableBalance,
            },
          )?;
        }
      }

      balances.allocate_transfers(&keepsake.transfers, enshrined_relic, tx);
    }

    let first_non_op_return_output = || {
      tx.output
        .iter()
        .enumerate()
        .find(|(_vout, tx_out)| !tx_out.script_pubkey.is_op_return())
        .map(|(vout, _tx_out)| vout)
    };

    let default_output = match artifact {
      // no protocol message: pass through to first non-op_return
      None => first_non_op_return_output(),
      // valid protocol message: use pointer as output or default to the first non-op_return
      Some(RelicArtifact::Keepsake(keepsake)) => keepsake
        .pointer
        .map(|pointer| pointer as usize)
        .or_else(first_non_op_return_output),
      // invalid protocol message: explicitly burn all Relics
      Some(RelicArtifact::Cenotaph(_)) => None,
    };

    if let Some(vout) = default_output {
      // note: vout might still point to an OP_RETURN output resulting in a burn on finalize
      balances.allocate_all(vout);
    } else {
      balances.burn_all();
    }

    balances.finalize(
      tx,
      txid,
      self.outpoint_to_balances,
      &mut self.unsafe_txids,
      &mut self.burned,
      self.event_emitter,
      self.index,
    )
  }

  pub(super) fn update(self) -> Result {
    // distribute Relic subsidy to all Chests on Syndicates that have rewards
    for result in self.id_to_syndicate.iter()? {
      let entry = result?;
      let syndicate_id = SyndicateId::load(entry.0.value());
      let syndicate = SyndicateEntry::load(entry.1.value());
      let reward = syndicate.reward.unwrap_or_default();
      if reward == 0 {
        continue;
      }
      let mut relic = self
        .load_relic_entry(syndicate.treasure)?
        .expect("Syndicate index inconsistent");
      // subsidies have been used up
      if relic.state.subsidy_remaining == 0 {
        continue;
      }
      // update all Chests belonging to this Syndicate
      for result in self
        .syndicate_to_chest_sequence_number
        .get(syndicate_id.store())?
      {
        let chest_sequence_number = result?.value();
        let chest_value = self
          .sequence_number_to_chest
          .get(chest_sequence_number)?
          .expect("Chest index inconsistent")
          .value();
        let mut chest = ChestEntry::load(chest_value);
        // limit payout to the available subsidy on the Relic
        let payout = reward.min(relic.state.subsidy_remaining);
        chest.amount += payout;
        // subtract collected rewards from subsidy supply on the Relic
        relic.state.subsidy_remaining -= payout;
        self
          .sequence_number_to_chest
          .insert(chest_sequence_number, chest.store())?;
        if relic.state.subsidy_remaining == 0 {
          break;
        }
      }
      // update Relic
      self
        .id_to_entry
        .insert(syndicate.treasure.store(), relic.store())?;
    }

    // update burned counters
    for (relic_id, burned) in self.burned {
      let mut entry = RelicEntry::load(self.id_to_entry.get(&relic_id.store())?.unwrap().value());
      entry.state.burned = entry.state.burned.checked_add(burned.n()).unwrap();
      self.id_to_entry.insert(&relic_id.store(), entry.store())?;
    }

    // update amounts of claimable balance
    for (owner, amount) in self.claimable {
      let current = self
        .relic_owner_to_claimable
        .get(&owner.store())?
        .map(|v| v.value())
        .unwrap_or_default();
      self
        .relic_owner_to_claimable
        .insert(&owner.store(), current.checked_add(amount).unwrap())?;
    }

    Ok(())
  }

  fn create_relic_entry(
    &mut self,
    txid: Txid,
    enshrining: Enshrining,
    id: RelicId,
    spaced_relic: SpacedRelic,
    owner_sequence_number: u32,
  ) -> Result {
    let Enshrining {
      symbol,
      subsidy,
      mint_terms,
      turbo,
    } = enshrining;

    self
      .relic_to_id
      .insert(spaced_relic.relic.store(), id.store())?;
    self
      .transaction_id_to_relic
      .insert(&txid.store(), spaced_relic.relic.store())?;

    let number = self.relics;
    self.relics += 1;

    self
      .statistic_to_count
      .insert(&Statistic::Relics.into(), self.relics)?;

    let entry = RelicEntry {
      block: id.block,
      enshrining: txid,
      number,
      spaced_relic,
      symbol,
      owner_sequence_number: Some(owner_sequence_number),
      mint_terms,
      state: RelicState {
        burned: 0,
        mints: 0,
        subsidy: subsidy.unwrap_or_default(),
        subsidy_remaining: subsidy.unwrap_or_default(),
        subsidy_locked: false,
      },
      pool: None,
      timestamp: self.block_time.into(),
      turbo,
    };

    self.id_to_entry.insert(id.store(), entry.store())?;

    self
      .event_emitter
      .emit(txid, EventInfo::RelicEnshrined { relic_id: id })?;

    Ok(())
  }

  fn load_relic_entry(&self, id: RelicId) -> Result<Option<RelicEntry>> {
    let Some(entry) = self.id_to_entry.get(&id.store())? else {
      return Ok(None);
    };
    Ok(Some(RelicEntry::load(entry.value())))
  }

  fn tx_inscriptions(&self, txid: Txid, tx: &Transaction) -> Result<Vec<InscriptionEntry>> {
    let mut inscriptions: Vec<InscriptionEntry> = Vec::new();
    // we search the outputs, not the inputs, because the InscriptionUpdater has already processed
    // this transaction and would have moved any Inscription to the outputs
    for vout in 0..tx.output.len() {
      let outpoint = OutPoint {
        txid,
        vout: u32::try_from(vout).unwrap(),
      };
      let start = SatPoint {
        outpoint,
        offset: 0,
      };
      let end = SatPoint {
        outpoint,
        offset: u64::MAX,
      };
      for range in self
        .satpoint_to_sequence_number
        .range::<&[u8; 44]>(&start.store()..=&end.store())?
      {
        let (_satpoint, sequence_numbers) = range?;
        if let Some(sequence_number) = sequence_numbers.into_iter().next() {
          let sequence_number = sequence_number?.value();
          let entry = self
            .sequence_number_to_inscription_entry
            .get(sequence_number)?
            .unwrap();
          inscriptions.push(InscriptionEntry::load(entry.value()));
        }
      }
    }
    Ok(inscriptions)
  }

  fn seal(
    &mut self,
    tx: &Transaction,
    txid: Txid,
    base_balance: u128,
  ) -> Result<Result<Lot, RelicError>> {
    // the sealing inscription must be revealed as the first inscription in this transaction
    let inscription_id = InscriptionId { txid, index: 0 };
    let Some(sequence_number) = self
      .inscription_id_to_sequence_number
      .get(&inscription_id.store())?
      .map(|s| s.value())
    else {
      return Ok(Err(RelicError::InscriptionMissing));
    };

    let Some(inscription) = self.get_inscription_by_id(inscription_id, sequence_number)? else {
      panic!(
        "failed to get Inscription: {} {} {}",
        txid, inscription_id, sequence_number
      );
    };

    // parse and verify Ticker from inscribed metadata
    let Some(metadata) = inscription.metadata() else {
      // missing metadata
      return Ok(Err(RelicError::InscriptionMetadataMissing));
    };
    let Some(spaced_relic) = SpacedRelic::from_metadata(metadata) else {
      // invalid metadata
      return Ok(Err(RelicError::InvalidMetadata));
    };
    if spaced_relic == SpacedRelic::from_str(RELIC_NAME)? {
      return Ok(Err(RelicError::SealingBaseToken));
    }
    if let Some(_existing) = self.relic_to_sequence_number.get(spaced_relic.relic.n())? {
      // Ticker already sealed to an inscription
      return Ok(Err(RelicError::SealingAlreadyExists(spaced_relic)));
    }
    let sealing_fee = spaced_relic.relic.sealing_fee();
    if base_balance < sealing_fee {
      // insufficient RELIC to cover sealing fee
      return Ok(Err(RelicError::SealingInsufficientBalance(sealing_fee)));
    }
    self
      .relic_to_sequence_number
      .insert(spaced_relic.relic.n(), sequence_number)?;
    self
      .sequence_number_to_spaced_relic
      .insert(sequence_number, &spaced_relic.store())?;
    self.event_emitter.emit(
      txid,
      EventInfo::RelicSealed {
        spaced_relic,
        sequence_number,
      },
    )?;
    Ok(Ok(Lot(sealing_fee)))
  }

  fn enshrine_relic(
    &mut self,
    tx: &Transaction,
    txid: Txid,
    tx_index: u32,
    enshrining: Enshrining,
  ) -> Result<Result<RelicId, RelicError>> {
    // Find all inscriptions on the outputs
    let inscriptions = self.tx_inscriptions(txid, tx)?;

    if inscriptions.is_empty() {
      return Ok(Err(RelicError::InscriptionMissing));
    }

    // Iterate through all inscriptions to find a sealed relic
    let mut spaced_relic = None;
    for entry in inscriptions {
      if let Some(relic) = self
        .sequence_number_to_spaced_relic
        .get(entry.sequence_number)?
        .map(|spaced_relic_value| SpacedRelic::load(spaced_relic_value.value()))
      {
        spaced_relic = Some((relic, entry.sequence_number));
        break; // Stop as soon as a sealed relic was found
      }
    }

    // Handle the case where no sealed relic was found
    let (spaced_relic, sequence_number) = match spaced_relic {
      Some(value) => value,
      None => return Ok(Err(RelicError::SealingNotFound)),
    };

    // Bail out if Relic ticker is already enshrined
    if self.relic_to_id.get(spaced_relic.relic.n())?.is_some() {
      return Ok(Err(RelicError::RelicAlreadyEnshrined));
    }

    // Create a new RelicId and enshrine the relic
    let id = RelicId {
      block: self.height.into(),
      tx: tx_index,
    };
    self.create_relic_entry(txid, enshrining, id, spaced_relic, sequence_number)?;
    Ok(Ok(id))
  }

  fn summon_syndicate(
    &mut self,
    txid: Txid,
    tx_index: u32,
    summoning: &Summoning,
  ) -> Result<Result<SyndicateId, RelicError>> {
    // the syndicate inscription must be revealed as the first inscription in this transaction
    let inscription_id = InscriptionId { txid, index: 0 };
    let Some(sequence_number) = self
      .inscription_id_to_sequence_number
      .get(&inscription_id.store())?
      .map(|s| s.value())
    else {
      return Ok(Err(RelicError::InscriptionMissing));
    };
    let syndicate = SyndicateEntry::new(*summoning, sequence_number, txid);
    let Some(relic_entry) = self.load_relic_entry(syndicate.treasure)? else {
      // relic not found
      return Ok(Err(RelicError::RelicNotFound(syndicate.treasure)));
    };
    // Syndicates with rewards can only be summoned by the owner of the Relic.
    // Also, the Relic subsidy can only be locked by the owner of the Relic.
    if summoning.reward.is_some() || summoning.lock_subsidy {
      let entry = InscriptionEntry::load(
        self
          .sequence_number_to_inscription_entry
          .get(sequence_number)?
          .unwrap()
          .value(),
      );
      // verify that the Syndicate inscription is a child of the Relic inscription
      if relic_entry
        .owner_sequence_number
        .map(|r| !entry.parents.contains(&r))
        .unwrap_or(true)
      {
        return Ok(Err(RelicError::RelicOwnerOnly));
      }
      // verify that Subsidies on the Relic are not locked yet
      if summoning.reward.is_some() && relic_entry.state.subsidy_locked {
        return Ok(Err(RelicError::RelicSubsidyLocked));
      }
      if summoning.lock_subsidy {
        if relic_entry.state.subsidy_locked {
          return Ok(Err(RelicError::RelicSubsidyLocked));
        }
        let mut relic_entry = relic_entry;
        relic_entry.state.subsidy_locked = true;
        self
          .id_to_entry
          .insert(&syndicate.treasure.store(), relic_entry.store())?;
      }
    }
    let syndicate_id = SyndicateId {
      block: self.height.into(),
      tx: tx_index,
    };
    self
      .id_to_syndicate
      .insert(syndicate_id.store(), syndicate.store())?;
    self
      .sequence_number_to_syndicate
      .insert(sequence_number, syndicate_id.store())?;
    self.event_emitter.emit(
      txid,
      EventInfo::SyndicateSummoned {
        relic_id: syndicate.treasure,
        syndicate_id,
      },
    )?;
    if summoning.lock_subsidy {
      self.event_emitter.emit(
        txid,
        EventInfo::RelicSubsidyLocked {
          relic_id: syndicate.treasure,
        },
      )?;
    }
    Ok(Ok(syndicate_id))
  }

  fn encase_chest(
    &mut self,
    txid: Txid,
    syndicate_id: SyndicateId,
    balances: &RelicsBalance,
  ) -> Result<Result<(RelicId, u128, Option<RelicOwner>, u128), RelicError>> {
    // the chest inscription must be revealed as the first inscription in this transaction
    let inscription_id = InscriptionId { txid, index: 0 };
    let Some(sequence_number) = self
      .inscription_id_to_sequence_number
      .get(&inscription_id.store())?
      .map(|s| s.value())
    else {
      return Ok(Err(RelicError::InscriptionMissing));
    };
    let Some(mut syndicate) = self
      .id_to_syndicate
      .get(syndicate_id.store())?
      .map(|v| SyndicateEntry::load(v.value()))
    else {
      // syndicate not found
      return Ok(Err(RelicError::SyndicateNotFound(syndicate_id)));
    };
    let quota = match syndicate.chestable(self.height.into()) {
      Ok(quota) => quota,
      Err(cause) => return Ok(Err(cause)),
    };
    if syndicate.gated {
      let entry = InscriptionEntry::load(
        self
          .sequence_number_to_inscription_entry
          .get(sequence_number)?
          .unwrap()
          .value(),
      );
      // verify that the Chest inscription is a child of the Syndicate inscription
      if !entry.parents.contains(&syndicate.sequence_number) {
        return Ok(Err(RelicError::SyndicateIsGated));
      }
    }
    // check balance for quota and royalty
    let mut required: HashMap<RelicId, u128> = HashMap::new();
    // note: treasure can also be RELIC
    *required.entry(syndicate.treasure).or_default() += quota;
    if syndicate.royalty > 0 {
      *required.entry(RELIC_ID).or_default() += syndicate.royalty;
    }
    for (id, amount) in required {
      if balances.get(id) < amount {
        return Ok(Err(RelicError::ChestInsufficientBalance(id, amount)));
      }
    }
    // save mapping from sequence number to ChestEntry
    let chest = ChestEntry {
      sequence_number,
      syndicate: syndicate_id,
      created_block: self.height.into(),
      amount: quota,
    };
    self
      .sequence_number_to_chest
      .insert(sequence_number, chest.store())?;
    // save multi mapping from SyndicateId to sequence number
    self
      .syndicate_to_chest_sequence_number
      .insert(syndicate_id.store(), sequence_number)?;
    // update syndicate
    syndicate.chests += 1;
    self
      .id_to_syndicate
      .insert(syndicate_id.store(), syndicate.store())?;
    self
      .event_emitter
      .emit(txid, EventInfo::ChestEncased { syndicate_id })?;
    let syndicate_owner = self.get_inscription_owner(syndicate.sequence_number)?;
    Ok(Ok((
      syndicate.treasure,
      quota,
      syndicate_owner,
      syndicate.royalty,
    )))
  }

  fn release_chest(
    &mut self,
    txid: Txid,
    tx: &Transaction,
  ) -> Result<Result<(RelicId, u128), RelicError>> {
    // find the first Inscription on the outputs
    let inscriptions = self.tx_inscriptions(txid, tx)?;
    let Some(entry) = inscriptions.first() else {
      return Ok(Err(RelicError::InscriptionMissing));
    };
    let Some(chest) = self
      .sequence_number_to_chest
      .get(entry.sequence_number)?
      .map(|v| ChestEntry::load(v.value()))
    else {
      return Ok(Err(RelicError::ChestNotFound));
    };
    let Some(mut syndicate) = self
      .id_to_syndicate
      .get(chest.syndicate.store())?
      .map(|v| SyndicateEntry::load(v.value()))
    else {
      panic!(
        "Chest is part of Syndicate that cannot be found: {}",
        chest.syndicate
      );
    };
    let unlock_height = chest.created_block + syndicate.lock.unwrap_or_default();
    if unlock_height > self.height.into() {
      return Ok(Err(RelicError::ChestLocked(unlock_height)));
    }
    // destroy Chest
    assert!(syndicate.chests > 0, "Syndicate Chest count underflow bug");
    syndicate.chests -= 1;
    self
      .id_to_syndicate
      .insert(chest.syndicate.store(), syndicate.store())?;
    self
      .sequence_number_to_chest
      .remove(chest.sequence_number)?;
    self
      .syndicate_to_chest_sequence_number
      .remove(chest.syndicate.store(), chest.sequence_number)?;
    self.event_emitter.emit(
      txid,
      EventInfo::ChestReleased {
        syndicate_id: chest.syndicate,
        amount: chest.amount,
      },
    )?;
    Ok(Ok((syndicate.treasure, chest.amount)))
  }

  fn swap(
    &mut self,
    txid: Txid,
    swap: &Swap,
    input: RelicId,
    output: RelicId,
    input_balance: u128,
  ) -> Result<Result<(u128, u128, Vec<(Option<RelicOwner>, u128)>), RelicError>> {
    assert_ne!(
      input, output,
      "the parser produced an invalid Swap with input Relic == output Relic"
    );
    let input_entry = self.load_relic_entry(input)?;
    let output_entry = self.load_relic_entry(output)?;
    match self.swap_calculate(
      swap,
      input,
      &input_entry,
      output,
      &output_entry,
      input_balance,
    ) {
      Ok((sell, buy)) => {
        let mut fees = Vec::new();
        if let Some(diff) = sell {
          fees.push(self.swap_apply(swap, txid, input, &mut input_entry.unwrap(), diff)?);
        }
        if let Some(diff) = buy {
          fees.push(self.swap_apply(swap, txid, output, &mut output_entry.unwrap(), diff)?);
        }
        match (sell, buy) {
          (Some(sell), None) => Ok(Ok((sell.input, sell.output, fees))),
          (None, Some(buy)) => Ok(Ok((buy.input, buy.output, fees))),
          (Some(sell), Some(buy)) => Ok(Ok((sell.input, buy.output, fees))),
          (None, None) => unreachable!(),
        }
      }
      Err(cause) => Ok(Err(cause)),
    }
  }

  fn swap_calculate(
    &self,
    swap: &Swap,
    input: RelicId,
    input_entry: &Option<RelicEntry>,
    output: RelicId,
    output_entry: &Option<RelicEntry>,
    input_balance: u128,
  ) -> Result<(Option<BalanceDiff>, Option<BalanceDiff>), RelicError> {
    let simple_swap = |direction: SwapDirection| {
      if swap.is_exact_input {
        PoolSwap::Input {
          direction,
          input: swap.input_amount.unwrap_or_default(),
          min_output: swap.output_amount,
        }
      } else {
        PoolSwap::Output {
          direction,
          output: swap.output_amount.unwrap_or_default(),
          max_input: swap.input_amount,
        }
      }
    };
    let input_entry = input_entry.ok_or(RelicError::RelicNotFound(input))?;
    let output_entry = output_entry.ok_or(RelicError::RelicNotFound(output))?;
    match (input, output) {
      // buy output relic
      (RELIC_ID, _) => Ok((
        None,
        Some(output_entry.swap(
          simple_swap(SwapDirection::BaseToQuote),
          Some(input_balance),
          self.height.into(),
        )?),
      )),
      // sell input relic
      (_, RELIC_ID) => Ok((
        Some(input_entry.swap(
          simple_swap(SwapDirection::QuoteToBase),
          Some(input_balance),
          self.height.into(),
        )?),
        None,
      )),
      // dual swap: sell input relic to buy output relic
      _ => {
        if swap.is_exact_input {
          // sell input
          let diff_sell = input_entry.swap(
            PoolSwap::Input {
              direction: SwapDirection::QuoteToBase,
              input: swap.input_amount.unwrap_or_default(),
              // no slippage check here, we check on the other swap
              min_output: None,
            },
            Some(input_balance),
            self.height.into(),
          )?;
          // buy output
          let diff_buy = output_entry.swap(
            PoolSwap::Input {
              direction: SwapDirection::BaseToQuote,
              input: diff_sell.output,
              // slippage check is performed on the second swap, on slippage error both swaps will not be executed
              min_output: swap.output_amount,
            },
            None,
            self.height.into(),
          )?;
          Ok((Some(diff_sell), Some(diff_buy)))
        } else {
          // calculate the "buy" first to determine how many base tokens we need to get out of the "sell"
          let diff_buy = output_entry.swap(
            PoolSwap::Output {
              direction: SwapDirection::BaseToQuote,
              output: swap.output_amount.unwrap_or_default(),
              // no slippage check here, we check on the other swap
              max_input: None,
            },
            None,
            self.height.into(),
          )?;
          // sell input
          let diff_sell = input_entry.swap(
            PoolSwap::Output {
              direction: SwapDirection::QuoteToBase,
              output: diff_buy.input,
              // slippage check is performed on the second swap, on slippage error both swaps will not be executed
              max_input: swap.input_amount,
            },
            Some(input_balance),
            self.height.into(),
          )?;
          Ok((Some(diff_sell), Some(diff_buy)))
        }
      }
    }
  }

  fn get_inscription_owner(&self, sequence_number: u32) -> Result<Option<RelicOwner>> {
    let Some(satpoint) = self
      .sequence_number_to_satpoint
      .get(sequence_number)?
      .map(|satpoint| SatPoint::load(*satpoint.value()))
    else {
      panic!("unable to find satpoint for sequence number {sequence_number}");
    };
    if satpoint.outpoint == unbound_outpoint() || satpoint.outpoint == OutPoint::null() {
      return Ok(None);
    }
    let Some(tx_info) = self
      .index
      .client
      .get_raw_transaction_info(&satpoint.outpoint.txid)
      .into_option()?
    else {
      panic!("can't get input transaction: {}", satpoint.outpoint.txid);
    };
    let script = tx_info.vout[satpoint.outpoint.vout as usize]
      .script_pub_key
      .script()?;
    Ok(Some(RelicOwner(script.script_hash())))
  }

  fn swap_apply(
    &mut self,
    swap: &Swap,
    txid: Txid,
    relic_id: RelicId,
    entry: &mut RelicEntry,
    diff: BalanceDiff,
  ) -> Result<(Option<RelicOwner>, u128)> {
    entry.pool.as_mut().unwrap().apply(diff);
    self.id_to_entry.insert(&relic_id.store(), entry.store())?;
    let owner = if diff.fee > 0 {
      if let Some(sequence_number) = entry.owner_sequence_number {
        self.get_inscription_owner(sequence_number)?
      } else {
        None
      }
    } else {
      None
    };
    let (base_amount, quote_amount, fee, is_sell_order) = match diff.direction {
      SwapDirection::BaseToQuote => (diff.input, diff.output, diff.fee, false),
      SwapDirection::QuoteToBase => (diff.output, diff.input, diff.fee, true),
    };
    self.event_emitter.emit(
      txid,
      EventInfo::RelicSwapped {
        relic_id,
        base_amount,
        quote_amount,
        fee,
        is_sell_order,
        is_exact_input: swap.is_exact_input,
      },
    )?;
    Ok((owner, diff.fee))
  }

  /// mint base token for every burned bonestone inscription in the tx
  fn mint_base_token(&mut self, txid: Txid, tx: &Transaction) -> Result<Option<Lot>> {
    let burned_bonestones = self
      .tx_inscriptions(txid, tx)?
      .iter()
      .filter(|inscription| Charm::Burned.is_set(inscription.charms))
      .map(|inscription| {
        self
          .sequence_number_to_bonestone_block_height
          .get(inscription.sequence_number)
          .map(|block_height| block_height.is_some().then_some(1).unwrap_or_default())
      })
      .sum::<Result<u128, _>>()?;

    if burned_bonestones == 0 {
      return Ok(None);
    }

    let mut bone = self.load_relic_entry(RELIC_ID)?.unwrap();
    let terms = bone.mint_terms.unwrap();
    assert!(
      bone.state.mints + burned_bonestones <= terms.cap.unwrap(),
      "too many mints of the base token, is the cap set correctly?"
    );
    bone.state.mints += burned_bonestones;
    let amount = terms.amount.unwrap() * burned_bonestones;

    self.id_to_entry.insert(&RELIC_ID.store(), bone.store())?;

    self.event_emitter.emit(
      txid,
      EventInfo::RelicMinted {
        relic_id: RELIC_ID,
        amount,
      },
    )?;

    Ok(Some(Lot(amount)))
  }

  fn mint(
    &mut self,
    txid: Txid,
    id: RelicId,
    base_balance: u128,
  ) -> Result<Result<(Lot, Lot), RelicError>> {
    assert_ne!(
      id, RELIC_ID,
      "the parser produced an invalid Mint for the base token"
    );
    let Some(mut relic_entry) = self.load_relic_entry(id)? else {
      return Ok(Err(RelicError::RelicNotFound(id)));
    };

    // Check per-block mint limit if set.
    if let Some(terms) = relic_entry.mint_terms {
      if let Some(max_per_block) = terms.max_per_block {
        let count = self.mints_in_block.entry(id).or_insert(0);
        if *count >= max_per_block {
          return Ok(Err(RelicError::MintBlockCapExceeded(max_per_block)));
        }
      }
    }

    let (amount, price) = match relic_entry.mintable(base_balance) {
      Ok(result) => result,
      Err(cause) => {
        return Ok(Err(cause));
      }
    };

    // Increment per-block mint counter.
    if let Some(terms) = relic_entry.mint_terms {
      if terms.max_per_block.is_some() {
        let count = self.mints_in_block.entry(id).or_insert(0);
        *count += 1;
      }
    }

    relic_entry.state.mints += 1;

    // mint cap reached, create liquidity pool
    if relic_entry.state.mints == relic_entry.mint_terms.unwrap().cap.unwrap_or_default() {
      assert_eq!(relic_entry.pool, None, "pool already exists");
      let base_supply = relic_entry.locked_base_supply();
      let quote_supply = relic_entry.mint_terms.unwrap().seed.unwrap_or_default();
      if base_supply == 0 || quote_supply == 0 {
        // this is explicitly not an error, it's expected to happen at least with the Base Token Relic, but is not limited to it
        eprintln!(
          "unable to create pool for Relic {}: both token supplies must be non-zero, but got base/quote supply of {base_supply}/{quote_supply}",
          relic_entry.spaced_relic
        );
      } else {
        relic_entry.pool = Some(Pool {
          base_supply,
          quote_supply,
          // for now the fee is always 1%
          fee_percentage: 1,
        })
      }
    }

    self.id_to_entry.insert(&id.store(), relic_entry.store())?;

    self.event_emitter.emit(
      txid,
      EventInfo::RelicMinted {
        relic_id: id,
        amount,
      },
    )?;

    Ok(Ok((Lot(amount), Lot(price))))
  }

  fn multi_mint(
    &mut self,
    txid: Txid,
    id: RelicId,
    base_balance: u128,
    num_mints: u32,
    base_limit: u128,
  ) -> Result<Result<Vec<(Lot, Lot)>, RelicError>> {
    assert_ne!(id, RELIC_ID, "the parser produced an invalid Mint for the base token");
    let Some(mut relic_entry) = self.load_relic_entry(id)? else {
      return Ok(Err(RelicError::RelicNotFound(id)));
    };

    // Enforce max_per_block for multi-mint.
    if let Some(terms) = relic_entry.mint_terms {
      if let Some(max_per_block) = terms.max_per_block {
        let current = self.mints_in_block.get(&id).cloned().unwrap_or(0);
        if (current as u32).saturating_add(num_mints) > max_per_block as u32 {
          return Ok(Err(RelicError::MintBlockCapExceeded(max_per_block)));
        }
      }
    }

    let mint_results = relic_entry.multi_mintable(base_balance, num_mints, base_limit)?;
    relic_entry.state.mints += mint_results.len() as u128;

    // Update per-block mint counter.
    if let Some(terms) = relic_entry.mint_terms {
      if let Some(_) = terms.max_per_block {
        let counter = self.mints_in_block.entry(id).or_insert(0);
        *counter = counter.saturating_add(mint_results.len() as u16);
      }
    }

    if let Some(terms) = relic_entry.mint_terms {
      if relic_entry.state.mints == terms.cap.unwrap_or_default() {
        assert_eq!(relic_entry.pool, None, "pool already exists");
        let base_supply = relic_entry.locked_base_supply();
        let quote_supply = terms.seed.unwrap_or_default();
        if base_supply == 0 || quote_supply == 0 {
          eprintln!(
            "unable to create pool for Relic {}: both token supplies must be non-zero, but got base/quote supply of {}/{}",
            relic_entry.spaced_relic, base_supply, quote_supply
          );
        } else {
          relic_entry.pool = Some(Pool {
            base_supply,
            quote_supply,
            fee_percentage: 1,
          });
        }
      }
    }

    let lots: Vec<(Lot, Lot)> = mint_results
      .iter()
      .map(|&(amount, price)| (Lot(amount), Lot(price)))
      .collect();

    let amount = mint_results.get(0).map(|(a, _)| *a).unwrap_or_default();
    self.event_emitter.emit(
      txid,
      EventInfo::RelicMultiMinted {
        relic_id: id,
        amount,
        num_mints,
        base_limit,
      },
    )?;

    self.id_to_entry.insert(&id.store(), relic_entry.store())?;
    Ok(Ok(lots))
  }

  fn unmint(
    &mut self,
    txid: Txid,
    id: RelicId,
    token_balance: u128, // now represents the balance of the token to be unminted
  ) -> Result<Result<(Lot, Lot), RelicError>> {
    assert_ne!(id, RELIC_ID, "unmint for base token is not allowed");
    let Some(mut relic_entry) = self.load_relic_entry(id)? else {
      return Ok(Err(RelicError::RelicNotFound(id)));
    };
    let (amount, price) = relic_entry.unmintable()?;
    // Ensure the caller has enough of the minted token to be unminted.
    if token_balance < amount {
      return Ok(Err(RelicError::UnmintInsufficientBalance(amount, token_balance)));
    }
    relic_entry.state.mints -= 1;
    self.id_to_entry.insert(&id.store(), relic_entry.store())?;
    self.event_emitter.emit(
      txid,
      EventInfo::RelicUnminted { relic_id: id, amount },
    )?;
    Ok(Ok((Lot(amount), Lot(price))))
  }

  fn multi_unmint(
    &mut self,
    txid: Txid,
    id: RelicId,
    token_balance: u128,
    count: u32,
    base_limit: u128, // minimum base tokens the user expects to receive
  ) -> Result<Result<Vec<(Lot, Lot)>, RelicError>> {
    assert_ne!(id, RELIC_ID, "unmint for base token is not allowed");
    let Some(mut relic_entry) = self.load_relic_entry(id)? else {
      return Ok(Err(RelicError::RelicNotFound(id)));
    };

    let results = relic_entry.multi_unmintable(count)?;
    // Total minted tokens to be removed.
    let total_minted: u128 = results.iter().map(|(a, _)| *a).sum();
    if token_balance < total_minted {
      return Ok(Err(RelicError::UnmintInsufficientBalance(total_minted, token_balance)));
    }
    // Total base tokens to be refunded.
    let total_refund: u128 = results.iter().map(|(_, price)| *price).sum();
    if total_refund < base_limit {
      return Ok(Err(RelicError::MintBaseLimitExceeded(base_limit, total_refund)));
    }

    relic_entry.state.mints -= count as u128;
    self.id_to_entry.insert(&id.store(), relic_entry.store())?;
    self.event_emitter.emit(
      txid,
      EventInfo::RelicUnminted { relic_id: id, amount: total_minted },
    )?;
    let lots = results.into_iter().map(|(a, p)| (Lot(a), Lot(p))).collect();
    Ok(Ok(lots))
  }

  fn claim(&mut self, txid: Txid, owner: RelicOwner) -> Result<Option<Lot>> {
    // claimable balance collected before the current block and persisted to the database
    let old = self
      .relic_owner_to_claimable
      .remove(&owner.store())?
      .map(|v| v.value());
    // claimable balance collected during indexing of the current block
    let new = self.claimable.remove(&owner);
    if old.is_none() && new.is_none() {
      return Ok(None);
    }
    let amount = Lot(old.unwrap_or_default()) + new.unwrap_or_default();
    self
      .event_emitter
      .emit(txid, EventInfo::RelicClaimed { amount: amount.n() })?;
    Ok(Some(amount))
  }

  pub(crate) fn get_inscription_by_id(
    &self,
    inscription_id: InscriptionId,
    sequence_number: u32,
  ) -> Result<Option<Inscription>> {
    if self
      .sequence_number_to_satpoint
      .get(&sequence_number)?
      .is_none()
    {
      return Ok(None);
    }

    let txids_result = self.inscription_id_to_txids.get(&inscription_id.store())?;

    match txids_result {
      Some(txids) => {
        let mut txs = vec![];

        let txids = txids.value();

        for i in 0..txids.len() / 32 {
          let txid_buf = &txids[i * 32..i * 32 + 32];
          let tx_result = self.inscription_txid_to_tx.get(txid_buf)?;

          match tx_result {
            Some(tx_result) => {
              let tx_buf = tx_result.value().to_vec();
              let mut cursor = Cursor::new(tx_buf);
              let tx = Transaction::consensus_decode(&mut cursor)?;
              txs.push(tx);
            }
            None => return Ok(None),
          }
        }

        let parsed_inscription = Inscription::from_transactions(txs);

        match parsed_inscription {
          ParsedInscription::None => Ok(None),
          ParsedInscription::Partial => Ok(None),
          ParsedInscription::Complete(inscription) => Ok(Some(inscription)),
        }
      }

      None => Ok(None),
    }
  }
}
