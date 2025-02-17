use {
  self::{
    entry::{
      BlockHashValue, Entry, InscriptionEntry, InscriptionEntryValue, InscriptionIdValue,
      OutPointMapValue, OutPointValue, SatPointValue, SatRange, TxidValue,
    },
    reorg::*,
    updater::Updater,
  },
  super::*,
  crate::{
    charm::Charm,
    index::{
      chest_entry::ChestEntryValue,
      event::Event,
      manfest_entry::ManifestEntryValue,
      relics_entry::{
        RelicEntry, RelicEntryValue, RelicIdValue, RelicOwner, RelicOwnerValue, RelicState,
        SpacedRelicValue,
      },
      syndicate_entry::{SyndicateEntry, SyndicateEntryValue, SyndicateIdValue},
    },
    inscription::ParsedInscription,
    relics::{
      Enshrining, MintTerms, Relic, RelicError, RelicId, SpacedRelic, SyndicateId, RELIC_ID,
      RELIC_NAME,
    },
    sat::Sat,
    sat_point::SatPoint,
    templates::BlockHashAndConfirmations,
    wallet::Wallet,
  },
  bitcoin::BlockHeader,
  bitcoincore_rpc::{json::GetBlockHeaderResult, Auth, Client},
  chrono::SubsecRound,
  indicatif::{ProgressBar, ProgressStyle},
  log::log_enabled,
  redb::ReadableTableMetadata,
  redb::{
    Database, DatabaseError, MultimapTable, MultimapTableDefinition, ReadableMultimapTable,
    ReadableTable, StorageError, Table, TableDefinition, WriteTransaction,
  },
  std::collections::HashMap,
  std::io::Cursor,
  std::sync::atomic::{self, AtomicBool},
  url::Url,
};
use crate::index::manfest_entry::ManifestedMinterValue;

mod chest_entry;
pub(crate) mod entry;
pub(crate) mod event;
mod fetcher;
mod lot;
mod manfest_entry;
pub(crate) mod relics_entry;
mod reorg;
mod rtx;
pub(crate) mod syndicate_entry;
pub(crate) mod testing;
mod updater;

const SCHEMA_VERSION: u64 = 6;

macro_rules! define_table {
  ($name:ident, $key:ty, $value:ty) => {
    const $name: TableDefinition<$key, $value> = TableDefinition::new(stringify!($name));
  };
}

macro_rules! define_multimap_table {
  ($name:ident, $key:ty, $value:ty) => {
    const $name: MultimapTableDefinition<$key, $value> =
      MultimapTableDefinition::new(stringify!($name));
  };
}

define_table! { HEIGHT_TO_BLOCK_HASH, u32, &BlockHashValue }
define_table! { INSCRIPTION_ID_TO_SATPOINT, &InscriptionIdValue, &SatPointValue }
define_table! { INSCRIPTION_ID_TO_TXIDS, &InscriptionIdValue, &[u8] }
define_table! { INSCRIPTION_TXID_TO_TX, &[u8], &[u8] }
define_table! { PARTIAL_TXID_TO_INSCRIPTION_TXIDS, &[u8], &[u8] }
define_table! { OUTPOINT_TO_SAT_RANGES, &OutPointValue, &[u8] }
define_table! { OUTPOINT_TO_VALUE, &OutPointValue, u64}
define_multimap_table! { ADDRESS_TO_OUTPOINT, &[u8], &OutPointValue}
define_table! { SATPOINT_TO_INSCRIPTION_ID, &SatPointValue, &InscriptionIdValue }
define_table! { SAT_TO_SATPOINT, u64, &SatPointValue }
define_table! { STATISTIC_TO_COUNT, u64, u64 }
define_table! { TRANSACTION_ID_TO_TRANSACTION, &TxidValue, &[u8] }
define_table! { WRITE_TRANSACTION_STARTING_BLOCK_COUNT_TO_TIMESTAMP, u32, u128 }
define_table! { RELIC_TO_SEQUENCE_NUMBER, u128, u32 }
define_table! { SEQUENCE_NUMBER_TO_SPACED_RELIC, u32, SpacedRelicValue }
define_table! { SEQUENCE_NUMBER_TO_SYNDICATE_ID, u32, SyndicateIdValue }
define_table! { SEQUENCE_NUMBER_TO_CHEST, u32, ChestEntryValue }
define_multimap_table! { SYNDICATE_TO_CHEST_SEQUENCE_NUMBER, SyndicateIdValue, u32 }
define_table! { RELIC_ID_TO_RELIC_ENTRY, RelicIdValue, RelicEntryValue }
define_table! { RELIC_TO_RELIC_ID, u128, RelicIdValue }
define_table! { RELIC_OWNER_TO_CLAIMABLE, &RelicOwnerValue, u128 }
define_table! { SYNDICATE_ID_TO_SYNDICATE_ENTRY, SyndicateIdValue, SyndicateEntryValue }
define_multimap_table! { RELIC_ID_TO_EVENTS, RelicIdValue, Event }
define_table! { OUTPOINT_TO_RELIC_BALANCES, &OutPointValue, &[u8] }
define_table! { TRANSACTION_ID_TO_RELIC, &TxidValue, u128 }
define_table! { HOME_INSCRIPTIONS, u32, InscriptionIdValue }
define_table! { INSCRIPTION_NUMBER_TO_SEQUENCE_NUMBER, u64, u32 }
define_table! { INSCRIPTION_ID_TO_SEQUENCE_NUMBER, &InscriptionIdValue, u32 }
define_multimap_table! { SAT_TO_SEQUENCE_NUMBER, u64, u32 }
define_table! { SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY, u32, InscriptionEntryValue }
define_table! { SEQUENCE_NUMBER_TO_SATPOINT, u32, &SatPointValue }
define_multimap_table! { SATPOINT_TO_SEQUENCE_NUMBER, &SatPointValue, u32 }
define_multimap_table! { TRANSACTION_ID_TO_EVENTS, &TxidValue, Event }
define_table! { HEIGHT_TO_LAST_SEQUENCE_NUMBER, u32, u32 }
define_table! { SEQUENCE_NUMBER_TO_BONESTONE_BLOCK_HEIGHT, u32, u32 }
define_multimap_table! { SEQUENCE_NUMBER_TO_CHILDREN, u32, u32 }
// using RelicIdValue type for manifest ID (block + tx index)
define_table! { MANIFEST_ID_TO_MANIFEST, RelicIdValue, ManifestEntryValue }
define_table! { MANIFESTED_MINTER_TO_MINTS_LEFT, ManifestedMinterValue, u8 }

pub(crate) struct Index {
  auth: Auth,
  client: Client,
  database: Database,
  path: PathBuf,
  event_sender: Option<tokio::sync::mpsc::Sender<Event>>,
  first_inscription_height: u32,
  first_relic_height: u32,
  first_relic_syndicate_height: u32,
  genesis_block_coinbase_transaction: Transaction,
  genesis_block_coinbase_txid: Txid,
  height_limit: Option<u32>,
  index_sats: bool,
  index_transactions: bool,
  index_relics: bool,
  unrecoverably_reorged: AtomicBool,
  rpc_url: String,
  nr_parallel_requests: usize,
  chain: Chain,
}

#[derive(Debug, PartialEq)]
pub(crate) enum List {
  Spent,
  Unspent(Vec<(u64, u64)>),
}

#[derive(Copy, Clone)]
#[repr(u64)]
pub(crate) enum Statistic {
  Commits,
  IndexSats,
  LostSats,
  OutputsTraversed,
  SatRanges,
  Schema,
  IndexTransactions,
  IndexRelics = 17,
  Relics = 18,
  Manifests = 19,
}

impl Statistic {
  fn key(self) -> u64 {
    self.into()
  }
}

impl From<Statistic> for u64 {
  fn from(statistic: Statistic) -> Self {
    statistic as u64
  }
}

#[derive(Serialize)]
pub(crate) struct Info {
  pub(crate) blocks_indexed: u32,
  pub(crate) branch_pages: u64,
  pub(crate) fragmented_bytes: u64,
  pub(crate) index_file_size: u64,
  pub(crate) index_path: PathBuf,
  pub(crate) leaf_pages: u64,
  pub(crate) metadata_bytes: u64,
  pub(crate) outputs_traversed: u64,
  pub(crate) page_size: usize,
  pub(crate) sat_ranges: u64,
  pub(crate) stored_bytes: u64,
  pub(crate) transactions: Vec<TransactionInfo>,
  pub(crate) tree_height: u32,
  pub(crate) utxos_indexed: u64,
}

#[derive(Serialize)]
pub(crate) struct TransactionInfo {
  pub(crate) starting_block_count: u32,
  pub(crate) starting_timestamp: u128,
}

trait BitcoinCoreRpcResultExt<T> {
  fn into_option(self) -> Result<Option<T>>;
}

impl<T> BitcoinCoreRpcResultExt<T> for Result<T, bitcoincore_rpc::Error> {
  fn into_option(self) -> Result<Option<T>> {
    match self {
      Ok(ok) => Ok(Some(ok)),
      Err(bitcoincore_rpc::Error::JsonRpc(bitcoincore_rpc::jsonrpc::error::Error::Rpc(
        bitcoincore_rpc::jsonrpc::error::RpcError { code: -8, .. },
      ))) => Ok(None),
      Err(bitcoincore_rpc::Error::JsonRpc(bitcoincore_rpc::jsonrpc::error::Error::Rpc(
        bitcoincore_rpc::jsonrpc::error::RpcError { message, .. },
      )))
        if message.ends_with("not found") =>
      {
        Ok(None)
      }
      Err(err) => Err(err.into()),
    }
  }
}

impl Index {
  pub(crate) fn open(options: &Options) -> Result<Self> {
    Index::open_with_event_sender(options, None)
  }
  pub fn open_with_event_sender(
    options: &Options,
    event_sender: Option<tokio::sync::mpsc::Sender<Event>>,
  ) -> Result<Self> {
    let rpc_url = options.rpc_url();
    let nr_parallel_requests = options.nr_parallel_requests();
    let cookie_file = options.cookie_file()?;
    // if cookie_file is emtpy / not set try to parse username:password from RPC URL to create the UserPass auth
    let auth: Auth = if !cookie_file.exists() {
      let url = Url::parse(&rpc_url)?;
      let username = url.username().to_string();
      let password = url.password().map(|x| x.to_string()).unwrap_or_default();

      log::info!(
        "Connecting to Dogecoin Core RPC server at {rpc_url} using credentials from the url"
      );

      Auth::UserPass(username, password)
    } else {
      log::info!(
        "Connecting to Dogecoin Core RPC server at {rpc_url} using credentials from `{}`",
        cookie_file.display()
      );

      Auth::CookieFile(cookie_file)
    };

    let client = Client::new(&rpc_url, auth.clone()).context("failed to connect to RPC URL")?;

    let data_dir = options.data_dir()?;

    if let Err(err) = fs::create_dir_all(&data_dir) {
      bail!("failed to create data dir `{}`: {err}", data_dir.display());
    }

    let path = if let Some(path) = &options.index {
      path.clone()
    } else {
      data_dir.join("index.redb")
    };

    let index_sats;
    let index_transactions;
    let index_relics;

    let database = match unsafe { Database::builder().open(&path) } {
      Ok(database) => {
        {
          let tx = database.begin_read()?;
          let schema_version = tx
            .open_table(STATISTIC_TO_COUNT)?
            .get(&Statistic::Schema.key())?
            .map(|x| x.value())
            .unwrap_or(0);

          match schema_version.cmp(&SCHEMA_VERSION) {
            cmp::Ordering::Less => bail!(
              "index at `{}` appears to have been built with an older, incompatible version of ord, consider deleting and rebuilding the index: index schema {schema_version}, ord schema {SCHEMA_VERSION}",
              path.display()
            ),
            cmp::Ordering::Greater => bail!(
              "index at `{}` appears to have been built with a newer, incompatible version of ord, consider updating ord: index schema {schema_version}, ord schema {SCHEMA_VERSION}",
              path.display()
            ),
            cmp::Ordering::Equal => {}
          }

          let statistics = tx.open_table(STATISTIC_TO_COUNT)?;

          index_sats = statistics
            .get(&Statistic::IndexSats.key())?
            .unwrap()
            .value()
            != 0;
          index_transactions = statistics
            .get(&Statistic::IndexTransactions.key())?
            .unwrap()
            .value()
            != 0;
          index_relics = statistics
            .get(&Statistic::IndexRelics.key())?
            .unwrap()
            .value()
            != 0;
        }

        database
      }
      Err(DatabaseError::Storage(StorageError::Io(error)))
        if error.kind() == io::ErrorKind::NotFound =>
      {
        let db_cache_size = match options.db_cache_size {
          Some(db_cache_size) => db_cache_size,
          None => {
            let mut sys = System::new();
            sys.refresh_memory();
            usize::try_from(sys.total_memory() / 4)?
          }
        };

        let database = Database::builder()
          .set_cache_size(db_cache_size)
          .create(&path)?;

        let tx = database.begin_write()?;

        #[cfg(test)]
        let tx = {
          let mut tx = tx;
          tx.set_durability(redb::Durability::None);
          tx
        };

        tx.open_table(HEIGHT_TO_BLOCK_HASH)?;
        tx.open_table(INSCRIPTION_ID_TO_SATPOINT)?;
        tx.open_table(INSCRIPTION_ID_TO_TXIDS)?;
        tx.open_table(INSCRIPTION_TXID_TO_TX)?;
        tx.open_table(PARTIAL_TXID_TO_INSCRIPTION_TXIDS)?;
        tx.open_table(OUTPOINT_TO_VALUE)?;
        tx.open_multimap_table(ADDRESS_TO_OUTPOINT)?;
        tx.open_table(SATPOINT_TO_INSCRIPTION_ID)?;
        tx.open_table(SAT_TO_SATPOINT)?;
        tx.open_table(WRITE_TRANSACTION_STARTING_BLOCK_COUNT_TO_TIMESTAMP)?;
        tx.open_table(HOME_INSCRIPTIONS)?;
        tx.open_table(TRANSACTION_ID_TO_RELIC)?;
        tx.open_multimap_table(SAT_TO_SEQUENCE_NUMBER)?;
        tx.open_multimap_table(RELIC_ID_TO_EVENTS)?;
        tx.open_multimap_table(TRANSACTION_ID_TO_EVENTS)?;
        tx.open_table(HEIGHT_TO_LAST_SEQUENCE_NUMBER)?;
        tx.open_table(INSCRIPTION_ID_TO_SEQUENCE_NUMBER)?;
        tx.open_table(INSCRIPTION_NUMBER_TO_SEQUENCE_NUMBER)?;
        tx.open_table(OUTPOINT_TO_RELIC_BALANCES)?;
        tx.open_table(RELIC_TO_SEQUENCE_NUMBER)?;
        tx.open_table(SEQUENCE_NUMBER_TO_SPACED_RELIC)?;
        tx.open_table(SEQUENCE_NUMBER_TO_SYNDICATE_ID)?;
        tx.open_table(SEQUENCE_NUMBER_TO_CHEST)?;
        tx.open_multimap_table(SYNDICATE_TO_CHEST_SEQUENCE_NUMBER)?;
        tx.open_table(RELIC_ID_TO_RELIC_ENTRY)?;
        tx.open_table(RELIC_TO_RELIC_ID)?;
        tx.open_table(RELIC_OWNER_TO_CLAIMABLE)?;
        tx.open_table(SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY)?;
        tx.open_table(SEQUENCE_NUMBER_TO_SATPOINT)?;
        tx.open_table(SEQUENCE_NUMBER_TO_BONESTONE_BLOCK_HEIGHT)?;
        tx.open_multimap_table(SEQUENCE_NUMBER_TO_CHILDREN)?;
        tx.open_table(MANIFEST_ID_TO_MANIFEST)?;
        tx.open_table(MANIFESTED_MINTER_TO_MINTS_LEFT)?;

        {
          let mut outpoint_to_sat_ranges = tx.open_table(OUTPOINT_TO_SAT_RANGES)?;
          let mut statistics = tx.open_table(STATISTIC_TO_COUNT)?;

          if options.index_relics {
            // create hardcoded RELIC
            let relic = SpacedRelic::from_str(RELIC_NAME)?.relic;

            let id = RELIC_ID;
            let enshrining = Txid::all_zeros();

            tx.open_table(RELIC_TO_RELIC_ID)?
              .insert(relic.store(), id.store())?;

            statistics.insert(&Statistic::Relics.into(), 1)?;

            tx.open_table(RELIC_ID_TO_RELIC_ENTRY)?.insert(
              id.store(),
              RelicEntry {
                block: id.block,
                enshrining,
                number: 0,
                spaced_relic: SpacedRelic { relic, spacers: 0 },
                symbol: Some('🦴'),
                owner_sequence_number: None,
                boost_terms: None,
                mint_terms: Some(MintTerms {
                  // mint amount per burned bonestone = ~21M total supply
                  amount: Some(572_000_000),
                  // total amount of bonestone delegate inscriptions
                  cap: Some(3_670_709),
                  manifest: None,
                  max_per_block: None,
                  max_per_tx: None,
                  max_unmints: None,
                  price: None,
                  seed: None,
                  swap_height: None,
                }),
                state: RelicState {
                  subsidy_locked: true,
                  ..default()
                },
                pool: None,
                timestamp: 0,
                turbo: true,
              }
              .store(),
            )?;

            tx.open_table(TRANSACTION_ID_TO_RELIC)?
              .insert(&enshrining.store(), relic.store())?;
          }

          if options.index_sats {
            outpoint_to_sat_ranges.insert(&OutPoint::null().store(), [].as_slice())?;
          }

          index_sats = options.index_sats;
          index_transactions = options.index_transactions;
          index_relics = options.index_relics;

          statistics.insert(&Statistic::IndexSats.key(), &u64::from(index_sats))?;

          statistics.insert(&Statistic::IndexRelics.key(), &u64::from(index_relics))?;

          statistics.insert(
            &Statistic::IndexTransactions.key(),
            &u64::from(index_transactions),
          )?;

          statistics.insert(&Statistic::Schema.key(), &SCHEMA_VERSION)?;
        }

        tx.commit()?;

        database
      }
      Err(error) => return Err(error.into()),
    };

    let genesis_block_coinbase_transaction =
      options.chain().genesis_block().coinbase().unwrap().clone();

    Ok(Self {
      genesis_block_coinbase_txid: genesis_block_coinbase_transaction.txid(),
      auth,
      client,
      database,
      path,
      event_sender,
      first_inscription_height: options.first_inscription_height(),
      first_relic_height: options.first_relic_height(),
      first_relic_syndicate_height: options.first_relic_syndicate_height(),
      genesis_block_coinbase_transaction,
      height_limit: options.height_limit,
      index_sats,
      index_transactions,
      index_relics,
      unrecoverably_reorged: AtomicBool::new(false),
      rpc_url,
      nr_parallel_requests,
      chain: options.chain_argument,
    })
  }

  pub(crate) fn get_unspent_outputs(&self, _wallet: Wallet) -> Result<BTreeMap<OutPoint, Amount>> {
    let mut utxos = BTreeMap::new();
    utxos.extend(
      self
        .client
        .list_unspent(None, None, None, None, None)?
        .into_iter()
        .map(|utxo| {
          let outpoint = OutPoint::new(utxo.txid, utxo.vout);
          let amount = utxo.amount;

          (outpoint, amount)
        }),
    );

    #[derive(Deserialize)]
    pub(crate) struct JsonOutPoint {
      txid: bitcoin::Txid,
      vout: u32,
    }

    for JsonOutPoint { txid, vout } in self
      .client
      .call::<Vec<JsonOutPoint>>("listlockunspent", &[])?
    {
      utxos.insert(
        OutPoint { txid, vout },
        Amount::from_sat(self.client.get_raw_transaction(&txid)?.output[vout as usize].value),
      );
    }
    let rtx = self.database.begin_read()?;
    let outpoint_to_value = rtx.open_table(OUTPOINT_TO_VALUE)?;
    for outpoint in utxos.keys() {
      if outpoint_to_value.get(&outpoint.store())?.is_none() {
        return Err(anyhow!(
          "output in Dogecoin Core wallet but not in ord index: {outpoint}"
        ));
      }
    }

    Ok(utxos)
  }

  pub(crate) fn get_unspent_output_ranges(
    &self,
    wallet: Wallet,
  ) -> Result<Vec<(OutPoint, Vec<(u64, u64)>)>> {
    self
      .get_unspent_outputs(wallet)?
      .into_keys()
      .map(|outpoint| match self.list(outpoint)? {
        Some(List::Unspent(sat_ranges)) => Ok((outpoint, sat_ranges)),
        Some(List::Spent) => bail!("output {outpoint} in wallet but is spent according to index"),
        None => bail!("index has not seen {outpoint}"),
      })
      .collect()
  }

  pub(crate) fn has_sat_index(&self) -> bool {
    self.index_sats
  }

  pub(crate) fn info(&self) -> Result<Info> {
    let wtx = self.begin_write()?;

    let stats = wtx.stats()?;

    let info = {
      let statistic_to_count = wtx.open_table(STATISTIC_TO_COUNT)?;
      let sat_ranges = statistic_to_count
        .get(&Statistic::SatRanges.key())?
        .map(|x| x.value())
        .unwrap_or(0);
      let outputs_traversed = statistic_to_count
        .get(&Statistic::OutputsTraversed.key())?
        .map(|x| x.value())
        .unwrap_or(0);
      let transactions: Vec<TransactionInfo> = wtx
        .open_table(WRITE_TRANSACTION_STARTING_BLOCK_COUNT_TO_TIMESTAMP)?
        .range(0..)?
        .map(|result| {
          result.map(
            |(starting_block_count, starting_timestamp)| TransactionInfo {
              starting_block_count: starting_block_count.value(),
              starting_timestamp: starting_timestamp.value(),
            },
          )
        })
        .collect::<Result<Vec<_>, _>>()?;
      Info {
        index_path: self.path.clone(),
        blocks_indexed: wtx
          .open_table(HEIGHT_TO_BLOCK_HASH)?
          .range(0..)?
          .rev()
          .next()
          .map(|result| result.map(|(height, _hash)| height.value() + 1))
          .transpose()?
          .unwrap_or(0),
        branch_pages: stats.branch_pages(),
        fragmented_bytes: stats.fragmented_bytes(),
        index_file_size: fs::metadata(&self.path)?.len(),
        leaf_pages: stats.leaf_pages(),
        metadata_bytes: stats.metadata_bytes(),
        sat_ranges,
        outputs_traversed,
        page_size: stats.page_size(),
        stored_bytes: stats.stored_bytes(),
        transactions,
        tree_height: stats.tree_height(),
        utxos_indexed: wtx.open_table(OUTPOINT_TO_SAT_RANGES)?.len()?,
      }
    };

    Ok(info)
  }

  pub(crate) fn update(&self) -> Result {
    let mut updater = Updater::new(self)?;

    loop {
      match updater.update_index() {
        Ok(ok) => return Ok(ok),
        Err(err) => {
          log::info!("{}", err.to_string());

          match err.downcast_ref() {
            Some(&ReorgError::Recoverable { height, depth }) => {
              Reorg::handle_reorg(self, height, depth)?;

              updater = Updater::new(self)?;
            }
            Some(&ReorgError::Unrecoverable) => {
              self
                .unrecoverably_reorged
                .store(true, atomic::Ordering::Relaxed);
              return Err(anyhow!(ReorgError::Unrecoverable));
            }
            _ => return Err(err),
          };
        }
      }
    }
  }

  pub(crate) fn is_unrecoverably_reorged(&self) -> bool {
    self.unrecoverably_reorged.load(atomic::Ordering::Relaxed)
  }

  fn begin_read(&self) -> Result<rtx::Rtx> {
    Ok(rtx::Rtx(self.database.begin_read()?))
  }

  fn begin_write(&self) -> Result<WriteTransaction> {
    if cfg!(test) {
      let mut tx = self.database.begin_write()?;
      tx.set_durability(redb::Durability::None);
      Ok(tx)
    } else {
      Ok(self.database.begin_write()?)
    }
  }

  fn increment_statistic(wtx: &WriteTransaction, statistic: Statistic, n: u64) -> Result {
    let mut statistic_to_count = wtx.open_table(STATISTIC_TO_COUNT)?;
    let value = statistic_to_count
      .get(&(statistic.key()))?
      .map(|x| x.value())
      .unwrap_or(0)
      + n;
    statistic_to_count.insert(&statistic.key(), &value)?;
    Ok(())
  }

  #[cfg(test)]
  pub(crate) fn statistic(&self, statistic: Statistic) -> u64 {
    self
      .database
      .begin_read()
      .unwrap()
      .open_table(STATISTIC_TO_COUNT)
      .unwrap()
      .get(&statistic.key())
      .unwrap()
      .map(|x| x.value())
      .unwrap_or(0)
  }

  pub(crate) fn height(&self) -> Result<Option<Height>> {
    self.begin_read()?.height()
  }

  pub(crate) fn block_count(&self) -> Result<u32> {
    self.begin_read()?.block_count()
  }

  pub(crate) fn block_hash(&self, height: Option<u32>) -> Result<Option<BlockHash>> {
    self.begin_read()?.block_hash(height)
  }

  pub(crate) fn blocks(&self, take: usize) -> Result<Vec<(u32, BlockHash)>> {
    let mut blocks = Vec::new();

    let rtx = self.begin_read()?;

    let block_count = rtx.block_count()?;

    let height_to_block_hash = rtx.0.open_table(HEIGHT_TO_BLOCK_HASH)?;

    for result in height_to_block_hash.range(0..block_count)?.rev().take(take) {
      let (height, block_hash) = match result {
        Ok(value) => value,
        Err(e) => {
          return Err(e.into());
        }
      };

      blocks.push((height.value(), Entry::load(*block_hash.value())));
    }

    Ok(blocks)
  }

  pub(crate) fn rare_sat_satpoints(&self) -> Result<Vec<(Sat, SatPoint)>> {
    let rtx = self.database.begin_read()?;

    let sat_to_satpoint = rtx.open_table(SAT_TO_SATPOINT)?;

    let mut result = Vec::with_capacity(sat_to_satpoint.len()?.try_into().unwrap());

    for range in sat_to_satpoint.range(0..)? {
      let (sat, satpoint) = range?;
      result.push((Sat(sat.value()), Entry::load(*satpoint.value())));
    }

    Ok(result)
  }

  pub(crate) fn rare_sat_satpoint(&self, sat: Sat) -> Result<Option<SatPoint>> {
    Ok(
      self
        .database
        .begin_read()?
        .open_table(SAT_TO_SATPOINT)?
        .get(&sat.n())?
        .map(|satpoint| Entry::load(*satpoint.value())),
    )
  }

  pub(crate) fn get_account_outputs(&self, address: String) -> Result<Vec<OutPoint>> {
    let mut result: Vec<OutPoint> = Vec::new();

    self
      .database
      .begin_read()?
      .open_multimap_table(ADDRESS_TO_OUTPOINT)?
      .get(address.as_bytes())?
      .for_each(|res| {
        if let Ok(item) = res {
          result.push(OutPoint::load(*item.value()));
        } else {
          println!("Error: {:?}", res.err().unwrap());
        }
      });

    Ok(result)
  }

  pub(crate) fn block_header(&self, hash: BlockHash) -> Result<Option<BlockHeader>> {
    self.client.get_block_header(&hash).into_option()
  }

  pub(crate) fn block_header_info(&self, hash: BlockHash) -> Result<Option<GetBlockHeaderResult>> {
    self.client.get_block_header_info(&hash).into_option()
  }

  pub(crate) fn get_block_by_height(&self, height: u32) -> Result<Option<Block>> {
    let tx = self.database.begin_read()?;

    let indexed = tx.open_table(HEIGHT_TO_BLOCK_HASH)?.get(&height)?.is_some();

    if !indexed {
      return Ok(None);
    }

    Ok(
      self
        .client
        .get_block_hash(height.into())
        .into_option()?
        .map(|hash| self.client.get_block(&hash))
        .transpose()?,
    )
  }

  pub(crate) fn get_block_by_hash(&self, hash: BlockHash) -> Result<Option<Block>> {
    let tx = self.database.begin_read()?;

    // check if the given hash exists as a value in the database
    let indexed =
      tx.open_table(HEIGHT_TO_BLOCK_HASH)?
        .range(0..)?
        .rev()
        .any(|result| match result {
          Ok((_, block_hash)) => block_hash.value() == hash.as_inner(),
          Err(_) => false,
        });

    if !indexed {
      return Ok(None);
    }

    self.client.get_block(&hash).into_option()
  }

  pub fn get_inscription_ids_by_sat(&self, sat: Sat) -> Result<Vec<InscriptionId>> {
    let rtx = self.database.begin_read()?;

    let sequence_number_to_inscription_entry =
      rtx.open_table(SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY)?;

    let ids = rtx
      .open_multimap_table(SAT_TO_SEQUENCE_NUMBER)?
      .get(&sat.n())?
      .map(|result| {
        result
          .and_then(|sequence_number| {
            let sequence_number = sequence_number.value();
            sequence_number_to_inscription_entry
              .get(sequence_number)
              .map(|entry| InscriptionEntry::load(entry.unwrap().value()).id)
          })
          .map_err(|err| err.into())
      })
      .collect::<Result<Vec<InscriptionId>>>()?;

    Ok(ids)
  }

  pub fn get_inscription_id_by_sat_indexed(
    &self,
    sat: Sat,
    inscription_index: isize,
  ) -> Result<Option<InscriptionId>> {
    let rtx = self.database.begin_read()?;

    let sequence_number_to_inscription_entry =
      rtx.open_table(SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY)?;

    let sat_to_sequence_number = rtx.open_multimap_table(SAT_TO_SEQUENCE_NUMBER)?;

    if inscription_index < 0 {
      sat_to_sequence_number
        .get(&sat.n())?
        .nth_back((inscription_index + 1).abs_diff(0))
    } else {
      sat_to_sequence_number
        .get(&sat.n())?
        .nth(inscription_index.abs_diff(0))
    }
    .map(|result| {
      result
        .and_then(|sequence_number| {
          let sequence_number = sequence_number.value();
          sequence_number_to_inscription_entry
            .get(sequence_number)
            .map(|entry| InscriptionEntry::load(entry.unwrap().value()).id)
        })
        .map_err(|err| anyhow!(err.to_string()))
    })
    .transpose()
  }

  pub(crate) fn get_inscription_id_by_inscription_number(
    &self,
    inscription_number: u64,
  ) -> Result<Option<InscriptionId>> {
    let rtx = self.database.begin_read()?;

    let Some(sequence_number) = rtx
      .open_table(INSCRIPTION_NUMBER_TO_SEQUENCE_NUMBER)?
      .get(inscription_number)?
      .map(|guard| guard.value())
    else {
      return Ok(None);
    };

    let inscription_id = rtx
      .open_table(SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY)?
      .get(&sequence_number)?
      .map(|entry| InscriptionEntry::load(entry.value()).id);

    Ok(inscription_id)
  }

  pub fn events_for_relic(
    &self,
    relic: Relic,
    page_size: usize,
    page_index: usize,
  ) -> Result<Option<Vec<Event>>> {
    let rtx = self.database.begin_read()?;

    let Some(id) = rtx
      .open_table(RELIC_TO_RELIC_ID)?
      .get(relic.0)?
      .map(|guard| guard.value())
    else {
      return Ok(None);
    };

    let events = rtx
      .open_multimap_table(RELIC_ID_TO_EVENTS)?
      .get(id)?
      .rev()
      .skip(page_index * page_size)
      .take(page_size.saturating_add(1))
      .map(|result| result.map(|entry| entry.value()).map_err(|err| err.into()))
      .collect::<Result<Vec<Event>>>()?;

    Ok(Some(events))
  }

  pub fn events_for_tx(&self, txid: Txid) -> Result<Vec<Event>> {
    let rtx = self.database.begin_read()?;

    let events = rtx
      .open_multimap_table(TRANSACTION_ID_TO_EVENTS)?
      .get(&txid.store())?
      .map(|result| result.map(|entry| entry.value()).map_err(|err| err.into()))
      .collect::<Result<Vec<Event>>>()?;

    Ok(events)
  }

  pub fn has_relic_index(&self) -> bool {
    self.index_relics
  }

  pub fn get_relic_by_id(&self, id: RelicId) -> Result<Option<Relic>> {
    Ok(
      self
        .database
        .begin_read()?
        .open_table(RELIC_ID_TO_RELIC_ENTRY)?
        .get(&id.store())?
        .map(|entry| RelicEntry::load(entry.value()).spaced_relic.relic),
    )
  }

  pub fn get_relic_by_number(&self, number: usize) -> Result<Option<Relic>> {
    match self
      .database
      .begin_read()?
      .open_table(RELIC_ID_TO_RELIC_ENTRY)?
      .iter()?
      .nth(number)
    {
      Some(result) => {
        let rune_result =
          result.map(|(_id, entry)| RelicEntry::load(entry.value()).spaced_relic.relic);
        Ok(rune_result.ok())
      }
      None => Ok(None),
    }
  }

  pub fn relic(
    &self,
    relic: Relic,
  ) -> Result<Option<(RelicId, RelicEntry, Option<InscriptionId>)>> {
    let rtx = self.database.begin_read()?;

    let Some(id) = rtx
      .open_table(RELIC_TO_RELIC_ID)?
      .get(relic.0)?
      .map(|guard| guard.value())
    else {
      return Ok(None);
    };

    let entry = RelicEntry::load(
      rtx
        .open_table(RELIC_ID_TO_RELIC_ENTRY)?
        .get(id)?
        .unwrap()
        .value(),
    );

    let owner = if let Some(owner_sequence_number) = entry.owner_sequence_number {
      Some(
        InscriptionEntry::load(
          rtx
            .open_table(SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY)?
            .get(owner_sequence_number)?
            .unwrap()
            .value(),
        )
        .id,
      )
    } else {
      None
    };

    Ok(Some((RelicId::load(id), entry, owner)))
  }

  pub fn relics(&self) -> Result<Vec<(RelicId, RelicEntry)>> {
    let mut entries = Vec::new();

    for result in self
      .database
      .begin_read()?
      .open_table(RELIC_ID_TO_RELIC_ENTRY)?
      .iter()?
    {
      let (id, entry) = result?;
      entries.push((RelicId::load(id.value()), RelicEntry::load(entry.value())));
    }

    Ok(entries)
  }

  pub fn relics_paginated(
    &self,
    page_size: usize,
    page_index: usize,
  ) -> Result<(Vec<(RelicId, RelicEntry, Option<InscriptionId>)>, bool)> {
    let rtx = self.database.begin_read()?;

    let relic_id_to_relic_entry = rtx.open_table(RELIC_ID_TO_RELIC_ENTRY)?;
    let relic_to_sequence_number = rtx.open_table(RELIC_TO_SEQUENCE_NUMBER)?;
    let sequence_number_to_inscription_entry =
      rtx.open_table(SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY)?;

    let mut entries = Vec::new();

    for result in relic_id_to_relic_entry
      .iter()?
      .rev()
      .skip(page_index.saturating_mul(page_size))
      .take(page_size.saturating_add(1))
    {
      let (id_bytes, entry_bytes) = result?;
      let relic_id = RelicId::load(id_bytes.value());
      let relic_entry = RelicEntry::load(entry_bytes.value());

      let inscription_id_opt = if let Some(seq_number) =
        relic_to_sequence_number.get(relic_entry.spaced_relic.relic.n())?
      {
        if let Some(inscription_entry_guard) =
          sequence_number_to_inscription_entry.get(seq_number.value())?
        {
          let inscription_entry = InscriptionEntry::load(inscription_entry_guard.value());
          Some(inscription_entry.id)
        } else {
          None
        }
      } else {
        None
      };

      entries.push((relic_id, relic_entry, inscription_id_opt));
    }

    let more = entries.len() > page_size;
    if more {
      entries.pop();
    }

    Ok((entries, more))
  }

  pub fn sealing(&self, relic: Relic) -> Result<(Option<api::Inscription>, Option<Txid>)> {
    let rtx = self.database.begin_read()?;

    let relic_to_sequence_number = rtx.open_table(RELIC_TO_SEQUENCE_NUMBER)?;
    let sequence_number_to_inscription_entry =
      rtx.open_table(SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY)?;
    let relic_to_relic_id = rtx.open_table(RELIC_TO_RELIC_ID)?;
    let relic_id_to_relic_entry = rtx.open_table(RELIC_ID_TO_RELIC_ENTRY)?;

    if let Some(seq_number) = relic_to_sequence_number.get(&relic.store())? {
      let seq_number = seq_number.value();
      if let Some(inscription_entry_val) = sequence_number_to_inscription_entry.get(seq_number)? {
        let inscription_entry = InscriptionEntry::load(inscription_entry_val.value());
        let inscription_id = inscription_entry.id;

        if let Some((mut api_inscription, _, _, _)) = self.inscription_info(
          subcommand::server::query::Inscription::Id(inscription_id),
          true,
        )? {
          let mut enshrining_txid = None;

          if api_inscription.relic_enshrined {
            if let Some(raw_id_val) = relic_to_relic_id.get(relic.store())? {
              let relic_id = RelicId::load(raw_id_val.value());

              if let Some(relic_entry_val) = relic_id_to_relic_entry.get(&relic_id.store())? {
                let relic_entry = RelicEntry::load(relic_entry_val.value());
                enshrining_txid = Some(relic_entry.enshrining);
              }
            }
          }
          return Ok((Some(api_inscription), enshrining_txid));
        }
      }
    }
    Ok((None, None))
  }

  pub fn sealings_paginated(
    &self,
    page_size: usize,
    page_index: usize,
  ) -> Result<(Vec<(api::Inscription, Option<Txid>)>, bool)> {
    let rtx = self.database.begin_read()?;

    let relic_to_sequence_number = rtx.open_table(RELIC_TO_SEQUENCE_NUMBER)?;
    let sequence_number_to_inscription_entry =
      rtx.open_table(SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY)?;
    let seq_to_spaced_relic = rtx.open_table(SEQUENCE_NUMBER_TO_SPACED_RELIC)?;
    let relic_to_relic_id = rtx.open_table(RELIC_TO_RELIC_ID)?;
    let relic_id_to_relic_entry = rtx.open_table(RELIC_ID_TO_RELIC_ENTRY)?;

    let mut results = Vec::new();
    let start = page_index.saturating_mul(page_size);
    let end = start.saturating_add(page_size).saturating_add(1);

    for (i, row) in relic_to_sequence_number.iter()?.enumerate() {
      if i < start {
        continue;
      } else if i >= end {
        break;
      }

      let (_raw_relic, raw_seq_number) = row?;
      let seq_number = raw_seq_number.value();

      if let Some(inscription_entry_val) = sequence_number_to_inscription_entry.get(seq_number)? {
        let inscription_entry = InscriptionEntry::load(inscription_entry_val.value());
        let inscription_id = inscription_entry.id;

        if let Some((mut api_inscription, _, _, _)) = self.inscription_info(
          subcommand::server::query::Inscription::Id(inscription_id),
          true,
        )? {
          let mut enshrining_txid = None;

          if api_inscription.relic_enshrined {
            if let Some(spaced_relic_val) = seq_to_spaced_relic.get(seq_number)? {
              let spaced_relic = SpacedRelic::load(spaced_relic_val.value());

              if let Some(raw_id_val) = relic_to_relic_id.get(&spaced_relic.relic.store())? {
                let relic_id = RelicId::load(raw_id_val.value());

                if let Some(relic_entry_val) = relic_id_to_relic_entry.get(&relic_id.store())? {
                  let relic_entry = RelicEntry::load(relic_entry_val.value());
                  enshrining_txid = Some(relic_entry.enshrining);
                }
              }
            }
          }

          results.push((api_inscription, enshrining_txid));
        }
      }
    }

    let more = results.len() > page_size;
    if more {
      results.pop();
    }

    Ok((results, more))
  }

  pub fn syndicate(
    &self,
    id: SyndicateId,
  ) -> Result<Option<(SyndicateId, SyndicateEntry, Option<InscriptionId>)>> {
    let rtx = self.database.begin_read()?;

    let Some(entry) = rtx
      .open_table(SYNDICATE_ID_TO_SYNDICATE_ENTRY)?
      .get(id.store())?
      .map(|guard| guard.value())
    else {
      return Ok(None);
    };

    let entry = SyndicateEntry::load(entry);

    let owner = Some(
      InscriptionEntry::load(
        rtx
          .open_table(SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY)?
          .get(entry.sequence_number)?
          .unwrap()
          .value(),
      )
      .id,
    );

    Ok(Some((id, entry, owner)))
  }

  pub fn syndicates(&self) -> Result<Vec<(SyndicateId, SyndicateEntry)>> {
    let mut entries = Vec::new();

    for result in self
      .database
      .begin_read()?
      .open_table(SYNDICATE_ID_TO_SYNDICATE_ENTRY)?
      .iter()?
    {
      let (id, entry) = result?;
      entries.push((
        SyndicateId::load(id.value()),
        SyndicateEntry::load(entry.value()),
      ));
    }

    Ok(entries)
  }

  pub fn syndicates_paginated(
    &self,
    page_size: usize,
    page_index: usize,
  ) -> Result<(Vec<(SyndicateId, SyndicateEntry)>, bool)> {
    let mut entries = Vec::new();

    for result in self
      .database
      .begin_read()?
      .open_table(SYNDICATE_ID_TO_SYNDICATE_ENTRY)?
      .iter()?
      .rev()
      .skip(page_index.saturating_mul(page_size))
      .take(page_size.saturating_add(1))
    {
      let (id, entry) = result?;
      entries.push((
        SyndicateId::load(id.value()),
        SyndicateEntry::load(entry.value()),
      ));
    }

    let more = entries.len() > page_size;

    Ok((entries, more))
  }

  pub fn get_relic_balances_for_outpoint(
    &self,
    outpoint: OutPoint,
  ) -> Result<BTreeMap<SpacedRelic, Pile>> {
    let rtx = self.database.begin_read()?;

    let outpoint_to_balances = rtx.open_table(OUTPOINT_TO_RELIC_BALANCES)?;

    let id_to_relic_entries = rtx.open_table(RELIC_ID_TO_RELIC_ENTRY)?;

    let Some(balances) = outpoint_to_balances.get(&outpoint.store())? else {
      return Ok(BTreeMap::new());
    };

    let balances_buffer = balances.value();

    let mut balances = BTreeMap::new();
    let mut i = 0;
    while i < balances_buffer.len() {
      let ((id, amount), length) = Index::decode_relic_balance(&balances_buffer[i..]).unwrap();
      i += length;

      let entry = RelicEntry::load(id_to_relic_entries.get(id.store())?.unwrap().value());

      balances.insert(
        entry.spaced_relic,
        Pile {
          amount,
          divisibility: Enshrining::DIVISIBILITY,
          symbol: entry.symbol,
        },
      );
    }

    Ok(balances)
  }

  pub fn get_relic_balance_map(&self) -> Result<BTreeMap<SpacedRelic, BTreeMap<OutPoint, Pile>>> {
    let outpoint_balances = self.get_relic_balances()?;

    let rtx = self.database.begin_read()?;

    let relic_id_to_relic_entry = rtx.open_table(RELIC_ID_TO_RELIC_ENTRY)?;

    let mut relic_balances_by_id: BTreeMap<RelicId, BTreeMap<OutPoint, u128>> = BTreeMap::new();

    for (outpoint, balances) in outpoint_balances {
      for (relic_id, amount) in balances {
        *relic_balances_by_id
          .entry(relic_id)
          .or_default()
          .entry(outpoint)
          .or_default() += amount;
      }
    }

    let mut relic_balances = BTreeMap::new();

    for (relic_id, balances) in relic_balances_by_id {
      let RelicEntry {
        spaced_relic,
        symbol,
        ..
      } = RelicEntry::load(
        relic_id_to_relic_entry
          .get(&relic_id.store())?
          .unwrap()
          .value(),
      );

      relic_balances.insert(
        spaced_relic,
        balances
          .into_iter()
          .map(|(outpoint, amount)| {
            (
              outpoint,
              Pile {
                amount,
                divisibility: Enshrining::DIVISIBILITY,
                symbol,
              },
            )
          })
          .collect(),
      );
    }

    Ok(relic_balances)
  }

  pub fn get_relic_balances(&self) -> Result<Vec<(OutPoint, Vec<(RelicId, u128)>)>> {
    let mut result = Vec::new();

    for entry in self
      .database
      .begin_read()?
      .open_table(OUTPOINT_TO_RELIC_BALANCES)?
      .iter()?
    {
      let (outpoint, balances_buffer) = entry?;
      let outpoint = OutPoint::load(*outpoint.value());
      let balances_buffer = balances_buffer.value();

      let mut balances = Vec::new();
      let mut i = 0;
      while i < balances_buffer.len() {
        let ((id, balance), length) = Index::decode_relic_balance(&balances_buffer[i..]).unwrap();
        i += length;
        balances.push((id, balance));
      }

      result.push((outpoint, balances));
    }

    Ok(result)
  }

  pub fn get_relic_claimable(&self) -> Result<Vec<(RelicOwner, u128)>> {
    let mut result = Vec::new();

    for entry in self
      .database
      .begin_read()?
      .open_table(RELIC_OWNER_TO_CLAIMABLE)?
      .iter()?
    {
      let (owner, amount) = entry?;
      result.push((RelicOwner::load(*owner.value()), amount.value()));
    }

    Ok(result)
  }

  pub(crate) fn inscription_relic_info(
    &self,
    query: subcommand::server::query::Inscription,
  ) -> Result<Option<(api::RelicInscription)>> {
    let rtx = self.database.begin_read()?;

    let sequence_number = match query {
      subcommand::server::query::Inscription::Id(id) => rtx
        .open_table(INSCRIPTION_ID_TO_SEQUENCE_NUMBER)?
        .get(&id.store())?
        .map(|guard| guard.value()),
      subcommand::server::query::Inscription::Number(inscription_number) => rtx
        .open_table(INSCRIPTION_NUMBER_TO_SEQUENCE_NUMBER)?
        .get(inscription_number)?
        .map(|guard| guard.value()),
    };

    let Some(sequence_number) = sequence_number else {
      return Ok(None);
    };

    let sequence_number_to_inscription_entry =
      rtx.open_table(SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY)?;

    let entry = InscriptionEntry::load(
      sequence_number_to_inscription_entry
        .get(&sequence_number)?
        .unwrap()
        .value(),
    );

    let is_bonestone = self.get_bonestone_by_sequence_number(sequence_number)?;
    let relic_sealed = rtx
      .open_table(SEQUENCE_NUMBER_TO_SPACED_RELIC)?
      .get(sequence_number)?
      .map(|entry| SpacedRelic::load(entry.value()));

    let relic_enshrined = if let Some(spaced_relic) = relic_sealed {
      rtx
        .open_table(RELIC_TO_RELIC_ID)?
        .get(spaced_relic.relic.store())?
        .is_some()
    } else {
      false
    };

    Ok(Some(api::RelicInscription {
      id: entry.id,
      is_bonestone,
      relic_sealed,
      relic_enshrined,
    }))
  }

  pub(crate) fn inscription_info(
    &self,
    query: subcommand::server::query::Inscription,
    get_output: bool,
  ) -> Result<
    Option<(
      api::Inscription,
      Option<TxOut>,
      Inscription,
      InscriptionEntry,
    )>,
  > {
    let rtx = self.database.begin_read()?;

    let sequence_number = match query {
      subcommand::server::query::Inscription::Id(id) => rtx
        .open_table(INSCRIPTION_ID_TO_SEQUENCE_NUMBER)?
        .get(&id.store())?
        .map(|guard| guard.value()),
      subcommand::server::query::Inscription::Number(inscription_number) => rtx
        .open_table(INSCRIPTION_NUMBER_TO_SEQUENCE_NUMBER)?
        .get(inscription_number)?
        .map(|guard| guard.value()),
    };

    let Some(sequence_number) = sequence_number else {
      return Ok(None);
    };

    let sequence_number_to_inscription_entry =
      rtx.open_table(SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY)?;

    let entry = InscriptionEntry::load(
      sequence_number_to_inscription_entry
        .get(&sequence_number)?
        .unwrap()
        .value(),
    );

    let reader = self.database.begin_read()?;

    let table = reader.open_table(INSCRIPTION_ID_TO_TXIDS)?;
    let txids_result = table.get(&entry.id.store())?;

    let Some(inscription) = (match txids_result {
      Some(txids) => {
        let mut txs = vec![];

        let txids = txids.value();

        for i in 0..txids.len() / 32 {
          let txid_buf = &txids[i * 32..i * 32 + 32];
          let table = reader.open_table(INSCRIPTION_TXID_TO_TX)?;
          let tx_result = table.get(txid_buf)?;

          match tx_result {
            Some(tx_result) => {
              let tx_buf = tx_result.value().to_vec();
              let mut cursor = Cursor::new(tx_buf);
              let tx = Transaction::consensus_decode(&mut cursor)?;
              txs.push(tx);
            }
            _ => {}
          }
        }

        let parsed_inscription = Inscription::from_transactions(txs);

        match parsed_inscription {
          ParsedInscription::None => None,
          ParsedInscription::Partial => None,
          ParsedInscription::Complete(inscription) => Some(inscription),
        }
      }

      None => None,
    }) else {
      return Ok(None);
    };

    let satpoint = SatPoint::load(
      *rtx
        .open_table(INSCRIPTION_ID_TO_SATPOINT)?
        .get(&entry.id.store())?
        .unwrap()
        .value(),
    );

    let output = if get_output {
      if satpoint.outpoint == unbound_outpoint() || satpoint.outpoint == OutPoint::null() {
        None
      } else {
        if let Some(transaction) = self.get_transaction(satpoint.outpoint.txid)? {
          transaction
            .output
            .into_iter()
            .nth(satpoint.outpoint.vout.try_into().unwrap())
        } else {
          return Ok(None);
        }
      }
    } else {
      None
    };

    let previous = if let Some(n) = sequence_number.checked_sub(1) {
      Some(
        InscriptionEntry::load(
          sequence_number_to_inscription_entry
            .get(n)?
            .unwrap()
            .value(),
        )
        .id,
      )
    } else {
      None
    };

    let next = sequence_number_to_inscription_entry
      .get(sequence_number + 1)?
      .map(|guard| InscriptionEntry::load(guard.value()).id);

    let all_children = rtx
      .open_multimap_table(SEQUENCE_NUMBER_TO_CHILDREN)?
      .get(sequence_number)?;

    let child_count = all_children.len();

    let sequence_number_to_inscription_entry =
      rtx.open_table(SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY)?;

    let children = all_children
      .take(4)
      .map(|result| {
        result
          .and_then(|sequence_number| {
            sequence_number_to_inscription_entry
              .get(sequence_number.value())
              .map(|entry| InscriptionEntry::load(entry.unwrap().value()).id)
          })
          .map_err(|err| err.into())
      })
      .collect::<Result<Vec<InscriptionId>>>()?;

    let relic_sealed = rtx
      .open_table(SEQUENCE_NUMBER_TO_SPACED_RELIC)?
      .get(sequence_number)?
      .map(|entry| SpacedRelic::load(entry.value()));

    let relic_enshrined = if let Some(spaced_relic) = relic_sealed {
      rtx
        .open_table(RELIC_TO_RELIC_ID)?
        .get(spaced_relic.relic.store())?
        .is_some()
    } else {
      false
    };

    let syndicate = rtx
      .open_table(SEQUENCE_NUMBER_TO_SYNDICATE_ID)?
      .get(sequence_number)?
      .map(|entry| SyndicateId::load(entry.value()));

    let chest = rtx
      .open_table(SEQUENCE_NUMBER_TO_CHEST)?
      .get(sequence_number)?
      .is_some();

    let parents = entry
      .parents
      .iter()
      .take(4)
      .map(|parent| {
        Ok(
          InscriptionEntry::load(
            sequence_number_to_inscription_entry
              .get(parent)?
              .unwrap()
              .value(),
          )
          .id,
        )
      })
      .collect::<Result<Vec<InscriptionId>>>()?;

    let effective_mime_type = if let Some(delegate_id) = inscription.delegate() {
      let delegate_result = self.get_inscription_by_id(delegate_id);
      if let Ok(Some(delegate)) = delegate_result {
        delegate.content_type().map(str::to_string)
      } else {
        inscription.content_type().map(str::to_string)
      }
    } else {
      inscription.content_type().map(str::to_string)
    };
    let charms = entry.charms;
    let address = if let Some(out) = &output {
      self
        .chain
        .address_from_script(&out.script_pubkey)
        .ok()
        .map(|address| address.to_string())
    } else {
      None
    };

    Ok(Some((
      api::Inscription {
        address,
        charms: Charm::charms(charms),
        child_count,
        children,
        content_length: inscription.content_length(),
        content_type: inscription.content_type().map(|s| s.to_string()),
        effective_content_type: effective_mime_type,
        fee: entry.fee,
        height: entry.height,
        id: entry.id,
        next,
        number: entry.inscription_number,
        parents,
        previous,
        relic_sealed,
        relic_enshrined,
        syndicate,
        chest,
        sat: entry.sat,
        satpoint,
        timestamp: timestamp(entry.timestamp.into()).timestamp(),
        value: output.as_ref().map(|o| o.value),
      },
      output,
      inscription,
      entry,
    )))
  }

  pub(crate) fn get_bonestone_by_sequence_number(&self, sequence_number: u32) -> Result<bool> {
    Ok(
      self
        .database
        .begin_read()?
        .open_table(SEQUENCE_NUMBER_TO_BONESTONE_BLOCK_HEIGHT)?
        .get(&sequence_number)?
        .is_some(),
    )
  }

  pub(crate) fn get_all_bonestones_inscription_ids(&self) -> Result<Vec<(InscriptionId, u32)>> {
    let min_range: u32 = 0;
    let max_range: u32 = u32::MAX;

    let read_txn = self.database.begin_read()?;

    let seq_to_inscription_entry = read_txn.open_table(SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY)?;
    let seq_to_bonestone_block_height =
      read_txn.open_table(SEQUENCE_NUMBER_TO_BONESTONE_BLOCK_HEIGHT)?;

    let results: Vec<(InscriptionId, u32)> = seq_to_bonestone_block_height
      .range(min_range..max_range)?
      .filter_map(|result| {
        result.ok().and_then(|(sequence_number, block_height)| {
          seq_to_inscription_entry
            .get(sequence_number.value())
            .ok()
            .and_then(|entry| {
              entry.map(|inscription| {
                (
                  InscriptionEntry::load(inscription.value()).id,
                  block_height.value(),
                )
              })
            })
        })
      })
      .collect();

    Ok(results)
  }

  pub(crate) fn get_inscription_satpoint_by_id(
    &self,
    inscription_id: InscriptionId,
  ) -> Result<Option<SatPoint>> {
    Ok(
      self
        .database
        .begin_read()?
        .open_table(INSCRIPTION_ID_TO_SATPOINT)?
        .get(&inscription_id.store())?
        .map(|satpoint| Entry::load(*satpoint.value())),
    )
  }

  pub(crate) fn get_inscription_by_id(
    &self,
    inscription_id: InscriptionId,
  ) -> Result<Option<Inscription>> {
    if self
      .database
      .begin_read()?
      .open_table(INSCRIPTION_ID_TO_SATPOINT)?
      .get(&inscription_id.store())?
      .is_none()
    {
      return Ok(None);
    }

    let reader = self.database.begin_read()?;

    let table = reader.open_table(INSCRIPTION_ID_TO_TXIDS)?;
    let txids_result = table.get(&inscription_id.store())?;

    match txids_result {
      Some(txids) => {
        let mut txs = vec![];

        let txids = txids.value();

        for i in 0..txids.len() / 32 {
          let txid_buf = &txids[i * 32..i * 32 + 32];
          let table = reader.open_table(INSCRIPTION_TXID_TO_TX)?;
          let tx_result = table.get(txid_buf)?;

          match tx_result {
            Some(tx_result) => {
              let tx_buf = tx_result.value().to_vec();
              let mut cursor = Cursor::new(tx_buf);
              let tx = bitcoin::Transaction::consensus_decode(&mut cursor)?;
              txs.push(tx);
            }
            None => return Ok(None),
          }
        }

        let parsed_inscription = Inscription::from_transactions(txs);

        match parsed_inscription {
          ParsedInscription::None => return Ok(None),
          ParsedInscription::Partial => return Ok(None),
          ParsedInscription::Complete(inscription) => Ok(Some(inscription)),
        }
      }

      None => return Ok(None),
    }
  }

  pub(crate) fn inscription_exists(&self, inscription_id: InscriptionId) -> Result<bool> {
    Ok(
      self
        .database
        .begin_read()?
        .open_table(INSCRIPTION_ID_TO_SATPOINT)?
        .get(&inscription_id.store())?
        .is_some(),
    )
  }

  pub(crate) fn inscription_count(&self, txid: Txid) -> Result<u32> {
    let start_id = InscriptionId { index: 0, txid };

    let end_id = InscriptionId {
      index: u32::MAX,
      txid,
    };

    Ok(
      self
        .database
        .begin_read()?
        .open_table(INSCRIPTION_ID_TO_SATPOINT)?
        .range::<&InscriptionIdValue>(&start_id.store()..&end_id.store())?
        .count()
        .try_into()?,
    )
  }

  pub(crate) fn get_inscriptions_on_output(
    &self,
    outpoint: OutPoint,
  ) -> Result<Vec<InscriptionId>> {
    Self::inscriptions_on_output(
      &self
        .database
        .begin_read()?
        .open_multimap_table(SATPOINT_TO_SEQUENCE_NUMBER)?,
      &self
        .database
        .begin_read()?
        .open_table(SATPOINT_TO_INSCRIPTION_ID)?,
      outpoint,
    )?
    .into_iter()
    .map(|result| {
      result
        .map(|(_satpoint, inscription_id, _)| inscription_id)
        .map_err(|e| e.into())
    })
    .collect()
  }

  pub(crate) fn get_transaction(&self, txid: Txid) -> Result<Option<Transaction>> {
    if txid == self.genesis_block_coinbase_txid {
      return Ok(Some(self.genesis_block_coinbase_transaction.clone()));
    }

    if self.index_transactions {
      if let Some(transaction) = self
        .database
        .begin_read()?
        .open_table(TRANSACTION_ID_TO_TRANSACTION)?
        .get(&txid.store())?
      {
        return Ok(Some(consensus::encode::deserialize(transaction.value())?));
      }
    }

    if let Ok(tx) = self.client.get_raw_transaction(&txid) {
      Ok(Some(tx))
    } else {
      Ok(None)
    }
  }

  pub(crate) fn get_network(&self) -> Result<Network> {
    Ok(self.chain.network())
  }

  pub(crate) fn get_transaction_blockhash(
    &self,
    txid: Txid,
  ) -> Result<Option<BlockHashAndConfirmations>> {
    if let Ok(result) = self.client.get_raw_transaction_info(&txid) {
      Ok(Some(BlockHashAndConfirmations {
        hash: result.blockhash,
        confirmations: result.confirmations,
      }))
    } else {
      Ok(None)
    }
  }

  pub(crate) fn is_transaction_in_active_chain(&self, txid: Txid) -> Result<bool> {
    Ok(
      self
        .client
        .get_raw_transaction_info(&txid)
        .into_option()?
        .and_then(|info| info.in_active_chain)
        .unwrap_or(false),
    )
  }

  pub(crate) fn find(&self, sat: Sat) -> Result<Option<SatPoint>> {
    let rtx = self.begin_read()?;

    if rtx.block_count()? <= Sat(sat.0).height().n() {
      return Ok(None);
    }

    let outpoint_to_sat_ranges = rtx.0.open_table(OUTPOINT_TO_SAT_RANGES)?;

    for result in outpoint_to_sat_ranges.range::<&[u8; 36]>(&[0; 36]..)? {
      let (key, value) = match result {
        Ok(pair) => pair,
        Err(err) => {
          return Err(err.into());
        }
      };

      let mut offset = 0;
      for chunk in value.value().chunks_exact(24) {
        let (start, end) = SatRange::load(chunk.try_into().unwrap());
        if start <= sat.0 && sat.0 < end {
          return Ok(Some(SatPoint {
            outpoint: Entry::load(*key.value()),
            offset: offset + u64::try_from(sat.0 - start).unwrap(),
          }));
        }
        offset += u64::try_from(end - start).unwrap();
      }
    }

    Ok(None)
  }

  fn list_inner(&self, outpoint: OutPointValue) -> Result<Option<Vec<u8>>> {
    Ok(
      self
        .database
        .begin_read()?
        .open_table(OUTPOINT_TO_SAT_RANGES)?
        .get(&outpoint)?
        .map(|outpoint| outpoint.value().to_vec()),
    )
  }

  pub(crate) fn list(&self, outpoint: OutPoint) -> Result<Option<List>> {
    if !self.index_sats {
      return Ok(None);
    }

    let array = outpoint.store();

    let sat_ranges = self.list_inner(array)?;

    match sat_ranges {
      Some(sat_ranges) => Ok(Some(List::Unspent(
        sat_ranges
          .chunks_exact(24)
          .map(|chunk| SatRange::load(chunk.try_into().unwrap()))
          .collect(),
      ))),
      None => {
        if self.is_transaction_in_active_chain(outpoint.txid)? {
          Ok(Some(List::Spent))
        } else {
          Ok(None)
        }
      }
    }
  }

  pub(crate) fn blocktime(&self, height: Height) -> Result<Blocktime> {
    let height = height.n();

    match self.get_block_by_height(height)? {
      Some(block) => Ok(Blocktime::confirmed(block.header.time)),
      None => {
        let tx = self.database.begin_read()?;

        let current = tx
          .open_table(HEIGHT_TO_BLOCK_HASH)?
          .range(0..)?
          .rev()
          .next()
          .map(|result| match result {
            Ok((height, _hash)) => Some(height.value()),
            Err(_) => None,
          })
          .flatten()
          .unwrap_or(0);

        let expected_blocks = height.checked_sub(current).with_context(|| {
          format!("current {current} height is greater than sat height {height}")
        })?;

        Ok(Blocktime::Expected(
          Utc::now()
            .round_subsecs(0)
            .checked_add_signed(chrono::Duration::seconds(
              10 * 60 * i64::try_from(expected_blocks)?,
            ))
            .ok_or_else(|| anyhow!("block timestamp out of range"))?,
        ))
      }
    }
  }

  pub(crate) fn get_inscriptions(
    &self,
    n: Option<usize>,
  ) -> Result<BTreeMap<SatPoint, InscriptionId>> {
    self
      .database
      .begin_read()?
      .open_table(SATPOINT_TO_INSCRIPTION_ID)?
      .range::<&[u8; 44]>(&[0; 44]..)?
      .map(|result| {
        result
          .map(|(satpoint, id)| (Entry::load(*satpoint.value()), Entry::load(*id.value())))
          .map_err(|e| e.into())
      })
      .take(n.unwrap_or(usize::MAX))
      .collect()
  }

  pub fn get_home_inscriptions(&self) -> Result<Vec<InscriptionId>> {
    Ok(
      self
        .database
        .begin_read()?
        .open_table(HOME_INSCRIPTIONS)?
        .iter()?
        .rev()
        .flat_map(|result| result.map(|(_number, id)| InscriptionId::load(id.value())))
        .collect(),
    )
  }

  pub fn get_inscriptions_paginated(
    &self,
    page_size: u32,
    page_index: u32,
  ) -> Result<(Vec<InscriptionId>, bool)> {
    let rtx = self.database.begin_read()?;

    let sequence_number_to_inscription_entry =
      rtx.open_table(SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY)?;

    let last = sequence_number_to_inscription_entry
      .iter()?
      .next_back()
      .map(|result| result.map(|(number, _entry)| number.value()))
      .transpose()?
      .unwrap_or_default();

    let start = last.saturating_sub(page_size.saturating_mul(page_index));

    let end = start.saturating_sub(page_size);

    let mut inscriptions = sequence_number_to_inscription_entry
      .range(end..=start)?
      .rev()
      .map(|result| result.map(|(_number, entry)| InscriptionEntry::load(entry.value()).id))
      .collect::<Result<Vec<InscriptionId>, StorageError>>()?;

    let more = u32::try_from(inscriptions.len()).unwrap_or(u32::MAX) > page_size;

    if more {
      inscriptions.pop();
    }

    Ok((inscriptions, more))
  }

  pub fn get_feed_inscriptions(&self, n: usize) -> Result<Vec<(u32, InscriptionId)>> {
    Ok(
      self
        .database
        .begin_read()?
        .open_table(SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY)?
        .iter()?
        .rev()
        .take(n)
        .flat_map(|result| {
          result.map(|(number, entry)| (number.value(), InscriptionEntry::load(entry.value()).id))
        })
        .collect(),
    )
  }

  pub fn get_inscription_entry(
    &self,
    inscription_id: InscriptionId,
  ) -> Result<Option<InscriptionEntry>> {
    let rtx = self.database.begin_read()?;

    let Some(sequence_number) = rtx
      .open_table(INSCRIPTION_ID_TO_SEQUENCE_NUMBER)?
      .get(&inscription_id.store())?
      .map(|guard| guard.value())
    else {
      return Ok(None);
    };

    let entry = rtx
      .open_table(SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY)?
      .get(sequence_number)?
      .map(|value| InscriptionEntry::load(value.value()));

    Ok(entry)
  }

  pub fn encode_relic_balance(id: RelicId, balance: u128, buffer: &mut Vec<u8>) {
    relics::varint::encode_to_vec(id.block.into(), buffer);
    relics::varint::encode_to_vec(id.tx.into(), buffer);
    relics::varint::encode_to_vec(balance, buffer);
  }

  pub fn decode_relic_balance(buffer: &[u8]) -> Result<((RelicId, u128), usize)> {
    let mut len = 0;
    let (block, block_len) = relics::varint::decode(&buffer[len..])?;
    len += block_len;
    let (tx, tx_len) = relics::varint::decode(&buffer[len..])?;
    len += tx_len;
    let id = RelicId {
      block: block.try_into()?,
      tx: tx.try_into()?,
    };
    let (balance, balance_len) = relics::varint::decode(&buffer[len..])?;
    len += balance_len;
    Ok(((id, balance), len))
  }

  fn inscriptions_on_output<'a: 'tx, 'tx>(
    satpoint_to_sequence_number: &'a impl ReadableMultimapTable<&'static SatPointValue, u32>,
    satpoint_to_id: &'a impl ReadableTable<&'static SatPointValue, &'static InscriptionIdValue>,
    outpoint: OutPoint,
  ) -> Result<impl Iterator<Item = Result<(SatPoint, InscriptionId, Vec<u32>), StorageError>> + 'tx>
  {
    let start = SatPoint {
      outpoint,
      offset: 0,
    }
    .store();

    let end = SatPoint {
      outpoint,
      offset: u64::MAX,
    }
    .store();

    let satpoint_and_seq_numbers = satpoint_to_sequence_number
      .range::<&[u8; 44]>(&start..=&end)?
      .map(|range| {
        let (satpoint, sequence_numbers) = range?;
        let satpoint = Entry::load(*satpoint.value());
        let seqs = sequence_numbers.into_iter().filter_map(|seq| seq.ok().map(|s| s.value())) // Extract and filter valid `u32` values
        .collect::<Vec<u32>>();
        Ok((satpoint, seqs))
      })
      .collect::<Result<HashMap<SatPoint, Vec<u32>>, anyhow::Error>>()?;

    Ok(
      satpoint_to_id
        .range::<&[u8; 44]>(&start..=&end)?
        .map(move |result| {
          result.and_then(|(satpoint, id)| {
            let satpoint = Entry::load(*satpoint.value());
            let id = Entry::load(*id.value());
            if let Some(seqs) = satpoint_and_seq_numbers.get(&satpoint) {
              // Handle all sequences associated with the satpoint
              Ok((satpoint, id, seqs.clone()))
            } else {
              // Return default for missing entries
              Ok((SatPoint::default(), InscriptionId::default(), vec![]))
            }
          })
        }),
    )
  }
}
