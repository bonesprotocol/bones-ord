use super::*;

#[derive(Boilerplate)]
pub(crate) struct TransactionHtml {
  blockhash: Option<BlockHash>,
  confirmations: Option<u32>,
  chain: Chain,
  inscription_count: u32,
  transaction: Transaction,
  txid: Txid,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct TransactionJson {
  blockhash: Option<BlockHash>,
  confirmations: Option<u32>,
  chain: Chain,
  inscription_count: u32,
  transaction: Transaction,
  txid: Txid,
}

impl TransactionHtml {
  pub(crate) fn new(
    transaction: Transaction,
    blockhash: Option<BlockHash>,
    confirmations: Option<u32>,
    inscription_count: u32,
    chain: Chain,
  ) -> Self {
    Self {
      txid: transaction.txid(),
      blockhash,
      confirmations,
      chain,
      inscription_count,
      transaction,
    }
  }

  pub(crate) fn to_json(&self) -> TransactionJson {
    TransactionJson {
      blockhash: self.blockhash.clone(),
      confirmations: self.confirmations,
      chain: self.chain.clone(),
      inscription_count: self.inscription_count.clone(),
      transaction: self.transaction.clone(),
      txid: self.txid.clone(),
    }
  }
}

impl PageContent for TransactionHtml {
  fn title(&self) -> String {
    format!("Transaction {}", self.txid)
  }
}
