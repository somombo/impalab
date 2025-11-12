use crate::cli::OrchestratorCliParser;
use anyhow::Context;
use anyhow::Result;
use rand::RngCore;
use std::collections::HashMap;
use std::path::PathBuf;

/// Generates a secure 64-bit seed.
fn generate_seed() -> u64 {
  let mut rng = rand::rng();
  rng.next_u64()
}

// --- Default Values ---
fn default_gen_path() -> PathBuf {
  PathBuf::from("./.lake/build/bin/data_generator")
}

fn default_sorters_json_string() -> String {
  r#"
  {
    "cpp": "./src/cpp/sorter_cpp",
    "lean": "./.lake/build/bin/sorter_lean"
  }
  "#
  .to_string()
}

/// Appends ".exe" to a path on Windows.
fn ensure_exe_suffix(path: PathBuf) -> PathBuf {
  #[cfg(target_os = "windows")]
  {
    if path.extension().is_none() {
      let mut p = path.into_os_string();
      p.push(".exe");
      return PathBuf::from(p);
    }
  }
  path
}

/// Type alias for the sorter path map.
pub type SorterPaths = HashMap<String, PathBuf>;

/// Type alias for the algorithm map.
pub type Algorithms = HashMap<String, Vec<String>>;

/// Fully validated and resolved configuration.
#[derive(Debug)]
pub struct Config {
  pub algorithms: Algorithms,
  pub seed: u64,
  pub generator_exe: PathBuf,
  pub sorter_paths: SorterPaths,
  pub generator_args: Vec<String>,
}

impl TryFrom<OrchestratorCliParser> for Config {
  type Error = anyhow::Error;

  fn try_from(
    OrchestratorCliParser {
      algorithms,
      seed,
      generator_exe_path,
      sorter_exe_paths,
      generator_args,
    }: OrchestratorCliParser,
  ) -> Result<Self, Self::Error> {
    let seed = seed.unwrap_or_else(generate_seed);

    let generator_exe = ensure_exe_suffix(generator_exe_path.unwrap_or_else(default_gen_path));
    let sorter_paths_json = sorter_exe_paths.unwrap_or_else(default_sorters_json_string);

    let sorter_paths: SorterPaths =
      serde_json::from_str(&sorter_paths_json).context("Failed to parse --sorter-exe-paths")?;

    // Ensure sorter paths also have .exe if needed
    let sorter_paths = sorter_paths
      .into_iter()
      .map(|(lang, path)| (lang, ensure_exe_suffix(path)))
      .collect();

    let algorithms: Algorithms =
      serde_json::from_str(&algorithms).context("Failed to parse --algorithms")?;

    Ok(Config {
      algorithms,
      seed,
      generator_exe,
      sorter_paths,
      generator_args,
    })
  }
}
