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
use Commands::Build;
use Commands::Run;
use anyhow::Result;
use clap::Parser;
use impalab::benchmark::run_benchmarks;
use impalab::builder::build_components;
use impalab::cli::Cli;
use impalab::cli::Commands;
use impalab::config::Config;
use impalab::logging::setup_tracing;
// use tracing::Instrument;

#[tokio::main]
async fn main() -> Result<()> {
  setup_tracing()?;

  let Cli { command } = Cli::parse();
  let main_span = tracing::info_span!("orchestrator");
  let _enter = main_span.enter();

  match command {
    Build {
      components_dir,
      manifest_path,
    } => {
      tracing::info!("Starting Build Process...");

      build_components(components_dir, manifest_path).await?;

      tracing::info!("Build Process Complete.");
    }
    Run(run_args) => {
      tracing::info!("Initializing Benchmark Run...");

      let config = Config::try_from(run_args)?;

      run_benchmarks(config).await?;
    }
  }

  Ok(())
}
