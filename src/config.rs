use crate::builder::BuildManifest;
use crate::cli::RunArgs;
use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use rand::RngCore;
use serde_json;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Generates a secure 64-bit seed.
fn generate_seed() -> u64 {
  let mut rng = rand::rng();
  rng.next_u64()
}

/// Implements the 3-tiered logic for resolving the generator path.
fn resolve_generator(
  args: &RunArgs,
  manifest: &Option<BuildManifest>,
) -> Result<Option<CommandArgs>> {
  if args.generator == "none" {
    if args.generator_override_path.is_some() {
      tracing::warn!("--generator=none is set, so --generator-override-path will be ignored.");
    }
    if !args.generator_args.is_empty() {
      tracing::warn!("--generator=none is set, so trailing generator arguments will be ignored.");
    }
    if args.seed.is_some() {
      tracing::warn!("--generator=none is set, so --seed will be ignored.");
    } // TODO: somombo> double check this
    return Ok(None);
  }

  // A generator name was provided, so we must find an executable.
  let exe_path = if let Some(path) = &args.generator_override_path {
    // Priority 1: CLI Override
    tracing::debug!("Using generator override path: {}", path.display());
    path.clone()
  } else if let Some(m) = manifest {
    // Priority 2: Build Manifest
    if let Some(path) = m.generators.get(&args.generator) {
      tracing::debug!("Using generator path from manifest: {}", path.display());
      path.clone()
    } else {
      // Priority 3: Fail
      let available: Vec<_> = m.generators.keys().collect();
      bail!(
        "Generator '{}' not found in manifest. Available generators: {:?}. Or, provide --generator-override-path.",
        args.generator,
        available
      );
    }
  } else {
    // Priority 3: Fail (no manifest)
    bail!(
      "Generator '{}' not specified via override and no build manifest was found at {}",
      args.generator,
      args.manifest_path.display()
    );
  };

  // Append seed and passthrough args
  let mut gen_args = args.generator_args.clone();
  let seed = args.seed.unwrap_or_else(generate_seed);
  gen_args.push(format!("--seed={}", seed));
  tracing::info!(seed, "Using generator seed");

  Ok(Some(CommandArgs {
    exe: exe_path,
    args: gen_args,
  }))
}

/// Implements the 3-tiered logic for resolving all required algorithm executable paths.
fn resolve_algorithms(
  args: &RunArgs,
  tasks: &Algorithms,
  manifest: &Option<BuildManifest>,
) -> Result<AlgorithmCommandMap> {
  // Parse override map (if it exists)
  let override_map: Option<HashMap<String, PathBuf>> = if let Some(json_str) =
    &args.algorithm_override_paths
  {
    Some(serde_json::from_str(json_str).context("Failed to parse --algorithm-override-paths JSON")?)
  } else {
    None
  };

  let mut resolved_commands = HashMap::new();

  // Find a path for every language specified in the --algorithms task list
  for lang in tasks.keys() {
    let exe_path = if let Some(map) = &override_map {
      // Priority 1: CLI Override
      if let Some(path) = map.get(lang) {
        tracing::debug!(
          "Using algorithm override path for '{}': {}",
          lang,
          path.display()
        );
        Some(path.clone())
      } else {
        None // No override for *this* language, fall through
      }
    } else {
      None // No override map at all, fall through
    };

    // If override wasn't found, try manifest
    let exe_path = exe_path.or_else(|| {
      if let Some(m) = manifest {
        // Priority 2: Build Manifest
        if let Some(path) = m.algorithm_executables.get(lang) {
          tracing::debug!(
            "Using algorithm path from manifest for '{}': {}",
            lang,
            path.display()
          );
          Some(path.clone())
        } else {
          None // Not in manifest, fall through to error
        }
      } else {
        None // No manifest, fall through to error
      }
    });

    // Check result
    if let Some(path) = exe_path {
      resolved_commands.insert(
        lang.clone(),
        CommandArgs {
          exe: path,
          args: vec![], // Algorithm-specific args come from the --algorithms JSON
        },
      );
    } else {
      // Priority 3: Fail
      bail!(
        "No executable path found for language '{}'. Searched overrides and manifest.",
        lang
      );
    }
  }

  Ok(resolved_commands)
}

/// Type alias for the map of resolved executable paths: `{"lang": CommandArgs}`
pub type AlgorithmCommandMap = HashMap<String, CommandArgs>;

/// Holds the executable path and arguments for a component.
#[derive(Debug, Clone)]
pub struct CommandArgs {
  pub exe: PathBuf,
  pub args: Vec<String>,
}

/// Type alias for the map of algorithm functions to run: `{"lang": ["func1", "func2"]}`
pub type Algorithms = HashMap<String, Vec<String>>;

/// The fully resolved configuration for a benchmark run.
#[derive(Debug)]
pub struct Config {
  pub generator_command: Option<CommandArgs>,
  pub algorithm_commands: AlgorithmCommandMap,
  pub algorithms: Algorithms,
}

impl TryFrom<RunArgs> for Config {
  type Error = anyhow::Error;

  fn try_from(args: RunArgs) -> Result<Self, Self::Error> {
    // Load Manifest (if it exists)
    let manifest: Option<BuildManifest> = if args.manifest_path.exists() {
      let content = fs::read_to_string(&args.manifest_path).context(format!(
        "Failed to read manifest file: {}",
        args.manifest_path.display()
      ))?;
      serde_json::from_str(&content).context("Failed to parse manifest JSON")?
    } else {
      None
    };

    // Parse Tasks
    let algorithms: Algorithms = serde_json::from_str(&args.algorithms).context(format!(
      "Failed to parse --algorithms JSON   {}",
      &args.algorithms
    ))?;

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
