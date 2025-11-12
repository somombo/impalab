use anyhow::Result;
use clap::Parser;
use impalab::benchmark::run_benchmarks;
use impalab::cli::OrchestratorCliParser;
use impalab::config::Config;
use impalab::logging::setup_tracing;
use tracing::Instrument;

#[tokio::main]
async fn main() -> Result<()> {
  setup_tracing()?;

  tracing::info!("--- New run starting ---");

  let cli_args = OrchestratorCliParser::parse();

  let main_span = tracing::info_span!("orchestrator");
  async {
    let config = match Config::try_from(cli_args) {
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

    Ok(())
  }
  .instrument(main_span)
  .await
}
