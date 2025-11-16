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
use crate::builder::BuildManifest;
use crate::cli::RunArgs;
use crate::command::CommandArgs;
use crate::error::ConfigError;
use serde_json;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Implements the 3-tiered logic for resolving the generator path.
fn resolve_generator(
  args: &RunArgs,
  manifest: &Option<BuildManifest>,
) -> Result<Option<CommandArgs>, ConfigError> {
  if args.generator == "none" {
    if args.generator_override_path.is_some() {
      tracing::warn!("--generator=none is set, so --generator-override-path will be ignored.");
    }
    if !args.generator_args.is_empty() {
      tracing::warn!("--generator=none is set, so trailing generator arguments will be ignored.");
    }
    if args.seed.is_some() {
      tracing::warn!("--generator=none is set, so --seed will be ignored.");
    }
    return Ok(None);
  }

  // A generator name was provided. Find its base command.
  let mut base_command = if let Some(path) = &args.generator_override_path {
    // Priority 1: CLI Override (converts PathBuf to CommandArgs)
    tracing::debug!("Using generator override path: {}", path.display());
    CommandArgs {
      command: path.clone(),
      args: vec![],
    }
  } else if let Some(m) = manifest {
    // Priority 2: Build Manifest (clones CommandArgs)
    if let Some(cmd) = m.generators.get(&args.generator) {
      tracing::debug!("Using generator command from manifest");
      cmd.clone()
    } else {
      // Priority 3: Fail
      let available: Vec<_> = m.generators.keys().cloned().collect();
      return Err(ConfigError::GeneratorNotFound {
        generator_name: args.generator.clone(),
        available,
      });
    }
  } else {
    // Priority 3: Fail (no manifest)
    return Err(ConfigError::GeneratorOverrideNoManifest {
      generator_name: args.generator.clone(),
      manifest_path: args.manifest_path.clone(),
    });
  };

  // Append seed and passthrough args
  let seed = args.seed.unwrap_or_else(rand::random);
  base_command.args.extend(args.generator_args.clone());
  base_command.args.push(format!("--seed={}", seed));
  tracing::info!(seed, "Using generator seed");

  Ok(Some(base_command))
}

/// Implements the 3-tiered logic for resolving all required algorithm executable paths.
fn resolve_algorithms(
  args: &RunArgs,
  tasks: &Algorithms,
  manifest: &Option<BuildManifest>,
) -> Result<AlgorithmCommandMap, ConfigError> {
  // Parse override map (if it exists)
  let override_map: Option<HashMap<String, PathBuf>> =
    if let Some(json_str) = &args.algorithm_override_paths {
      Some(serde_json::from_str(json_str).map_err(ConfigError::ParseAlgoOverrideJson)?)
    } else {
      None
    };

  let mut resolved_commands = HashMap::new();

  // Find a path for every language specified in the --algorithms task list
  for lang in tasks.keys() {
    // Find the base command for this language
    let base_command = if let Some(map) = &override_map {
      // Priority 1: CLI Override
      if let Some(path) = map.get(lang) {
        tracing::debug!(
          "Using algorithm override path for '{}': {}",
          lang,
          path.display()
        );
        Some(CommandArgs {
          command: path.clone(),
          args: vec![],
        })
      } else {
        None // No override for *this* language, fall through
      }
    } else {
      None // No override map at all, fall through
    };

    // If override wasn't found, try manifest
    let base_command = base_command.or_else(|| {
      if let Some(m) = manifest {
        // Priority 2: Build Manifest
        if let Some(cmd) = m.algorithm_executables.get(lang) {
          tracing::debug!("Using algorithm command from manifest for '{}'", lang);
          Some(cmd.clone())
        } else {
          None // Not in manifest, fall through to error
        }
      } else {
        None // No manifest, fall through to error
      }
    });

    // Check result
    if let Some(cmd) = base_command {
      resolved_commands.insert(lang.clone(), cmd);
    } else {
      // Priority 3: Fail
      return Err(ConfigError::AlgoExecutableNotFound {
        language: lang.clone(),
      });
    }
  }

  Ok(resolved_commands)
}

/// Type alias for the map of resolved executable paths: `{"lang": CommandArgs}`
pub type AlgorithmCommandMap = HashMap<String, CommandArgs>;

/// Type alias for the map of algorithm functions to run: `{"lang": ["func1", "func2"]}`
pub type Algorithms = HashMap<String, Vec<String>>;

/// The fully resolved configuration for a benchmark run.
///
/// This struct is created from `RunArgs` and the `BuildManifest`
/// and contains all information needed to execute the benchmark.
#[derive(Debug)]
pub struct Config {
  /// The resolved command for the generator, or `None` if `generator = "none"`.
  pub generator_command: Option<CommandArgs>,

  /// A map of language names to their resolved `CommandArgs`.
  pub algorithm_commands: AlgorithmCommandMap,

  /// The map of tasks (lang -> functions) to run.
  pub algorithms: Algorithms,
}

impl TryFrom<RunArgs> for Config {
  type Error = ConfigError;

  fn try_from(args: RunArgs) -> Result<Self, Self::Error> {
    // Load Manifest (if it exists)
    let manifest: Option<BuildManifest> = if args.manifest_path.exists() {
      let content =
        fs::read_to_string(&args.manifest_path).map_err(|e| ConfigError::ReadManifest {
          path: args.manifest_path.clone(),
          source: e,
        })?;
      Some(serde_json::from_str(&content).map_err(ConfigError::ParseManifest)?)
    } else {
      None
    };

    // Parse Tasks
    let algorithms: Algorithms =
      serde_json::from_str(&args.algorithms).map_err(ConfigError::ParseAlgorithmsJson)?;

    // Resolve Generator (Priority: Override -> Manifest -> Fail)
    let generator_command = resolve_generator(&args, &manifest)?;

    // Resolve Algorithm Executables (Priority: Override -> Manifest -> Fail)
    let algorithm_commands = resolve_algorithms(&args, &algorithms, &manifest)?;

    Ok(Config {
      algorithms,
      generator_command,
      algorithm_commands,
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::builder::BuildManifest;
  use crate::cli::RunArgs;
  use crate::command::CommandArgs;
  use std::collections::HashMap;
  use std::path::PathBuf;

  // Helper to create mock RunArgs
  fn mock_run_args() -> RunArgs {
    RunArgs {
      algorithms: "{}".to_string(),
      seed: None,
      generator: "default-gen".to_string(),
      generator_override_path: None,
      algorithm_override_paths: None,
      manifest_path: PathBuf::from("impa_manifest.json"),
      generator_args: vec![],
    }
  }

  // Helper to create a mock BuildManifest
  fn mock_manifest() -> BuildManifest {
    let mut generators = HashMap::new();
    generators.insert(
      "default-gen".to_string(),
      CommandArgs {
        command: PathBuf::from("/bin/manifest-gen"),
        args: vec!["--from-manifest".to_string()],
      },
    );

    let mut algorithm_executables = HashMap::new();
    algorithm_executables.insert(
      "cpp".to_string(),
      CommandArgs {
        command: PathBuf::from("/bin/manifest-cpp"),
        args: vec![],
      },
    );
    algorithm_executables.insert(
      "rust".to_string(),
      CommandArgs {
        command: PathBuf::from("/bin/manifest-rust"),
        args: vec![],
      },
    );

    BuildManifest {
      generators,
      algorithm_executables,
    }
  }

  // ---------------------------------
  // Tests for resolve_generator
  // ---------------------------------

  #[test]
  fn test_gen_priority_1_override_path() {
    let mut args = mock_run_args();
    args.generator_override_path = Some(PathBuf::from("/bin/override-gen"));

    let manifest = Some(mock_manifest());

    let cmd = resolve_generator(&args, &manifest).unwrap().unwrap();

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
    let manifest = Some(mock_manifest());

    let cmd = resolve_generator(&args, &manifest).unwrap().unwrap();

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
    args.generator = "missing-gen".to_string(); // Not in manifest
    let manifest = Some(mock_manifest());

    let err = resolve_generator(&args, &manifest).unwrap_err();

    // Should fail with a helpful error
    assert!(
      err
        .to_string()
        .contains("Generator 'missing-gen' not found in manifest")
    );
  }

  #[test]
  fn test_gen_none() {
    let mut args = mock_run_args();
    args.generator = "none".to_string();
    args.generator_override_path = Some(PathBuf::from("/bin/override-gen")); // Will be ignored
    args.seed = Some(12345); // Will be ignored

    let manifest = Some(mock_manifest());

    // Should return Ok(None)
    let cmd = resolve_generator(&args, &manifest).unwrap();
    assert!(cmd.is_none());
  }

  // ---------------------------------
  // Tests for resolve_algorithms
  // ---------------------------------

  #[test]
  fn test_algo_priority_1_override_path() {
    let mut args = mock_run_args();
    // Override "cpp", but not "rust"
    args.algorithm_override_paths = Some(r#"{"cpp": "/bin/override-cpp"}"#.to_string());

    let tasks: Algorithms =
      serde_json::from_str(r#"{"cpp": ["func1"], "rust": ["func2"]}"#).unwrap();
    let manifest = Some(mock_manifest());

    let map = resolve_algorithms(&args, &tasks, &manifest).unwrap();

    // "cpp" should come from the override
    assert_eq!(
      map.get("cpp").unwrap().command,
      PathBuf::from("/bin/override-cpp")
    );
    // "rust" should fall back to the manifest
    assert_eq!(
      map.get("rust").unwrap().command,
      PathBuf::from("/bin/manifest-rust")
    );
  }

  #[test]
  fn test_algo_priority_2_manifest() {
    let args = mock_run_args(); // No overrides
    let tasks: Algorithms = serde_json::from_str(r#"{"cpp": ["func1"]}"#).unwrap();
    let manifest = Some(mock_manifest());

    let map = resolve_algorithms(&args, &tasks, &manifest).unwrap();

    // "cpp" should come from the manifest
    assert_eq!(
      map.get("cpp").unwrap().command,
      PathBuf::from("/bin/manifest-cpp")
    );
  }

  #[test]
  fn test_algo_priority_3_fail_not_found() {
    let args = mock_run_args(); // No overrides
    // Request "python", which is not in the manifest
    let tasks: Algorithms = serde_json::from_str(r#"{"python": ["func1"]}"#).unwrap();
    let manifest = Some(mock_manifest());

    let err = resolve_algorithms(&args, &tasks, &manifest).unwrap_err();

    // Should fail with a helpful error
    assert!(
      err
        .to_string()
        .contains("No executable path found for language 'python'")
    );
  }
}
