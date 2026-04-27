// Copyright 2025 Chisomo Makombo Sakala
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(rename_all = "lowercase")]
pub enum ComponentType {
  Generator,
  Executor,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ManifestComponent {
  #[serde(rename = "type")]
  pub component_type: ComponentType,

  #[serde(flatten)]
  pub run: CommandArgs,
}

/// Holds the executable command and base arguments for a component.
///
/// This struct is the "contract" for a runnable component, stored
/// in the `impa_manifest.json` and used by the orchestrator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandArgs {
  /// The command to execute (e.g., "python3" or "/path/to/binary").
  pub command: PathBuf,

  /// A list of base arguments to pass to the command (e.g., ["./run.py"]).
  #[serde(default)]
  #[serde(skip_serializing_if = "Vec::is_empty")]
  pub args: Vec<String>,

  #[serde(default)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub working_dir: Option<PathBuf>,
}

pub type ComponentCommandMap = HashMap<String, ManifestComponent>;

/// Defines the structure of the `impa_manifest.json` file.
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct BuildManifest {
  /// A map of language names to their runnable `ManifestComponent`.
  pub components: ComponentCommandMap,
}
