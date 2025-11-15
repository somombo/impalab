use crate::command::CommandArgs;
use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

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

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct BuildManifest {
  pub generators: HashMap<String, CommandArgs>,
  pub algorithm_executables: HashMap<String, CommandArgs>,
}

pub async fn build_components(components_dir: PathBuf, manifest_out: PathBuf) -> Result<()> {
  tracing::info!("Scanning for components in {}", components_dir.display());

  if !components_dir.exists() {
    anyhow::bail!(
      "Components directory not found: {}",
      components_dir.display()
    );
  }

  let mut manifest = BuildManifest::default();

  for entry in fs::read_dir(&components_dir)? {
    let entry = entry?;
    let path = entry.path();

    if path.is_dir() {
      let config_path = path.join("impafile.toml");
      if config_path.exists() {
        process_component(&path, &config_path, &mut manifest).await?;
      }
    }
  }

  let json = serde_json::to_string_pretty(&manifest)?;
  fs::write(&manifest_out, json)?;
  tracing::info!("Build manifest written to {}", manifest_out.display());

  Ok(())
}

async fn process_component(
  base_dir: &Path,
  config_path: &Path,
  manifest: &mut BuildManifest,
) -> Result<()> {
  let content = fs::read_to_string(config_path)?;
  let config: ComponentConfig = toml::from_str(&content)
    .with_context(|| format!("Failed to parse {}", config_path.display()))?;

  // Run optional build step
  if let Some(build_step) = &config.build {
    tracing::info!(
      "Building component: {} ({:?})",
      config.name,
      config.component_type
    );
    let status = Command::new(&build_step.command)
      .args(&build_step.args)
      .current_dir(base_dir)
      .status()
      .with_context(|| format!("Failed to execute build command for {}", config.name))?;

    if !status.success() {
      anyhow::bail!("Build failed for {}", config.name);
    }
  } else {
    tracing::info!("No build step for {}. Skipping.", config.name);
  }

  // Resolve paths in run command
  let mut run_command = config.run;

  // Check if command is a relative path to an existing file
  let potential_cmd_path = base_dir.join(&run_command.command);
  if potential_cmd_path.exists() && potential_cmd_path.is_file() {
    run_command.command = potential_cmd_path.canonicalize().context(format!(
      "Failed to canonicalize command path for {}",
      config.name
    ))?;
  }

  // Check args for relative paths
  let mut resolved_args = Vec::new();
  for arg in run_command.args {
    let potential_arg_path = base_dir.join(&arg);
    if potential_arg_path.exists() {
      resolved_args.push(
        potential_arg_path
          .canonicalize()
          .context(format!(
            "Failed to canonicalize arg path '{}' for {}",
            arg, config.name
          ))?
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
