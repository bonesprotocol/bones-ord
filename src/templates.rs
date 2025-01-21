use {super::*, boilerplate::Boilerplate};
pub(crate) use {
  block::BlockHashAndConfirmations,
  block::BlockHtml,
  block::BlockJson,
  home::HomeHtml,
  iframe::Iframe,
  input::InputHtml,
  inscription::{
    InscriptionByAddressJson, InscriptionDecoded, InscriptionDecodedHtml, InscriptionHtml,
    InscriptionJson, RelicShibescriptionJson, ShibescriptionJson,
  },
  inscriptions::InscriptionsHtml,
  metadata::MetadataHtml,
  output::{AddressOutputJson, OutputCompactJson, OutputHtml, OutputJson},
  page_config::PageConfig,
  preview::{
    PreviewAudioHtml, PreviewImageHtml, PreviewModelHtml, PreviewPdfHtml, PreviewTextHtml,
    PreviewUnknownHtml, PreviewVideoHtml,
  },
  range::RangeHtml,
  rare::RareTxt,
  sat::SatHtml,
  transaction::TransactionHtml,
  utxo::Utxo,
};

mod block;
mod home;
mod iframe;
mod input;
mod inscription;
mod inscriptions;
pub(crate) mod metadata;
mod output;
mod preview;
mod range;
mod rare;
pub(crate) mod relic;
pub(crate) mod relic_events;
pub(crate) mod relics;
mod sat;
pub(crate) mod sealing;
pub(crate) mod sealings;
pub(crate) mod syndicate;
pub(crate) mod syndicates;
mod transaction;
mod utxo;

#[derive(Boilerplate)]
pub(crate) struct PageHtml<T: PageContent> {
  content: T,
  config: Arc<PageConfig>,
}

impl<T> PageHtml<T>
where
  T: PageContent,
{
  pub(crate) fn new(content: T, config: Arc<PageConfig>) -> Self {
    Self { content, config }
  }

  fn og_image(&self) -> String {
    if let Some(domain) = &self.config.domain {
      format!("https://{domain}/static/favicon.png")
    } else {
      "https://ordinals.com/static/favicon.png".into()
    }
  }

  fn superscript(&self) -> String {
    if self.config.chain == Chain::Mainnet {
      "alpha".into()
    } else {
      self.config.chain.to_string()
    }
  }
}

pub(crate) trait PageContent: Display + 'static {
  fn title(&self) -> String;

  fn page(self, page_config: Arc<PageConfig>) -> PageHtml<Self>
  where
    Self: Sized,
  {
    PageHtml::new(self, page_config)
  }

  fn preview_image_url(&self) -> Option<Trusted<String>> {
    None
  }
}
