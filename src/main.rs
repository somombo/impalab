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
      if let Err(e) = build_components(components_dir, manifest_path).await {
        tracing::error!(error = %e, "Build failed");
        return Err(e);
      }
      tracing::info!("Build Process Complete.");
    }
    Run(run_args) => {
      tracing::info!("Initializing Benchmark Run...");
      let config = match Config::try_from(run_args) {
        Ok(cfg) => cfg,
        Err(e) => {
          tracing::error!(error = %e, "Failed to initialize configuration");
          return Err(e);
        }
      };

      if let Err(e) = run_benchmarks(config).await {
        tracing::error!(error = %e, "Benchmark run failed");
        return Err(e);
      }
    }
  }

  Ok(())
}
