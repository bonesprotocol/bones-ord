use crate::index::event::{EventInfo, EventWithInscriptionInfo};
use {
  self::{
    deserialize_from_str::DeserializeFromStr,
    error::{OptionExt, ServerError, ServerResult},
  },
  super::*,
  crate::{
    charm::Charm,
    index::{entry::Entry, relics_entry::RelicOwner},
    page_config::PageConfig,
    relics::{RelicId, SpacedRelic},
    subcommand::server::accept_json::AcceptJson,
    templates::{
      relic::RelicHtml, relic_events::RelicEventsHtml, relics::RelicsHtml, sealing::SealingHtml,
      sealings::SealingsHtml, syndicate::SyndicateHtml, syndicates::SyndicatesHtml,
      AddressOutputJson, BlockHtml, BlockJson, HomeHtml, InputHtml, InscriptionByAddressJson,
      InscriptionDecoded, InscriptionDecodedHtml, InscriptionHtml, InscriptionJson,
      InscriptionsHtml, OutputHtml, OutputJson, PageContent, PageHtml, PreviewAudioHtml,
      PreviewImageHtml, PreviewModelHtml, PreviewPdfHtml, PreviewTextHtml, PreviewUnknownHtml,
      PreviewVideoHtml, RangeHtml, RareTxt, SatHtml, ShibescriptionJson, TransactionHtml, Utxo,
    },
  },
  axum::{
    body,
    extract::{Extension, Json, Path, Query},
    headers::UserAgent,
    http::{header, HeaderMap, HeaderValue, StatusCode, Uri},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
    Router, TypedHeader,
  },
  axum_server::Handle,
  http::HeaderName,
  linked_hash_map::LinkedHashMap,
  rayon::prelude::{IntoParallelRefIterator, ParallelIterator},
  rust_embed::RustEmbed,
  rustls::ServerConfig,
  rustls_acme::{
    acme::{LETS_ENCRYPT_PRODUCTION_DIRECTORY, LETS_ENCRYPT_STAGING_DIRECTORY},
    axum::AxumAcceptor,
    caches::DirCache,
    AcmeConfig,
  },
  serde_json::{json, to_string},
  std::collections::HashMap,
  std::{cmp::Ordering, str},
  tokio_stream::StreamExt,
  tower_http::{
    compression::CompressionLayer,
    cors::{Any, CorsLayer},
    set_header::SetResponseHeaderLayer,
  },
};

mod accept_json;
mod error;
pub(crate) mod query;

// Helper function to get transaction details
fn get_transaction_details(
  input: &TxIn,
  index: &Arc<Index>,
  page_config: &Arc<PageConfig>,
) -> (String, String) {
  let txid = input.previous_output.txid;
  let result = if txid
    == Txid::from_str("0000000000000000000000000000000000000000000000000000000000000000").unwrap()
  {
    (String::new(), String::new())
  } else {
    index
      .get_transaction(txid)
      .map(|transaction| {
        transaction
          .map(|t| {
            let value = t
              .output
              .clone()
              .into_iter()
              .nth(input.previous_output.vout as usize)
              .map(|output| output.value.to_string())
              .unwrap_or_else(|| "0".to_string());

            let script_pubkey = t
              .output
              .into_iter()
              .nth(input.previous_output.vout as usize)
              .map(|output| output.script_pubkey)
              .unwrap_or_else(|| Script::new());

            let address = page_config
              .chain
              .address_from_script(&script_pubkey)
              .map(|address| address.to_string())
              .unwrap_or(String::new());

            (value, address)
          })
          .unwrap_or((String::new(), String::new()))
      })
      .unwrap_or((String::new(), String::new()))
  };

  result
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct InscriptionAddressJson {
  pub(crate) inscriptions: Vec<InscriptionByAddressJson>,
  pub(crate) total_inscriptions: usize,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct UtxoAddressJson {
  pub(crate) utxos: Vec<Utxo>,
  pub(crate) total_utxos: usize,
  pub(crate) total_shibes: u128,
  pub(crate) total_inscription_shibes: u128,
}

#[derive(Deserialize)]
struct UtxoBalanceQuery {
  limit: Option<usize>,
  show_all: Option<bool>,
  show_unsafe: Option<bool>,
  value_filter: Option<u64>,
}

#[derive(Deserialize)]
struct OutputsQuery {
  outputs: String,
}

#[derive(Deserialize)]
struct JsonQuery {
  json: Option<bool>,
}

#[derive(Deserialize)]
struct EventsQuery {
  json: Option<bool>,
  show_inscriptions: Option<bool>,
}

enum BlockQuery {
  Height(u32),
  Hash(BlockHash),
}

impl FromStr for BlockQuery {
  type Err = Error;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    Ok(if s.len() == 64 {
      BlockQuery::Hash(s.parse()?)
    } else {
      BlockQuery::Height(s.parse()?)
    })
  }
}

enum SpawnConfig {
  Https(AxumAcceptor),
  Http,
  Redirect(String),
}

#[derive(Deserialize)]
struct InscriptionsByOutputsQuery {
  outputs: String,
}

#[derive(Deserialize)]
struct BlocksQuery {
  no_inscriptions: Option<bool>,
  no_input_data: Option<bool>,
}

#[derive(Deserialize)]
struct InscriptionContentQuery {
  no_content: Option<bool>,
}

#[derive(Deserialize)]
struct ValidityQuery {
  addresses: Option<String>,
  inscription_ids: String,
}

#[derive(Deserialize)]
struct Search {
  query: String,
}

#[derive(RustEmbed)]
#[folder = "static"]
struct StaticAssets;

struct StaticHtml {
  title: &'static str,
  html: &'static str,
}

impl PageContent for StaticHtml {
  fn title(&self) -> String {
    self.title.into()
  }
}

impl Display for StaticHtml {
  fn fmt(&self, f: &mut Formatter) -> fmt::Result {
    f.write_str(self.html)
  }
}

#[derive(Debug, Parser)]
pub(crate) struct Server {
  #[clap(
    long,
    default_value = "0.0.0.0",
    help = "Listen on <ADDRESS> for incoming requests."
  )]
  address: String,
  #[clap(
    long,
    help = "Request ACME TLS certificate for <ACME_DOMAIN>. This ord instance must be reachable at <ACME_DOMAIN>:443 to respond to Let's Encrypt ACME challenges."
  )]
  acme_domain: Vec<String>,
  #[clap(
    long,
    help = "Listen on <HTTP_PORT> for incoming HTTP requests. [default: 80]."
  )]
  http_port: Option<u16>,
  #[clap(
    long,
    group = "port",
    help = "Listen on <HTTPS_PORT> for incoming HTTPS requests. [default: 443]."
  )]
  https_port: Option<u16>,
  #[clap(long, help = "Store ACME TLS certificates in <ACME_CACHE>.")]
  acme_cache: Option<PathBuf>,
  #[clap(long, help = "Provide ACME contact <ACME_CONTACT>.")]
  acme_contact: Vec<String>,
  #[clap(long, help = "Serve HTTP traffic on <HTTP_PORT>.")]
  http: bool,
  #[clap(long, help = "Serve HTTPS traffic on <HTTPS_PORT>.")]
  https: bool,
  #[clap(long, help = "Redirect HTTP traffic to HTTPS.")]
  redirect_http_to_https: bool,
}

impl Server {
  pub(crate) fn run(self, options: Options, index: Arc<Index>, handle: Handle) -> SubcommandResult {
    Runtime::new()?.block_on(async {
      let index_clone = index.clone();

      let index_thread = thread::spawn(move || loop {
        if SHUTTING_DOWN.load(atomic::Ordering::Relaxed) {
          break;
        }
        if let Err(error) = index_clone.update() {
          log::warn!("{error}");
        }
        thread::sleep(Duration::from_millis(5000));
      });
      INDEXER.lock().unwrap().replace(index_thread);

      let config = options.load_config()?;
      let acme_domains = self.acme_domains()?;

      let page_config = Arc::new(PageConfig {
        chain: options.chain(),
        domain: acme_domains.first().cloned(),
        index_sats: index.has_sat_index(),
        csp_origin: options.csp_origin(),
      });

      let router = Router::new()
        .route("/", get(Self::home))
        .route("/block-count", get(Self::block_count))
        .route("/block/:query", get(Self::block))
        .route("/blocks/:query/:endquery", get(Self::blocks))
        .route("/bounties", get(Self::bounties))
        .route("/content/:inscription_id", get(Self::content))
        .route("/faq", get(Self::faq))
        .route("/favicon.ico", get(Self::favicon))
        .route("/feed.xml", get(Self::feed))
        .route("/input/:block/:transaction/:input", get(Self::input))
        .route("/inscription/:inscription_id", get(Self::inscription))
        .route("/inscriptions", get(Self::inscriptions))
        .route("/inscriptions/:from", get(Self::inscriptions_from))
        .route("/shibescription/:inscription_id", get(Self::inscription))
        .route("/shibescriptions", get(Self::inscriptions))
        .route("/shibescriptions/:from", get(Self::inscriptions_from))
        .route(
          "/shibescriptions_on_outputs",
          get(Self::inscriptions_by_outputs),
        )
        .route(
          "/shibescriptions_by_outputs",
          get(Self::shibescriptions_by_outputs),
        )
        .route("/install.sh", get(Self::install_script))
        .route("/ordinal/:sat", get(Self::ordinal))
        .route("/output/:output", get(Self::output))
        .route("/outputs/:output_list", get(Self::outputs))
        .route("/address/:address", get(Self::outputs_by_address))
        .route("/preview/:inscription_id", get(Self::preview))
        .route("/range/:start/:end", get(Self::range))
        .route("/rare.txt", get(Self::rare_txt))
        .route(
          "/utxos/balance/:address",
          get(Self::utxos_by_address_unpaginated),
        )
        .route("/utxos/balance/:address/:page", get(Self::utxos_by_address))
        .route(
          "/inscriptions/balance/:address",
          get(Self::inscriptions_by_address_unpaginated),
        )
        .route(
          "/inscriptions/balance/:address/:page",
          get(Self::inscriptions_by_address),
        )
        .route("/inscriptions/validate", get(Self::inscriptions_validate))
        .route("/sat/:sat", get(Self::sat))
        .route("/search", get(Self::search_by_query))
        .route("/search/*query", get(Self::search_by_path))
        .route("/static/*path", get(Self::static_asset))
        .route("/status", get(Self::status))
        .route("/tx/:txid", get(Self::transaction))
        .route("/events/:block", get(Self::block_events))
        .route("/events", post(Self::tx_events))
        .route("/events/:bone/:page", get(Self::relic_events_paginated))
        .route("/bone/:bone", get(Self::relic))
        .route("/bones", get(Self::relics))
        .route("/bones/:page", get(Self::relics_paginated))
        .route("/bones/balances", get(Self::relics_balances))
        .route("/bones/claimable", get(Self::relics_claimable))
        .route("/tick/:tick", get(Self::sealing_info))
        .route("/tickers/:page", get(Self::sealings_paginated))
        .route("/syndicate/:syndicate", get(Self::syndicate))
        .route("/syndicates", get(Self::syndicates))
        .route("/syndicates/:page", get(Self::syndicates_paginated))
        .route("/bonestones", get(Self::bonestones))
        .route("/bonestones/length", get(Self::bonestones_length))
        .layer(Extension(index))
        .layer(Extension(page_config))
        .layer(Extension(Arc::new(config)))
        .layer(SetResponseHeaderLayer::if_not_present(
          header::CONTENT_SECURITY_POLICY,
          HeaderValue::from_static("default-src 'self'"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
          header::STRICT_TRANSPORT_SECURITY,
          HeaderValue::from_static("max-age=31536000; includeSubDomains; preload"),
        ))
        .layer(
          CorsLayer::new()
            .allow_methods([http::Method::GET])
            .allow_origin(Any),
        )
        .layer(CompressionLayer::new());

      match (self.http_port(), self.https_port()) {
        (Some(http_port), None) => {
          self
            .spawn(router, handle, http_port, SpawnConfig::Http)?
            .await??
        }
        (None, Some(https_port)) => {
          self
            .spawn(
              router,
              handle,
              https_port,
              SpawnConfig::Https(self.acceptor(&options)?),
            )?
            .await??
        }
        (Some(http_port), Some(https_port)) => {
          let http_spawn_config = if self.redirect_http_to_https {
            SpawnConfig::Redirect(if https_port == 443 {
              format!("https://{}", acme_domains[0])
            } else {
              format!("https://{}:{https_port}", acme_domains[0])
            })
          } else {
            SpawnConfig::Http
          };

          let (http_result, https_result) = tokio::join!(
            self.spawn(router.clone(), handle.clone(), http_port, http_spawn_config)?,
            self.spawn(
              router,
              handle,
              https_port,
              SpawnConfig::Https(self.acceptor(&options)?),
            )?
          );
          http_result.and(https_result)??;
        }
        (None, None) => unreachable!(),
      }

      Ok(Box::new(Empty {}) as Box<dyn Output>)
    })
  }

  fn spawn(
    &self,
    router: Router,
    handle: Handle,
    port: u16,
    config: SpawnConfig,
  ) -> Result<task::JoinHandle<io::Result<()>>> {
    let addr = (self.address.as_str(), port)
      .to_socket_addrs()?
      .next()
      .ok_or_else(|| anyhow!("failed to get socket addrs"))?;

    if !integration_test() {
      eprintln!(
        "Listening on {}://{addr}",
        match config {
          SpawnConfig::Https(_) => "https",
          _ => "http",
        }
      );
    }

    Ok(tokio::spawn(async move {
      match config {
        SpawnConfig::Https(acceptor) => {
          axum_server::Server::bind(addr)
            .handle(handle)
            .acceptor(acceptor)
            .serve(router.into_make_service())
            .await
        }
        SpawnConfig::Redirect(destination) => {
          axum_server::Server::bind(addr)
            .handle(handle)
            .serve(
              Router::new()
                .fallback(Self::redirect_http_to_https)
                .layer(Extension(destination))
                .into_make_service(),
            )
            .await
        }
        SpawnConfig::Http => {
          axum_server::Server::bind(addr)
            .handle(handle)
            .serve(router.into_make_service())
            .await
        }
      }
    }))
  }

  fn acme_cache(acme_cache: Option<&PathBuf>, options: &Options) -> Result<PathBuf> {
    let acme_cache = if let Some(acme_cache) = acme_cache {
      acme_cache.clone()
    } else {
      options.data_dir()?.join("acme-cache")
    };

    Ok(acme_cache)
  }

  fn acme_domains(&self) -> Result<Vec<String>> {
    if !self.acme_domain.is_empty() {
      Ok(self.acme_domain.clone())
    } else {
      Ok(vec![System::host_name().expect("Host name not found")])
    }
  }

  fn http_port(&self) -> Option<u16> {
    if self.http || self.http_port.is_some() || (self.https_port.is_none() && !self.https) {
      Some(self.http_port.unwrap_or(80))
    } else {
      None
    }
  }

  fn https_port(&self) -> Option<u16> {
    if self.https || self.https_port.is_some() {
      Some(self.https_port.unwrap_or(443))
    } else {
      None
    }
  }

  fn acceptor(&self, options: &Options) -> Result<AxumAcceptor> {
    let config = AcmeConfig::new(self.acme_domains()?)
      .contact(&self.acme_contact)
      .cache_option(Some(DirCache::new(Self::acme_cache(
        self.acme_cache.as_ref(),
        options,
      )?)))
      .directory(if cfg!(test) {
        LETS_ENCRYPT_STAGING_DIRECTORY
      } else {
        LETS_ENCRYPT_PRODUCTION_DIRECTORY
      });

    let mut state = config.state();

    let acceptor = state.axum_acceptor(Arc::new(
      rustls::ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_cert_resolver(state.resolver()),
    ));

    tokio::spawn(async move {
      while let Some(result) = state.next().await {
        match result {
          Ok(ok) => log::info!("ACME event: {:?}", ok),
          Err(err) => log::error!("ACME error: {:?}", err),
        }
      }
    });

    Ok(acceptor)
  }

  fn index_height(index: &Index) -> ServerResult<Height> {
    index.height()?.ok_or_not_found(|| "genesis block")
  }

  async fn sat(
    Extension(page_config): Extension<Arc<PageConfig>>,
    Extension(index): Extension<Arc<Index>>,
    Path(DeserializeFromStr(sat)): Path<DeserializeFromStr<Sat>>,
  ) -> ServerResult<PageHtml<SatHtml>> {
    let satpoint = index.rare_sat_satpoint(sat)?;
    let mut inscriptions = index.get_inscription_ids_by_sat(sat)?;

    Ok(
      SatHtml {
        sat,
        satpoint,
        blocktime: index.blocktime(sat.height())?,
        inscription: inscriptions.pop(),
      }
      .page(page_config),
    )
  }

  async fn ordinal(Path(sat): Path<String>) -> Redirect {
    Redirect::to(&format!("/sat/{sat}"))
  }

  async fn output(
    Extension(page_config): Extension<Arc<PageConfig>>,
    Extension(index): Extension<Arc<Index>>,
    Path(outpoint): Path<OutPoint>,
  ) -> ServerResult<PageHtml<OutputHtml>> {
    let list = index.list(outpoint)?;

    let output = if outpoint == OutPoint::null() {
      let mut value = 0;

      if let Some(List::Unspent(ranges)) = &list {
        for (start, end) in ranges {
          value += u64::try_from(end - start).unwrap();
        }
      }

      TxOut {
        value,
        script_pubkey: Script::new(),
      }
    } else {
      index
        .get_transaction(outpoint.txid)?
        .ok_or_not_found(|| format!("output {outpoint}"))?
        .output
        .into_iter()
        .nth(outpoint.vout as usize)
        .ok_or_not_found(|| format!("output {outpoint}"))?
    };

    let inscriptions = index.get_inscriptions_on_output(outpoint)?;

    let relics = index.get_relic_balances_for_outpoint(outpoint)?;

    Ok(
      OutputHtml {
        outpoint,
        inscriptions,
        list,
        chain: page_config.chain,
        output,
        relics,
      }
      .page(page_config),
    )
  }

  async fn utxos_by_address(
    Extension(index): Extension<Arc<Index>>,
    Path(params): Path<(String, u32)>,
    Query(query): Query<UtxoBalanceQuery>,
  ) -> ServerResult<Response> {
    Self::get_utxos_by_address(index, params.0, Some(params.1), query).await
  }

  async fn utxos_by_address_unpaginated(
    Extension(index): Extension<Arc<Index>>,
    Path(params): Path<String>,
    Query(query): Query<UtxoBalanceQuery>,
  ) -> ServerResult<Response> {
    Self::get_utxos_by_address(index, params, None, query).await
  }

  async fn get_utxos_by_address(
    index: Arc<Index>,
    address: String,
    page: Option<u32>,
    query: UtxoBalanceQuery,
  ) -> ServerResult<Response> {
    let (address, page) = (address, page.unwrap_or(0));
    let show_all = query.show_all.unwrap_or(false);
    let value_filter = query.value_filter.unwrap_or(0);
    let show_unsafe = query.show_unsafe.unwrap_or(false);

    let items_per_page = query.limit.unwrap_or(10);
    let page = page as usize;
    let start_index = if page == 0 || page == 1 {
      0
    } else {
      (page - 1) * items_per_page + 1
    };
    let mut element_counter = 0;

    let outpoints: Vec<OutPoint> = index.get_account_outputs(address.clone())?;

    let mut utxos = Vec::new();
    let mut total_shibes = 0u128;
    let mut inscription_shibes = 0u128;

    for outpoint in outpoints {
      if !index.get_relic_balances_for_outpoint(outpoint)?.is_empty() {
        continue;
      }
      if !show_all
        && (element_counter < start_index || element_counter > start_index + items_per_page - 1)
      {
        continue;
      }

      let txid = outpoint.txid;
      let vout = outpoint.vout;
      let output = index
        .get_transaction(txid)?
        .ok_or_not_found(|| format!("{txid} current transaction"))?
        .output
        .into_iter()
        .nth(vout.try_into().unwrap())
        .ok_or_not_found(|| format!("{vout} current transaction output"))?;

      if value_filter > 0 && output.value <= value_filter {
        continue;
      }

      if !index.get_inscriptions_on_output(outpoint)?.is_empty() {
        inscription_shibes += output.value as u128;
        if !show_unsafe {
          continue;
        }
      }

      element_counter += 1;

      total_shibes += output.value as u128;

      let confirmations = if let Some(block_hash_info) = index.get_transaction_blockhash(txid)? {
        block_hash_info.confirmations
      } else {
        None
      };

      utxos.push(Utxo {
        txid,
        vout,
        script: output.script_pubkey,
        shibes: output.value,
        confirmations,
      });
    }
    Ok(
      Json(UtxoAddressJson {
        utxos,
        total_shibes,
        total_utxos: element_counter,
        total_inscription_shibes: inscription_shibes,
      })
      .into_response(),
    )
  }

  async fn inscriptions_by_address(
    Extension(index): Extension<Arc<Index>>,
    Path(params): Path<(String, u32)>,
    Query(query): Query<UtxoBalanceQuery>,
  ) -> ServerResult<Response> {
    Self::get_inscriptions_by_address(index, params.0, Some(params.1), query).await
  }

  async fn inscriptions_by_address_unpaginated(
    Extension(index): Extension<Arc<Index>>,
    Path(params): Path<String>,
    Query(query): Query<UtxoBalanceQuery>,
  ) -> ServerResult<Response> {
    Self::get_inscriptions_by_address(index, params, None, query).await
  }

  async fn get_inscriptions_by_address(
    index: Arc<Index>,
    address: String,
    page: Option<u32>,
    query: UtxoBalanceQuery,
  ) -> ServerResult<Response> {
    let (address, page) = (address, page.unwrap_or(0));
    let show_all = query.show_all.unwrap_or(false);
    let value_filter = query.value_filter.unwrap_or(0);

    let items_per_page = query.limit.unwrap_or(10);
    let page = page as usize;
    let start_index = if page == 0 || page == 1 {
      0
    } else {
      (page - 1) * items_per_page + 1
    };
    let mut element_counter = 0;

    let mut all_inscriptions_json = Vec::new();
    let outpoints: Vec<OutPoint> = index.get_account_outputs(address)?;

    for outpoint in outpoints {
      let inscriptions = index.get_inscriptions_on_output(outpoint)?;

      if inscriptions.is_empty() {
        continue;
      }

      element_counter += 1;
      if !show_all
        && (element_counter < start_index || element_counter > start_index + items_per_page - 1)
      {
        continue;
      }

      let txid = outpoint.txid;
      let vout = outpoint.vout;

      let output = index
        .get_transaction(txid)?
        .ok_or_not_found(|| format!("dunes {txid} current transaction"))?
        .output
        .into_iter()
        .nth(vout.try_into().unwrap())
        .ok_or_not_found(|| format!("dunes {vout} current transaction output"))?;
      let shibes = output.value;
      let script = output.script_pubkey;

      if value_filter > 0 && shibes <= value_filter {
        element_counter -= 1;
        continue;
      }

      for inscription_id in inscriptions {
        let inscription = index
          .get_inscription_by_id(inscription_id)?
          .ok_or_not_found(|| format!("inscription {inscription_id}"))?;

        let entry = index
          .get_inscription_entry(inscription_id)?
          .ok_or_not_found(|| format!("inscription {inscription_id}"))?;

        let satpoint = index
          .get_inscription_satpoint_by_id(inscription_id)?
          .ok_or_not_found(|| format!("inscription {inscription_id}"))?;

        let content_type = inscription.content_type().map(|s| s.to_string());
        let content_length = inscription.content_length();
        let content = inscription.into_body();

        let str_content = match (content_type.clone(), content) {
          (Some(ref ct), Some(c))
            if ct.starts_with("application/json") || ct.starts_with("text") =>
          {
            Some(String::from_utf8_lossy(c.as_slice()).to_string())
          }
          (None, Some(c)) => Some(String::from_utf8_lossy(c.as_slice()).to_string()),
          _ => None,
        };

        let confirmations = if let Some(block_hash_info) = index.get_transaction_blockhash(txid)? {
          block_hash_info.confirmations
        } else {
          None
        };

        let inscription_json = InscriptionByAddressJson {
          utxo: Utxo {
            txid,
            vout,
            script: script.clone(),
            shibes,
            confirmations,
          },
          content: str_content,
          content_length,
          content_type,
          genesis_height: entry.height,
          inscription_id,
          inscription_number: entry.inscription_number,
          timestamp: entry.timestamp,
          offset: satpoint.offset,
        };

        all_inscriptions_json.push(inscription_json);
      }
    }
    Ok(
      Json(InscriptionAddressJson {
        inscriptions: all_inscriptions_json,
        total_inscriptions: element_counter,
      })
      .into_response(),
    )
  }

  async fn outputs_by_address(
    Extension(index): Extension<Arc<Index>>,
    Path(address): Path<String>,
  ) -> Result<String, ServerError> {
    let mut outputs = vec![];
    let outpoints = index.get_account_outputs(address)?;

    outputs.push(AddressOutputJson::new(outpoints));

    let outputs_json = to_string(&outputs).context("Failed to serialize outputs")?;

    Ok(outputs_json)
  }

  async fn outputs(
    Extension(server_config): Extension<Arc<PageConfig>>,
    Extension(index): Extension<Arc<Index>>,
    Path(outpoints_str): Path<String>,
    Query(query): Query<InscriptionContentQuery>,
  ) -> Result<String, ServerError> {
    let outpoints: Vec<OutPoint> = outpoints_str
      .split(',')
      .map(|s| {
        OutPoint::from_str(s).map_err(|_| ServerError::BadRequest("outpoint not found".to_string()))
      })
      .collect::<Result<Vec<_>, _>>()?;

    let mut outputs = vec![];
    for outpoint in outpoints {
      let list = index.list(outpoint)?;

      let output = if outpoint == OutPoint::null() {
        let mut value = 0;

        if let Some(List::Unspent(ranges)) = &list {
          for (start, end) in ranges {
            value += u64::try_from(end - start).unwrap();
          }
        }

        TxOut {
          value,
          script_pubkey: Script::new(),
        }
      } else {
        index
          .get_transaction(outpoint.txid)?
          .ok_or_not_found(|| format!("output {outpoint}"))?
          .output
          .into_iter()
          .nth(outpoint.vout as usize)
          .ok_or_not_found(|| format!("output {outpoint}"))?
      };

      let inscription_ids = index.get_inscriptions_on_output(outpoint)?;

      let mut inscriptions: Vec<InscriptionDecodedHtml> = Vec::new();

      for id in inscription_ids {
        let Some(inscription_info) = index.inscription_info(query::Inscription::Id(id), false)?
        else {
          return Err(ServerError::BadRequest(
            "inscription data not found".to_string(),
          ));
        };
        let inscription = inscription_info.2;
        let info = inscription_info.0;
        let entry = inscription_info.3;

        let satpoint = index
          .get_inscription_satpoint_by_id(id)?
          .ok_or_not_found(|| format!("inscription {id}"))?;

        let body = if query.no_content.unwrap_or(false) {
          None
        } else {
          inscription.body.clone()
        };
        let content_type = inscription.content_type().map(|s| s.to_string());
        let delegate = inscription.delegate();
        let parents = inscription.parents();
        let metadata = inscription.metadata();
        let charms = Charm::Vindicated.unset(info.charms.iter().fold(0, |mut acc, charm| {
          charm.set(&mut acc);
          acc
        }));
        let mut charm_icons = Vec::new();
        for charm in Charm::ALL {
          if charm.is_set(charms) {
            charm_icons.push(charm.icon().to_string());
          }
        }

        let inscription_html = InscriptionDecodedHtml {
          chain: server_config.chain,
          genesis_fee: entry.fee,
          genesis_height: entry.height,
          inscription: InscriptionDecoded {
            body,
            content_type,
            delegate,
            metadata,
            parents,
          },
          inscription_id: entry.id,
          inscription_number: entry.inscription_number,
          next: info.next,
          output: output.clone(),
          previous: info.previous,
          sat: entry.sat,
          satpoint,
          timestamp: timestamp(entry.timestamp.into()),
          relic_sealed: info.relic_sealed,
          relic_enshrined: info.relic_enshrined,
          syndicate: info.syndicate,
          charms: charm_icons,
          child_count: info.child_count,
          children: info.children,
        };
        inscriptions.push(inscription_html);
      }

      let relics = index.get_relic_balances_for_outpoint(outpoint)?;

      outputs.push(OutputJson::new(
        server_config.chain,
        inscriptions,
        outpoint,
        output,
        relics,
      ))
    }

    let outputs_json = to_string(&outputs).context("Failed to serialize outputs")?;

    Ok(outputs_json)
  }

  async fn range(
    Extension(page_config): Extension<Arc<PageConfig>>,
    Path((DeserializeFromStr(start), DeserializeFromStr(end))): Path<(
      DeserializeFromStr<Sat>,
      DeserializeFromStr<Sat>,
    )>,
  ) -> ServerResult<PageHtml<RangeHtml>> {
    match start.cmp(&end) {
      Ordering::Equal => Err(ServerError::BadRequest("empty range".to_string())),
      Ordering::Greater => Err(ServerError::BadRequest(
        "range start greater than range end".to_string(),
      )),
      Ordering::Less => Ok(RangeHtml { start, end }.page(page_config)),
    }
  }

  async fn rare_txt(Extension(index): Extension<Arc<Index>>) -> ServerResult<RareTxt> {
    Ok(RareTxt(index.rare_sat_satpoints()?))
  }

  async fn home(
    Extension(page_config): Extension<Arc<PageConfig>>,
    Extension(index): Extension<Arc<Index>>,
  ) -> ServerResult<PageHtml<HomeHtml>> {
    Ok(HomeHtml::new(index.blocks(100)?, index.get_home_inscriptions()?).page(page_config))
  }

  async fn install_script() -> Redirect {
    Redirect::to("https://raw.githubusercontent.com/apezord/ord-dogecoin/master/install.sh")
  }

  async fn block(
    Extension(page_config): Extension<Arc<PageConfig>>,
    Extension(index): Extension<Arc<Index>>,
    Path(DeserializeFromStr(query)): Path<DeserializeFromStr<query::Block>>,
  ) -> ServerResult<PageHtml<BlockHtml>> {
    let (block, height) = match query {
      query::Block::Height(height) => {
        let block = index
          .get_block_by_height(height)?
          .ok_or_not_found(|| format!("block {height}"))?;

        (block, height)
      }
      query::Block::Hash(hash) => {
        let info = index
          .block_header_info(hash)?
          .ok_or_not_found(|| format!("block {hash}"))?;

        let block = index
          .get_block_by_hash(hash)?
          .ok_or_not_found(|| format!("block {hash}"))?;

        (block, u32::try_from(info.height).unwrap())
      }
    };

    // Prepare the inputs_per_tx map
    let inputs_per_tx = block
      .txdata
      .iter()
      .map(|tx| {
        let txid = tx.txid();
        let inputs = tx
          .input
          .iter()
          .map(|input| input.previous_output.to_string())
          .collect::<Vec<_>>()
          .join(",");
        (txid, inputs)
      })
      .collect::<HashMap<_, _>>();

    // Parallelize the processing using Rayon
    let results: Vec<_> = block
      .txdata
      .par_iter()
      .flat_map_iter(|tx| {
        let txid = tx.txid();
        tx.input
          .par_iter()
          .map(|input| get_transaction_details(input, &index, &page_config))
          .map(move |(value, address)| (txid.clone(), value, address))
          .collect::<Vec<_>>()
      })
      .collect();

    // Separate the results into the desired HashMaps
    let input_values_per_tx: HashMap<_, _> = results
      .iter()
      .map(|(txid, value, _)| (txid.clone(), value.clone()))
      .collect();

    let input_addresses_per_tx: HashMap<_, _> = results
      .iter()
      .map(|(txid, _, address)| (txid.clone(), address.clone()))
      .collect();

    // Prepare the outputs_per_tx map
    let outputs_per_tx = block
      .txdata
      .iter()
      .map(|tx| {
        let txid = tx.txid();
        let outputs = tx.output.iter()
            .enumerate()  // Enumerate the iterator to get the index of each output
            .map(|(vout, _output)| {
              let outpoint = OutPoint::new(txid, vout as u32);  // Create the OutPoint from txid and vout
              outpoint.to_string()  // Convert the OutPoint to a string
            })
            .collect::<Vec<_>>()
            .join(",");
        (txid, outputs)
      })
      .collect::<HashMap<_, _>>();

    // Prepare the output values per tx
    let output_values_per_tx = block
      .txdata
      .iter()
      .map(|tx| {
        let txid = tx.txid();
        let output_values = tx
          .output
          .iter()
          .map(|output| output.value.to_string())
          .collect::<Vec<_>>()
          .join(",");
        (txid, output_values)
      })
      .collect::<HashMap<_, _>>();

    let output_addresses_per_tx: HashMap<_, _> = block
      .txdata
      .iter()
      .map(|tx| {
        let txid = tx.txid();
        let addresses = tx
          .output
          .iter()
          .map(|output| {
            page_config
              .chain
              .address_from_script(&output.script_pubkey)
              .map(|address| address.to_string())
              .unwrap_or_else(|_| String::new())
          })
          .collect::<Vec<_>>()
          .join(",");
        (txid, addresses)
      })
      .collect();

    let inscriptions_per_tx: HashMap<_, _> = block
      .txdata
      .iter()
      .filter_map(|tx| {
        let txid = tx.txid();
        match index.get_inscription_by_id(txid.into()) {
          Ok(Some(inscription)) => {
            let inscription_id = InscriptionId::from(txid);
            let content_type = inscription.content_type().map(|s| s.to_string()); // Convert content type to Option<String>
            let content = inscription.into_body();
            Some((txid, (inscription_id, content_type, content)))
          }
          _ => None,
        }
      })
      .collect();

    Ok(
      BlockHtml::new(
        block,
        Height(height),
        Self::index_height(&index)?,
        inputs_per_tx,
        input_values_per_tx,
        input_addresses_per_tx,
        outputs_per_tx,
        output_values_per_tx,
        inscriptions_per_tx,
        output_addresses_per_tx,
      )
      .page(page_config),
    )
  }

  async fn blocks(
    Extension(page_config): Extension<Arc<PageConfig>>,
    Extension(index): Extension<Arc<Index>>,
    Path(path): Path<(u32, u32)>,
    Query(query): Query<BlocksQuery>,
  ) -> Result<String, ServerError> {
    let (height, endheight) = path;
    let mut blocks = vec![];
    for height in height..endheight {
      let block = index
        .get_block_by_height(height)?
        .ok_or_not_found(|| format!("block {}", height))?;

      let txids = block
        .txdata
        .iter()
        .map(|tx| tx.txid().to_string())
        .collect::<Vec<_>>()
        .join(",");

      // Prepare the inputs_per_tx map
      let inputs_per_tx = block
        .txdata
        .iter()
        .map(|tx| {
          let txid = tx.txid();
          let inputs = tx
            .input
            .iter()
            .map(|input| input.previous_output.to_string())
            .collect::<Vec<_>>()
            .join(",");
          (txid, inputs)
        })
        .collect::<HashMap<_, _>>();

      let mut input_values_per_tx: HashMap<_, _> = HashMap::new();
      let mut input_addresses_per_tx: HashMap<_, _> = HashMap::new();

      if !query.no_input_data.unwrap_or(true) {
        // Parallelize the processing using Rayon
        let results: Vec<_> = block
          .txdata
          .par_iter()
          .flat_map_iter(|tx| {
            let txid = tx.txid();
            tx.input
              .par_iter()
              .map(|input| get_transaction_details(input, &index, &page_config))
              .map(move |(value, address)| (txid.clone(), value, address))
              .collect::<Vec<_>>()
          })
          .collect();

        // Separate the results into the desired HashMaps
        input_values_per_tx = results
          .iter()
          .map(|(txid, value, _)| (txid.clone(), value.clone()))
          .collect();

        input_addresses_per_tx = results
          .iter()
          .map(|(txid, _, address)| (txid.clone(), address.clone()))
          .collect();
      }

      // Prepare the outputs_per_tx map
      let outputs_per_tx = block
        .txdata
        .iter()
        .map(|tx| {
          let txid = tx.txid();
          let outputs = tx.output.iter()
            .enumerate()  // Enumerate the iterator to get the index of each output
            .map(|(vout, _output)| {
              let outpoint = OutPoint::new(txid, vout as u32);  // Create the OutPoint from txid and vout
              outpoint.to_string()  // Convert the OutPoint to a string
            })
            .collect::<Vec<_>>()
            .join(",");
          (txid, outputs)
        })
        .collect::<HashMap<_, _>>();

      // Prepare the output values per tx
      let output_values_per_tx = block
        .txdata
        .iter()
        .map(|tx| {
          let txid = tx.txid();
          let output_values = tx
            .output
            .iter()
            .map(|output| output.value.to_string())
            .collect::<Vec<_>>()
            .join(",");
          (txid, output_values)
        })
        .collect::<HashMap<_, _>>();

      let output_addresses_per_tx: HashMap<_, _> = block
        .txdata
        .iter()
        .map(|tx| {
          let txid = tx.txid();
          let addresses = tx
            .output
            .iter()
            .map(|output| {
              page_config
                .chain
                .address_from_script(&output.script_pubkey)
                .map(|address| address.to_string())
                .unwrap_or_else(|_| String::new())
            })
            .collect::<Vec<_>>()
            .join(",");
          (txid, addresses)
        })
        .collect();

      let output_scripts_per_tx: HashMap<_, _> = block
        .txdata
        .iter()
        .map(|tx| {
          let txid = tx.txid();
          let scripts = tx
            .output
            .iter()
            .map(|output| {
              // Convert the byte array to a hexadecimal string.
              // If the byte array is empty, this will result in an empty string.
              hex::encode(&output.script_pubkey)
            })
            .collect::<Vec<_>>()
            .join(",");
          (txid, scripts)
        })
        .collect();

      let inscriptions_per_tx: HashMap<_, _> = if !query.no_inscriptions.unwrap_or_default() {
        block
          .txdata
          .iter()
          .filter_map(|tx| {
            let txid = tx.txid();
            match index.get_inscription_by_id(txid.into()) {
              Ok(Some(inscription)) => {
                let inscription_id = InscriptionId::from(txid);
                let content_type = inscription.content_type().map(|s| s.to_string()); // Convert content type to Option<String>

                // Check if content_type starts with "image" or "video"
                let content = if let Some(ref ct) = content_type {
                  if ct.starts_with("application/json") || ct.starts_with("text") {
                    // If it's an image or video, set content to None
                    None
                  } else {
                    // Otherwise, use the actual content
                    inscription.into_body()
                  }
                } else {
                  // If there's no content type, use the actual content
                  inscription.into_body()
                };

                Some((txid, (inscription_id, content_type, content)))
              }
              _ => None,
            }
          })
          .collect()
      } else {
        HashMap::new()
      };

      blocks.push(BlockJson::new(
        block,
        Height(height).0,
        txids,
        inputs_per_tx,
        input_values_per_tx,
        input_addresses_per_tx,
        outputs_per_tx,
        output_values_per_tx,
        inscriptions_per_tx,
        output_addresses_per_tx,
        output_scripts_per_tx,
      ));
    }

    // This will convert the Vec<BlocksJson> into a JSON string
    let blocks_json = to_string(&blocks).context("Failed to serialize blocks")?;

    Ok(blocks_json)
  }

  async fn transaction(
    Extension(page_config): Extension<Arc<PageConfig>>,
    Extension(index): Extension<Arc<Index>>,
    Path(txid): Path<Txid>,
    Query(query): Query<JsonQuery>,
  ) -> ServerResult<Response> {
    let json = query.json.unwrap_or(false);

    let mut blockhash = None;
    let mut confirmations = None;

    if let Some(block_hash_info) = index.get_transaction_blockhash(txid)? {
      blockhash = block_hash_info.hash;
      confirmations = block_hash_info.confirmations;
    }

    let tx_object = TransactionHtml::new(
      index
        .get_transaction(txid)?
        .ok_or_not_found(|| format!("transaction {txid}"))?,
      blockhash,
      confirmations,
      index.inscription_count(txid)?,
      page_config.chain,
    );

    Ok(if !json {
      tx_object.page(page_config).into_response()
    } else {
      Json(tx_object.to_json()).into_response()
    })
  }

  async fn block_events(
    Extension(index): Extension<Arc<Index>>,
    Query(query): Query<JsonQuery>,
    Path(block_number): Path<u32>,
  ) -> ServerResult<Response> {
    task::block_in_place(|| {
      Ok(if query.json.unwrap_or(false) {
        let block = index
          .get_block_by_height(block_number)?
          .ok_or_not_found(|| format!("block {}", block_number))?;

        let txids = block
          .txdata
          .iter()
          .map(|tx| tx.txid())
          .collect::<Vec<Txid>>();

        let mut response = Vec::new();

        for txid in txids {
          if let Ok(events) = index.events_for_tx(txid) {
            for event in events {
              response.push(event);
            }
          }
        }
        Json(response).into_response()
      } else {
        StatusCode::NOT_FOUND.into_response()
      })
    })
  }

  async fn tx_events(
    Extension(index): Extension<Arc<Index>>,
    Extension(page_config): Extension<Arc<PageConfig>>,
    Query(query): Query<EventsQuery>,
    Json(txids): Json<Vec<Txid>>,
  ) -> ServerResult<Response> {
    task::block_in_place(|| {
      Ok(if query.json.unwrap_or(false) {
        let mut response = Vec::new();
        for txid in txids {
          if let Ok(events) = index.events_for_tx(txid) {
            for event in events {
              if query.show_inscriptions.unwrap_or(false) {
                let mut event_with_inscription = EventWithInscriptionInfo {
                  block_height: event.block_height,
                  event_index: event.event_index,
                  txid: event.txid,
                  inscription: None,
                  info: event.info.clone(),
                };
                match event.info {
                  EventInfo::InscriptionTransferred {
                    inscription_id,
                    new_location,
                    ..
                  } => {
                    let Some(inscription_info) =
                      index.inscription_info(query::Inscription::Id(inscription_id), false)?
                    else {
                      response.push(event_with_inscription);
                      continue;
                    };
                    let mut inscription = inscription_info.2;
                    let info = inscription_info.0;
                    let entry = inscription_info.3;

                    if let Some(delegate) = inscription.delegate() {
                      let delegate_inscription = index
                        .get_inscription_by_id(delegate)?
                        .ok_or_not_found(|| format!("delegate {inscription_id}"))?;
                      inscription.body = Some(Vec::new());
                      inscription.content_type = delegate_inscription.content_type;
                    }

                    let output = index
                      .get_transaction(new_location.outpoint.txid)?
                      .ok_or_not_found(|| {
                        format!("inscription {inscription_id} current transaction")
                      })?
                      .output
                      .into_iter()
                      .nth(new_location.outpoint.vout.try_into().unwrap())
                      .ok_or_not_found(|| {
                        format!("inscription {inscription_id} current transaction output")
                      })?;
                    let mut address: Option<String> = None;

                    match page_config.chain.address_from_script(&output.script_pubkey) {
                      Ok(add) => {
                        address = Some(add.to_string());
                      }
                      Err(_error) => {
                        // do nothing
                      }
                    }
                    let charms =
                      Charm::Vindicated.unset(info.charms.iter().fold(0, |mut acc, charm| {
                        charm.set(&mut acc);
                        acc
                      }));
                    let mut charm_icons = Vec::new();
                    for charm in Charm::ALL {
                      if charm.is_set(charms) {
                        charm_icons.push(charm.icon().to_string());
                      }
                    }

                    event_with_inscription.inscription = Some(ShibescriptionJson {
                      chain: page_config.chain,
                      genesis_fee: entry.fee,
                      genesis_height: entry.height,
                      inscription,
                      inscription_id,
                      next: info.next,
                      inscription_number: entry.inscription_number,
                      output,
                      address,
                      previous: info.previous,
                      sat: entry.sat,
                      satpoint: new_location,
                      timestamp: timestamp(entry.timestamp.into()),
                      relic_sealed: info.relic_sealed,
                      relic_enshrined: info.relic_enshrined,
                      syndicate: info.syndicate,
                      charms: charm_icons,
                      child_count: info.child_count,
                      children: info.children,
                    });
                    response.push(event_with_inscription);
                  }
                  _ => {
                    response.push(event_with_inscription);
                  }
                }
              } else {
                response.push(EventWithInscriptionInfo {
                  block_height: event.block_height,
                  event_index: event.event_index,
                  txid: event.txid,
                  inscription: None,
                  info: event.info,
                });
              }
            }
          }
        }
        Json(response).into_response()
      } else {
        StatusCode::NOT_FOUND.into_response()
      })
    })
  }

  async fn relic_events_paginated(
    Extension(server_config): Extension<Arc<PageConfig>>,
    Extension(index): Extension<Arc<Index>>,
    Path((DeserializeFromStr(relic_query), DeserializeFromStr(page_index))): Path<(
      DeserializeFromStr<query::Relic>,
      DeserializeFromStr<usize>,
    )>,
    Query(query): Query<JsonQuery>,
  ) -> ServerResult<Response> {
    task::block_in_place(|| {
      if !index.has_relic_index() {
        return Err(ServerError::NotFound(
          "this server has no bone index".to_string(),
        ));
      }

      let relic = match relic_query {
        query::Relic::Spaced(spaced_relic) => spaced_relic.relic,
        query::Relic::Id(relic_id) => index
          .get_relic_by_id(relic_id)?
          .ok_or_not_found(|| format!("bone {relic_id}"))?,
        query::Relic::Number(number) => index
          .get_relic_by_number(usize::try_from(number).unwrap())?
          .ok_or_not_found(|| format!("bone number {number}"))?,
      };

      let (_id, entry, _owner) = index
        .relic(relic)?
        .ok_or_not_found(|| format!("bone {relic}"))?;

      let events = index
        .events_for_relic(relic, 1_000, page_index)?
        .ok_or_not_found(|| format!("bone {relic}"))?;

      Ok(if query.json.unwrap_or(false) {
        Json(RelicEventsHtml {
          spaced_relic: entry.spaced_relic,
          events,
        })
        .into_response()
      } else {
        StatusCode::NOT_FOUND.into_response()
      })
    })
  }

  async fn relic(
    Extension(server_config): Extension<Arc<PageConfig>>,
    Extension(index): Extension<Arc<Index>>,
    Path(DeserializeFromStr(relic_query)): Path<DeserializeFromStr<query::Relic>>,
    Query(query): Query<JsonQuery>,
  ) -> ServerResult<Response> {
    task::block_in_place(|| {
      if !index.has_relic_index() {
        return Err(ServerError::NotFound(
          "this server has no bone index".to_string(),
        ));
      }

      let relic = match relic_query {
        query::Relic::Spaced(spaced_relic) => spaced_relic.relic,
        query::Relic::Id(relic_id) => index
          .get_relic_by_id(relic_id)?
          .ok_or_not_found(|| format!("bone {relic_id}"))?,
        query::Relic::Number(number) => index
          .get_relic_by_number(usize::try_from(number).unwrap())?
          .ok_or_not_found(|| format!("bone number {number}"))?,
      };

      let (id, entry, owner) = index
        .relic(relic)?
        .ok_or_not_found(|| format!("bone {relic}"))?;

      if entry.enshrining != Txid::all_zeros() {
        let enshrining_txid = entry.enshrining;

        let transaction = index
          .get_transaction(enshrining_txid)?
          .ok_or_not_found(|| format!("transaction {enshrining_txid}"))?;

        let mut thumb: Option<InscriptionId> = None;

        for (vout, output) in transaction.output.iter().enumerate() {
          let outpoint = OutPoint::new(enshrining_txid, vout as u32);

          let inscriptions = index.get_inscriptions_on_output(outpoint)?;

          if let Some(inscription_id) = inscriptions.first() {
            thumb = Some(*inscription_id);
            break;
          }
        }

        let mintable = entry.mintable(u128::MAX).is_ok();

        return Ok(if query.json.unwrap_or(false) {
          Json(RelicHtml {
            entry: entry.into(),
            id,
            mintable,
            owner,
            thumb,
          })
          .into_response()
        } else {
          RelicHtml {
            entry: entry.into(),
            id,
            mintable,
            owner,
            thumb,
          }
          .page(server_config)
          .into_response()
        });
      }

      let mintable = entry.mintable(u128::MAX).is_ok();

      Ok(if query.json.unwrap_or(false) {
        Json(RelicHtml {
          entry: entry.into(),
          id,
          mintable,
          owner,
          thumb: None,
        })
        .into_response()
      } else {
        RelicHtml {
          entry: entry.into(),
          id,
          mintable,
          owner,
          thumb: None,
        }
        .page(server_config)
        .into_response()
      })
    })
  }

  async fn relics(
    Extension(server_config): Extension<Arc<PageConfig>>,
    Extension(index): Extension<Arc<Index>>,
    Query(query): Query<JsonQuery>,
  ) -> ServerResult<Response> {
    Self::relics_paginated(
      Extension(server_config),
      Extension(index),
      Path(0),
      Query(query),
    )
    .await
  }

  async fn relics_paginated(
    Extension(server_config): Extension<Arc<PageConfig>>,
    Extension(index): Extension<Arc<Index>>,
    Path(page_index): Path<usize>,
    Query(query): Query<JsonQuery>,
  ) -> ServerResult<Response> {
    task::block_in_place(|| {
      let (entries, more) = index.relics_paginated(50, page_index)?;

      let prev = page_index.checked_sub(1);
      let next = more.then_some(page_index + 1);

      let entries = entries
        .into_iter()
        .map(|(id, entry, inscription_id)| (id, entry.into(), inscription_id))
        .collect();

      Ok(if query.json.unwrap_or(false) {
        Json(RelicsHtml {
          entries,
          more,
          prev,
          next,
        })
        .into_response()
      } else {
        RelicsHtml {
          entries,
          more,
          prev,
          next,
        }
        .page(server_config)
        .into_response()
      })
    })
  }

  async fn sealing_info(
    Extension(server_config): Extension<Arc<PageConfig>>,
    Extension(index): Extension<Arc<Index>>,
    Path(DeserializeFromStr(relic_query)): Path<DeserializeFromStr<query::Relic>>,
    Query(query): Query<JsonQuery>,
  ) -> ServerResult<Response> {
    // Offload blocking DB operations
    task::block_in_place(|| {
      let relic = match relic_query {
        query::Relic::Spaced(spaced_relic) => spaced_relic.relic,
        query::Relic::Id(relic_id) => index
          .get_relic_by_id(relic_id)?
          .ok_or_not_found(|| format!("tick {relic_id}"))?,
        query::Relic::Number(number) => index
          .get_relic_by_number(usize::try_from(number).unwrap())?
          .ok_or_not_found(|| format!("tick number {number}"))?,
      };

      let entry = index.sealing(relic)?;
      let inscription = if let Some(inscription) = entry.0 {
        inscription
      } else {
        return Err(ServerError::BadRequest(format!("tick {relic} not found")));
      };
      let enshrining_tx = entry.1;
      // Decide on JSON or HTML
      Ok(if query.json.unwrap_or(false) {
        // Return raw JSON
        Json(SealingHtml {
          inscription,
          enshrining_tx,
        })
        .into_response()
      } else {
        // Return HTML
        SealingHtml {
          inscription,
          enshrining_tx,
        }
        .page(server_config)
        .into_response()
      })
    })
  }

  async fn sealings_paginated(
    Extension(server_config): Extension<Arc<PageConfig>>,
    Extension(index): Extension<Arc<Index>>,
    Path(page_index): Path<usize>,
    Query(query): Query<JsonQuery>,
  ) -> ServerResult<Response> {
    // Offload blocking DB operations
    task::block_in_place(|| {
      // page_size here is 50 — adjust as you wish
      let (entries, more) = index.sealings_paginated(50, page_index)?;

      let prev = page_index.checked_sub(1);
      let next = more.then_some(page_index + 1);

      // Decide on JSON or HTML
      Ok(if query.json.unwrap_or(false) {
        // Return raw JSON
        Json(SealingsHtml {
          entries,
          more,
          prev,
          next,
        })
        .into_response()
      } else {
        // Return HTML
        SealingsHtml {
          entries,
          more,
          prev,
          next,
        }
        .page(server_config)
        .into_response()
      })
    })
  }

  async fn relics_balances(
    Extension(index): Extension<Arc<Index>>,
    Query(query): Query<JsonQuery>,
  ) -> ServerResult<Response> {
    task::block_in_place(|| {
      Ok(if query.json.unwrap_or(false) {
        Json(
          index
            .get_relic_balance_map()?
            .into_iter()
            .map(|(relic, balances)| {
              (
                relic,
                balances
                  .into_iter()
                  .map(|(outpoint, pile)| (outpoint, pile.amount))
                  .collect(),
              )
            })
            .collect::<BTreeMap<SpacedRelic, BTreeMap<OutPoint, u128>>>(),
        )
        .into_response()
      } else {
        StatusCode::NOT_FOUND.into_response()
      })
    })
  }

  async fn relics_claimable(
    Extension(index): Extension<Arc<Index>>,
    Query(query): Query<JsonQuery>,
  ) -> ServerResult<Response> {
    task::block_in_place(|| {
      Ok(if query.json.unwrap_or(false) {
        Json(
          index
            .get_relic_claimable()?
            .into_iter()
            .collect::<BTreeMap<RelicOwner, u128>>(),
        )
        .into_response()
      } else {
        StatusCode::NOT_FOUND.into_response()
      })
    })
  }

  async fn syndicate(
    Extension(server_config): Extension<Arc<PageConfig>>,
    Extension(index): Extension<Arc<Index>>,
    Path(DeserializeFromStr(syndicate_query)): Path<DeserializeFromStr<query::Syndicate>>,
    Query(query): Query<JsonQuery>,
  ) -> ServerResult<Response> {
    task::block_in_place(|| {
      if !index.has_relic_index() {
        return Err(ServerError::NotFound(
          "this server has no relic index".to_string(),
        ));
      }

      let syndicate_id = match syndicate_query {
        query::Syndicate::Id(id) => id,
        query::Syndicate::Inscription(id) => {
          let (info, _txout, _inscription, _) = index
            .inscription_info(query::Inscription::Id(id), true)?
            .ok_or_not_found(|| format!("inscription {id}"))?;
          info
            .syndicate
            .ok_or_not_found(|| format!("syndicate on inscription {id}"))?
        }
      };

      let (id, entry, owner) = index
        .syndicate(syndicate_id)?
        .ok_or_not_found(|| format!("syndicate {syndicate_id}"))?;

      let relic = index.get_relic_by_id(entry.treasure)?.unwrap();

      let (_, treasure, _) = index
        .relic(relic)?
        .ok_or_not_found(|| format!("relic {relic}"))?;

      let chestable = entry.chestable(index.block_count()?.into()).is_ok();
      let response = SyndicateHtml {
        entry: entry.into(),
        id,
        chestable,
        owner,
        treasure: treasure.into(),
      };

      Ok(if query.json.unwrap_or(false) {
        Json(response).into_response()
      } else {
        response.page(server_config).into_response()
      })
    })
  }

  async fn syndicates(
    Extension(server_config): Extension<Arc<PageConfig>>,
    Extension(index): Extension<Arc<Index>>,
    Query(query): Query<JsonQuery>,
  ) -> ServerResult<Response> {
    Self::syndicates_paginated(
      Extension(server_config),
      Extension(index),
      Path(0),
      Query(query),
    )
    .await
  }

  async fn syndicates_paginated(
    Extension(server_config): Extension<Arc<PageConfig>>,
    Extension(index): Extension<Arc<Index>>,
    Path(page_index): Path<usize>,
    Query(query): Query<JsonQuery>,
  ) -> ServerResult<Response> {
    task::block_in_place(|| {
      let (entries, more) = index.syndicates_paginated(50, page_index)?;

      let prev = page_index.checked_sub(1);

      let next = more.then_some(page_index + 1);

      let entries = entries
        .into_iter()
        .map(|(id, entry)| (id, entry.into()))
        .collect();

      Ok(if query.json.unwrap_or(false) {
        Json(SyndicatesHtml {
          entries,
          more,
          prev,
          next,
        })
        .into_response()
      } else {
        SyndicatesHtml {
          entries,
          more,
          prev,
          next,
        }
        .page(server_config)
        .into_response()
      })
    })
  }

  async fn bonestones(
    Extension(index): Extension<Arc<Index>>,
    Extension(server_config): Extension<Arc<PageConfig>>,
  ) -> Result<Response, ServerError> {
    let mut result: Vec<(InscriptionId, u32)> = Vec::new();
    if let Ok(res) = index.get_all_bonestones_inscription_ids() {
      for pairs in res {
        let id = pairs.0;
        let block_height = pairs.1;
        result.push((id, block_height));
      }
    }
    Ok(Json(json!({"bonestones": result})).into_response())
  }

  async fn bonestones_length(Extension(index): Extension<Arc<Index>>) -> ServerResult<Json<usize>> {
    let bonestones = index.get_all_bonestones_inscription_ids()?;
    Ok(Json(bonestones.len()))
  }

  async fn status(Extension(index): Extension<Arc<Index>>) -> (StatusCode, &'static str) {
    if index.is_unrecoverably_reorged() {
      (
        StatusCode::OK,
        "unrecoverable reorg detected, please rebuild the database.",
      )
    } else {
      (
        StatusCode::OK,
        StatusCode::OK.canonical_reason().unwrap_or_default(),
      )
    }
  }

  async fn search_by_query(
    Extension(index): Extension<Arc<Index>>,
    Query(search): Query<Search>,
  ) -> ServerResult<Redirect> {
    Self::search(&index, &search.query).await
  }

  async fn search_by_path(
    Extension(index): Extension<Arc<Index>>,
    Path(search): Path<Search>,
  ) -> ServerResult<Redirect> {
    Self::search(&index, &search.query).await
  }

  async fn search(index: &Index, query: &str) -> ServerResult<Redirect> {
    Self::search_inner(index, query)
  }

  fn search_inner(index: &Index, query: &str) -> ServerResult<Redirect> {
    lazy_static! {
      static ref HASH: Regex = Regex::new(r"^[[:xdigit:]]{64}$").unwrap();
      static ref OUTPOINT: Regex = Regex::new(r"^[[:xdigit:]]{64}:\d+$").unwrap();
      static ref INSCRIPTION_ID: Regex = Regex::new(r"^[[:xdigit:]]{64}i\d+$").unwrap();
      static ref RELIC: Regex = Regex::new(r"^[A-Z•.]+$").unwrap();
      static ref RELIC_ID: Regex = Regex::new(r"^[0-9]+:[0-9]+$").unwrap();
    }

    let query = query.trim();

    if HASH.is_match(query) {
      if index.block_header(query.parse().unwrap())?.is_some() {
        Ok(Redirect::to(&format!("/block/{query}")))
      } else {
        Ok(Redirect::to(&format!("/tx/{query}")))
      }
    } else if OUTPOINT.is_match(query) {
      Ok(Redirect::to(&format!("/output/{query}")))
    } else if INSCRIPTION_ID.is_match(query) {
      Ok(Redirect::to(&format!("/shibescription/{query}")))
    } else if RELIC.is_match(query) {
      Ok(Redirect::to(&format!("/relic/{query}")))
    } else if RELIC_ID.is_match(query) {
      let id = query
        .parse::<RelicId>()
        .map_err(|err| ServerError::BadRequest(err.to_string()))?;

      let relic = index.get_relic_by_id(id)?.ok_or_not_found(|| "relic ID")?;

      Ok(Redirect::to(&format!("/relic/{relic}")))
    } else {
      Ok(Redirect::to(&format!("/sat/{query}")))
    }
  }

  async fn favicon(user_agent: Option<TypedHeader<UserAgent>>) -> ServerResult<Response> {
    if user_agent
      .map(|user_agent| {
        user_agent.as_str().contains("Safari/")
          && !user_agent.as_str().contains("Chrome/")
          && !user_agent.as_str().contains("Chromium/")
      })
      .unwrap_or_default()
    {
      Ok(
        Self::static_asset(Path("/favicon.png".to_string()))
          .await
          .into_response(),
      )
    } else {
      Ok(
        (
          [(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("default-src 'unsafe-inline'"),
          )],
          Self::static_asset(Path("/favicon.svg".to_string())).await?,
        )
          .into_response(),
      )
    }
  }

  async fn feed(
    Extension(page_config): Extension<Arc<PageConfig>>,
    Extension(index): Extension<Arc<Index>>,
  ) -> ServerResult<Response> {
    let mut builder = rss::ChannelBuilder::default();

    let chain = page_config.chain;
    match chain {
      Chain::Mainnet => builder.title("Shibescriptions"),
      _ => builder.title(format!("Shibescriptions – {chain:?}")),
    };

    builder.generator(Some("ord".to_string()));

    for (number, id) in index.get_feed_inscriptions(300)? {
      builder.item(
        rss::ItemBuilder::default()
          .title(format!("Shibescription {number}"))
          .link(format!("/shibescription/{id}"))
          .guid(Some(rss::Guid {
            value: format!("/shibescription/{id}"),
            permalink: true,
          }))
          .build(),
      );
    }

    Ok(
      (
        [
          (header::CONTENT_TYPE, "application/rss+xml"),
          (
            header::CONTENT_SECURITY_POLICY,
            "default-src 'unsafe-inline'",
          ),
        ],
        builder.build().to_string(),
      )
        .into_response(),
    )
  }

  async fn static_asset(Path(path): Path<String>) -> ServerResult<Response> {
    let content = StaticAssets::get(if let Some(stripped) = path.strip_prefix('/') {
      stripped
    } else {
      &path
    })
    .ok_or_not_found(|| format!("asset {path}"))?;
    let body = body::boxed(body::Full::from(content.data));
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    Ok(
      Response::builder()
        .header(header::CONTENT_TYPE, mime.as_ref())
        .body(body)
        .unwrap(),
    )
  }

  async fn block_count(Extension(index): Extension<Arc<Index>>) -> ServerResult<String> {
    Ok(index.block_count()?.to_string())
  }

  async fn input(
    Extension(page_config): Extension<Arc<PageConfig>>,
    Extension(index): Extension<Arc<Index>>,
    Path(path): Path<(u32, usize, usize)>,
  ) -> Result<PageHtml<InputHtml>, ServerError> {
    let not_found = || format!("input /{}/{}/{}", path.0, path.1, path.2);

    let block = index
      .get_block_by_height(path.0)?
      .ok_or_not_found(not_found)?;

    let transaction = block
      .txdata
      .into_iter()
      .nth(path.1)
      .ok_or_not_found(not_found)?;

    let input = transaction
      .input
      .into_iter()
      .nth(path.2)
      .ok_or_not_found(not_found)?;

    Ok(InputHtml { path, input }.page(page_config))
  }

  async fn faq() -> Redirect {
    Redirect::to("https://docs.ordinals.com/faq/")
  }

  async fn bounties() -> Redirect {
    Redirect::to("https://docs.ordinals.com/bounty/")
  }

  async fn content(
    Extension(index): Extension<Arc<Index>>,
    Extension(config): Extension<Arc<Config>>,
    Path(inscription_id): Path<InscriptionId>,
    Extension(page_config): Extension<Arc<PageConfig>>,
  ) -> ServerResult<Response> {
    if config.is_hidden(inscription_id) {
      return Ok(PreviewUnknownHtml.into_response());
    }

    let mut inscription = index
      .get_inscription_by_id(inscription_id)?
      .ok_or_not_found(|| format!("inscription {inscription_id}"))?;

    if let Some(delegate) = inscription.delegate() {
      inscription = index
        .get_inscription_by_id(delegate)?
        .ok_or_not_found(|| format!("delegate {inscription_id}"))?
    }

    Ok(
      Self::content_response(inscription, &page_config)
        .ok_or_not_found(|| format!("inscription {inscription_id} content"))?
        .into_response(),
    )
  }

  fn content_response(
    inscription: Inscription,
    page_config: &PageConfig,
  ) -> Option<(HeaderMap, Vec<u8>)> {
    let mut headers = HeaderMap::new();
    match &page_config.csp_origin {
      None => {
        headers.insert(
          header::CONTENT_SECURITY_POLICY,
          HeaderValue::from_static("default-src 'self' 'unsafe-eval' 'unsafe-inline' data: blob:"),
        );
        headers.append(
          header::CONTENT_SECURITY_POLICY,
          HeaderValue::from_static("default-src *:*/content/ *:*/blockheight *:*/blockhash *:*/blockhash/ *:*/blocktime *:*/r/ 'unsafe-eval' 'unsafe-inline' data: blob:"),
        );
      }
      Some(origin) => {
        let csp = format!("default-src {origin}/content/ {origin}/blockheight {origin}/blockhash {origin}/blockhash/ {origin}/blocktime {origin}/r/ 'unsafe-eval' 'unsafe-inline' data: blob:");
        headers.insert(
          header::CONTENT_SECURITY_POLICY,
          HeaderValue::from_str(&csp)
            .map_err(|err| ServerError::Internal(Error::from(err)))
            .ok()?,
        );
      }
    }
    headers.insert(
      header::CACHE_CONTROL,
      HeaderValue::from_static("max-age=31536000, immutable"),
    );
    headers.insert(
      header::CONTENT_TYPE,
      inscription
        .content_type()
        .and_then(|content_type| content_type.parse().ok())
        .unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );

    Some((headers, inscription.into_body()?))
  }

  pub(super) fn preview_content_security_policy(
    media: Media,
    csp: &Option<String>,
  ) -> ServerResult<[(HeaderName, HeaderValue); 1]> {
    let default = match media {
      Media::Audio => "default-src 'self'",
      Media::Image => "default-src 'self' 'unsafe-inline'",
      Media::Model => "script-src-elem 'self' https://ajax.googleapis.com",
      Media::Pdf => "script-src-elem 'self' https://cdn.jsdelivr.net",
      Media::Text => "default-src 'self'",
      Media::Unknown => "default-src 'self'",
      Media::Video => "default-src 'self'",
      _ => "",
    };

    let value = if let Some(csp_origin) = &csp {
      default
        .replace("'self'", csp_origin)
        .parse()
        .map_err(|err| anyhow!("invalid content-security-policy origin `{csp_origin}`: {err}"))?
    } else {
      HeaderValue::from_static(default)
    };

    Ok([(header::CONTENT_SECURITY_POLICY, value)])
  }

  async fn preview(
    Extension(index): Extension<Arc<Index>>,
    Extension(config): Extension<Arc<Config>>,
    Extension(page_config): Extension<Arc<PageConfig>>,
    Path(inscription_id): Path<InscriptionId>,
  ) -> ServerResult<Response> {
    if config.is_hidden(inscription_id) {
      return Ok(PreviewUnknownHtml.into_response());
    }

    let mut inscription = index
      .get_inscription_by_id(inscription_id)?
      .ok_or_not_found(|| format!("inscription {inscription_id}"))?;

    if let Some(delegate) = inscription.delegate() {
      inscription = index
        .get_inscription_by_id(delegate)?
        .ok_or_not_found(|| format!("delegate {inscription_id}"))?
    }

    let media = inscription.media();
    let content_security_policy =
      Self::preview_content_security_policy(media, &page_config.csp_origin)?;

    match media {
      Media::Audio => Ok(PreviewAudioHtml { inscription_id }.into_response()),
      Media::Iframe => Ok(
        Self::content_response(inscription, &page_config)
          .ok_or_not_found(|| format!("inscription {inscription_id} content"))?
          .into_response(),
      ),
      Media::Model => {
        Ok((content_security_policy, PreviewModelHtml { inscription_id }).into_response())
      }
      Media::Image => {
        Ok((content_security_policy, PreviewImageHtml { inscription_id }).into_response())
      }
      Media::Pdf => {
        Ok((content_security_policy, PreviewPdfHtml { inscription_id }).into_response())
      }
      Media::Text => {
        Ok((content_security_policy, PreviewTextHtml { inscription_id }).into_response())
      }
      Media::Unknown => Ok((content_security_policy, PreviewUnknownHtml).into_response()),
      Media::Video => {
        Ok((content_security_policy, PreviewVideoHtml { inscription_id }).into_response())
      }
    }
  }

  async fn inscription(
    Extension(page_config): Extension<Arc<PageConfig>>,
    Extension(index): Extension<Arc<Index>>,
    Path(inscription_id): Path<InscriptionId>,
    Query(query): Query<JsonQuery>,
  ) -> ServerResult<Response> {
    let Some(inscription_info) =
      index.inscription_info(query::Inscription::Id(inscription_id), false)?
    else {
      return Err(ServerError::BadRequest(
        "inscription data not found".to_string(),
      ));
    };
    let mut inscription = inscription_info.2;
    let info = inscription_info.0;
    let entry = inscription_info.3;

    if let Some(delegate) = inscription.delegate() {
      let delegate_inscription = index
        .get_inscription_by_id(delegate)?
        .ok_or_not_found(|| format!("delegate {inscription_id}"))?;
      inscription.body = Some(Vec::new());
      inscription.content_type = delegate_inscription.content_type;
    }

    let satpoint = index
      .get_inscription_satpoint_by_id(inscription_id)?
      .ok_or_not_found(|| format!("inscription {inscription_id}"))?;

    let output = index
      .get_transaction(satpoint.outpoint.txid)?
      .ok_or_not_found(|| format!("inscription {inscription_id} current transaction"))?
      .output
      .into_iter()
      .nth(satpoint.outpoint.vout.try_into().unwrap())
      .ok_or_not_found(|| format!("inscription {inscription_id} current transaction output"))?;

    if !query.json.unwrap_or_default() {
      Ok(
        InscriptionHtml {
          chain: page_config.chain,
          genesis_fee: entry.fee,
          genesis_height: entry.height,
          inscription,
          inscription_id,
          next: info.next,
          inscription_number: entry.inscription_number,
          output,
          previous: info.previous,
          sat: entry.sat,
          satpoint,
          timestamp: timestamp(entry.timestamp.into()),
          relic_sealed: info.relic_sealed,
          relic_enshrined: info.relic_enshrined,
          syndicate: info.syndicate,
          charms: Charm::Vindicated.unset(info.charms.iter().fold(0, |mut acc, charm| {
            charm.set(&mut acc);
            acc
          })),
          child_count: info.child_count,
          children: info.children,
        }
        .page(page_config)
        .into_response(),
      )
    } else {
      let mut address: Option<String> = None;

      match page_config.chain.address_from_script(&output.script_pubkey) {
        Ok(add) => {
          address = Some(add.to_string());
        }
        Err(_error) => {
          // do nothing
        }
      }
      let charms = Charm::Vindicated.unset(info.charms.iter().fold(0, |mut acc, charm| {
        charm.set(&mut acc);
        acc
      }));
      let mut charm_icons = Vec::new();
      for charm in Charm::ALL {
        if charm.is_set(charms) {
          charm_icons.push(charm.icon().to_string());
        }
      }

      Ok(
        Json(ShibescriptionJson {
          chain: page_config.chain,
          genesis_fee: entry.fee,
          genesis_height: entry.height,
          inscription,
          inscription_id,
          next: info.next,
          inscription_number: entry.inscription_number,
          output,
          address,
          previous: info.previous,
          sat: entry.sat,
          satpoint,
          timestamp: timestamp(entry.timestamp.into()),
          relic_sealed: info.relic_sealed,
          relic_enshrined: info.relic_enshrined,
          syndicate: info.syndicate,
          charms: charm_icons,
          child_count: info.child_count,
          children: info.children,
        })
        .into_response(),
      )
    }
  }

  async fn inscriptions(
    Extension(page_config): Extension<Arc<PageConfig>>,
    Extension(index): Extension<Arc<Index>>,
  ) -> ServerResult<PageHtml<InscriptionsHtml>> {
    Self::inscriptions_inner(page_config, index, None).await
  }

  async fn inscriptions_validate(
    Extension(index): Extension<Arc<Index>>,
    Extension(server_config): Extension<Arc<PageConfig>>,
    Query(query): Query<ValidityQuery>,
  ) -> Result<Response, ServerError> {
    let inscription_ids: Vec<&str> = query.inscription_ids.split(',').collect();

    let mut validate_response: HashMap<InscriptionId, bool> = HashMap::new();

    if let Some(address_string) = query.addresses {
      let addresses: Vec<&str> = address_string.split(',').collect();

      for (id, address_str) in inscription_ids.iter().zip(addresses.iter()) {
        let inscription_id =
          InscriptionId::from_str(id).map_err(|err| ServerError::BadRequest(err.to_string()))?;

        let satpoint = index
          .get_inscription_satpoint_by_id(inscription_id)?
          .ok_or_not_found(|| format!("inscription {inscription_id}"))?;

        let output = index
          .get_transaction(satpoint.outpoint.txid)?
          .ok_or_not_found(|| format!("inscription {inscription_id} current transaction"))?
          .output
          .into_iter()
          .nth(satpoint.outpoint.vout.try_into().unwrap())
          .ok_or_not_found(|| format!("inscription {inscription_id} current transaction output"))?;

        let address = Address::from_str(address_str);
        let address_to_compare =
          Address::from_script(&output.script_pubkey, server_config.chain.network());

        if address.is_ok() && address_to_compare.is_ok() {
          if address.unwrap().to_string() == address_to_compare.unwrap().to_string() {
            validate_response.insert(inscription_id, true);
          } else {
            validate_response.insert(inscription_id, false);
          }
        } else {
          validate_response.insert(inscription_id, false);
        }
      }
    }

    Ok(Json(validate_response).into_response())
  }

  async fn shibescriptions_by_outputs(
    Extension(index): Extension<Arc<Index>>,
    Query(query): Query<OutputsQuery>,
  ) -> ServerResult<Response> {
    let mut all_inscription_jsons = Vec::new();

    // Split the outputs string into individual outputs
    let outputs = query.outputs.split(',');

    for output in outputs {
      // Split the output into tx_id and vout
      let parts: Vec<&str> = output.split(':').collect();
      if parts.len() != 2 {
        return Err(ServerError::BadRequest("wrong output format".to_string()));
      }

      let tx_id = Txid::from_str(parts[0])
        .map_err(|_| ServerError::BadRequest("wrong tx id format".to_string()))?;
      let vout = parts[1]
        .parse::<u32>()
        .map_err(|_| ServerError::BadRequest("wrong vout format".to_string()))?;

      // Create OutPoint
      let outpoint = OutPoint::new(tx_id, vout);

      // Query the index for inscriptions on this OutPoint
      let inscriptions = index.get_inscriptions_on_output(outpoint)?;

      let output = index
        .get_transaction(outpoint.txid)?
        .ok_or_not_found(|| format!("inscription {tx_id} current transaction"))?
        .output
        .into_iter()
        .nth(outpoint.vout.try_into().unwrap())
        .ok_or_not_found(|| format!("inscription {vout} current transaction output"))?;

      for inscription_id in inscriptions {
        let inscription = index
          .get_inscription_by_id(inscription_id)?
          .ok_or_not_found(|| format!("inscription {inscription_id}"))?;

        let entry = index
          .get_inscription_entry(inscription_id)?
          .ok_or_not_found(|| format!("inscription {inscription_id}"))?;

        let satpoint = index
          .get_inscription_satpoint_by_id(inscription_id)?
          .ok_or_not_found(|| format!("inscription {inscription_id}"))?;

        let content_type = inscription.content_type().map(|s| s.to_string());
        let content_length = inscription.content_length();
        let content = inscription.into_body();

        let str_content = match (content_type.clone(), content) {
          (Some(ref ct), Some(c))
            if ct.starts_with("application/json") || ct.starts_with("text") =>
          {
            Some(String::from_utf8_lossy(c.as_slice()).to_string())
          }
          (None, Some(c)) => Some(String::from_utf8_lossy(c.as_slice()).to_string()),
          _ => None,
        };

        let confirmations =
          if let Some(block_hash_info) = index.get_transaction_blockhash(outpoint.txid)? {
            block_hash_info.confirmations
          } else {
            None
          };

        let inscription_json = InscriptionByAddressJson {
          utxo: Utxo {
            txid: tx_id,
            vout,
            script: output.script_pubkey.clone(),
            shibes: output.value,
            confirmations,
          },
          content: str_content,
          content_length,
          content_type,
          genesis_height: entry.height,
          inscription_id,
          inscription_number: entry.inscription_number,
          timestamp: entry.timestamp,
          offset: satpoint.offset,
        };

        all_inscription_jsons.push(inscription_json);
      }
    }

    // Build your response
    Ok(Json(all_inscription_jsons).into_response())
  }

  async fn inscriptions_by_outputs(
    Extension(index): Extension<Arc<Index>>,
    Query(query): Query<OutputsQuery>,
  ) -> ServerResult<Response> {
    let mut all_inscription_jsons = Vec::new();

    // Split the outputs string into individual outputs
    let outputs = query.outputs.split(',');

    for output in outputs {
      // Split the output into tx_id and vout
      let parts: Vec<&str> = output.split(':').collect();
      if parts.len() != 2 {
        return Err(ServerError::BadRequest("wrong output format".to_string()));
      }

      let tx_id = Txid::from_str(parts[0])
        .map_err(|_| ServerError::BadRequest("wrong tx id format".to_string()))?;
      let vout = parts[1]
        .parse::<u32>()
        .map_err(|_| ServerError::BadRequest("wrong vout format".to_string()))?;

      // Create OutPoint
      let outpoint = OutPoint::new(tx_id, vout);

      // Query the index for inscriptions on this OutPoint
      let inscriptions = index.get_inscriptions_on_output(outpoint)?;

      for inscription_id in inscriptions {
        let inscription = index
          .get_inscription_by_id(inscription_id)?
          .ok_or_not_found(|| format!("inscription {inscription_id}"))?;

        let entry = index
          .get_inscription_entry(inscription_id)?
          .ok_or_not_found(|| format!("inscription {inscription_id}"))?;

        let content_type = inscription.content_type().map(|s| s.to_string());
        let content_length = inscription.content_length();
        let content = inscription.into_body();

        let str_content = if let Some(ref ct) = content_type {
          if ct.starts_with("application/json") || ct.starts_with("text") {
            content
          } else {
            // Otherwise, don't serve it
            None
          }
        } else {
          // If there's no content type, use the actual content
          content
        };

        let inscription_json = InscriptionJson {
          content: str_content,
          content_length,
          content_type,
          genesis_height: entry.height,
          inscription_id,
          inscription_number: entry.inscription_number,
          timestamp: entry.timestamp,
          tx_id: tx_id.to_string(),
          vout,
        };

        all_inscription_jsons.push(inscription_json);
      }
    }

    // Build your response
    Ok(Json(all_inscription_jsons).into_response())
  }

  async fn inscriptions_from(
    Extension(page_config): Extension<Arc<PageConfig>>,
    Extension(index): Extension<Arc<Index>>,
    Path(from): Path<u32>,
  ) -> ServerResult<PageHtml<InscriptionsHtml>> {
    Self::inscriptions_inner(page_config, index, Some(from)).await
  }

  async fn inscriptions_inner(
    page_config: Arc<PageConfig>,
    index: Arc<Index>,
    from: Option<u32>,
  ) -> ServerResult<PageHtml<InscriptionsHtml>> {
    let from = from.unwrap_or(0);

    let (inscriptions, more) = index.get_inscriptions_paginated(100, from)?;

    let prev = from.checked_sub(1);

    let next = more.then_some(from + 1);

    Ok(
      InscriptionsHtml {
        inscriptions,
        next,
        prev,
      }
      .page(page_config),
    )
  }

  async fn redirect_http_to_https(
    Extension(mut destination): Extension<String>,
    uri: Uri,
  ) -> Redirect {
    if let Some(path_and_query) = uri.path_and_query() {
      destination.push_str(path_and_query.as_str());
    }

    Redirect::to(&destination)
  }
}

// Helper function to process inscriptions and create InscriptionJson
async fn process_inscriptions(
  index: &Index,
  inscription_ids: &[InscriptionId],
  tx_id: &Txid,
  vout: u32,
) -> ServerResult<Vec<InscriptionJson>> {
  let mut inscriptions_json = Vec::new();

  for inscription_id in inscription_ids {
    let inscription = index
      .get_inscription_by_id(*inscription_id)?
      .ok_or_not_found(|| format!("inscription {}", inscription_id))?;

    let entry = index
      .get_inscription_entry(*inscription_id)?
      .ok_or_not_found(|| format!("inscription {}", inscription_id))?;

    let content_type = inscription.content_type().map(|s| s.to_string());
    let content_length = inscription.content_length();
    let content = inscription.into_body();

    let str_content = if let Some(ref ct) = content_type {
      if ct.starts_with("application/json") || ct.starts_with("text") {
        content
      } else {
        None
      }
    } else {
      content
    };

    let inscription_json = InscriptionJson {
      content: str_content,
      content_length,
      content_type,
      genesis_height: entry.height,
      inscription_id: *inscription_id,
      inscription_number: entry.inscription_number,
      timestamp: entry.timestamp,
      tx_id: tx_id.to_string(),
      vout,
    };

    inscriptions_json.push(inscription_json);
  }

  Ok(inscriptions_json)
}

fn format_balance(balance: u128, decimal_places: u8) -> String {
  let factor = 10u128.pow(decimal_places as u32);
  let integer_part = balance / factor; // Get the integer part
  let fractional_part = balance % factor; // Get the fractional part

  // If balance is zero or the fractional part is zero, return just the integer part
  if fractional_part == 0 {
    return format!("{}", integer_part);
  }

  // Format the fractional part, trimming trailing zeros
  let mut fractional_string = format!(
    "{:0>width$}",
    fractional_part,
    width = decimal_places as usize
  );

  // Remove trailing zeros from the fractional part
  while fractional_string.ends_with('0') {
    fractional_string.pop();
  }

  // Combine integer and cleaned-up fractional part
  format!("{}.{}", integer_part, fractional_string)
}
