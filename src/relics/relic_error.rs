use super::*;

#[derive(Debug, PartialEq, Copy, Clone, Serialize, Deserialize)]
pub enum RelicError {
  SealingAlreadyExists(SpacedRelic),
  SealingInsufficientBalance(u128),
  SealingBaseToken,
  SealingNotFound,
  Unmintable,
  MintCap(u128),
  MintInsufficientBalance(u128),
  UnmintNotAllowed,
  NoMintsToUnmint,
  MaxMintPerTxExceeded(u32),
  MintBaseLimitExceeded(u128, u128),
  UnmintInsufficientBalance(u128, u128),
  MintBlockCapExceeded(u16),
  SwapNotAvailable,
  SwapHeightNotReached(u64),
  SwapFailed(PoolError),
  SwapInsufficientBalance(u128),
  InscriptionMissing,
  InscriptionMetadataMissing,
  InvalidMetadata,
  PriceComputationError,
  SyndicateStart(u64),
  SyndicateEnd(u64),
  SyndicateCap(u32),
  SyndicateIsGated,
  SyndicateNotFound(SyndicateId),
  #[serde(rename = "BoneAlreadyEnshrined")]
  RelicAlreadyEnshrined,
  #[serde(rename = "BoneNotFound")]
  RelicNotFound(RelicId),
  #[serde(rename = "BoneOwnerOnly")]
  RelicOwnerOnly,
  #[serde(rename = "BoneSubsidyLocked")]
  RelicSubsidyLocked,
  ChestInsufficientBalance(RelicId, u128),
  ChestNotFound,
  ChestLocked(u64),
  NoClaimableBalance,
}

impl Display for RelicError {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    match self {
      RelicError::Unmintable => write!(f, "not mintable"),
      RelicError::MintCap(cap) => write!(f, "limited to {cap} mints"),
      RelicError::MintInsufficientBalance(price) => {
        write!(f, "insufficient balance for mint price of {price}")
      }
      RelicError::UnmintNotAllowed => write!(f, "unmint not allowed (here)"),
      RelicError::NoMintsToUnmint => write!(f, "no mints to unmint"),
      RelicError::MaxMintPerTxExceeded(max) => {
        write!(f, "maximum mints per transaction exceeded: {max}")
      }
      RelicError::MintBaseLimitExceeded(limit, price) => {
        write!(f, "mint base limit exceeded: limit {limit}, price {price}")
      }
      RelicError::UnmintInsufficientBalance(required, available) => {
        write!(f, "insufficient minted token balance for unmint: required {required}, available {available}")
      }
      RelicError::MintBlockCapExceeded(limit) => write!(f, "max mints per block exceeded: only {limit} allowed per block"),
      RelicError::PriceComputationError => write!(f, "price computation error"),
      RelicError::SwapNotAvailable => write!(f, "liquidity pool for swap not available (yet)"),
      RelicError::SwapHeightNotReached(swap_height) => {
        write!(
          f,
          "this Bone cannot be swapped yet: minimum block height of {swap_height} not reached"
        )
      }
      RelicError::SwapFailed(cause) => write!(f, "swap failed: {cause}"),
      RelicError::SwapInsufficientBalance(required) => {
        write!(f, "insufficient balance for swap {required}")
      }
      RelicError::SyndicateStart(start) => write!(f, "Syndicate opens on block {start}"),
      RelicError::SyndicateEnd(end) => write!(f, "Syndicate closed on block {end}"),
      RelicError::SyndicateCap(cap) => write!(f, "Syndicate limited to {cap} Chests"),
      RelicError::InscriptionMissing => write!(f, "no Inscription found in transaction"),
      RelicError::InscriptionMetadataMissing => write!(f, "no metadata on Inscription found"),
      RelicError::InvalidMetadata => write!(
        f,
        "Inscription metadata does not contain a valid Bone ticker"
      ),
      RelicError::SealingAlreadyExists(relic) => {
        write!(f, "Bone has already been sealed: {relic}")
      }
      RelicError::SealingInsufficientBalance(fee) => {
        write!(f, "insufficient balance for sealing fee: {fee}")
      }
      RelicError::SealingNotFound => write!(f, "Sealing not found"),
      RelicError::SealingBaseToken => write!(f, "Sealing base token is invalid"),
      RelicError::RelicAlreadyEnshrined => write!(f, "Bone has already been enshrined"),
      RelicError::RelicNotFound(id) => write!(f, "Bone not found: {id}"),
      RelicError::RelicOwnerOnly => {
        write!(f, "this operation can only be performed by the Bone owner")
      }
      RelicError::RelicSubsidyLocked => write!(f, "Bone subsidy is locked"),
      RelicError::SyndicateNotFound(id) => write!(f, "Syndicate not found: {id}"),
      RelicError::ChestInsufficientBalance(id, amount) => {
        write!(f, "insufficient balance for Chest quota: {amount} {id}")
      }
      RelicError::ChestNotFound => write!(f, "Chest not found"),
      RelicError::ChestLocked(unlock_height) => {
        write!(f, "Chest is locked until block {unlock_height}")
      }
      RelicError::SyndicateIsGated => {
        write!(f, "unable to encase Chest: Syndicate is gated to the owner")
      }
      RelicError::NoClaimableBalance => {
        write!(f, "unable to claim: No claimable balance for given output")
      }
    }
  }
}

impl std::error::Error for RelicError {}

#[cfg(test)]
mod tests {
  use crate::index::event::{Event, EventInfo, RelicOperation};
  use crate::index::relics_entry::{RelicEntry, RelicState};
  use crate::relics::enshrining::{Enshrining, MintTerms};
  use crate::relics::keepsake::Keepsake;
  use crate::relics::pool::Pool;
  use crate::relics::relic::Relic;
  use crate::relics::summoning::Summoning;
  use crate::relics::swap::Swap;
  use crate::relics::transfer::Transfer;
  use {super::*, crate::index::testing::Context};

  const RELIC: u128 = 99246114928149462;

  #[test]
  fn index_starts_with_no_relics() {
    let context = Context::builder().arg("--index-relics").build();
    context.assert_relics([(RELIC_ID, Context::base_token_entry())], []);
  }

  #[test]
  fn default_index_does_not_index_relics() {
    let context = Context::builder().build();

    context.mine_blocks(1);
    context.mint_base_token(1, 2);
    context.assert_relics([], []);
  }

  #[test]
  fn empty_keepsake_does_not_create_relic() {
    let context = Context::builder().arg("--index-relics").build();

    context.mine_blocks(1);
    context.relic_tx(&[], 1, Keepsake::default());
    context.mine_blocks(1);
    context.assert_relics([(RELIC_ID, Context::base_token_entry())], []);
  }

  #[test]
  fn enshrining_with_no_transfers_creates_relic() {
    let context = Context::builder().arg("--index-relics").build();

    let (_, mut entry_base) = context.mint_base_token(1, 1);

    let (txid, id) = context.enshrine(
      SpacedRelic::new(Relic(RELIC), 0),
      Enshrining {
        mint_terms: Some(MintTerms {
          amount: Some(1000),
          cap: Some(1),
          price: Some(1),
          seed: Some(1000),
          swap_height: None,
        }),
        ..default()
      },
    );

    // sealing fee is burned
    entry_base.state.burned += 100000000;

    context.assert_relics(
      [
        (RELIC_ID, entry_base),
        (
          id,
          RelicEntry {
            block: id.block,
            enshrining: txid,
            number: 1,
            spaced_relic: SpacedRelic {
              relic: Relic(RELIC),
              spacers: 0,
            },
            timestamp: id.block,
            owner_sequence_number: Some(0),
            mint_terms: Some(MintTerms {
              amount: Some(1000),
              cap: Some(1),
              price: Some(1),
              seed: Some(1000),
              swap_height: None,
            }),
            ..default()
          },
        ),
      ],
      // all base tokens were spent on the sealing fee
      [],
    );
  }

  #[test]
  fn base_token_is_mintable() {
    let context = Context::builder().arg("--index-relics").build();

    let (txids, entry) = context.mint_base_token(1, 1);

    context.assert_relics(
      [(RELIC_ID, entry)],
      [(
        OutPoint {
          txid: txids[0],
          vout: 0,
        },
        vec![(RELIC_ID, 100_000_000)],
      )],
    );

    assert_eq!(entry.circulating_supply(), 100_000_000);
  }

  #[test]
  fn quote_token_is_mintable() {
    let context = Context::builder().arg("--index-relics").build();

    let (_, mut entry_base) = context.mint_base_token(2, 1);

    let (txid_enshrine, id) = context.enshrine(
      SpacedRelic::new(Relic(RELIC), 0),
      Enshrining {
        mint_terms: Some(MintTerms {
          amount: Some(1000),
          cap: Some(1),
          price: Some(5000),
          seed: Some(1000),
          swap_height: None,
        }),
        ..default()
      },
    );

    // sealing fee is burned
    entry_base.state.burned += 100000000;

    let txid_mint = context.relic_tx(
      &context.relic_outpoints(vec![(RELIC_ID, 5000)]),
      1,
      Keepsake {
        mint: Some(id),
        ..default()
      },
    );

    context.mine_blocks(1);

    let mut entry_quote = RelicEntry {
      block: id.block,
      enshrining: txid_enshrine,
      number: 1,
      spaced_relic: SpacedRelic {
        relic: Relic(RELIC),
        spacers: 0,
      },
      symbol: None,
      owner_sequence_number: Some(0),
      mint_terms: Some(MintTerms {
        amount: Some(1000),
        cap: Some(1),
        price: Some(5000),
        seed: Some(1000),
        swap_height: None,
      }),
      state: RelicState {
        mints: 1,
        ..default()
      },
      pool: Some(Pool {
        base_supply: 5000,
        quote_supply: 1000,
        fee_percentage: 1,
      }),
      timestamp: id.block,
      turbo: false,
    };

    context.assert_relics(
      [(RELIC_ID, entry_base), (id, entry_quote)],
      [(
        OutPoint {
          txid: txid_mint,
          vout: 0,
        },
        vec![(RELIC_ID, 99995000), (id, 1000)],
      )],
    );

    let txid_swap = context.relic_tx(
      &context.relic_outpoints(vec![(RELIC_ID, 560)]),
      1,
      Keepsake {
        swap: Some(Swap {
          output: Some(id),
          output_amount: Some(100),
          input: None,
          // max input of base tokens
          input_amount: Some(562),
          is_exact_input: false,
        }),
        ..default()
      },
    );

    context.mine_blocks(1);

    // update expected balances after the swap
    entry_quote.pool.as_mut().unwrap().base_supply += 556;
    entry_quote.pool.as_mut().unwrap().quote_supply -= 100;

    context.assert_relics(
      [(RELIC_ID, entry_base), (id, entry_quote)],
      [(
        OutPoint {
          txid: txid_swap,
          vout: 0,
        },
        vec![(RELIC_ID, 99995000 - 556 - 6), (id, 1100)],
      )],
    );
  }

  #[test]
  fn summoning_creates_syndicate() {
    let context = Context::builder().arg("--index-relics").build();

    let (_txid_base, mut entry_base) = context.mint_base_token(1, 1);

    // we need to enshrine a relic first
    let (txid_relic_enshrine, relic_id) = context.enshrine(
      SpacedRelic::new(Relic(RELIC), 0),
      Enshrining {
        mint_terms: Some(MintTerms {
          amount: Some(1000),
          cap: Some(1),
          price: Some(1),
          seed: Some(1000),
          swap_height: None,
        }),
        ..default()
      },
    );

    // sealing fee is burned
    entry_base.state.burned += 100000000;

    let relic_entry = RelicEntry {
      block: relic_id.block,
      enshrining: txid_relic_enshrine,
      number: 1,
      spaced_relic: SpacedRelic {
        relic: Relic(RELIC),
        spacers: 0,
      },
      timestamp: relic_id.block,
      owner_sequence_number: Some(0),
      mint_terms: Some(MintTerms {
        amount: Some(1000),
        cap: Some(1),
        price: Some(1),
        seed: Some(1000),
        swap_height: None,
      }),
      ..default()
    };

    // summon a super basic syndicate
    let (_, syndicate_id, mut syndicate_entry) = context.syndicate(Summoning {
      treasure: Some(relic_id),
      quota: Some(1),
      ..default()
    });
    // sequence_number 0 will be the sealed Relic ticker
    syndicate_entry.sequence_number = 1;

    context.assert_relics(
      [(RELIC_ID, entry_base), (relic_id, relic_entry)],
      // all base tokens were spent on the sealing fee
      [],
    );

    context.assert_syndicates([(syndicate_id, syndicate_entry)]);
  }

  #[test]
  fn relic_events() {
    let (event_sender, mut event_receiver) = tokio::sync::mpsc::channel(1024);
    let context = Context::builder()
      .arg("--index-relics")
      .event_sender(event_sender)
      .build();

    let (txid_base, _) = context.mint_base_token(3, 1);

    let (txid_enshrine1, id1) = context.enshrine(
      SpacedRelic::new(Relic(RELIC), 0),
      Enshrining {
        mint_terms: Some(MintTerms {
          cap: Some(1),
          amount: Some(1000),
          price: Some(5000),
          seed: Some(1000),
          swap_height: None,
        }),
        ..default()
      },
    );

    let (txid_enshrine2, id2) = context.enshrine(
      SpacedRelic::new(Relic(RELIC + 1), 0),
      Enshrining {
        mint_terms: Some(MintTerms {
          cap: Some(1),
          amount: Some(1000),
          price: Some(5000),
          seed: Some(1000),
          swap_height: None,
        }),
        ..default()
      },
    );

    let txid_mint1 = context.relic_tx(
      &context.relic_outpoints(vec![(RELIC_ID, 5000)]),
      2,
      Keepsake {
        mint: Some(id1),
        // put minted Relics into a different output than the remaining Base Tokens
        transfers: vec![Transfer {
          id: id1,
          amount: 0,
          output: 1,
        }],
        ..default()
      },
    );

    context.mine_blocks(1);

    let txid_mint2 = context.relic_tx(
      &context.relic_outpoints(vec![(RELIC_ID, 5000)]),
      2,
      Keepsake {
        mint: Some(id2),
        // put minted Relics into a different output than the remaining Base Tokens
        transfers: vec![Transfer {
          id: id2,
          amount: 0,
          output: 1,
        }],
        ..default()
      },
    );

    context.mine_blocks(1);

    let txid_swap = context.relic_tx(
      &context.relic_outpoints(vec![(id1, 600)]),
      1,
      Keepsake {
        swap: Some(Swap {
          input: Some(id1),
          output: Some(id2),
          // max input of token 1
          input_amount: Some(600),
          // expected output of token 2
          output_amount: Some(100),
          is_exact_input: false,
        }),
        ..default()
      },
    );

    context.mine_blocks(1);

    let txid_claim = context.relic_tx(
      &[],
      1,
      Keepsake {
        claim: Some(0),
        ..default()
      },
    );

    // add a failing operation on purpose to emit an error event
    let txid_claim_with_error = context.relic_tx(
      &[],
      1,
      Keepsake {
        claim: Some(0),
        ..default()
      },
    );

    context.mine_blocks(1);

    context.assert_events(
      &mut event_receiver,
      vec![
        Event {
          block_height: 1,
          event_index: 0,
          txid: txid_base[0],
          info: EventInfo::RelicMinted {
            relic_id: RELIC_ID,
            amount: 100000000,
          },
        },
        Event {
          block_height: 1,
          event_index: 1,
          txid: txid_base[0],
          info: EventInfo::RelicTransferred {
            relic_id: RELIC_ID,
            amount: 100000000,
            output: 0,
          },
        },
        Event {
          block_height: 1,
          event_index: 2,
          txid: txid_base[1],
          info: EventInfo::RelicMinted {
            relic_id: RELIC_ID,
            amount: 100000000,
          },
        },
        Event {
          block_height: 1,
          event_index: 3,
          txid: txid_base[1],
          info: EventInfo::RelicTransferred {
            relic_id: RELIC_ID,
            amount: 200000000,
            output: 0,
          },
        },
        Event {
          block_height: 1,
          event_index: 4,
          txid: txid_base[2],
          info: EventInfo::RelicMinted {
            relic_id: RELIC_ID,
            amount: 100000000,
          },
        },
        Event {
          block_height: 1,
          event_index: 5,
          txid: txid_base[2],
          info: EventInfo::RelicTransferred {
            relic_id: RELIC_ID,
            amount: 300000000,
            output: 0,
          },
        },
        Event {
          block_height: 9,
          event_index: 0,
          txid: txid_enshrine1,
          info: EventInfo::InscriptionCreated {
            charms: 0,
            inscription_id: InscriptionId {
              txid: txid_enshrine1,
              index: 0,
            },
            location: Some(SatPoint {
              outpoint: OutPoint {
                txid: txid_enshrine1,
                vout: 0,
              },
              offset: 0,
            }),
            parent_inscription_ids: vec![],
            sequence_number: 0,
          },
        },
        Event {
          block_height: 9,
          event_index: 1,
          txid: txid_enshrine1,
          info: EventInfo::RelicSealed {
            spaced_relic: SpacedRelic::new(Relic(RELIC), 0),
            sequence_number: 0,
          },
        },
        Event {
          block_height: 9,
          event_index: 2,
          txid: txid_enshrine1,
          info: EventInfo::RelicEnshrined { relic_id: id1 },
        },
        Event {
          block_height: 9,
          event_index: 3,
          txid: txid_enshrine1,
          info: EventInfo::RelicTransferred {
            relic_id: RELIC_ID,
            amount: 200000000,
            output: 1,
          },
        },
        Event {
          block_height: 9,
          event_index: 4,
          txid: txid_enshrine1,
          info: EventInfo::RelicBurned {
            relic_id: RELIC_ID,
            // sealing fee is burned
            amount: 100000000,
          },
        },
        Event {
          block_height: 17,
          event_index: 0,
          txid: txid_enshrine2,
          info: EventInfo::InscriptionCreated {
            charms: 0,
            inscription_id: InscriptionId {
              txid: txid_enshrine2,
              index: 0,
            },
            location: Some(SatPoint {
              outpoint: OutPoint {
                txid: txid_enshrine2,
                vout: 0,
              },
              offset: 0,
            }),
            parent_inscription_ids: vec![],
            sequence_number: 1,
          },
        },
        Event {
          block_height: 17,
          event_index: 1,
          txid: txid_enshrine2,
          info: EventInfo::RelicSealed {
            spaced_relic: SpacedRelic::new(Relic(RELIC + 1), 0),
            sequence_number: 1,
          },
        },
        Event {
          block_height: 17,
          event_index: 2,
          txid: txid_enshrine2,
          info: EventInfo::RelicEnshrined { relic_id: id2 },
        },
        Event {
          block_height: 17,
          event_index: 3,
          txid: txid_enshrine2,
          info: EventInfo::RelicTransferred {
            relic_id: RELIC_ID,
            amount: 100000000,
            output: 1,
          },
        },
        Event {
          block_height: 17,
          event_index: 4,
          txid: txid_enshrine2,
          info: EventInfo::RelicBurned {
            relic_id: RELIC_ID,
            // sealing fee is burned
            amount: 100000000,
          },
        },
        Event {
          block_height: 18,
          event_index: 0,
          txid: txid_mint1,
          info: EventInfo::RelicMinted {
            relic_id: id1,
            amount: 1000,
          },
        },
        Event {
          block_height: 18,
          event_index: 1,
          txid: txid_mint1,
          info: EventInfo::RelicTransferred {
            relic_id: RELIC_ID,
            amount: 99995000,
            output: 0,
          },
        },
        Event {
          block_height: 18,
          event_index: 2,
          txid: txid_mint1,
          info: EventInfo::RelicTransferred {
            relic_id: id1,
            amount: 1000,
            output: 1,
          },
        },
        Event {
          block_height: 19,
          event_index: 0,
          txid: txid_mint2,
          info: EventInfo::RelicMinted {
            relic_id: id2,
            amount: 1000,
          },
        },
        Event {
          block_height: 19,
          event_index: 1,
          txid: txid_mint2,
          info: EventInfo::RelicTransferred {
            relic_id: RELIC_ID,
            amount: 99990000,
            output: 0,
          },
        },
        Event {
          block_height: 19,
          event_index: 2,
          txid: txid_mint2,
          info: EventInfo::RelicTransferred {
            relic_id: id2,
            amount: 1000,
            output: 1,
          },
        },
        Event {
          block_height: 20,
          event_index: 0,
          txid: txid_swap,
          info: EventInfo::RelicSwapped {
            relic_id: id1,
            base_amount: 562,
            quote_amount: 129,
            fee: 6,
            is_sell_order: true,
            is_exact_input: false,
          },
        },
        Event {
          block_height: 20,
          event_index: 1,
          txid: txid_swap,
          info: EventInfo::RelicSwapped {
            relic_id: id2,
            base_amount: 562,
            quote_amount: 100,
            fee: 6,
            is_sell_order: false,
            is_exact_input: false,
          },
        },
        Event {
          block_height: 20,
          event_index: 2,
          txid: txid_swap,
          info: EventInfo::RelicTransferred {
            relic_id: id1,
            amount: 871,
            output: 0,
          },
        },
        Event {
          block_height: 20,
          event_index: 3,
          txid: txid_swap,
          info: EventInfo::RelicTransferred {
            relic_id: id2,
            amount: 100,
            output: 0,
          },
        },
        Event {
          block_height: 21,
          event_index: 0,
          txid: txid_claim,
          info: EventInfo::RelicClaimed { amount: 6 + 6 },
        },
        Event {
          block_height: 21,
          event_index: 1,
          txid: txid_claim,
          info: EventInfo::RelicTransferred {
            relic_id: RELIC_ID,
            amount: 6 + 6,
            output: 0,
          },
        },
        Event {
          block_height: 21,
          event_index: 2,
          txid: txid_claim_with_error,
          info: EventInfo::RelicError {
            operation: RelicOperation::Claim,
            error: RelicError::NoClaimableBalance,
          },
        },
      ],
    );
  }

  // #[test]
  // fn enshrining_with_edict_creates_relic() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: u128::MAX,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(OutPoint { txid, vout: 0 }, vec![(id, u128::MAX)])],
  //   );
  // }
  //
  // #[test]
  // fn relics_must_be_greater_than_or_equal_to_minimum_for_height() {
  //   let minimum = Relic::minimum_at_height(
  //     Chain::Regtest.network(),
  //     Height((Keepsake::COMMIT_CONFIRMATIONS + 2).into()),
  //   )
  //     .0;
  //
  //   {
  //     let context = Context::builder()
  //       .chain(Chain::Regtest)
  //       .arg("--index-relics")
  //       .build();
  //
  //     context.enshrine(
  //       Keepsake {
  //         transfers: vec![Transfer {
  //           id: RelicId::default(),
  //           amount: u128::MAX,
  //           output: 0,
  //         }],
  //         enshrining: Some(Enshrining {
  //           relic: Some(Relic(minimum - 1)),
  //           premine: Some(u128::MAX),
  //           ..default()
  //         }),
  //         ..default()
  //       },
  //       1,
  //     );
  //
  //     context.assert_relics([], []);
  //   }
  //
  //   {
  //     let context = Context::builder()
  //       .chain(Chain::Regtest)
  //       .arg("--index-relics")
  //       .build();
  //
  //     let (txid, id) = context.enshrine(
  //       Keepsake {
  //         transfers: vec![Transfer {
  //           id: RelicId::default(),
  //           amount: u128::MAX,
  //           output: 0,
  //         }],
  //         enshrining: Some(Enshrining {
  //           relic: Some(Relic(minimum)),
  //           premine: Some(u128::MAX),
  //           ..default()
  //         }),
  //         ..default()
  //       },
  //       1,
  //     );
  //
  //     context.assert_relics(
  //       [(
  //         id,
  //         RelicEntry {
  //           block: id.block,
  //           enshrining: txid,
  //           spaced_relic: SpacedRelic {
  //             relic: Relic(minimum),
  //             spacers: 0,
  //           },
  //           premine: u128::MAX,
  //           timestamp: id.block,
  //           ..default()
  //         },
  //       )],
  //       [(OutPoint { txid, vout: 0 }, vec![(id, u128::MAX)])],
  //     );
  //   }
  // }
  //
  // #[test]
  // fn enshrining_cannot_specify_reserved_relic() {
  //   {
  //     let context = Context::builder().arg("--index-relics").build();
  //
  //     context.enshrine(
  //       Keepsake {
  //         transfers: vec![Transfer {
  //           id: RelicId::default(),
  //           amount: u128::MAX,
  //           output: 0,
  //         }],
  //         enshrining: Some(Enshrining {
  //           relic: Some(Relic::reserved(0, 0)),
  //           ..default()
  //         }),
  //         ..default()
  //       },
  //       1,
  //     );
  //
  //     context.assert_relics([], []);
  //   }
  //
  //   {
  //     let context = Context::builder().arg("--index-relics").build();
  //
  //     let (txid, id) = context.enshrine(
  //       Keepsake {
  //         transfers: vec![Transfer {
  //           id: RelicId::default(),
  //           amount: u128::MAX,
  //           output: 0,
  //         }],
  //         enshrining: Some(Enshrining {
  //           relic: Some(Relic(Relic::reserved(0, 0).n() - 1)),
  //           premine: Some(u128::MAX),
  //           ..default()
  //         }),
  //         ..default()
  //       },
  //       1,
  //     );
  //
  //     context.assert_relics(
  //       [(
  //         id,
  //         RelicEntry {
  //           block: id.block,
  //           enshrining: txid,
  //           spaced_relic: SpacedRelic {
  //             relic: Relic(Relic::reserved(0, 0).n() - 1),
  //             spacers: 0,
  //           },
  //           premine: u128::MAX,
  //           timestamp: id.block,
  //           ..default()
  //         },
  //       )],
  //       [(OutPoint { txid, vout: 0 }, vec![(id, u128::MAX)])],
  //     );
  //   }
  // }
  //
  // #[test]
  // fn reserved_relics_may_be_enshrined() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   context.mine_blocks(1);
  //
  //   let txid0 = context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(1, 0, 0, Witness::new())],
  //     outputs: 2,
  //     op_return: Some(
  //       Keepsake {
  //         transfers: vec![Transfer {
  //           id: RelicId::default(),
  //           amount: u128::MAX,
  //           output: 0,
  //         }],
  //         enshrining: Some(Enshrining {
  //           relic: None,
  //           premine: Some(u128::MAX),
  //           ..default()
  //         }),
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   let id0 = RelicId { block: 2, tx: 1 };
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [(
  //       id0,
  //       RelicEntry {
  //         block: id0.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic::reserved(id0.block, id0.tx),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: 2,
  //         ..default()
  //       },
  //     )],
  //     [(
  //       OutPoint {
  //         txid: txid0,
  //         vout: 0,
  //       },
  //       vec![(id0, u128::MAX)],
  //     )],
  //   );
  //
  //   context.mine_blocks(1);
  //
  //   let txid1 = context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(2, 0, 0, Witness::new())],
  //     op_return: Some(
  //       Keepsake {
  //         transfers: vec![Transfer {
  //           id: RelicId::default(),
  //           amount: u128::MAX,
  //           output: 0,
  //         }],
  //         enshrining: Some(Enshrining {
  //           premine: Some(u128::MAX),
  //           relic: None,
  //           ..default()
  //         }),
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   let id1 = RelicId { block: 4, tx: 1 };
  //
  //   context.assert_relics(
  //     [
  //       (
  //         id0,
  //         RelicEntry {
  //           block: id0.block,
  //           enshrining: txid0,
  //           spaced_relic: SpacedRelic {
  //             relic: Relic::reserved(id0.block, id0.tx),
  //             spacers: 0,
  //           },
  //           premine: u128::MAX,
  //           timestamp: 2,
  //           ..default()
  //         },
  //       ),
  //       (
  //         id1,
  //         RelicEntry {
  //           block: id1.block,
  //           enshrining: txid1,
  //           spaced_relic: SpacedRelic {
  //             relic: Relic::reserved(id1.block, id0.tx),
  //             spacers: 0,
  //           },
  //           premine: u128::MAX,
  //           timestamp: 4,
  //           number: 1,
  //           ..default()
  //         },
  //       ),
  //     ],
  //     [
  //       (
  //         OutPoint {
  //           txid: txid0,
  //           vout: 0,
  //         },
  //         vec![(id0, u128::MAX)],
  //       ),
  //       (
  //         OutPoint {
  //           txid: txid1,
  //           vout: 0,
  //         },
  //         vec![(id1, u128::MAX)],
  //       ),
  //     ],
  //   );
  // }
  //
  // #[test]
  // fn enshrining_with_non_zero_divisibility_and_relic() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: u128::MAX,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         divisibility: Some(1),
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         enshrining: txid,
  //         divisibility: 1,
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(OutPoint { txid, vout: 0 }, vec![(id, u128::MAX)])],
  //   );
  // }
  //
  // #[test]
  // fn allocations_over_max_supply_are_ignored() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![
  //         Transfer {
  //           id: RelicId::default(),
  //           amount: u128::MAX,
  //           output: 0,
  //         },
  //         Transfer {
  //           id: RelicId::default(),
  //           amount: u128::MAX,
  //           output: 0,
  //         },
  //       ],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(OutPoint { txid, vout: 0 }, vec![(id, u128::MAX)])],
  //   );
  // }
  //
  // #[test]
  // fn allocations_partially_over_max_supply_are_honored() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![
  //         Transfer {
  //           id: RelicId::default(),
  //           amount: u128::MAX / 2,
  //           output: 0,
  //         },
  //         Transfer {
  //           id: RelicId::default(),
  //           amount: u128::MAX,
  //           output: 0,
  //         },
  //       ],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         symbol: None,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(OutPoint { txid, vout: 0 }, vec![(id, u128::MAX)])],
  //   );
  // }
  //
  // #[test]
  // fn enshrining_may_allocate_less_than_max_supply() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   context.mine_blocks(1);
  //
  //   let (txid, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: 100,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(100),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: 100,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(OutPoint { txid, vout: 0 }, vec![(id, 100)])],
  //   );
  // }
  //
  // #[test]
  // fn enshrining_may_allocate_to_multiple_outputs() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![
  //         Transfer {
  //           id: RelicId::default(),
  //           amount: 100,
  //           output: 0,
  //         },
  //         Transfer {
  //           id: RelicId::default(),
  //           amount: 100,
  //           output: 1,
  //         },
  //       ],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(200),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         burned: 100,
  //         enshrining: txid,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: 200,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(OutPoint { txid, vout: 0 }, vec![(id, 100)])],
  //   );
  // }
  //
  // #[test]
  // fn allocations_to_invalid_outputs_produce_cenotaph() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![
  //         Transfer {
  //           id: RelicId::default(),
  //           amount: 100,
  //           output: 0,
  //         },
  //         Transfer {
  //           id: RelicId::default(),
  //           amount: 100,
  //           output: 3,
  //         },
  //       ],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: 0,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [],
  //   );
  // }
  //
  // #[test]
  // fn input_relics_may_be_allocated() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: u128::MAX,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(
  //       OutPoint {
  //         txid: txid0,
  //         vout: 0,
  //       },
  //       vec![(id, u128::MAX)],
  //     )],
  //   );
  //
  //   let txid1 = context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(id.block.try_into().unwrap(), 1, 0, Witness::new())],
  //     op_return: Some(
  //       Keepsake {
  //         transfers: vec![Transfer {
  //           id,
  //           amount: u128::MAX,
  //           output: 0,
  //         }],
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(
  //       OutPoint {
  //         txid: txid1,
  //         vout: 0,
  //       },
  //       vec![(id, u128::MAX)],
  //     )],
  //   );
  // }
  //
  // #[test]
  // fn enshrined_relic_is_allocated_with_zero_supply_for_cenotaph() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: u128::MAX,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         ..default()
  //       }),
  //       pointer: Some(10),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [],
  //   );
  // }
  //
  // #[test]
  // fn enshrined_relic_parameters_are_unset_for_cenotaph() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: u128::MAX,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         premine: Some(u128::MAX),
  //         relic: Some(Relic(RELIC)),
  //         terms: Some(Terms {
  //           cap: Some(1),
  //           amount: Some(1),
  //           offset: (Some(1), Some(1)),
  //           height: (None, None),
  //         }),
  //         divisibility: Some(1),
  //         symbol: Some('$'),
  //         spacers: Some(1),
  //         turbo: true,
  //       }),
  //       pointer: Some(10),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         burned: 0,
  //         divisibility: 0,
  //         enshrining: txid0,
  //         terms: None,
  //         mints: 0,
  //         number: 0,
  //         premine: 0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         symbol: None,
  //         timestamp: id.block,
  //         turbo: false,
  //       },
  //     )],
  //     [],
  //   );
  // }
  //
  // #[test]
  // fn reserved_relics_are_not_allocated_in_cenotaph() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   context.mine_blocks(1);
  //
  //   context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(1, 0, 0, Witness::new())],
  //     op_return: Some(
  //       Keepsake {
  //         transfers: vec![Transfer {
  //           id: RelicId::default(),
  //           amount: u128::MAX,
  //           output: 0,
  //         }],
  //         enshrining: Some(Enshrining::default()),
  //         pointer: Some(10),
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics([], []);
  // }
  //
  // #[test]
  // fn input_relics_are_burned_if_an_unrecognized_even_tag_is_encountered() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: u128::MAX,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(
  //       OutPoint {
  //         txid: txid0,
  //         vout: 0,
  //       },
  //       vec![(id, u128::MAX)],
  //     )],
  //   );
  //
  //   context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(id.block.try_into().unwrap(), 1, 0, Witness::new())],
  //     op_return: Some(
  //       Keepsake {
  //         pointer: Some(10),
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         burned: u128::MAX,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [],
  //   );
  // }
  //
  // #[test]
  // fn unallocated_relics_are_assigned_to_first_non_op_return_output() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: u128::MAX,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(
  //       OutPoint {
  //         txid: txid0,
  //         vout: 0,
  //       },
  //       vec![(id, u128::MAX)],
  //     )],
  //   );
  //
  //   let txid1 = context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(id.block.try_into().unwrap(), 1, 0, Witness::new())],
  //     op_return: Some(Keepsake::default().encipher()),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(
  //       OutPoint {
  //         txid: txid1,
  //         vout: 0,
  //       },
  //       vec![(id, u128::MAX)],
  //     )],
  //   );
  // }
  //
  // #[test]
  // fn unallocated_relics_are_burned_if_no_non_op_return_output_is_present() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: u128::MAX,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(
  //       OutPoint {
  //         txid: txid0,
  //         vout: 0,
  //       },
  //       vec![(id, u128::MAX)],
  //     )],
  //   );
  //
  //   context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(id.block.try_into().unwrap(), 1, 0, Witness::new())],
  //     op_return: Some(Keepsake::default().encipher()),
  //     outputs: 0,
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         burned: u128::MAX,
  //         ..default()
  //       },
  //     )],
  //     [],
  //   );
  // }
  //
  // #[test]
  // fn unallocated_relics_are_assigned_to_default_output() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: u128::MAX,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(
  //       OutPoint {
  //         txid: txid0,
  //         vout: 0,
  //       },
  //       vec![(id, u128::MAX)],
  //     )],
  //   );
  //
  //   let txid1 = context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(id.block.try_into().unwrap(), 1, 0, Witness::new())],
  //     outputs: 2,
  //     op_return: Some(
  //       Keepsake {
  //         pointer: Some(1),
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(
  //       OutPoint {
  //         txid: txid1,
  //         vout: 1,
  //       },
  //       vec![(id, u128::MAX)],
  //     )],
  //   );
  // }
  //
  // #[test]
  // fn unallocated_relics_are_burned_if_default_output_is_op_return() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: u128::MAX,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(
  //       OutPoint {
  //         txid: txid0,
  //         vout: 0,
  //       },
  //       vec![(id, u128::MAX)],
  //     )],
  //   );
  //
  //   context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(id.block.try_into().unwrap(), 1, 0, Witness::new())],
  //     outputs: 2,
  //     op_return: Some(
  //       Keepsake {
  //         pointer: Some(2),
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         burned: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [],
  //   );
  // }
  //
  // #[test]
  // fn unallocated_relics_in_transactions_with_no_keepsake_are_assigned_to_first_non_op_return_output(
  // ) {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: u128::MAX,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(
  //       OutPoint {
  //         txid: txid0,
  //         vout: 0,
  //       },
  //       vec![(id, u128::MAX)],
  //     )],
  //   );
  //
  //   let txid1 = context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(id.block.try_into().unwrap(), 1, 0, Witness::new())],
  //     op_return: None,
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(
  //       OutPoint {
  //         txid: txid1,
  //         vout: 0,
  //       },
  //       vec![(id, u128::MAX)],
  //     )],
  //   );
  // }
  //
  // #[test]
  // fn duplicate_relics_are_forbidden() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: u128::MAX,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(OutPoint { txid, vout: 0 }, vec![(id, u128::MAX)])],
  //   );
  //
  //   context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: u128::MAX,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(OutPoint { txid, vout: 0 }, vec![(id, u128::MAX)])],
  //   );
  // }
  //
  // #[test]
  // fn output_may_hold_multiple_relics() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id0) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: u128::MAX,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id0,
  //       RelicEntry {
  //         block: id0.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id0.block,
  //         ..default()
  //       },
  //     )],
  //     [(
  //       OutPoint {
  //         txid: txid0,
  //         vout: 0,
  //       },
  //       vec![(id0, u128::MAX)],
  //     )],
  //   );
  //
  //   let (txid1, id1) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: u128::MAX,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC + 1)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [
  //       (
  //         id0,
  //         RelicEntry {
  //           block: id0.block,
  //           enshrining: txid0,
  //           spaced_relic: SpacedRelic {
  //             relic: Relic(RELIC),
  //             spacers: 0,
  //           },
  //           premine: u128::MAX,
  //           timestamp: id0.block,
  //           ..default()
  //         },
  //       ),
  //       (
  //         id1,
  //         RelicEntry {
  //           block: id1.block,
  //           enshrining: txid1,
  //           spaced_relic: SpacedRelic {
  //             relic: Relic(RELIC + 1),
  //             spacers: 0,
  //           },
  //           premine: u128::MAX,
  //           timestamp: id1.block,
  //           number: 1,
  //           ..default()
  //         },
  //       ),
  //     ],
  //     [
  //       (
  //         OutPoint {
  //           txid: txid0,
  //           vout: 0,
  //         },
  //         vec![(id0, u128::MAX)],
  //       ),
  //       (
  //         OutPoint {
  //           txid: txid1,
  //           vout: 0,
  //         },
  //         vec![(id1, u128::MAX)],
  //       ),
  //     ],
  //   );
  //
  //   let txid2 = context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[
  //       (id0.block.try_into().unwrap(), 1, 0, Witness::new()),
  //       (id1.block.try_into().unwrap(), 1, 0, Witness::new()),
  //     ],
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [
  //       (
  //         id0,
  //         RelicEntry {
  //           block: id0.block,
  //           enshrining: txid0,
  //           spaced_relic: SpacedRelic {
  //             relic: Relic(RELIC),
  //             spacers: 0,
  //           },
  //           premine: u128::MAX,
  //           timestamp: id0.block,
  //           ..default()
  //         },
  //       ),
  //       (
  //         id1,
  //         RelicEntry {
  //           block: id1.block,
  //           enshrining: txid1,
  //           spaced_relic: SpacedRelic {
  //             relic: Relic(RELIC + 1),
  //             spacers: 0,
  //           },
  //           premine: u128::MAX,
  //           timestamp: id1.block,
  //           number: 1,
  //           ..default()
  //         },
  //       ),
  //     ],
  //     [(
  //       OutPoint {
  //         txid: txid2,
  //         vout: 0,
  //       },
  //       vec![(id0, u128::MAX), (id1, u128::MAX)],
  //     )],
  //   );
  // }
  //
  // #[test]
  // fn multiple_input_relics_on_the_same_input_may_be_allocated() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id0) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: u128::MAX,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id0,
  //       RelicEntry {
  //         block: id0.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id0.block,
  //         ..default()
  //       },
  //     )],
  //     [(
  //       OutPoint {
  //         txid: txid0,
  //         vout: 0,
  //       },
  //       vec![(id0, u128::MAX)],
  //     )],
  //   );
  //
  //   let (txid1, id1) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: u128::MAX,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC + 1)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [
  //       (
  //         id0,
  //         RelicEntry {
  //           block: id0.block,
  //           enshrining: txid0,
  //           spaced_relic: SpacedRelic {
  //             relic: Relic(RELIC),
  //             spacers: 0,
  //           },
  //           premine: u128::MAX,
  //           timestamp: id0.block,
  //           ..default()
  //         },
  //       ),
  //       (
  //         id1,
  //         RelicEntry {
  //           block: id1.block,
  //           enshrining: txid1,
  //           spaced_relic: SpacedRelic {
  //             relic: Relic(RELIC + 1),
  //             spacers: 0,
  //           },
  //           premine: u128::MAX,
  //           timestamp: id1.block,
  //           number: 1,
  //           ..default()
  //         },
  //       ),
  //     ],
  //     [
  //       (
  //         OutPoint {
  //           txid: txid0,
  //           vout: 0,
  //         },
  //         vec![(id0, u128::MAX)],
  //       ),
  //       (
  //         OutPoint {
  //           txid: txid1,
  //           vout: 0,
  //         },
  //         vec![(id1, u128::MAX)],
  //       ),
  //     ],
  //   );
  //
  //   let txid2 = context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[
  //       (id0.block.try_into().unwrap(), 1, 0, Witness::new()),
  //       (id1.block.try_into().unwrap(), 1, 0, Witness::new()),
  //     ],
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [
  //       (
  //         id0,
  //         RelicEntry {
  //           block: id0.block,
  //           enshrining: txid0,
  //           spaced_relic: SpacedRelic {
  //             relic: Relic(RELIC),
  //             spacers: 0,
  //           },
  //           premine: u128::MAX,
  //           timestamp: id0.block,
  //           ..default()
  //         },
  //       ),
  //       (
  //         id1,
  //         RelicEntry {
  //           block: id1.block,
  //           enshrining: txid1,
  //           spaced_relic: SpacedRelic {
  //             relic: Relic(RELIC + 1),
  //             spacers: 0,
  //           },
  //           premine: u128::MAX,
  //           timestamp: id1.block,
  //           number: 1,
  //           ..default()
  //         },
  //       ),
  //     ],
  //     [(
  //       OutPoint {
  //         txid: txid2,
  //         vout: 0,
  //       },
  //       vec![(id0, u128::MAX), (id1, u128::MAX)],
  //     )],
  //   );
  //
  //   let txid3 = context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[((id1.block + 1).try_into().unwrap(), 1, 0, Witness::new())],
  //     outputs: 2,
  //     op_return: Some(
  //       Keepsake {
  //         transfers: vec![
  //           Transfer {
  //             id: id0,
  //             amount: u128::MAX / 2,
  //             output: 1,
  //           },
  //           Transfer {
  //             id: id1,
  //             amount: u128::MAX / 2,
  //             output: 1,
  //           },
  //         ],
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [
  //       (
  //         id0,
  //         RelicEntry {
  //           block: id0.block,
  //           enshrining: txid0,
  //           spaced_relic: SpacedRelic {
  //             relic: Relic(RELIC),
  //             spacers: 0,
  //           },
  //           premine: u128::MAX,
  //           timestamp: id0.block,
  //           ..default()
  //         },
  //       ),
  //       (
  //         id1,
  //         RelicEntry {
  //           block: id1.block,
  //           enshrining: txid1,
  //           spaced_relic: SpacedRelic {
  //             relic: Relic(RELIC + 1),
  //             spacers: 0,
  //           },
  //           premine: u128::MAX,
  //           timestamp: id1.block,
  //           number: 1,
  //           ..default()
  //         },
  //       ),
  //     ],
  //     [
  //       (
  //         OutPoint {
  //           txid: txid3,
  //           vout: 0,
  //         },
  //         vec![(id0, u128::MAX / 2 + 1), (id1, u128::MAX / 2 + 1)],
  //       ),
  //       (
  //         OutPoint {
  //           txid: txid3,
  //           vout: 1,
  //         },
  //         vec![(id0, u128::MAX / 2), (id1, u128::MAX / 2)],
  //       ),
  //     ],
  //   );
  // }
  //
  // #[test]
  // fn multiple_input_relics_on_different_inputs_may_be_allocated() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id0) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: u128::MAX,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id0,
  //       RelicEntry {
  //         block: id0.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id0.block,
  //         ..default()
  //       },
  //     )],
  //     [(
  //       OutPoint {
  //         txid: txid0,
  //         vout: 0,
  //       },
  //       vec![(id0, u128::MAX)],
  //     )],
  //   );
  //
  //   let (txid1, id1) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: u128::MAX,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC + 1)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [
  //       (
  //         id0,
  //         RelicEntry {
  //           block: id0.block,
  //           enshrining: txid0,
  //           spaced_relic: SpacedRelic {
  //             relic: Relic(RELIC),
  //             spacers: 0,
  //           },
  //           premine: u128::MAX,
  //           timestamp: id0.block,
  //           ..default()
  //         },
  //       ),
  //       (
  //         id1,
  //         RelicEntry {
  //           block: id1.block,
  //           enshrining: txid1,
  //           spaced_relic: SpacedRelic {
  //             relic: Relic(RELIC + 1),
  //             spacers: 0,
  //           },
  //           premine: u128::MAX,
  //           timestamp: id1.block,
  //           number: 1,
  //           ..default()
  //         },
  //       ),
  //     ],
  //     [
  //       (
  //         OutPoint {
  //           txid: txid0,
  //           vout: 0,
  //         },
  //         vec![(id0, u128::MAX)],
  //       ),
  //       (
  //         OutPoint {
  //           txid: txid1,
  //           vout: 0,
  //         },
  //         vec![(id1, u128::MAX)],
  //       ),
  //     ],
  //   );
  //
  //   let txid2 = context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[
  //       (id0.block.try_into().unwrap(), 1, 0, Witness::new()),
  //       (id1.block.try_into().unwrap(), 1, 0, Witness::new()),
  //     ],
  //     op_return: Some(
  //       Keepsake {
  //         transfers: vec![
  //           Transfer {
  //             id: id0,
  //             amount: u128::MAX,
  //             output: 0,
  //           },
  //           Transfer {
  //             id: id1,
  //             amount: u128::MAX,
  //             output: 0,
  //           },
  //         ],
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [
  //       (
  //         id0,
  //         RelicEntry {
  //           block: id0.block,
  //           enshrining: txid0,
  //           spaced_relic: SpacedRelic {
  //             relic: Relic(RELIC),
  //             spacers: 0,
  //           },
  //           premine: u128::MAX,
  //           timestamp: id0.block,
  //           ..default()
  //         },
  //       ),
  //       (
  //         id1,
  //         RelicEntry {
  //           block: id1.block,
  //           enshrining: txid1,
  //           spaced_relic: SpacedRelic {
  //             relic: Relic(RELIC + 1),
  //             spacers: 0,
  //           },
  //           premine: u128::MAX,
  //           timestamp: id1.block,
  //           number: 1,
  //           ..default()
  //         },
  //       ),
  //     ],
  //     [(
  //       OutPoint {
  //         txid: txid2,
  //         vout: 0,
  //       },
  //       vec![(id0, u128::MAX), (id1, u128::MAX)],
  //     )],
  //   );
  // }
  //
  // #[test]
  // fn unallocated_relics_are_assigned_to_first_non_op_return_output_when_op_return_is_not_last_output(
  // ) {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: u128::MAX,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(
  //       OutPoint {
  //         txid: txid0,
  //         vout: 0,
  //       },
  //       vec![(id, u128::MAX)],
  //     )],
  //   );
  //
  //   let txid = context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(id.block.try_into().unwrap(), 1, 0, Witness::new())],
  //     op_return: Some(
  //       script::Builder::new()
  //         .push_opcode(opcodes::all::OP_RETURN)
  //         .into_script(),
  //     ),
  //     op_return_index: Some(0),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(OutPoint { txid, vout: 1 }, vec![(id, u128::MAX)])],
  //   );
  // }
  //
  // #[test]
  // fn multiple_relics_may_be_enshrined_in_one_block() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id0) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: u128::MAX,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   let (txid1, id1) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: u128::MAX,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC + 1)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [
  //       (
  //         id0,
  //         RelicEntry {
  //           block: id0.block,
  //           enshrining: txid0,
  //           spaced_relic: SpacedRelic {
  //             relic: Relic(RELIC),
  //             spacers: 0,
  //           },
  //           premine: u128::MAX,
  //           timestamp: id0.block,
  //           ..default()
  //         },
  //       ),
  //       (
  //         id1,
  //         RelicEntry {
  //           block: id1.block,
  //           enshrining: txid1,
  //           spaced_relic: SpacedRelic {
  //             relic: Relic(RELIC + 1),
  //             spacers: 0,
  //           },
  //           premine: u128::MAX,
  //           timestamp: id1.block,
  //           number: 1,
  //           ..default()
  //         },
  //       ),
  //     ],
  //     [
  //       (
  //         OutPoint {
  //           txid: txid0,
  //           vout: 0,
  //         },
  //         vec![(id0, u128::MAX)],
  //       ),
  //       (
  //         OutPoint {
  //           txid: txid1,
  //           vout: 0,
  //         },
  //         vec![(id1, u128::MAX)],
  //       ),
  //     ],
  //   );
  // }
  //
  // #[test]
  // fn transfers_with_id_zero_are_skipped() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: u128::MAX,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(
  //       OutPoint {
  //         txid: txid0,
  //         vout: 0,
  //       },
  //       vec![(id, u128::MAX)],
  //     )],
  //   );
  //
  //   let txid1 = context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(id.block.try_into().unwrap(), 1, 0, Witness::new())],
  //     op_return: Some(
  //       Keepsake {
  //         transfers: vec![
  //           Transfer {
  //             id: RelicId::default(),
  //             amount: 100,
  //             output: 0,
  //           },
  //           Transfer {
  //             id,
  //             amount: u128::MAX,
  //             output: 0,
  //           },
  //         ],
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(
  //       OutPoint {
  //         txid: txid1,
  //         vout: 0,
  //       },
  //       vec![(id, u128::MAX)],
  //     )],
  //   );
  // }
  //
  // #[test]
  // fn transfers_which_refer_to_input_relic_with_no_balance_are_skipped() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id0) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: u128::MAX,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id0,
  //       RelicEntry {
  //         block: id0.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id0.block,
  //         ..default()
  //       },
  //     )],
  //     [(
  //       OutPoint {
  //         txid: txid0,
  //         vout: 0,
  //       },
  //       vec![(id0, u128::MAX)],
  //     )],
  //   );
  //
  //   let (txid1, id1) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: u128::MAX,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC + 1)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [
  //       (
  //         id0,
  //         RelicEntry {
  //           block: id0.block,
  //           enshrining: txid0,
  //           spaced_relic: SpacedRelic {
  //             relic: Relic(RELIC),
  //             spacers: 0,
  //           },
  //           premine: u128::MAX,
  //           timestamp: id0.block,
  //           ..default()
  //         },
  //       ),
  //       (
  //         id1,
  //         RelicEntry {
  //           block: id1.block,
  //           enshrining: txid1,
  //           spaced_relic: SpacedRelic {
  //             relic: Relic(RELIC + 1),
  //             spacers: 0,
  //           },
  //           premine: u128::MAX,
  //           timestamp: id1.block,
  //           number: 1,
  //           ..default()
  //         },
  //       ),
  //     ],
  //     [
  //       (
  //         OutPoint {
  //           txid: txid0,
  //           vout: 0,
  //         },
  //         vec![(id0, u128::MAX)],
  //       ),
  //       (
  //         OutPoint {
  //           txid: txid1,
  //           vout: 0,
  //         },
  //         vec![(id1, u128::MAX)],
  //       ),
  //     ],
  //   );
  //
  //   let txid2 = context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(id0.block.try_into().unwrap(), 1, 0, Witness::new())],
  //     op_return: Some(
  //       Keepsake {
  //         transfers: vec![
  //           Transfer {
  //             id: id0,
  //             amount: u128::MAX,
  //             output: 0,
  //           },
  //           Transfer {
  //             id: id1,
  //             amount: u128::MAX,
  //             output: 0,
  //           },
  //         ],
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [
  //       (
  //         id0,
  //         RelicEntry {
  //           block: id0.block,
  //           enshrining: txid0,
  //           spaced_relic: SpacedRelic {
  //             relic: Relic(RELIC),
  //             spacers: 0,
  //           },
  //           premine: u128::MAX,
  //           timestamp: id0.block,
  //           ..default()
  //         },
  //       ),
  //       (
  //         id1,
  //         RelicEntry {
  //           block: id1.block,
  //           enshrining: txid1,
  //           spaced_relic: SpacedRelic {
  //             relic: Relic(RELIC + 1),
  //             spacers: 0,
  //           },
  //           premine: u128::MAX,
  //           timestamp: id1.block,
  //           number: 1,
  //           ..default()
  //         },
  //       ),
  //     ],
  //     [
  //       (
  //         OutPoint {
  //           txid: txid1,
  //           vout: 0,
  //         },
  //         vec![(id1, u128::MAX)],
  //       ),
  //       (
  //         OutPoint {
  //           txid: txid2,
  //           vout: 0,
  //         },
  //         vec![(id0, u128::MAX)],
  //       ),
  //     ],
  //   );
  // }
  //
  // #[test]
  // fn transfers_over_max_inputs_are_ignored() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: u128::MAX / 2,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(u128::MAX / 2),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX / 2,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(
  //       OutPoint {
  //         txid: txid0,
  //         vout: 0,
  //       },
  //       vec![(id, u128::MAX / 2)],
  //     )],
  //   );
  //
  //   let txid1 = context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(id.block.try_into().unwrap(), 1, 0, Witness::new())],
  //     op_return: Some(
  //       Keepsake {
  //         transfers: vec![Transfer {
  //           id,
  //           amount: u128::MAX,
  //           output: 0,
  //         }],
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX / 2,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(
  //       OutPoint {
  //         txid: txid1,
  //         vout: 0,
  //       },
  //       vec![(id, u128::MAX / 2)],
  //     )],
  //   );
  // }
  //
  // #[test]
  // fn transfers_may_transfer_relics_to_op_return_outputs() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: u128::MAX,
  //         output: 1,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         burned: u128::MAX,
  //         enshrining: txid,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [],
  //   );
  // }
  //
  // #[test]
  // fn outputs_with_no_relics_have_no_balance() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: u128::MAX,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(OutPoint { txid, vout: 0 }, vec![(id, u128::MAX)])],
  //   );
  // }
  //
  // #[test]
  // fn transfers_which_transfer_no_relics_to_output_create_no_balance_entry() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![
  //         Transfer {
  //           id: RelicId::default(),
  //           amount: u128::MAX,
  //           output: 0,
  //         },
  //         Transfer {
  //           id: RelicId::default(),
  //           amount: 0,
  //           output: 1,
  //         },
  //       ],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(OutPoint { txid, vout: 0 }, vec![(id, u128::MAX)])],
  //   );
  // }
  //
  // #[test]
  // fn split_in_enshrining() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: 0,
  //         output: 5,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     4,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [
  //       (OutPoint { txid, vout: 0 }, vec![(id, u128::MAX / 4 + 1)]),
  //       (OutPoint { txid, vout: 1 }, vec![(id, u128::MAX / 4 + 1)]),
  //       (OutPoint { txid, vout: 2 }, vec![(id, u128::MAX / 4 + 1)]),
  //       (OutPoint { txid, vout: 3 }, vec![(id, u128::MAX / 4)]),
  //     ],
  //   );
  // }
  //
  // #[test]
  // fn split_in_enshrining_with_preceding_edict() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![
  //         Transfer {
  //           id: RelicId::default(),
  //           amount: 1000,
  //           output: 0,
  //         },
  //         Transfer {
  //           id: RelicId::default(),
  //           amount: 0,
  //           output: 5,
  //         },
  //       ],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     4,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [
  //       (
  //         OutPoint { txid, vout: 0 },
  //         vec![(id, 1000 + (u128::MAX - 1000) / 4 + 1)],
  //       ),
  //       (
  //         OutPoint { txid, vout: 1 },
  //         vec![(id, (u128::MAX - 1000) / 4 + 1)],
  //       ),
  //       (
  //         OutPoint { txid, vout: 2 },
  //         vec![(id, (u128::MAX - 1000) / 4 + 1)],
  //       ),
  //       (
  //         OutPoint { txid, vout: 3 },
  //         vec![(id, (u128::MAX - 1000) / 4)],
  //       ),
  //     ],
  //   );
  // }
  //
  // #[test]
  // fn split_in_enshrining_with_following_edict() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![
  //         Transfer {
  //           id: RelicId::default(),
  //           amount: 0,
  //           output: 5,
  //         },
  //         Transfer {
  //           id: RelicId::default(),
  //           amount: 1000,
  //           output: 0,
  //         },
  //       ],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     4,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [
  //       (OutPoint { txid, vout: 0 }, vec![(id, u128::MAX / 4 + 1)]),
  //       (OutPoint { txid, vout: 1 }, vec![(id, u128::MAX / 4 + 1)]),
  //       (OutPoint { txid, vout: 2 }, vec![(id, u128::MAX / 4 + 1)]),
  //       (OutPoint { txid, vout: 3 }, vec![(id, u128::MAX / 4)]),
  //     ],
  //   );
  // }
  //
  // #[test]
  // fn split_with_amount_in_enshrining() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: 1000,
  //         output: 5,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(4000),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     4,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: 4000,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [
  //       (OutPoint { txid, vout: 0 }, vec![(id, 1000)]),
  //       (OutPoint { txid, vout: 1 }, vec![(id, 1000)]),
  //       (OutPoint { txid, vout: 2 }, vec![(id, 1000)]),
  //       (OutPoint { txid, vout: 3 }, vec![(id, 1000)]),
  //     ],
  //   );
  // }
  //
  // #[test]
  // fn split_in_enshrining_with_amount_with_preceding_edict() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![
  //         Transfer {
  //           id: RelicId::default(),
  //           amount: u128::MAX - 3000,
  //           output: 0,
  //         },
  //         Transfer {
  //           id: RelicId::default(),
  //           amount: 1000,
  //           output: 5,
  //         },
  //       ],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     4,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [
  //       (OutPoint { txid, vout: 0 }, vec![(id, u128::MAX - 2000)]),
  //       (OutPoint { txid, vout: 1 }, vec![(id, 1000)]),
  //       (OutPoint { txid, vout: 2 }, vec![(id, 1000)]),
  //     ],
  //   );
  // }
  //
  // #[test]
  // fn split_in_enshrining_with_amount_with_following_edict() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![
  //         Transfer {
  //           id: RelicId::default(),
  //           amount: 1000,
  //           output: 5,
  //         },
  //         Transfer {
  //           id: RelicId::default(),
  //           amount: u128::MAX,
  //           output: 0,
  //         },
  //       ],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     4,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [
  //       (
  //         OutPoint { txid, vout: 0 },
  //         vec![(id, u128::MAX - 4000 + 1000)],
  //       ),
  //       (OutPoint { txid, vout: 1 }, vec![(id, 1000)]),
  //       (OutPoint { txid, vout: 2 }, vec![(id, 1000)]),
  //       (OutPoint { txid, vout: 3 }, vec![(id, 1000)]),
  //     ],
  //   );
  // }
  //
  // #[test]
  // fn split() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: u128::MAX,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(
  //       OutPoint {
  //         txid: txid0,
  //         vout: 0,
  //       },
  //       vec![(id, u128::MAX)],
  //     )],
  //   );
  //
  //   let txid1 = context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(id.block.try_into().unwrap(), 1, 0, Witness::new())],
  //     outputs: 2,
  //     op_return: Some(
  //       Keepsake {
  //         transfers: vec![Transfer {
  //           id,
  //           amount: 0,
  //           output: 3,
  //         }],
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [
  //       (
  //         OutPoint {
  //           txid: txid1,
  //           vout: 0,
  //         },
  //         vec![(id, u128::MAX / 2 + 1)],
  //       ),
  //       (
  //         OutPoint {
  //           txid: txid1,
  //           vout: 1,
  //         },
  //         vec![(id, u128::MAX / 2)],
  //       ),
  //     ],
  //   );
  // }
  //
  // #[test]
  // fn split_with_preceding_edict() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: u128::MAX,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(
  //       OutPoint {
  //         txid: txid0,
  //         vout: 0,
  //       },
  //       vec![(id, u128::MAX)],
  //     )],
  //   );
  //
  //   let txid1 = context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(id.block.try_into().unwrap(), 1, 0, Witness::new())],
  //     outputs: 2,
  //     op_return: Some(
  //       Keepsake {
  //         transfers: vec![
  //           Transfer {
  //             id,
  //             amount: 1000,
  //             output: 0,
  //           },
  //           Transfer {
  //             id,
  //             amount: 0,
  //             output: 3,
  //           },
  //         ],
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [
  //       (
  //         OutPoint {
  //           txid: txid1,
  //           vout: 0,
  //         },
  //         vec![(id, 1000 + (u128::MAX - 1000) / 2 + 1)],
  //       ),
  //       (
  //         OutPoint {
  //           txid: txid1,
  //           vout: 1,
  //         },
  //         vec![(id, (u128::MAX - 1000) / 2)],
  //       ),
  //     ],
  //   );
  // }
  //
  // #[test]
  // fn split_with_following_edict() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: u128::MAX,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(
  //       OutPoint {
  //         txid: txid0,
  //         vout: 0,
  //       },
  //       vec![(id, u128::MAX)],
  //     )],
  //   );
  //
  //   let txid1 = context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(id.block.try_into().unwrap(), 1, 0, Witness::new())],
  //     outputs: 2,
  //     op_return: Some(
  //       Keepsake {
  //         transfers: vec![
  //           Transfer {
  //             id,
  //             amount: 0,
  //             output: 3,
  //           },
  //           Transfer {
  //             id,
  //             amount: 1000,
  //             output: 1,
  //           },
  //         ],
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [
  //       (
  //         OutPoint {
  //           txid: txid1,
  //           vout: 0,
  //         },
  //         vec![(id, u128::MAX / 2 + 1)],
  //       ),
  //       (
  //         OutPoint {
  //           txid: txid1,
  //           vout: 1,
  //         },
  //         vec![(id, u128::MAX / 2)],
  //       ),
  //     ],
  //   );
  // }
  //
  // #[test]
  // fn split_with_amount() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: u128::MAX,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(
  //       OutPoint {
  //         txid: txid0,
  //         vout: 0,
  //       },
  //       vec![(id, u128::MAX)],
  //     )],
  //   );
  //
  //   let txid1 = context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(id.block.try_into().unwrap(), 1, 0, Witness::new())],
  //     outputs: 2,
  //     op_return: Some(
  //       Keepsake {
  //         transfers: vec![Transfer {
  //           id,
  //           amount: 1000,
  //           output: 3,
  //         }],
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [
  //       (
  //         OutPoint {
  //           txid: txid1,
  //           vout: 0,
  //         },
  //         vec![(id, u128::MAX - 1000)],
  //       ),
  //       (
  //         OutPoint {
  //           txid: txid1,
  //           vout: 1,
  //         },
  //         vec![(id, 1000)],
  //       ),
  //     ],
  //   );
  // }
  //
  // #[test]
  // fn split_with_amount_with_preceding_edict() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: u128::MAX,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(
  //       OutPoint {
  //         txid: txid0,
  //         vout: 0,
  //       },
  //       vec![(id, u128::MAX)],
  //     )],
  //   );
  //
  //   let txid1 = context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(id.block.try_into().unwrap(), 1, 0, Witness::new())],
  //     outputs: 4,
  //     op_return: Some(
  //       Keepsake {
  //         transfers: vec![
  //           Transfer {
  //             id,
  //             amount: u128::MAX - 2000,
  //             output: 0,
  //           },
  //           Transfer {
  //             id,
  //             amount: 1000,
  //             output: 5,
  //           },
  //         ],
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [
  //       (
  //         OutPoint {
  //           txid: txid1,
  //           vout: 0,
  //         },
  //         vec![(id, u128::MAX - 2000 + 1000)],
  //       ),
  //       (
  //         OutPoint {
  //           txid: txid1,
  //           vout: 1,
  //         },
  //         vec![(id, 1000)],
  //       ),
  //     ],
  //   );
  // }
  //
  // #[test]
  // fn split_with_amount_with_following_edict() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: u128::MAX,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(
  //       OutPoint {
  //         txid: txid0,
  //         vout: 0,
  //       },
  //       vec![(id, u128::MAX)],
  //     )],
  //   );
  //
  //   let txid1 = context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(id.block.try_into().unwrap(), 1, 0, Witness::new())],
  //     outputs: 4,
  //     op_return: Some(
  //       Keepsake {
  //         transfers: vec![
  //           Transfer {
  //             id,
  //             amount: 1000,
  //             output: 5,
  //           },
  //           Transfer {
  //             id,
  //             amount: u128::MAX,
  //             output: 0,
  //           },
  //         ],
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [
  //       (
  //         OutPoint {
  //           txid: txid1,
  //           vout: 0,
  //         },
  //         vec![(id, u128::MAX - 4000 + 1000)],
  //       ),
  //       (
  //         OutPoint {
  //           txid: txid1,
  //           vout: 1,
  //         },
  //         vec![(id, 1000)],
  //       ),
  //       (
  //         OutPoint {
  //           txid: txid1,
  //           vout: 2,
  //         },
  //         vec![(id, 1000)],
  //       ),
  //       (
  //         OutPoint {
  //           txid: txid1,
  //           vout: 3,
  //         },
  //         vec![(id, 1000)],
  //       ),
  //     ],
  //   );
  // }
  //
  // #[test]
  // fn enshrining_may_specify_symbol() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: u128::MAX,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         symbol: Some('$'),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         symbol: Some('$'),
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(OutPoint { txid, vout: 0 }, vec![(id, u128::MAX)])],
  //   );
  // }
  //
  // #[test]
  // fn allocate_all_remaining_relics_in_enshrining() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: 0,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(OutPoint { txid, vout: 0 }, vec![(id, u128::MAX)])],
  //   );
  // }
  //
  // #[test]
  // fn allocate_all_remaining_relics_in_inputs() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: u128::MAX,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(
  //       OutPoint {
  //         txid: txid0,
  //         vout: 0,
  //       },
  //       vec![(id, u128::MAX)],
  //     )],
  //   );
  //
  //   let txid1 = context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(id.block.try_into().unwrap(), 1, 0, Witness::new())],
  //     outputs: 2,
  //     op_return: Some(
  //       Keepsake {
  //         transfers: vec![Transfer {
  //           id,
  //           amount: 0,
  //           output: 1,
  //         }],
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(
  //       OutPoint {
  //         txid: txid1,
  //         vout: 1,
  //       },
  //       vec![(id, u128::MAX)],
  //     )],
  //   );
  // }
  //
  // #[test]
  // fn relic_can_be_minted_without_edict() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id) = context.enshrine(
  //     Keepsake {
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           cap: Some(100),
  //           ..default()
  //         }),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         timestamp: id.block,
  //         mints: 0,
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           cap: Some(100),
  //           ..default()
  //         }),
  //         ..default()
  //       },
  //     )],
  //     [],
  //   );
  //
  //   let txid1 = context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(2, 0, 0, Witness::new())],
  //     op_return: Some(
  //       Keepsake {
  //         mint: Some(id),
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           cap: Some(100),
  //           ..default()
  //         }),
  //         mints: 1,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: 0,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(
  //       OutPoint {
  //         txid: txid1,
  //         vout: 0,
  //       },
  //       vec![(id, 1000)],
  //     )],
  //   );
  // }
  //
  // #[test]
  // fn relic_cannot_be_minted_less_than_limit_amount() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id) = context.enshrine(
  //     Keepsake {
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           cap: Some(100),
  //           ..default()
  //         }),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         timestamp: id.block,
  //         mints: 0,
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           cap: Some(100),
  //           ..default()
  //         }),
  //         ..default()
  //       },
  //     )],
  //     [],
  //   );
  //
  //   let txid1 = context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(2, 0, 0, Witness::new())],
  //     outputs: 2,
  //     op_return: Some(
  //       Keepsake {
  //         mint: Some(id),
  //         transfers: vec![Transfer {
  //           id,
  //           amount: 111,
  //           output: 0,
  //         }],
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           cap: Some(100),
  //           ..default()
  //         }),
  //         mints: 1,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: 0,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(
  //       OutPoint {
  //         txid: txid1,
  //         vout: 0,
  //       },
  //       vec![(id, 1000)],
  //     )],
  //   );
  // }
  //
  // #[test]
  // fn enshrining_with_amount_can_be_minted() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id) = context.enshrine(
  //     Keepsake {
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         terms: Some(Terms {
  //           cap: Some(100),
  //           amount: Some(1000),
  //           ..default()
  //         }),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         timestamp: id.block,
  //         premine: 0,
  //         mints: 0,
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           cap: Some(100),
  //           ..default()
  //         }),
  //         ..default()
  //       },
  //     )],
  //     [],
  //   );
  //
  //   let txid1 = context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(3, 0, 0, Witness::new())],
  //     op_return: Some(
  //       Keepsake {
  //         transfers: vec![Transfer {
  //           id,
  //           amount: 1000,
  //           output: 0,
  //         }],
  //         mint: Some(id),
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           cap: Some(100),
  //           ..default()
  //         }),
  //         mints: 1,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: 0,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(
  //       OutPoint {
  //         txid: txid1,
  //         vout: 0,
  //       },
  //       vec![(id, 1000)],
  //     )],
  //   );
  //
  //   // claim the relic
  //   let txid2 = context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(4, 0, 0, Witness::new())],
  //     op_return: Some(
  //       Keepsake {
  //         transfers: vec![Transfer {
  //           id,
  //           amount: 1000,
  //           output: 0,
  //         }],
  //         mint: Some(id),
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           cap: Some(100),
  //           ..default()
  //         }),
  //         mints: 2,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: 0,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [
  //       (
  //         OutPoint {
  //           txid: txid2,
  //           vout: 0,
  //         },
  //         vec![(id, 1000)],
  //       ),
  //       (
  //         OutPoint {
  //           txid: txid1,
  //           vout: 0,
  //         },
  //         vec![(id, 1000)],
  //       ),
  //     ],
  //   );
  //
  //   // claim the relic in a burn keepsake
  //   context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(5, 0, 0, Witness::new())],
  //     op_return: Some(
  //       Keepsake {
  //         pointer: Some(10),
  //         mint: Some(id),
  //         transfers: vec![Transfer {
  //           id,
  //           amount: 1000,
  //           output: 0,
  //         }],
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         burned: 1000,
  //         enshrining: txid0,
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           cap: Some(100),
  //           ..default()
  //         }),
  //         mints: 3,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: 0,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [
  //       (
  //         OutPoint {
  //           txid: txid2,
  //           vout: 0,
  //         },
  //         vec![(id, 1000)],
  //       ),
  //       (
  //         OutPoint {
  //           txid: txid1,
  //           vout: 0,
  //         },
  //         vec![(id, 1000)],
  //       ),
  //     ],
  //   );
  // }
  //
  // #[test]
  // fn open_mints_can_be_limited_with_offset_end() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id) = context.enshrine(
  //     Keepsake {
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           cap: Some(100),
  //           offset: (None, Some(2)),
  //           ..default()
  //         }),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   let mut entry = RelicEntry {
  //     block: id.block,
  //     enshrining: txid0,
  //     spaced_relic: SpacedRelic {
  //       relic: Relic(RELIC),
  //       spacers: 0,
  //     },
  //     terms: Some(Terms {
  //       amount: Some(1000),
  //       offset: (None, Some(2)),
  //       cap: Some(100),
  //       ..default()
  //     }),
  //     timestamp: id.block,
  //     ..default()
  //   };
  //
  //   context.assert_relics([(id, entry)], []);
  //
  //   let txid1 = context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(2, 0, 0, Witness::new())],
  //     op_return: Some(
  //       Keepsake {
  //         mint: Some(id),
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   entry.mints += 1;
  //
  //   context.assert_relics(
  //     [(id, entry)],
  //     [(
  //       OutPoint {
  //         txid: txid1,
  //         vout: 0,
  //       },
  //       vec![(id, 1000)],
  //     )],
  //   );
  //
  //   context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(3, 0, 0, Witness::new())],
  //     op_return: Some(
  //       Keepsake {
  //         mint: Some(id),
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [(id, entry)],
  //     [(
  //       OutPoint {
  //         txid: txid1,
  //         vout: 0,
  //       },
  //       vec![(id, 1000)],
  //     )],
  //   );
  // }
  //
  // #[test]
  // fn open_mints_can_be_limited_with_offset_start() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id) = context.enshrine(
  //     Keepsake {
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           cap: Some(100),
  //           offset: (Some(2), None),
  //           ..default()
  //         }),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   let mut entry = RelicEntry {
  //     block: id.block,
  //     enshrining: txid0,
  //     spaced_relic: SpacedRelic {
  //       relic: Relic(RELIC),
  //       spacers: 0,
  //     },
  //     terms: Some(Terms {
  //       amount: Some(1000),
  //       offset: (Some(2), None),
  //       cap: Some(100),
  //       ..default()
  //     }),
  //     timestamp: id.block,
  //     ..default()
  //   };
  //
  //   context.assert_relics([(id, entry)], []);
  //
  //   context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(2, 0, 0, Witness::new())],
  //     op_return: Some(
  //       Keepsake {
  //         mint: Some(id),
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics([(id, entry)], []);
  //
  //   let txid1 = context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(3, 0, 0, Witness::new())],
  //     op_return: Some(
  //       Keepsake {
  //         mint: Some(id),
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   entry.mints += 1;
  //
  //   context.assert_relics(
  //     [(id, entry)],
  //     [(
  //       OutPoint {
  //         txid: txid1,
  //         vout: 0,
  //       },
  //       vec![(id, 1000)],
  //     )],
  //   );
  // }
  //
  // #[test]
  // fn open_mints_can_be_limited_with_height_start() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id) = context.enshrine(
  //     Keepsake {
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           cap: Some(100),
  //           height: (Some(10), None),
  //           ..default()
  //         }),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   let mut entry = RelicEntry {
  //     block: id.block,
  //     enshrining: txid0,
  //     spaced_relic: SpacedRelic {
  //       relic: Relic(RELIC),
  //       spacers: 0,
  //     },
  //     terms: Some(Terms {
  //       amount: Some(1000),
  //       height: (Some(10), None),
  //       cap: Some(100),
  //       ..default()
  //     }),
  //     timestamp: id.block,
  //     ..default()
  //   };
  //
  //   context.assert_relics([(id, entry)], []);
  //
  //   context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(2, 0, 0, Witness::new())],
  //     op_return: Some(
  //       Keepsake {
  //         mint: Some(id),
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics([(id, entry)], []);
  //
  //   let txid1 = context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(3, 0, 0, Witness::new())],
  //     op_return: Some(
  //       Keepsake {
  //         mint: Some(id),
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   entry.mints += 1;
  //
  //   context.assert_relics(
  //     [(id, entry)],
  //     [(
  //       OutPoint {
  //         txid: txid1,
  //         vout: 0,
  //       },
  //       vec![(id, 1000)],
  //     )],
  //   );
  // }
  //
  // #[test]
  // fn open_mints_can_be_limited_with_height_end() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id) = context.enshrine(
  //     Keepsake {
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           cap: Some(100),
  //           height: (None, Some(10)),
  //           ..default()
  //         }),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   let mut entry = RelicEntry {
  //     block: id.block,
  //     enshrining: txid0,
  //     spaced_relic: SpacedRelic {
  //       relic: Relic(RELIC),
  //       spacers: 0,
  //     },
  //     terms: Some(Terms {
  //       amount: Some(1000),
  //       height: (None, Some(10)),
  //       cap: Some(100),
  //       ..default()
  //     }),
  //     timestamp: id.block,
  //     ..default()
  //   };
  //
  //   context.assert_relics([(id, entry)], []);
  //
  //   let txid1 = context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(2, 0, 0, Witness::new())],
  //     op_return: Some(
  //       Keepsake {
  //         mint: Some(id),
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   entry.mints += 1;
  //
  //   context.assert_relics(
  //     [(id, entry)],
  //     [(
  //       OutPoint {
  //         txid: txid1,
  //         vout: 0,
  //       },
  //       vec![(id, 1000)],
  //     )],
  //   );
  //
  //   context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(3, 0, 0, Witness::new())],
  //     op_return: Some(
  //       Keepsake {
  //         mint: Some(id),
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [(id, entry)],
  //     [(
  //       OutPoint {
  //         txid: txid1,
  //         vout: 0,
  //       },
  //       vec![(id, 1000)],
  //     )],
  //   );
  // }
  //
  // #[test]
  // fn open_mints_must_be_ended_with_enshrined_height_plus_offset_end() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id) = context.enshrine(
  //     Keepsake {
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           cap: Some(100),
  //           height: (None, Some(100)),
  //           offset: (None, Some(2)),
  //         }),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   let mut entry = RelicEntry {
  //     block: id.block,
  //     enshrining: txid0,
  //     spaced_relic: SpacedRelic {
  //       relic: Relic(RELIC),
  //       spacers: 0,
  //     },
  //     terms: Some(Terms {
  //       amount: Some(1000),
  //       height: (None, Some(100)),
  //       offset: (None, Some(2)),
  //       cap: Some(100),
  //     }),
  //     timestamp: id.block,
  //     ..default()
  //   };
  //
  //   context.assert_relics([(id, entry)], []);
  //
  //   let txid1 = context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(2, 0, 0, Witness::new())],
  //     op_return: Some(
  //       Keepsake {
  //         mint: Some(id),
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //   entry.mints += 1;
  //
  //   context.assert_relics(
  //     [(id, entry)],
  //     [(
  //       OutPoint {
  //         txid: txid1,
  //         vout: 0,
  //       },
  //       vec![(id, 1000)],
  //     )],
  //   );
  //
  //   context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(3, 0, 0, Witness::new())],
  //     op_return: Some(
  //       Keepsake {
  //         mint: Some(id),
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [(id, entry)],
  //     [(
  //       OutPoint {
  //         txid: txid1,
  //         vout: 0,
  //       },
  //       vec![(id, 1000)],
  //     )],
  //   );
  // }
  //
  // #[test]
  // fn open_mints_must_be_ended_with_height_end() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id) = context.enshrine(
  //     Keepsake {
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           cap: Some(100),
  //           height: (None, Some(10)),
  //           offset: (None, Some(100)),
  //         }),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   let mut entry = RelicEntry {
  //     block: id.block,
  //     enshrining: txid0,
  //     spaced_relic: SpacedRelic {
  //       relic: Relic(RELIC),
  //       spacers: 0,
  //     },
  //     terms: Some(Terms {
  //       amount: Some(1000),
  //       height: (None, Some(10)),
  //       offset: (None, Some(100)),
  //       cap: Some(100),
  //     }),
  //     timestamp: id.block,
  //     ..default()
  //   };
  //
  //   context.assert_relics([(id, entry)], []);
  //
  //   let txid1 = context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(2, 0, 0, Witness::new())],
  //     op_return: Some(
  //       Keepsake {
  //         mint: Some(id),
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //   entry.mints += 1;
  //
  //   context.assert_relics(
  //     [(id, entry)],
  //     [(
  //       OutPoint {
  //         txid: txid1,
  //         vout: 0,
  //       },
  //       vec![(id, 1000)],
  //     )],
  //   );
  //
  //   context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(3, 0, 0, Witness::new())],
  //     op_return: Some(
  //       Keepsake {
  //         mint: Some(id),
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [(id, entry)],
  //     [(
  //       OutPoint {
  //         txid: txid1,
  //         vout: 0,
  //       },
  //       vec![(id, 1000)],
  //     )],
  //   );
  // }
  //
  // #[test]
  // fn open_mints_must_be_started_with_height_start() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id) = context.enshrine(
  //     Keepsake {
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           cap: Some(100),
  //           height: (Some(11), None),
  //           offset: (Some(1), None),
  //         }),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   let mut entry0 = RelicEntry {
  //     block: id.block,
  //     enshrining: txid0,
  //     spaced_relic: SpacedRelic {
  //       relic: Relic(RELIC),
  //       spacers: 0,
  //     },
  //     terms: Some(Terms {
  //       amount: Some(1000),
  //       height: (Some(11), None),
  //       offset: (Some(1), None),
  //       cap: Some(100),
  //     }),
  //     timestamp: id.block,
  //     ..default()
  //   };
  //
  //   context.mine_blocks(1);
  //
  //   context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(2, 0, 0, Witness::new())],
  //     op_return: Some(
  //       Keepsake {
  //         mint: Some(id),
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics([(id, entry0)], []);
  //
  //   context.mine_blocks(1);
  //
  //   let txid1 = context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(3, 0, 0, Witness::new())],
  //     op_return: Some(
  //       Keepsake {
  //         mint: Some(id),
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   entry0.mints += 1;
  //
  //   context.assert_relics(
  //     [(id, entry0)],
  //     [(
  //       OutPoint {
  //         txid: txid1,
  //         vout: 0,
  //       },
  //       vec![(id, 1000)],
  //     )],
  //   );
  // }
  //
  // #[test]
  // fn open_mints_must_be_started_with_enshrined_height_plus_offset_start() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id) = context.enshrine(
  //     Keepsake {
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           cap: Some(100),
  //           height: (Some(9), None),
  //           offset: (Some(3), None),
  //         }),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   let mut entry = RelicEntry {
  //     block: id.block,
  //     enshrining: txid0,
  //     spaced_relic: SpacedRelic {
  //       relic: Relic(RELIC),
  //       spacers: 0,
  //     },
  //     terms: Some(Terms {
  //       amount: Some(1000),
  //       height: (Some(9), None),
  //       offset: (Some(3), None),
  //       cap: Some(100),
  //     }),
  //     timestamp: id.block,
  //     ..default()
  //   };
  //
  //   context.mine_blocks(1);
  //
  //   context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(2, 0, 0, Witness::new())],
  //     op_return: Some(
  //       Keepsake {
  //         mint: Some(id),
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics([(id, entry)], []);
  //
  //   context.mine_blocks(1);
  //
  //   let txid1 = context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(3, 0, 0, Witness::new())],
  //     op_return: Some(
  //       Keepsake {
  //         mint: Some(id),
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   entry.mints += 1;
  //
  //   context.assert_relics(
  //     [(id, entry)],
  //     [(
  //       OutPoint {
  //         txid: txid1,
  //         vout: 0,
  //       },
  //       vec![(id, 1000)],
  //     )],
  //   );
  // }
  //
  // #[test]
  // fn open_mints_with_offset_end_zero_can_be_premined() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid, id) = context.enshrine(
  //     Keepsake {
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: 1111,
  //         output: 0,
  //       }],
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(1111),
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           offset: (None, Some(0)),
  //           ..default()
  //         }),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           offset: (None, Some(0)),
  //           ..default()
  //         }),
  //         timestamp: id.block,
  //         premine: 1111,
  //         ..default()
  //       },
  //     )],
  //     [(OutPoint { txid, vout: 0 }, vec![(id, 1111)])],
  //   );
  //
  //   context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(2, 0, 0, Witness::new())],
  //     outputs: 2,
  //     op_return: Some(
  //       Keepsake {
  //         mint: Some(id),
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         timestamp: id.block,
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           offset: (None, Some(0)),
  //           ..default()
  //         }),
  //         premine: 1111,
  //         ..default()
  //       },
  //     )],
  //     [(OutPoint { txid, vout: 0 }, vec![(id, 1111)])],
  //   );
  // }
  //
  // #[test]
  // fn open_mints_can_be_limited_to_cap() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id) = context.enshrine(
  //     Keepsake {
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           cap: Some(2),
  //           ..default()
  //         }),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         timestamp: id.block,
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           cap: Some(2),
  //           ..default()
  //         }),
  //         ..default()
  //       },
  //     )],
  //     [],
  //   );
  //
  //   let txid1 = context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(2, 0, 0, Witness::new())],
  //     op_return: Some(
  //       Keepsake {
  //         transfers: vec![Transfer {
  //           id,
  //           amount: 1000,
  //           output: 0,
  //         }],
  //         mint: Some(id),
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         timestamp: id.block,
  //         mints: 1,
  //         enshrining: txid0,
  //         terms: Some(Terms {
  //           cap: Some(2),
  //           amount: Some(1000),
  //           ..default()
  //         }),
  //         ..default()
  //       },
  //     )],
  //     [(
  //       OutPoint {
  //         txid: txid1,
  //         vout: 0,
  //       },
  //       vec![(id, 1000)],
  //     )],
  //   );
  //
  //   let txid2 = context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(3, 0, 0, Witness::new())],
  //     op_return: Some(
  //       Keepsake {
  //         transfers: vec![Transfer {
  //           id,
  //           amount: 1000,
  //           output: 0,
  //         }],
  //         mint: Some(id),
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         timestamp: id.block,
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           cap: Some(2),
  //           ..default()
  //         }),
  //         mints: 2,
  //         ..default()
  //       },
  //     )],
  //     [
  //       (
  //         OutPoint {
  //           txid: txid1,
  //           vout: 0,
  //         },
  //         vec![(id, 1000)],
  //       ),
  //       (
  //         OutPoint {
  //           txid: txid2,
  //           vout: 0,
  //         },
  //         vec![(id, 1000)],
  //       ),
  //     ],
  //   );
  //
  //   context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(4, 0, 0, Witness::new())],
  //     op_return: Some(
  //       Keepsake {
  //         transfers: vec![Transfer {
  //           id,
  //           amount: 1000,
  //           output: 0,
  //         }],
  //         mint: Some(id),
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         timestamp: id.block,
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           cap: Some(2),
  //           ..default()
  //         }),
  //         mints: 2,
  //         ..default()
  //       },
  //     )],
  //     [
  //       (
  //         OutPoint {
  //           txid: txid1,
  //           vout: 0,
  //         },
  //         vec![(id, 1000)],
  //       ),
  //       (
  //         OutPoint {
  //           txid: txid2,
  //           vout: 0,
  //         },
  //         vec![(id, 1000)],
  //       ),
  //     ],
  //   );
  // }
  //
  // #[test]
  // fn open_mints_without_a_cap_are_unmintable() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id) = context.enshrine(
  //     Keepsake {
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           offset: (None, Some(2)),
  //           ..default()
  //         }),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         timestamp: id.block,
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           offset: (None, Some(2)),
  //           ..default()
  //         }),
  //         ..default()
  //       },
  //     )],
  //     [],
  //   );
  //
  //   context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(2, 0, 0, Witness::new())],
  //     op_return: Some(
  //       Keepsake {
  //         transfers: vec![Transfer {
  //           id,
  //           amount: 1000,
  //           output: 0,
  //         }],
  //         mint: Some(id),
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         timestamp: id.block,
  //         mints: 0,
  //         enshrining: txid0,
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           offset: (None, Some(2)),
  //           ..default()
  //         }),
  //         ..default()
  //       },
  //     )],
  //     [],
  //   );
  // }
  //
  // #[test]
  // fn open_mint_claims_can_use_split() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id) = context.enshrine(
  //     Keepsake {
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           cap: Some(100),
  //           ..default()
  //         }),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           cap: Some(100),
  //           ..default()
  //         }),
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [],
  //   );
  //
  //   let txid1 = context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(3, 0, 0, Witness::new())],
  //     outputs: 2,
  //     op_return: Some(
  //       Keepsake {
  //         transfers: vec![Transfer {
  //           id,
  //           amount: 0,
  //           output: 3,
  //         }],
  //         mint: Some(id),
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         timestamp: id.block,
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           cap: Some(100),
  //           ..default()
  //         }),
  //         mints: 1,
  //         ..default()
  //       },
  //     )],
  //     [
  //       (
  //         OutPoint {
  //           txid: txid1,
  //           vout: 0,
  //         },
  //         vec![(id, 500)],
  //       ),
  //       (
  //         OutPoint {
  //           txid: txid1,
  //           vout: 1,
  //         },
  //         vec![(id, 500)],
  //       ),
  //     ],
  //   );
  // }
  //
  // #[test]
  // fn relics_can_be_enshrined_and_premined_in_the_same_transaction() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid, id) = context.enshrine(
  //     Keepsake {
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(2000),
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           ..default()
  //         }),
  //         ..default()
  //       }),
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: 2000,
  //         output: 0,
  //       }],
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           ..default()
  //         }),
  //         timestamp: id.block,
  //         premine: 2000,
  //         ..default()
  //       },
  //     )],
  //     [(OutPoint { txid, vout: 0 }, vec![(id, 2000)])],
  //   );
  // }
  //
  // #[test]
  // fn omitted_transfers_defaults_to_mint_amount() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid, id) = context.enshrine(
  //     Keepsake {
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         terms: Some(Terms {
  //           offset: (None, Some(1)),
  //           ..default()
  //         }),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         terms: Some(Terms {
  //           amount: None,
  //           offset: (None, Some(1)),
  //           ..default()
  //         }),
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [],
  //   );
  // }
  //
  // #[test]
  // fn premines_can_claim_over_mint_amount() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid, id) = context.enshrine(
  //     Keepsake {
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(2000),
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           cap: Some(1),
  //           ..default()
  //         }),
  //         ..default()
  //       }),
  //       transfers: vec![Transfer {
  //         id: RelicId::default(),
  //         amount: 2000,
  //         output: 0,
  //       }],
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           cap: Some(1),
  //           ..default()
  //         }),
  //         timestamp: id.block,
  //         premine: 2000,
  //         mints: 0,
  //         ..default()
  //       },
  //     )],
  //     [(OutPoint { txid, vout: 0 }, vec![(id, 2000)])],
  //   );
  // }
  //
  // #[test]
  // fn transactions_cannot_claim_more_than_mint_amount() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id) = context.enshrine(
  //     Keepsake {
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           cap: Some(100),
  //           ..default()
  //         }),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   let txid1 = context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(2, 0, 0, Witness::new())],
  //     op_return: Some(
  //       Keepsake {
  //         transfers: vec![Transfer {
  //           id,
  //           amount: 2000,
  //           output: 0,
  //         }],
  //         mint: Some(id),
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           cap: Some(100),
  //           ..default()
  //         }),
  //         timestamp: id.block,
  //         mints: 1,
  //         ..default()
  //       },
  //     )],
  //     [(
  //       OutPoint {
  //         txid: txid1,
  //         vout: 0,
  //       },
  //       vec![(id, 1000)],
  //     )],
  //   );
  // }
  //
  // #[test]
  // fn multiple_transfers_in_one_transaction_may_claim_open_mint() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id) = context.enshrine(
  //     Keepsake {
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           cap: Some(100),
  //           ..default()
  //         }),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           cap: Some(100),
  //           ..default()
  //         }),
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [],
  //   );
  //
  //   let txid1 = context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(2, 0, 0, Witness::new())],
  //     op_return: Some(
  //       Keepsake {
  //         transfers: vec![
  //           Transfer {
  //             id,
  //             amount: 500,
  //             output: 0,
  //           },
  //           Transfer {
  //             id,
  //             amount: 500,
  //             output: 0,
  //           },
  //           Transfer {
  //             id,
  //             amount: 500,
  //             output: 0,
  //           },
  //         ],
  //         mint: Some(id),
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         terms: Some(Terms {
  //           amount: Some(1000),
  //           cap: Some(100),
  //           ..default()
  //         }),
  //         timestamp: id.block,
  //         mints: 1,
  //         ..default()
  //       },
  //     )],
  //     [(
  //       OutPoint {
  //         txid: txid1,
  //         vout: 0,
  //       },
  //       vec![(id, 1000)],
  //     )],
  //   );
  // }
  //
  // #[test]
  // fn commits_are_not_valid_in_non_taproot_witnesses() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let block_count = context.index.block_count().unwrap().into_usize();
  //
  //   context.mine_blocks(1);
  //
  //   context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(block_count, 0, 0, Witness::new())],
  //     p2tr: false,
  //     ..default()
  //   });
  //
  //   context.mine_blocks(Keepsake::COMMIT_CONFIRMATIONS.into());
  //
  //   let mut witness = Witness::new();
  //
  //   let keepsake = Keepsake {
  //     enshrining: Some(Enshrining {
  //       relic: Some(Relic(RELIC)),
  //       terms: Some(Terms {
  //         amount: Some(1000),
  //         ..default()
  //       }),
  //       ..default()
  //     }),
  //     ..default()
  //   };
  //
  //   let tapscript = script::Builder::new()
  //     .push_slice::<&PushBytes>(
  //       keepsake
  //         .enshrining
  //         .unwrap()
  //         .relic
  //         .unwrap()
  //         .commitment()
  //         .as_slice()
  //         .try_into()
  //         .unwrap(),
  //     )
  //     .into_script();
  //
  //   witness.push(tapscript);
  //
  //   witness.push([]);
  //
  //   context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(block_count + 1, 1, 0, witness)],
  //     op_return: Some(keepsake.encipher()),
  //     outputs: 1,
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics([], []);
  // }
  //
  // #[test]
  // fn immature_commits_are_not_valid() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let block_count = context.index.block_count().unwrap().into_usize();
  //
  //   context.mine_blocks(1);
  //
  //   context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(block_count, 0, 0, Witness::new())],
  //     p2tr: true,
  //     ..default()
  //   });
  //
  //   context.mine_blocks((Keepsake::COMMIT_CONFIRMATIONS - 2).into());
  //
  //   let mut witness = Witness::new();
  //
  //   let keepsake = Keepsake {
  //     enshrining: Some(Enshrining {
  //       relic: Some(Relic(RELIC)),
  //       terms: Some(Terms {
  //         amount: Some(1000),
  //         ..default()
  //       }),
  //       ..default()
  //     }),
  //     ..default()
  //   };
  //
  //   let tapscript = script::Builder::new()
  //     .push_slice::<&PushBytes>(
  //       keepsake
  //         .enshrining
  //         .unwrap()
  //         .relic
  //         .unwrap()
  //         .commitment()
  //         .as_slice()
  //         .try_into()
  //         .unwrap(),
  //     )
  //     .into_script();
  //
  //   witness.push(tapscript);
  //
  //   witness.push([]);
  //
  //   context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(block_count + 1, 1, 0, witness)],
  //     op_return: Some(keepsake.encipher()),
  //     outputs: 1,
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics([], []);
  // }
  //
  // #[test]
  // fn immature_commits_are_not_valid_even_when_bitcoind_is_ahead() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let block_count = context.index.block_count().unwrap().into_usize();
  //
  //   context.mine_blocks_with_update(1, false);
  //
  //   context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(block_count, 0, 0, Witness::new())],
  //     p2tr: true,
  //     ..default()
  //   });
  //
  //   context.mine_blocks_with_update((Keepsake::COMMIT_CONFIRMATIONS - 2).into(), false);
  //
  //   let mut witness = Witness::new();
  //
  //   let keepsake = Keepsake {
  //     enshrining: Some(Enshrining {
  //       relic: Some(Relic(RELIC)),
  //       terms: Some(Terms {
  //         amount: Some(1000),
  //         ..default()
  //       }),
  //       ..default()
  //     }),
  //     ..default()
  //   };
  //
  //   let tapscript = script::Builder::new()
  //     .push_slice::<&PushBytes>(
  //       keepsake
  //         .enshrining
  //         .unwrap()
  //         .relic
  //         .unwrap()
  //         .commitment()
  //         .as_slice()
  //         .try_into()
  //         .unwrap(),
  //     )
  //     .into_script();
  //
  //   witness.push(tapscript);
  //
  //   witness.push([]);
  //
  //   context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(block_count + 1, 1, 0, witness)],
  //     op_return: Some(keepsake.encipher()),
  //     outputs: 1,
  //     ..default()
  //   });
  //
  //   context.mine_blocks_with_update(2, false);
  //
  //   context.mine_blocks_with_update(1, true);
  //
  //   context.assert_relics([], []);
  // }
  //
  // #[test]
  // fn enshrining_are_not_valid_without_commitment() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let block_count = context.index.block_count().unwrap().into_usize();
  //
  //   context.mine_blocks(1);
  //
  //   context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(block_count, 0, 0, Witness::new())],
  //     p2tr: true,
  //     ..default()
  //   });
  //
  //   context.mine_blocks(Keepsake::COMMIT_CONFIRMATIONS.into());
  //
  //   let mut witness = Witness::new();
  //
  //   let keepsake = Keepsake {
  //     enshrining: Some(Enshrining {
  //       relic: Some(Relic(RELIC)),
  //       terms: Some(Terms {
  //         amount: Some(1000),
  //         ..default()
  //       }),
  //       ..default()
  //     }),
  //     ..default()
  //   };
  //
  //   let tapscript = script::Builder::new()
  //     .push_slice::<&PushBytes>([].as_slice().try_into().unwrap())
  //     .into_script();
  //
  //   witness.push(tapscript);
  //
  //   witness.push([]);
  //
  //   context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(block_count + 1, 1, 0, witness)],
  //     op_return: Some(keepsake.encipher()),
  //     outputs: 1,
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics([], []);
  // }
  //
  // #[test]
  // fn tx_commits_to_relic_ignores_invalid_script() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   context.mine_blocks(1);
  //
  //   let keepsake = Keepsake {
  //     enshrining: Some(Enshrining {
  //       relic: Some(Relic(RELIC)),
  //       terms: Some(Terms {
  //         amount: Some(1000),
  //         ..default()
  //       }),
  //       ..default()
  //     }),
  //     ..default()
  //   };
  //
  //   let mut witness = Witness::new();
  //
  //   witness.push([opcodes::all::OP_PUSHDATA4.to_u8()]);
  //   witness.push([]);
  //
  //   context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(1, 0, 0, witness)],
  //     op_return: Some(keepsake.encipher()),
  //     outputs: 1,
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics([], []);
  // }
  //
  // #[test]
  // fn edict_with_amount_zero_and_no_destinations_is_ignored() {
  //   let context = Context::builder().arg("--index-relics").build();
  //
  //   let (txid0, id) = context.enshrine(
  //     Keepsake {
  //       enshrining: Some(Enshrining {
  //         relic: Some(Relic(RELIC)),
  //         premine: Some(u128::MAX),
  //         ..default()
  //       }),
  //       ..default()
  //     },
  //     1,
  //   );
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [(
  //       OutPoint {
  //         txid: txid0,
  //         vout: 0,
  //       },
  //       vec![(id, u128::MAX)],
  //     )],
  //   );
  //
  //   context.core.broadcast_tx(TransactionTemplate {
  //     inputs: &[(id.block.try_into().unwrap(), 1, 0, Witness::new())],
  //     op_return: Some(
  //       Keepsake {
  //         transfers: vec![Transfer {
  //           id,
  //           amount: 0,
  //           output: 1,
  //         }],
  //         ..default()
  //       }
  //         .encipher(),
  //     ),
  //     outputs: 0,
  //     ..default()
  //   });
  //
  //   context.mine_blocks(1);
  //
  //   context.assert_relics(
  //     [(
  //       id,
  //       RelicEntry {
  //         block: id.block,
  //         enshrining: txid0,
  //         spaced_relic: SpacedRelic {
  //           relic: Relic(RELIC),
  //           spacers: 0,
  //         },
  //         premine: u128::MAX,
  //         burned: u128::MAX,
  //         timestamp: id.block,
  //         ..default()
  //       },
  //     )],
  //     [],
  //   );
  // }
  //
  // #[test]
  // fn genesis_relic() {
  //   assert_eq!(
  //     Chain::Mainnet.first_relic_height(),
  //     SUBSIDY_HALVING_INTERVAL * 4,
  //   );
  //
  //   Context::builder()
  //     .chain(Chain::Mainnet)
  //     .arg("--index-relics")
  //     .build()
  //     .assert_relics(
  //       [(
  //         RelicId { block: 1, tx: 0 },
  //         RelicEntry {
  //           block: 1,
  //           burned: 0,
  //           divisibility: 0,
  //           enshrining: txid::all_zeros(),
  //           mints: 0,
  //           number: 0,
  //           premine: 0,
  //           spaced_relic: SpacedRelic {
  //             relic: Relic(2055900680524219742),
  //             spacers: 128,
  //           },
  //           symbol: Some('\u{29C9}'),
  //           terms: Some(Terms {
  //             amount: Some(1),
  //             cap: Some(u128::MAX),
  //             height: (
  //               Some((SUBSIDY_HALVING_INTERVAL * 4).into()),
  //               Some((SUBSIDY_HALVING_INTERVAL * 5).into()),
  //             ),
  //             offset: (None, None),
  //           }),
  //           timestamp: 0,
  //           turbo: true,
  //         },
  //       )],
  //       [],
  //     );
  // }
}
