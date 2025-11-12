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
  build: BuildStep,
  output: OutputStep,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum ComponentType {
  Generator,
  Sorter,
}

#[derive(Debug, Deserialize)]
struct BuildStep {
  command: String,
  args: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct OutputStep {
  executable: PathBuf,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct BuildManifest {
  pub generators: HashMap<String, PathBuf>,
  pub sorters: HashMap<String, PathBuf>,
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

  tracing::info!(
    "Building component: {} ({:?})",
    config.name,
    config.component_type
  );

  let status = Command::new(&config.build.command)
    .args(&config.build.args)
    .current_dir(base_dir)
    .status()
    .with_context(|| format!("Failed to execute build command for {}", config.name))?;

  if !status.success() {
    anyhow::bail!("Build failed for {}", config.name);
  }

  let exe_path = base_dir.join(&config.output.executable);

  let exe_path = if cfg!(target_os = "windows") && exe_path.extension().is_none() {
    let mut p = exe_path.into_os_string();
    p.push(".exe");
    PathBuf::from(p)
  } else {
    exe_path
  };

  if !exe_path.exists() {
    anyhow::bail!(
      "Build succeeded but output executable not found at: {}",
      exe_path.display()
    );
  }

  // Store absolute path or relative to root? Relative is usually safer for portable manifests.
  // Here we just use the path as resolved.
  match config.component_type {
    ComponentType::Generator => {
      manifest.generators.insert(config.name, exe_path);
    }
    ComponentType::Sorter => {
      if let Some(lang) = config.language {
        manifest.sorters.insert(lang, exe_path);
      } else {
        tracing::warn!(
          "Sorter '{}' missing 'language' field. Skipping registration.",
          config.name
        );
      }
    }
  }

  Ok(())
}
