use super::*;

pub(super) struct Message {
  pub(super) flaw: Option<RelicFlaw>,
  pub(super) transfers: Vec<Transfer>,
  pub(super) fields: HashMap<u128, VecDeque<u128>>,
}

impl Message {
  pub(super) fn from_integers(tx: &Transaction, payload: &[u128]) -> Self {
    let mut transfers = Vec::new();
    let mut fields = HashMap::<u128, VecDeque<u128>>::new();
    let mut flaw = None;

    for i in (0..payload.len()).step_by(2) {
      let tag = payload[i];

      if Tag::Body == tag {
        let mut id = RelicId::default();
        for chunk in payload[i + 1..].chunks(4) {
          if chunk.len() != 4 {
            flaw.get_or_insert(RelicFlaw::TrailingIntegers);
            break;
          }

          let Some(next) = id.next(chunk[0], chunk[1]) else {
            flaw.get_or_insert(RelicFlaw::TransferRelicId);
            break;
          };

          let Some(transfer) = Transfer::from_integers(tx, next, chunk[2], chunk[3]) else {
            flaw.get_or_insert(RelicFlaw::TransferOutput);
            break;
          };

          id = next;
          transfers.push(transfer)
        }
        break;
      }

      let Some(&value) = payload.get(i + 1) else {
        flaw.get_or_insert(RelicFlaw::TruncatedField);
        break;
      };

      fields.entry(tag).or_default().push_back(value);
    }

    Self {
      flaw,
      transfers,
      fields,
    }
  }
}
