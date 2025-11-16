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
use std::path::PathBuf;
use thiserror::Error;

/// Top-level error enum for the impalab library.
#[derive(Error, Debug)]
pub enum ImpalabError {
  #[error("Build process failed")]
  Build(#[from] BuildError),

  #[error("Configuration error")]
  Config(#[from] ConfigError),

  #[error("Benchmark run failed")]
  Benchmark(#[from] BenchmarkError),

  #[error("I/O error: {0}")]
  Io(#[from] std::io::Error),

  #[error("JSON serialization/deserialization error: {0}")]
  Json(#[from] serde_json::Error),
}

/// Errors related to the build process (src/builder.rs).
#[derive(Error, Debug)]
pub enum BuildError {
  #[error("Components directory not found: {0}")]
  ComponentsDirNotFound(PathBuf),

  #[error("Failed to read directory")]
  ReadDir(#[source] std::io::Error),

  #[error("Failed to parse TOML file: {0}")]
  TomlParse(#[from] toml::de::Error),

  #[error("Failed to read component config")]
  ReadConfig(#[source] std::io::Error),

  #[error(
    "Build command failed for component: {component_name}\n--- STDOUT ---\n{stdout}\n--- STDERR ---\n{stderr}"
  )]
  BuildCommandFailed {
    component_name: String,
    stdout: String,
    stderr: String,
  },

  #[error("Failed to execute build command for {component_name}")]
  BuildCommandExecFailed {
    component_name: String,
    #[source]
    source: std::io::Error,
  },

  #[error("Failed to canonicalize path for {component_name}: {path}")]
  CanonicalizePath {
    component_name: String,
    path: PathBuf,
    #[source]
    source: std::io::Error,
  },

  #[error("Failed to write manifest")]
  WriteManifest(#[source] std::io::Error),

  #[error("Failed to serialize manifest")]
  SerializeManifest(#[from] serde_json::Error),
}

/// Errors related to configuration resolution (src/config.rs).
#[derive(Error, Debug)]
pub enum ConfigError {
  #[error("Failed to read manifest file: {path}")]
  ReadManifest {
    path: PathBuf,
    #[source]
    source: std::io::Error,
  },

  #[error("Failed to parse manifest JSON")]
  ParseManifest(#[from] serde_json::Error),

  #[error("Failed to parse --algorithms JSON: {0}")]
  ParseAlgorithmsJson(#[source] serde_json::Error),

  #[error("Failed to parse --algorithm-override-paths JSON: {0}")]
  ParseAlgoOverrideJson(#[source] serde_json::Error),

  #[error(
    "Generator '{generator_name}' not found in manifest. Available: {available:?}. Or, provide --generator-override-path."
  )]
  GeneratorNotFound {
    generator_name: String,
    available: Vec<String>,
  },

  #[error(
    "Generator '{generator_name}' specified via override but no build manifest was found at {manifest_path}"
  )]
  GeneratorOverrideNoManifest {
    generator_name: String,
    manifest_path: PathBuf,
  },

  #[error("No executable path found for language '{language}'. Searched overrides and manifest.")]
  AlgoExecutableNotFound { language: String },
}

/// Errors related to the benchmark execution (src/benchmark.rs).
#[derive(Error, Debug)]
pub enum BenchmarkError {
  #[error("Internal error: No command found for language {language}. Skipping.")]
  NoCommandForLanguage { language: String },

  #[error("Failed to spawn generator")]
  SpawnGenerator(#[source] std::io::Error),

  #[error("Failed to take generator stdout pipe")]
  PipeGenStdout,

  #[error("Failed to take generator stderr pipe")]
  PipeGenStderr,

  #[error("Failed to convert generator stdout pipe")]
  ConvertGenStdout(#[source] std::io::Error),

  #[error("Failed to spawn algorithm component")]
  SpawnAlgorithm(#[source] std::io::Error),

  #[error("Failed to take algorithm stdout pipe")]
  PipeAlgoStdout,

  #[error("Failed to take algorithm stderr pipe")]
  PipeAlgoStderr,

  #[error("Failed to wait for child processes")]
  WaitChild(#[source] std::io::Error),

  #[error("Failed to wait for algorithm process")]
  WaitAlgo(#[source] std::io::Error),

  #[error("Generator stderr task failed")]
  GenStderrTask(tokio::task::JoinError),

  #[error("Stdout processing task failed")]
  StdoutTask(tokio::task::JoinError),

  #[error("Algorithm stderr task failed")]
  AlgoStderrTask(tokio::task::JoinError),

  #[error("Failed to read algorithm stdout")]
  ReadAlgoStdout(#[source] std::io::Error),

  #[error("Failed to serialize benchmark result")]
  SerializeResult(#[from] serde_json::Error),

  #[error("Malformed output line from algorithm: {line}")]
  MalformedAlgoOutput {
    line: String,
    #[source]
    source: Box<BenchmarkError>, // Wraps parsing errors
  },

  #[error("Expected 3 CSV parts, got {parts} for line: {line}")]
  CsvParts { parts: usize, line: String },

  #[error("Failed to parse duration '{duration}'")]
  ParseDuration {
    duration: String,
    #[source]
    source: std::num::ParseIntError,
  },

  #[error("Failed to read {target} stderr")]
  ReadStderr {
    target: &'static str,
    #[source]
    source: std::io::Error,
  },
}
