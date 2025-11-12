use clap::Parser;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(version, about = "Orchestrator of Algorithm Benchmarking")]
pub struct OrchestratorCliParser {
  /// JSON string mapping languages to lists of function names.
  /// Example: '{"cpp": ["std::sort"], "lean": ["List.mergeSort"]}'
  #[arg(long, required = true)]
  pub algorithms: String,

  /// Seed for the random number generator.
  #[arg(long)]
  pub seed: Option<u64>,

  /// Path to the data generator executable.
  #[arg(long)]
  pub generator_exe_path: Option<PathBuf>,

  /// JSON string mapping languages to sorter executable paths.
  /// Example: '{"cpp": "./sorter_cpp", "lean": "./sorter_lean"}'
  #[arg(long)]
  pub sorter_exe_paths: Option<String>,

  /// All remaining arguments are passed to the data generator.
  #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
  pub generator_args: Vec<String>,
}
