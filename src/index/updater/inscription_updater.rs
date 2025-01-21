use crate::index::event::EventInfo;
use {
  super::*,
  crate::{
    charm::Charm,
    inscription::ParsedInscription,
    relics::{BONESTONES_END_BLOCK, BONESTONES_INSCRIPTION_ID, BONESTONES_START_BLOCK},
    sat::Sat,
    sat_point::SatPoint,
  },
};

pub(super) struct Flotsam {
  txid: Txid,
  inscription_id: InscriptionId,
  offset: u64,
  old_satpoint: SatPoint,
  origin: Origin,
}

#[derive(Debug, Clone)]
enum Origin {
  New {
    fee: u64,
    inscription: Inscription,
  },
  Old {
    sequence_number: u32,
    old_satpoint: SatPoint,
  },
}

pub(super) struct InscriptionUpdater<'a, 'tx, 'emitter> {
  flotsam: Vec<Flotsam>,
  height: u32,
  id_to_satpoint: &'a mut Table<'tx, &'static InscriptionIdValue, &'static SatPointValue>,
  id_to_txids: &'a mut Table<'tx, &'static InscriptionIdValue, &'static [u8]>,
  txid_to_tx: &'a mut Table<'tx, &'static [u8], &'static [u8]>,
  partial_txid_to_txids: &'a mut Table<'tx, &'static [u8], &'static [u8]>,
  value_receiver: &'a mut Receiver<u64>,
  index_transactions: bool,
  transaction_buffer: Vec<u8>,
  sequence_number_to_inscription_entry: &'a mut Table<'tx, u32, InscriptionEntryValue>,
  inscription_number_to_sequence_number: &'a mut Table<'tx, u64, u32>,
  inscription_id_to_sequence_number: &'a mut Table<'tx, &'static InscriptionIdValue, u32>,
  home_inscriptions: &'a mut Table<'tx, u32, InscriptionIdValue>,
  home_inscription_count: u64,
  sat_to_sequence_number: &'a mut MultimapTable<'tx, u64, u32>,
  satpoint_to_sequence_number: &'a mut MultimapTable<'tx, &'static SatPointValue, u32>,
  sequence_number_to_bonestone_block_height: &'a mut Table<'tx, u32, u32>,
  sequence_number_to_children: &'a mut MultimapTable<'tx, u32, u32>,
  sequence_number_to_satpoint: &'a mut Table<'tx, u32, &'static SatPointValue>,
  sequence_number_to_spaced_relic: &'a mut Table<'tx, u32, SpacedRelicValue>,
  transaction_id_to_transaction: &'a mut Table<'tx, &'static TxidValue, &'static [u8]>,
  lost_sats: u64,
  next_number: u64,
  pub(crate) next_sequence_number: u32,
  outpoint_to_value: &'a mut Table<'tx, &'static OutPointValue, u64>,
  address_to_outpoint: &'a mut MultimapTable<'tx, &'static [u8], &'static OutPointValue>,
  reward: u64,
  satpoint_to_id: &'a mut Table<'tx, &'static SatPointValue, &'static InscriptionIdValue>,
  timestamp: u32,
  value_cache: &'a mut HashMap<OutPoint, OutPointMapValue>,
  chain: Chain,
  event_emitter: &'a mut EventEmitter<'emitter, 'tx>,
}

impl<'a, 'tx, 'emitter> InscriptionUpdater<'a, 'tx, 'emitter> {
  pub(super) fn new(
    height: u32,
    id_to_satpoint: &'a mut Table<'tx, &'static InscriptionIdValue, &'static SatPointValue>,
    id_to_txids: &'a mut Table<'tx, &'static InscriptionIdValue, &'static [u8]>,
    txid_to_tx: &'a mut Table<'tx, &'static [u8], &'static [u8]>,
    partial_txid_to_txids: &'a mut Table<'tx, &'static [u8], &'static [u8]>,
    value_receiver: &'a mut Receiver<u64>,
    index_transactions: bool,
    transaction_buffer: Vec<u8>,
    sequence_number_to_inscription_entry: &'a mut Table<'tx, u32, InscriptionEntryValue>,
    inscription_number_to_sequence_number: &'a mut Table<'tx, u64, u32>,
    inscription_id_to_sequence_number: &'a mut Table<'tx, &'static InscriptionIdValue, u32>,
    home_inscriptions: &'a mut Table<'tx, u32, InscriptionIdValue>,
    home_inscription_count: u64,
    sat_to_sequence_number: &'a mut MultimapTable<'tx, u64, u32>,
    satpoint_to_sequence_number: &'a mut MultimapTable<'tx, &SatPointValue, u32>,
    sequence_number_to_bonestone_block_height: &'a mut Table<'tx, u32, u32>,
    sequence_number_to_children: &'a mut MultimapTable<'tx, u32, u32>,
    sequence_number_to_satpoint: &'a mut Table<'tx, u32, &'static SatPointValue>,
    sequence_number_to_spaced_relic: &'a mut Table<'tx, u32, SpacedRelicValue>,
    transaction_id_to_transaction: &'a mut Table<'tx, &'static TxidValue, &'static [u8]>,
    lost_sats: u64,
    outpoint_to_value: &'a mut Table<'tx, &'static OutPointValue, u64>,
    address_to_outpoint: &'a mut MultimapTable<'tx, &'static [u8], &'static OutPointValue>,
    satpoint_to_id: &'a mut Table<'tx, &'static SatPointValue, &'static InscriptionIdValue>,
    timestamp: u32,
    value_cache: &'a mut HashMap<OutPoint, OutPointMapValue>,
    chain: Chain,
    event_emitter: &'a mut EventEmitter<'emitter, 'tx>,
  ) -> Result<Self> {
    let mut next_number = inscription_number_to_sequence_number
      .iter()?
      .rev()
      .map(|result| result.map(|(number, _id)| number.value() + 1))
      .next()
      .transpose()?
      .unwrap_or(0);
    let next_sequence_number = sequence_number_to_inscription_entry
      .iter()?
      .next_back()
      .transpose()?
      .map(|(number, _id)| number.value() + 1)
      .unwrap_or(0);

    Ok(Self {
      flotsam: Vec::new(),
      height,
      id_to_satpoint,
      id_to_txids,
      txid_to_tx,
      partial_txid_to_txids,
      value_receiver,
      index_transactions,
      transaction_buffer,
      sequence_number_to_inscription_entry,
      inscription_number_to_sequence_number,
      inscription_id_to_sequence_number,
      home_inscriptions,
      home_inscription_count,
      sat_to_sequence_number,
      satpoint_to_sequence_number,
      sequence_number_to_bonestone_block_height,
      sequence_number_to_children,
      sequence_number_to_satpoint,
      sequence_number_to_spaced_relic,
      transaction_id_to_transaction,
      lost_sats,
      next_number,
      next_sequence_number,
      outpoint_to_value,
      address_to_outpoint,
      reward: Height(height).subsidy(),
      satpoint_to_id,
      timestamp,
      value_cache,
      chain,
      event_emitter,
    })
  }

  pub(super) fn index_transaction_inscriptions(
    &mut self,
    tx: &Transaction,
    txid: Txid,
    input_sat_ranges: Option<&VecDeque<(u64, u64)>>,
  ) -> Result<u64> {
    let mut inscriptions = Vec::new();

    if self.index_transactions {
      tx.consensus_encode(&mut self.transaction_buffer)
        .expect("in-memory writers don't error");
      self
        .transaction_id_to_transaction
        .insert(&txid.store(), self.transaction_buffer.as_slice())?;

      self.transaction_buffer.clear();
    }

    let mut input_value = 0;
    for tx_in in &tx.input {
      if tx_in.previous_output.is_null() {
        input_value += Height(self.height).subsidy();
      } else {
        let result: Result<(), _> = (|| {
          for result in Index::inscriptions_on_output(
            self.satpoint_to_sequence_number,
            self.satpoint_to_id,
            tx_in.previous_output,
          )? {
            let (old_satpoint, inscription_id, sequence_number) = result?;
            inscriptions.push(Flotsam {
              txid,
              offset: input_value + old_satpoint.offset,
              old_satpoint,
              inscription_id,
              origin: Origin::Old {
                sequence_number: sequence_number.get(0).unwrap().clone(),
                old_satpoint,
              },
            });
          }
          Ok(())
        })();

        if let Err(e) = result {
          // Propagate the error e
          return Err(e);
        }

        input_value += if let Some(map) = self.value_cache.remove(&tx_in.previous_output) {
          map.0
        } else if let Some(map) = self
          .outpoint_to_value
          .remove(&tx_in.previous_output.store())?
        {
          if let Some(transaction) = self
            .transaction_id_to_transaction
            .get(&tx_in.previous_output.txid.store())?
          {
            let tx: Transaction = consensus::encode::deserialize(transaction.value())?;
            let output = tx.output[tx_in.previous_output.vout as usize].clone();
            if let Some(address_from_script) =
              self.chain.address_from_script(&output.script_pubkey).ok()
            {
              self.address_to_outpoint.remove(
                address_from_script.to_string().as_bytes(),
                &tx_in.previous_output.store(),
              )?;
            }
          }
          map.value()
        } else {
          self.value_receiver.blocking_recv().ok_or_else(|| {
            anyhow!(
              "failed to get transaction for {}",
              tx_in.previous_output.txid
            )
          })?
        }
      }
    }

    if inscriptions.iter().all(|flotsam| flotsam.offset != 0) {
      let previous_txid = tx.input[0].previous_output.txid;
      let previous_vout = tx.input[0].previous_output.vout;
      let previous_txid_bytes: [u8; 32] = previous_txid.into_inner();
      let mut txids_vec = vec![];

      let txs = match self
        .partial_txid_to_txids
        .get(&previous_txid_bytes.as_slice())?
      {
        Some(partial_txids) => {
          let txids = partial_txids.value();
          let mut txs = vec![];
          txids_vec = txids.to_vec();
          for i in 0..txids.len() / 32 {
            let txid = &txids[i * 32..i * 32 + 32];
            let tx_result = self.txid_to_tx.get(txid)?;
            let tx_result = tx_result.unwrap();
            let tx_buf = tx_result.value();
            let mut cursor = std::io::Cursor::new(tx_buf);
            let tx = bitcoin::Transaction::consensus_decode(&mut cursor)?;
            txs.push(tx);
          }
          txs.push(tx.clone());
          txs
        }
        None => {
          vec![tx.clone()]
        }
      };

      match Inscription::from_transactions(txs) {
        ParsedInscription::None => {
          // todo: clean up db
        }

        ParsedInscription::Partial => {
          let mut txid_vec = txid.into_inner().to_vec();
          txids_vec.append(&mut txid_vec);

          self
            .partial_txid_to_txids
            .remove(&previous_txid_bytes.as_slice())?;
          self
            .partial_txid_to_txids
            .insert(&txid.into_inner().as_slice(), txids_vec.as_slice())?;

          let mut tx_buf = vec![];
          tx.consensus_encode(&mut tx_buf)?;
          self
            .txid_to_tx
            .insert(&txid.into_inner().as_slice(), tx_buf.as_slice())?;
        }

        ParsedInscription::Complete(_inscription) => {
          self
            .partial_txid_to_txids
            .remove(&previous_txid_bytes.as_slice())?;

          let mut tx_buf = vec![];
          tx.consensus_encode(&mut tx_buf)?;
          self
            .txid_to_tx
            .insert(&txid.into_inner().as_slice(), tx_buf.as_slice())?;

          let mut txid_vec = txid.into_inner().to_vec();
          txids_vec.append(&mut txid_vec);

          let mut inscription_id = [0_u8; 36];
          unsafe {
            std::ptr::copy_nonoverlapping(txids_vec.as_ptr(), inscription_id.as_mut_ptr(), 32)
          }
          self
            .id_to_txids
            .insert(&inscription_id, txids_vec.as_slice())?;

          let og_inscription_id = InscriptionId {
            txid: Txid::from_slice(&txids_vec[0..32]).unwrap(),
            index: 0,
          };

          inscriptions.push(Flotsam {
            txid,
            inscription_id: og_inscription_id,
            offset: 0,
            old_satpoint: SatPoint {
              outpoint: OutPoint {
                txid: previous_txid,
                vout: previous_vout,
              },
              offset: 0,
            },
            origin: Origin::New {
              fee: input_value - tx.output.iter().map(|txout| txout.value).sum::<u64>(),
              inscription: _inscription.clone(),
            },
          });
        }
      }
    };

    let is_coinbase = tx
      .input
      .first()
      .map(|tx_in| tx_in.previous_output.is_null())
      .unwrap_or_default();

    if is_coinbase {
      inscriptions.append(&mut self.flotsam);
    }

    inscriptions.sort_by_key(|flotsam| flotsam.offset);
    let mut inscriptions = inscriptions.into_iter().peekable();
    let mut inscription_id_to_script = HashMap::new();

    let mut output_value = 0;
    for (vout, tx_out) in tx.output.iter().enumerate() {
      let end = output_value + tx_out.value;

      while let Some(flotsam) = inscriptions.peek() {
        if flotsam.offset >= end {
          break;
        }

        let new_satpoint = SatPoint {
          outpoint: OutPoint {
            txid,
            vout: vout.try_into().unwrap(),
          },
          offset: flotsam.offset - output_value,
        };

        let is_op_return = tx_out.script_pubkey.is_op_return();
        inscription_id_to_script.insert(flotsam.inscription_id, is_op_return);

        self.update_inscription_location(
          input_sat_ranges,
          inscriptions.next().unwrap(),
          new_satpoint,
          is_op_return,
          txid,
        )?;
      }

      output_value = end;

      let address_from_script = self
        .chain
        .address_from_script(&tx_out.clone().script_pubkey);

      let address = if address_from_script.is_err() {
        [0u8; 34]
      } else {
        address_from_script
          .unwrap()
          .to_string()
          .as_bytes()
          .try_into()
          .unwrap_or([0u8; 34])
      };

      self.value_cache.insert(
        OutPoint {
          vout: vout.try_into().unwrap(),
          txid,
        },
        (tx_out.clone().value, address),
      );
    }

    if is_coinbase {
      for flotsam in inscriptions {
        let new_satpoint = SatPoint {
          outpoint: OutPoint::null(),
          offset: self.lost_sats + flotsam.offset - output_value,
        };
        let op_return = inscription_id_to_script
          .get(&flotsam.inscription_id)
          .unwrap_or(&false);
        self.update_inscription_location(
          input_sat_ranges,
          flotsam,
          new_satpoint,
          *op_return,
          txid,
        )?;
      }

      Ok(self.reward.checked_sub(output_value).unwrap_or(0))
    } else {
      self.flotsam.extend(inscriptions.map(|flotsam| Flotsam {
        offset: self.reward + flotsam.offset - output_value,
        ..flotsam
      }));
      self.reward += input_value - output_value;
      Ok(0)
    }
  }

  fn update_inscription_location(
    &mut self,
    input_sat_ranges: Option<&VecDeque<(u64, u64)>>,
    flotsam: Flotsam,
    new_satpoint: SatPoint,
    op_return: bool,
    txid: Txid,
  ) -> Result {
    let inscription_id = flotsam.inscription_id;
    let mut seq_number = 0;

    match flotsam.origin {
      Origin::Old {
        old_satpoint,
        sequence_number,
      } => {
        if op_return {
          let entry = InscriptionEntry::load(
            self
              .sequence_number_to_inscription_entry
              .get(&sequence_number)?
              .unwrap()
              .value(),
          );

          let mut charms = entry.charms;
          Charm::Burned.set(&mut charms);

          self.sequence_number_to_inscription_entry.insert(
            sequence_number,
            &InscriptionEntry { charms, ..entry }.store(),
          )?;
        }

        // emit events only for valid bonestones
        if let Some(_height) = self
          .sequence_number_to_bonestone_block_height
          .get(&sequence_number)?
        {
          self.event_emitter.emit(
            txid,
            EventInfo::InscriptionTransferred {
              inscription_id,
              new_location: new_satpoint,
              old_location: old_satpoint,
              sequence_number,
            },
          )?;
        }

        let relic_sealed = self
          .sequence_number_to_spaced_relic
          .get(sequence_number)?
          .map(|entry| SpacedRelic::load(entry.value()));

        // also emit transfer event if relic_sealed is present
        if relic_sealed.is_some() {
          self.event_emitter.emit(
            txid,
            EventInfo::InscriptionTransferred {
              inscription_id,
              new_location: new_satpoint,
              old_location: old_satpoint,
              sequence_number,
            },
          )?;
        }

        self
          .satpoint_to_sequence_number
          .remove_all(&old_satpoint.store())?;
        self.satpoint_to_id.remove(&old_satpoint.store())?;
        seq_number = sequence_number;
      }
      Origin::New {
        fee,
        inscription: inscription_new,
      } => {
        seq_number = self.next_sequence_number;
        self.next_sequence_number += 1;
        self
          .inscription_number_to_sequence_number
          .insert(&self.next_number, seq_number)?;

        if self.height >= BONESTONES_START_BLOCK && self.height < BONESTONES_END_BLOCK {
          if let Some(delegate_id) = inscription_new.delegate() {
            if delegate_id == InscriptionId::from_str(BONESTONES_INSCRIPTION_ID)? {
              self
                .sequence_number_to_bonestone_block_height
                .insert(seq_number, self.height)?;
            };
          };
        }

        self
          .home_inscriptions
          .insert(&seq_number, inscription_id.store())?;

        if self.home_inscription_count == 100 {
          self.home_inscriptions.pop_first()?;
        } else {
          self.home_inscription_count += 1;
        }

        let mut sat = None;
        if let Some(input_sat_ranges) = input_sat_ranges {
          let mut offset = 0;
          for (start, end) in input_sat_ranges {
            let size = end - start;
            if offset + size > flotsam.offset {
              let n = start + flotsam.offset - offset;
              self.sat_to_sequence_number.insert(&n, &seq_number)?;
              sat = Some(Sat(n));
              break;
            }
            offset += size;
          }
        }

        let mut charms = 0;

        if op_return {
          Charm::Burned.set(&mut charms);
        }

        let parent_sequence_numbers = inscription_new
          .parents()
          .iter()
          .map(|parent| {
            let parent_sequence_number = self
              .inscription_id_to_sequence_number
              .get(&parent.store())?
              .unwrap()
              .value();

            self
              .sequence_number_to_children
              .insert(parent_sequence_number, seq_number)?;

            Ok(parent_sequence_number)
          })
          .collect::<Result<Vec<u32>>>()?;

        self.sequence_number_to_inscription_entry.insert(
          seq_number,
          &InscriptionEntry {
            charms,
            fee,
            height: self.height,
            id: inscription_id,
            inscription_number: self.next_number,
            parents: parent_sequence_numbers,
            sat,
            sequence_number: seq_number,
            timestamp: self.timestamp,
          }
          .store(),
        )?;

        self
          .inscription_id_to_sequence_number
          .insert(&inscription_id.store(), seq_number)?;

        self.next_number += 1;
      }
    }

    let new_satpoint = new_satpoint.store();

    self
      .satpoint_to_sequence_number
      .insert(&new_satpoint, seq_number)?;
    self
      .satpoint_to_id
      .insert(&new_satpoint, &inscription_id.store())?;
    self
      .id_to_satpoint
      .insert(&inscription_id.store(), &new_satpoint)?;
    self
      .sequence_number_to_satpoint
      .insert(seq_number, &new_satpoint)?;

    Ok(())
  }
}
