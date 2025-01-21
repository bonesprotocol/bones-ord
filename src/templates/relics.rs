use super::*;
use crate::relics::relic_id::RelicId;
use crate::templates::relic::RelicEntryHtml;

#[derive(Boilerplate, Debug, Serialize, Deserialize)]
pub struct RelicsHtml {
  pub entries: Vec<(RelicId, RelicEntryHtml, Option<InscriptionId>)>,
  pub more: bool,
  pub prev: Option<usize>,
  pub next: Option<usize>,
}

impl PageContent for RelicsHtml {
  fn title(&self) -> String {
    "Bones".to_string()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::relics::relic::Relic;
  use crate::relics::relic_id::RelicId;
  use crate::relics::spaced_relic::SpacedRelic;

  #[test]
  fn display() {
    assert_eq!(
      RelicsHtml {
        entries: vec![(
          RelicId { block: 0, tx: 0 },
          RelicEntryHtml {
            spaced_relic: SpacedRelic {
              relic: Relic(26),
              spacers: 1
            },
            ..default()
          },
          Some(InscriptionId {
            txid: Txid::all_zeros(),
            index: 0,
          })
        )],
        more: false,
        prev: None,
        next: None,
      }
      .to_string(),
      "<h1>Bones</h1>
<ul>
  <li><a href=/bone/A•A>A•A</a></li>
</ul>
<div class=center>
    prev
      next
  </div>"
    );
  }

  #[test]
  fn with_prev_and_next() {
    assert_eq!(
      RelicsHtml {
        entries: vec![
          (
            RelicId { block: 0, tx: 0 },
            RelicEntryHtml {
              spaced_relic: SpacedRelic {
                relic: Relic(0),
                spacers: 0
              },
              ..Default::default()
            },
            Some(InscriptionId {
              txid: Txid::all_zeros(),
              index: 0,
            })
          ),
          (
            RelicId { block: 0, tx: 1 },
            RelicEntryHtml {
              spaced_relic: SpacedRelic {
                relic: Relic(2),
                spacers: 0
              },
              ..Default::default()
            },
            Some(InscriptionId {
              txid: Txid::all_zeros(),
              index: 0,
            })
          )
        ],
        prev: Some(1),
        next: Some(2),
        more: true,
      }
      .to_string(),
      "<h1>Bones</h1>
<ul>
  <li><a href=/bone/A>A</a></li>
  <li><a href=/bone/C>C</a></li>
</ul>
<div class=center>
    <a class=prev href=/bones/1>prev</a>
      <a class=next href=/bones/2>next</a>
  </div>"
    );
  }
}
