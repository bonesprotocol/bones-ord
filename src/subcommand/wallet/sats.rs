use {super::*, crate::wallet::Wallet};

#[derive(Debug, Parser)]
pub(crate) struct Sats {
  #[clap(
    long,
    help = "Find satoshis listed in first column of tab-separated value file <TSV>."
  )]
  tsv: Option<PathBuf>,
}

#[derive(Serialize, Deserialize)]
pub struct OutputTsv {
  pub sat: String,
  pub output: OutPoint,
}

#[derive(Serialize, Deserialize)]
pub struct OutputRare {
  pub sat: Sat,
  pub output: OutPoint,
  pub offset: u64,
  pub rarity: Rarity,
}

impl Sats {
  pub(crate) fn run(&self, options: Options) -> SubcommandResult {
    let index = Index::open(&options)?;

    if !index.has_sat_index() {
      bail!("sats requires index created with `--index-sats` flag");
    }

    index.update()?;

    let utxos = index.get_unspent_output_ranges(Wallet::load(&options)?)?;

    if let Some(path) = &self.tsv {
      let mut output = Vec::new();
      for (outpoint, sat) in sats_from_tsv(
        utxos,
        &fs::read_to_string(path)
          .with_context(|| format!("I/O error reading `{}`", path.display()))?,
      )? {
        output.push(OutputTsv {
          sat: sat.into(),
          output: outpoint,
        });
      }
      Ok(Box::new(output))
    } else {
      let mut output = Vec::new();
      for (outpoint, sat, offset, rarity) in rare_sats(utxos) {
        output.push(OutputRare {
          sat,
          output: outpoint,
          offset,
          rarity,
        });
      }
      Ok(Box::new(output))
    }
  }
}

fn rare_sats(utxos: Vec<(OutPoint, Vec<(u64, u64)>)>) -> Vec<(OutPoint, Sat, u64, Rarity)> {
  utxos
    .into_iter()
    .flat_map(|(outpoint, sat_ranges)| {
      let mut offset = 0;
      sat_ranges.into_iter().filter_map(move |(start, end)| {
        let sat = Sat(start);
        let rarity = sat.rarity();
        let start_offset = offset;
        offset += u64::try_from(end - start).unwrap();
        if rarity > Rarity::Common {
          Some((outpoint, sat, start_offset, rarity))
        } else {
          None
        }
      })
    })
    .collect()
}

fn sats_from_tsv(
  utxos: Vec<(OutPoint, Vec<(u64, u64)>)>,
  tsv: &str,
) -> Result<Vec<(OutPoint, &str)>> {
  let mut needles = Vec::new();
  for (i, line) in tsv.lines().enumerate() {
    if line.is_empty() || line.starts_with('#') {
      continue;
    }

    if let Some(value) = line.split('\t').next() {
      let sat = Sat::from_str(value).map_err(|err| {
        anyhow!(
          "failed to parse sat from string \"{value}\" on line {}: {err}",
          i + 1,
        )
      })?;

      needles.push((sat, value));
    }
  }
  needles.sort();

  let mut haystacks = utxos
    .into_iter()
    .flat_map(|(outpoint, ranges)| {
      ranges
        .into_iter()
        .map(move |(start, end)| (start, end, outpoint))
    })
    .collect::<Vec<(u64, u64, OutPoint)>>();
  haystacks.sort();

  let mut i = 0;
  let mut j = 0;
  let mut results = Vec::new();
  while i < needles.len() && j < haystacks.len() {
    let (needle, value) = needles[i];
    let (start, end, outpoint) = haystacks[j];

    if needle >= start && needle < end {
      results.push((outpoint, value));
    }

    if needle >= end {
      j += 1;
    } else {
      i += 1;
    }
  }

  Ok(results)
}
