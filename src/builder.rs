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
use crate::command::CommandArgs;
use crate::error::BuildError;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Output;

#[derive(Debug, Deserialize)]
struct ComponentConfig {
  name: String,
  #[serde(rename = "type")]
  component_type: ComponentType,
  language: Option<String>,
  build: Option<BuildStep>,
  run: CommandArgs,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum ComponentType {
  Generator,
  Algorithm,
}

#[derive(Debug, Deserialize)]
struct BuildStep {
  command: String,
  args: Vec<String>,
}

/// Defines the structure of the `impa_manifest.json` file.
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct BuildManifest {
  /// A map of generator names to their runnable `CommandArgs`.
  pub generators: HashMap<String, CommandArgs>,

  /// A map of language names to their runnable `CommandArgs`.
  pub algorithm_executables: HashMap<String, CommandArgs>,
}

/// Scans a directory for components and runs their build steps.
///
/// This function finds all `impafile.toml` files in the `components_dir`,
/// runs their optional `[build]` steps, and generates a manifest file
/// at `manifest_out`.
pub async fn build_components(
  components_dir: PathBuf,
  manifest_out: PathBuf,
) -> Result<(), BuildError> {
  tracing::info!("Scanning for components in {}", components_dir.display());

  if !components_dir.exists() {
    return Err(BuildError::ComponentsDirNotFound(components_dir));
  }

  let mut manifest = BuildManifest::default();

  for entry in fs::read_dir(&components_dir).map_err(BuildError::ReadDir)? {
    let entry = entry.map_err(BuildError::ReadDir)?;
    let path = entry.path();

    if path.is_dir() {
      let config_path = path.join("impafile.toml");
      if config_path.exists() {
        process_component(&path, &config_path, &mut manifest).await?;
      }
    }
  }

  let json = serde_json::to_string_pretty(&manifest).map_err(BuildError::SerializeManifest)?;
  fs::write(&manifest_out, json).map_err(BuildError::WriteManifest)?;
  tracing::info!("Build manifest written to {}", manifest_out.display());

  Ok(())
}

async fn process_component(
  base_dir: &Path,
  config_path: &Path,
  manifest: &mut BuildManifest,
) -> Result<(), BuildError> {
  let content = fs::read_to_string(config_path).map_err(BuildError::ReadConfig)?;
  let config: ComponentConfig = toml::from_str(&content).map_err(BuildError::TomlParse)?;

  // Run optional build step
  if let Some(build_step) = &config.build {
    tracing::info!(
      "Building component: {} ({:?})",
      config.name,
      config.component_type
    );

    let Output {
      status,
      stdout,
      stderr,
    } = Command::new(&build_step.command)
      .args(&build_step.args)
      .current_dir(base_dir)
      .output()
      .map_err(|e| BuildError::BuildCommandExecFailed {
        component_name: config.name.clone(),
        source: e,
      })?;

    if !status.success() {
      let stderr = String::from_utf8_lossy(&stderr).to_string();
      let stdout = String::from_utf8_lossy(&stdout).to_string();

      return Err(BuildError::BuildCommandFailed {
        component_name: config.name,
        stdout,
        stderr,
      });
    }
  } else {
    tracing::info!("No build step for {}. Skipping.", config.name);
  }

  // Resolve paths in run command
  let mut run_command = config.run;

  // Check if command is a relative path to an existing file
  let potential_cmd_path = base_dir.join(&run_command.command);
  if potential_cmd_path.exists() && potential_cmd_path.is_file() {
    run_command.command =
      potential_cmd_path
        .canonicalize()
        .map_err(|e| BuildError::CanonicalizePath {
          component_name: config.name.clone(),
          path: potential_cmd_path,
          source: e,
        })?;
  }

  // Check args for relative paths
  let mut resolved_args = Vec::new();
  for arg in run_command.args {
    let potential_arg_path = base_dir.join(&arg);
    if potential_arg_path.exists() {
      resolved_args.push(
        potential_arg_path
          .canonicalize()
          .map_err(|e| BuildError::CanonicalizePath {
            component_name: config.name.clone(),
            path: potential_arg_path,
            source: e,
          })?
          .to_string_lossy()
          .to_string(),
      );
    } else {
      resolved_args.push(arg);
    }
  }
  run_command.args = resolved_args;

  // Store in manifest
  match config.component_type {
    ComponentType::Generator => {
      manifest.generators.insert(config.name, run_command);
    }
    ComponentType::Algorithm => {
      if let Some(lang) = config.language {
        manifest.algorithm_executables.insert(lang, run_command);
      } else {
        tracing::warn!(
          "Algorithm component '{}' missing 'language' field. Skipping registration.",
          config.name
        );
      }
    }
  }

  Ok(())
}
