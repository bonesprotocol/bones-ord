use super::*;

#[derive(Default, Serialize, Deserialize, Debug, PartialEq, Copy, Clone, Eq)]
pub struct Manifest {
  /// used to create a manifest tree
  pub left_parent: Option<u128>,
  /// used to create a manifest tree
  pub right_parent: Option<u128>,
}

impl Manifest {
  pub fn creation_fee(self) -> u128 {
    21 * 10u128.pow(Enshrining::DIVISIBILITY.into())
  }

  pub fn validate_title(title: &str) -> Result<(), anyhow::Error> {
    let title = title.trim();
    if title.is_empty() {
      Err(anyhow!("Manifest title is empty"))
    } else if !title.chars().all(|c| c.is_ascii_uppercase() || c == '_') {
      Err(anyhow!(
        "Manifest title must only contain upper case letters and underscores"
      ))
    } else {
      Ok(())
    }
  }
}
