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
use crate::cli::FileReader;
use crate::cli::GenArgs;
use crate::cli::ManifestArgs;
use crate::error::BenchmarkError;
use crate::error::ConfigError;
use crate::manifest::BuildManifest;
use crate::manifest::ComponentCommandMap;
use crate::manifest::ComponentType;
use crate::manifest::ManifestComponent;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

pub struct RootedManifest {
  root_dir: PathBuf,
  manifest: BuildManifest,
}

impl RootedManifest {
  fn new(root_dir: PathBuf, manifest: BuildManifest) -> Self {
    Self { root_dir, manifest }
  }

  /// Implements the logic for resolving the generator path.
  pub fn resolve_generator(
    &self,
    data_gen: Option<DataGen>,
  ) -> Result<Option<ManifestComponent>, BenchmarkError> {
    let Some(DataGen {
      generator_name,
      args: passthrough_args,
      seed,
    }) = data_gen
    else {
      return Ok(None);
    };

    let mut cmd = self.resolve_component(&generator_name, ComponentType::Generator)?;

    cmd.run.args.push(format!("--seed={}", seed));
    cmd.run.args.extend(passthrough_args.to_owned());

    Ok(Some(cmd))
  }

  /// A component name was provided. Find its base command.
  fn resolve_component(
    &self,
    component_name: &str,
    component_type: ComponentType,
  ) -> Result<ManifestComponent, BenchmarkError> {
    // Priority 2: Build Manifest (clones CommandArgs)
    let Some(cmp) = self.manifest.components.get(component_name) else {
      // Priority 3: Fail
      return Err(BenchmarkError::ComponentNotFound {
        component_name: component_name.to_owned(),
        available: self
          .manifest
          .components
          .iter()
          .filter(|(_, c)| c.component_type == component_type)
          .map(|(k, _)| k.to_owned())
          .collect(),
      });
    };

    tracing::debug!(
      "Using `{:?}` command from manifest '{}'",
      component_type,
      component_name
    );
    if component_type != cmp.component_type {
      return Err(BenchmarkError::IncorrectComponentType {
        component_name: component_name.to_owned(),
        component_type,
      });
    }

    let mut cmp = cmp.clone();
    cmp.dir = self.root_dir.join(&cmp.dir);
    Ok(cmp)
  }

  /// Implements the logic for resolving all required executor paths.
  pub fn resolve_executor(&self, task: &Task) -> Result<ManifestComponent, BenchmarkError> {
    let mut cmd = self.resolve_component(&task.executor_name, ComponentType::Executor)?;
    cmd.run.args.reserve(1 + task.kwargs.len());
    cmd.run.args.push(task.target.to_owned());
    cmd
      .run
      .args
      .extend(task.kwargs.iter().map(|(k, v)| format!("--{k}={v}")));

    Ok(cmd)
  }
}

#[derive(Debug, Deserialize, Serialize, Clone, Hash)]
pub struct Task {
  #[serde(rename = "executor")]
  pub executor_name: String,
  pub target: String,

  #[serde(default, rename = "args")]
  pub kwargs: BTreeMap<String, String>,
}

/// Type alias for the list of executor tasks to run
pub type Tasks = Vec<Task>;

#[derive(Debug, Deserialize)]
pub struct DataGen {
  pub generator_name: String,
  pub seed: u64,
  pub args: Vec<String>,
}

impl From<GenArgs> for Option<DataGen> {
  fn from(
    GenArgs {
      name,
      seed,
      trailing_args,
    }: GenArgs,
  ) -> Self {
    if &name != "none" {
      let seed = seed.unwrap_or_else(rand::random);
      tracing::info!(seed, "Using generator seed");

      Some(DataGen {
        generator_name: name,
        seed,
        args: trailing_args,
      })
    } else {
      if !trailing_args.is_empty() {
        tracing::warn!("--generator=none is set, so trailing generator arguments will be ignored.");
      }
      if seed.is_some() {
        tracing::warn!("--generator=none is set, so --seed will be ignored.");
      }
      None
    }
  }
}

impl<F: FileReader + Default + std::fmt::Debug> TryFrom<ManifestArgs<F>> for RootedManifest {
  type Error = ConfigError;

  /// Resolve Manifest (Priority: Override -> Manifest -> Fail)
  fn try_from(manifest_args: ManifestArgs<F>) -> Result<Self, Self::Error> {
    let overrides: &Option<String> = &manifest_args.overrides;
    let content: &Option<String> = &manifest_args.get_content()?;

    // Load Manifest from(if it exists)
    let manifest_from_file: Option<BuildManifest> = {
      if let Some(json_str) = content {
        Some(serde_json::from_str(json_str).map_err(Self::Error::ParseManifest)?)
      } else {
        None
      }
    };

    // Load Manifest from overrides (if it exists)
    let manifest_from_overrides: Option<BuildManifest> = {
      if let Some(json_str) = overrides {
        if let Ok(manifest) = serde_json::from_str::<BuildManifest>(json_str) {
          Some(manifest)
        } else {
          let components: ComponentCommandMap =
            serde_json::from_str(json_str).map_err(Self::Error::ParseCmpOverrideJson)?;
          Some(BuildManifest { components })
        }
      } else {
        None
      }
    };

    match (manifest_from_overrides, manifest_from_file) {
      (Some(mo), Some(mut mf)) => {
        for (k, v) in mo.components {
          mf.components.insert(k, v);
        }
        Ok(RootedManifest::new(manifest_args.root_dir, mf))
      }
      (Some(mo), None) => Ok(RootedManifest::new(manifest_args.root_dir, mo)),
      (None, Some(mf)) => Ok(RootedManifest::new(manifest_args.root_dir, mf)),
      _ => Err(Self::Error::NoManifestFileOrOverride),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::cli::FileReader;
  use crate::cli::ManifestArgs;
  use crate::cli::RunArgs;
  use crate::manifest::BuildManifest;
  use crate::manifest::CommandArgs;
  use crate::manifest::ComponentCommandMap;
  use crate::manifest::ComponentType::*;
  use crate::manifest::ManifestComponent;
  use std::collections::HashMap;
  use std::path::Path;
  use std::path::PathBuf;

  // Helper to create mock RunArgs
  fn mock_run_args() -> RunArgs<MockFileSystem> {
    let file_reader = MockFileSystem::default();

    RunArgs {
      tasks: vec![],
      manifest: ManifestArgs {
        root_dir: PathBuf::new(),
        overrides: None,
        file_path: None,
        file_reader,
      },
      generator: GenArgs {
        name: "default-gen".to_string(),
        seed: None,
        trailing_args: vec![],
      },
    }
  }

  // Helper to create a mock BuildManifest
  fn _mock_manifest() -> BuildManifest {
    let mut components: ComponentCommandMap = HashMap::new();
    components.insert(
      "default-gen".to_string(),
      ManifestComponent {
        component_type: Generator,
        dir: PathBuf::new(),
        run: CommandArgs {
          command: PathBuf::from("/bin/manifest-gen"),
          args: vec!["--from-manifest".to_string()],
        },
      },
    );

    components.insert(
      "cpp".to_string(),
      ManifestComponent {
        component_type: Executor,
        dir: PathBuf::new(),
        run: CommandArgs {
          command: PathBuf::from("/bin/manifest-cpp"),
          args: vec![],
        },
      },
    );
    components.insert(
      "rust".to_string(),
      ManifestComponent {
        component_type: Executor,
        dir: PathBuf::new(),
        run: CommandArgs {
          command: PathBuf::from("/bin/manifest-rust"),
          args: vec![],
        },
      },
    );

    BuildManifest { components }
  }

  #[derive(Default, Debug)]
  pub struct MockFileSystem;
  impl FileReader for MockFileSystem {
    fn read_to_string(&self, _: &Path) -> std::io::Result<Option<String>> {
      let manifest = _mock_manifest();
      Some(serde_json::to_string_pretty(&manifest).map_err(Into::into)).transpose()
    }
  }

  // ---------------------------------
  // Tests for resolve_generator
  // ---------------------------------

  #[test]
  fn test_gen_priority_1_override_path_build_manifest() {
    let mut args = mock_run_args();
    args.manifest.overrides = Some(String::from(
      r#"{ "components": { "default-gen": { "type": "generator", "command": "/bin/override-gen" } } }"#,
    ));

    let manifest = RootedManifest::try_from(args.manifest).unwrap();

    let cmd = manifest
      .resolve_generator(args.generator.into())
      .unwrap()
      .unwrap()
      .run;

    // Should use the override path
    assert_eq!(cmd.command, PathBuf::from("/bin/override-gen"));
    // Should NOT have args from manifest
    assert!(!cmd.args.contains(&"--from-manifest".to_string()));
    // Should contain the seed
    assert!(cmd.args.iter().any(|s| s.starts_with("--seed=")));
  }

  #[test]
  fn test_gen_priority_1_override_path_component_command_map() {
    let mut args = mock_run_args();
    args.manifest.overrides = Some(String::from(
      r#"{ "default-gen": { "type": "generator", "command": "/bin/override-gen" } }"#,
    ));

    let manifest = RootedManifest::try_from(args.manifest).unwrap();

    let cmd = manifest
      .resolve_generator(args.generator.into())
      .unwrap()
      .unwrap()
      .run;

    // Should use the override path
    assert_eq!(cmd.command, PathBuf::from("/bin/override-gen"));
    // Should NOT have args from manifest
    assert!(!cmd.args.contains(&"--from-manifest".to_string()));
    // Should contain the seed
    assert!(cmd.args.iter().any(|s| s.starts_with("--seed=")));
  }

  #[test]
  fn test_gen_priority_2_manifest() {
    let args = mock_run_args(); // No override
    let manifest = RootedManifest::try_from(args.manifest).unwrap();

    let cmd = manifest
      .resolve_generator(args.generator.into())
      .unwrap()
      .unwrap()
      .run;

    // Should use the manifest path
    assert_eq!(cmd.command, PathBuf::from("/bin/manifest-gen"));
    // Should have args from manifest
    assert!(cmd.args.contains(&"--from-manifest".to_string()));
    // Should contain the seed
    assert!(cmd.args.iter().any(|s| s.starts_with("--seed=")));
  }

  #[test]
  fn test_gen_priority_3_fail_not_found() {
    let mut args = mock_run_args();
    args.generator.name = "missing-gen".to_string(); // Not in manifest
    let manifest = RootedManifest::try_from(args.manifest).unwrap();

    let err = manifest
      .resolve_generator(args.generator.into())
      .unwrap_err();

    // Should fail with a helpful error
    assert!(
      err
        .to_string()
        .contains("Component 'missing-gen' not found in manifest")
    );
  }

  #[test]
  fn test_gen_none() {
    let mut args = mock_run_args();
    args.generator.name = "none".to_string();
    args.manifest.overrides = Some(String::from(
      r#"{"noopgen": { "type": "generator", "command": "/bin/override-gen" } }"#,
    )); // Will be ignored
    args.generator.seed = Some(12345); // Will be ignored

    let manifest = RootedManifest::try_from(args.manifest).unwrap();

    // Should return Ok(None)
    let cmd = manifest.resolve_generator(args.generator.into()).unwrap();
    assert!(cmd.is_none());
  }

  // ---------------------------------
  // Tests for resolve_algorithms
  // ---------------------------------

  #[test]
  fn test_algo_priority_1_override_path() {
    let mut args = mock_run_args();
    // Override "cpp", but not "rust"
    args.manifest.overrides =
      Some(r#"{"cpp": { "type": "executor", "command": "/bin/override-cpp" } }"#.to_string());

    let tasks: Tasks = serde_json::from_str(
      r#"[ { "executor": "cpp", "target": "func1" }, { "executor": "rust", "target": "func2" } ]"#,
    )
    .unwrap();
    let manifest = RootedManifest::try_from(args.manifest).unwrap();

    // let map : Vec<_> = tasks.iter().map(|task| manifest.resolve_executor(task)).collect();
    let cpp = manifest.resolve_executor(&tasks[0]);
    let rust = manifest.resolve_executor(&tasks[1]);

    // "cpp" should come from the override
    assert_eq!(cpp.unwrap().run.command, PathBuf::from("/bin/override-cpp"));
    // "rust" should fall back to the manifest
    assert_eq!(
      rust.unwrap().run.command,
      PathBuf::from("/bin/manifest-rust")
    );
  }

  #[test]
  fn test_algo_priority_2_manifest() {
    let args = mock_run_args(); // No overrides
    let tasks: Tasks =
      serde_json::from_str(r#"[ { "executor": "cpp", "target": "func1" } ]"#).unwrap();

    let manifest = RootedManifest::try_from(args.manifest).unwrap();

    let cpp = manifest.resolve_executor(&tasks[0]);

    // "cpp" should come from the manifest
    assert_eq!(cpp.unwrap().run.command, PathBuf::from("/bin/manifest-cpp"));
  }

  #[test]
  fn test_algo_priority_3_fail_not_found() {
    let args = mock_run_args(); // No overrides
    // Request "python", which is not in the manifest
    let tasks: Tasks =
      serde_json::from_str(r#"[ { "executor": "python", "target": "func1" } ]"#).unwrap();
    let manifest = RootedManifest::try_from(args.manifest).unwrap();

    let err = manifest.resolve_executor(&tasks[0]).unwrap_err();

    // Should fail with a helpful error
    assert!(
      err
        .to_string()
        .contains("Component 'python' not found in manifest")
    );
  }
}
