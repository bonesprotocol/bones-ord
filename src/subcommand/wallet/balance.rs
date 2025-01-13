use {super::*, crate::wallet::Wallet, std::collections::BTreeSet};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Output {
  pub cardinal: u64,
  pub ordinal: u64,
  pub total: u64,
}

pub(crate) fn run(options: Options) -> SubcommandResult {
  let index = Index::open(&options)?;
  index.update()?;

  let unspent_outputs = index.get_unspent_outputs(Wallet::load(&options)?)?;

  let inscription_outputs = index
    .get_inscriptions(None)?
    .keys()
    .map(|satpoint| satpoint.outpoint)
    .collect::<BTreeSet<OutPoint>>();

  let mut cardinal = 0;
  let mut ordinal = 0;
  for (outpoint, amount) in unspent_outputs {
    if inscription_outputs.contains(&outpoint) {
      ordinal += amount.to_sat();
    } else {
      cardinal += amount.to_sat();
    }
  }

  Ok(Box::new(Output {
    cardinal,
    ordinal,
    total: cardinal + ordinal,
  }))
}
