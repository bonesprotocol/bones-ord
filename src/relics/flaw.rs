use super::*;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RelicFlaw {
  EnshriningAndManifest,
  EnshriningAndSummoning,
  InvalidEnshrining,
  InvalidBaseTokenMint,
  InvalidBaseTokenUnmint,
  InvalidScript,
  InvalidSwap,
  Opcode,
  TrailingIntegers,
  TransferFlag,
  TransferInvalidOrder,
  TransferOutput,
  TransferRelicId,
  TruncatedField,
  UnrecognizedEvenTag,
  UnrecognizedFlag,
  Varint,
}

impl Display for RelicFlaw {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    match self {
      Self::EnshriningAndManifest => write!(f, "enshrining and manifest in one tx"),
      Self::EnshriningAndSummoning => write!(f, "enshrining and summoning in one tx"),
      Self::InvalidEnshrining => write!(f, "invalid enshrining"),
      Self::InvalidBaseTokenMint => write!(
        f,
        "invalid mint: to mint the base token eligible inscriptions must be burned"
      ),
      Self::InvalidBaseTokenUnmint => write!(f, "cannot unmint base token"),
      Self::InvalidScript => write!(f, "invalid script in OP_RETURN"),
      Self::InvalidSwap => write!(f, "invalid swap: input and output cannot be the same Relic"),
      Self::Opcode => write!(f, "non-pushdata opcode in OP_RETURN"),
      Self::TrailingIntegers => write!(f, "trailing integers in body"),
      Self::TransferFlag => write!(f, "unrecognized flag in transfer"),
      Self::TransferInvalidOrder => write!(f, "invalid transfer order"),
      Self::TransferOutput => write!(f, "transfer output greater than transaction output count"),
      Self::TransferRelicId => write!(f, "invalid relic ID in transfer"),
      Self::TruncatedField => write!(f, "field with missing value"),
      Self::UnrecognizedEvenTag => write!(f, "unrecognized even tag"),
      Self::UnrecognizedFlag => write!(f, "unrecognized field"),
      Self::Varint => write!(f, "invalid varint"),
    }
  }
}
