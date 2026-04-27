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
use crate::cli::FilterArgs;
use crate::cli::ManifestArgs;
use crate::error::BuildError;
use crate::manifest::BuildManifest;
use crate::manifest::CommandArgs;
use crate::manifest::ComponentType;
use crate::manifest::ManifestComponent;
use serde::Deserialize;
use std::collections::hash_map::Entry;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Output;

/// Scans a directory for components and runs their build steps.
///
/// This function finds all `impafile.toml` files in the `components_dir`,
/// runs their optional `[build]` steps, and generates a manifest file
/// at `manifest_out`.
pub fn build_components(
  components_dir: PathBuf,
  manifest_arg: ManifestArgs,
  filter_args: &FilterArgs,
) -> Result<(), BuildError> {
  let manifest_out: PathBuf = manifest_arg.get_path();
  tracing::info!("Scanning for components in {}", components_dir.display());

  if !components_dir.exists() {
    return Err(BuildError::ComponentsDirNotFound(components_dir));
  }

  let mut manifest = BuildManifest::default();

  for entry in fs::read_dir(&components_dir).map_err(BuildError::ReadDir)? {
    let entry = entry.map_err(BuildError::ReadDir)?;
    let path: PathBuf = entry.path();

    if path.is_dir() {
      let config_path = path.join("impafile.toml");
      if config_path.exists() && config_path.is_file() {
        let path_canon: PathBuf =
          path
            .canonicalize()
            .map_err(|e| BuildError::CanonicalizePath {
              path: path.clone(),
              source: e,
            })?;

        process_component(&manifest_arg, &path_canon, &mut manifest, filter_args)?;
      }
    }
  }

  let json = serde_json::to_string_pretty(&manifest).map_err(BuildError::SerializeManifest)?;
  fs::write(&manifest_out, json).map_err(BuildError::WriteManifest)?;
  tracing::info!("Build manifest written to {}", manifest_out.display());

  Ok(())
}

fn process_component(
  manifest_arg: &ManifestArgs,
  base_dir: &Path,
  manifest: &mut BuildManifest,
  filter_args: &FilterArgs,
) -> Result<(), BuildError> {
  let content =
    fs::read_to_string(base_dir.join("impafile.toml")).map_err(BuildError::ReadConfig)?;

  #[derive(Debug, Deserialize)]
  struct ConfigComponent {
    name: String,
    #[serde(rename = "type")]
    component_type: ComponentType,
    build: Option<CommandArgs>,
    run: CommandArgs,
  }
  #[derive(Debug, Deserialize)]
  struct Impafile {
    components: Vec<ConfigComponent>,
  }
  let impafile: Impafile = toml::from_str(&content).map_err(BuildError::TomlParse)?;

  for config in impafile.components {
    if let Some(es) = &filter_args.exclude
      && es.contains(&config.name)
    {
      continue;
    } else if let Some(is) = &filter_args.include
      && !is.contains(&config.name)
    {
      continue;
    }

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

    match manifest.components.entry(config.name) {
      Entry::Occupied(entry) => {
        return Err(BuildError::DuplicateComponentName {
          component_name: entry.key().to_owned(),
        });
      }
      Entry::Vacant(entry) => {
        let manifest_dir: PathBuf =
          manifest_arg
            .root_dir
            .canonicalize()
            .map_err(|e| BuildError::CanonicalizePath {
              path: manifest_arg.get_path(),
              source: e,
            })?;

        let cmp_relpath = pathdiff::diff_paths(base_dir, &manifest_dir)
          .ok_or_else(|| BuildError::PathDiff(base_dir.to_owned(), manifest_dir))?;

        // Store in manifest
        entry.insert(ManifestComponent {
          component_type: config.component_type,
          run: CommandArgs {
            working_dir: Some(cmp_relpath),
            ..config.run
          },
        });
      }
    }
  }

  Ok(())
}
