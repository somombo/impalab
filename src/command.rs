use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;

/// Holds the executable command and base arguments for a component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandArgs {
  pub command: PathBuf,

  #[serde(default)]
  #[serde(skip_serializing_if = "Vec::is_empty")]
  pub args: Vec<String>,
}
