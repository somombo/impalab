use crate::config::Config;
use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;
use std::path::Path;
use std::process::Stdio;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncRead;
use tokio::io::BufReader;
use tokio::process::Command;
use tracing::Instrument;

/// The structure of a single benchmark result, used for JSON serialization.
#[derive(Debug, Serialize, Deserialize, Clone)]
struct BenchmarkResult {
  id: String,
  language: String,
  function_name: String,
  duration: u64,
}

/// Main benchmark runner.
pub async fn run_benchmarks(
  Config {
    algorithms,
    seed,
    generator_exe,
    sorter_paths,
    generator_args,
  }: Config,
) -> Result<()> {
  let span = tracing::info_span!(
    "run_benchmarks",
    seed = seed,
    generator = %generator_exe.display()
  );

  async {
    tracing::info!("--- Starting Benchmark Pipeline ---");
    tracing::info!(seed = seed, "Using generator seed");
    tracing::info!(args = ?generator_args, "Generator args");

    for (language, functions) in &algorithms {
      let lang_span = tracing::info_span!("run_language", lang = %language);
      async {
        tracing::info!("Running natively for: {}...", language);

        let Some(sorter_path) = sorter_paths.get(language) else {
          tracing::warn!(lang = %language, "No sorter executable path configured. Skipping.");
          return;
        };

        if !sorter_path.exists() {
          tracing::warn!(lang = %language, path = %sorter_path.display(), "Sorter executable not found. Skipping.");
          return;
        }

        match run_pipeline(
          language,
          functions,
          &generator_exe,
          &generator_args,
          seed,
          sorter_path,
        )
        .await
        {
          Ok(_) => tracing::info!("Finished running pipeline: {}", language),
          Err(e) => tracing::error!(error = %e, "Pipeline failed for language: {}", language),
        }
      }
      .instrument(lang_span)
      .await;
    }
    tracing::info!("--- Benchmark run complete ---");
    Ok(())
  }
  .instrument(span)
  .await
}

/// Spawns and manages the generator -> sorter pipeline for one language.
async fn run_pipeline(
  language: &str,
  functions: &[String],
  gen_exe: &Path,
  gen_args: &[String],
  seed: u64,
  sorter_exe: &Path,
) -> Result<()> {
  let mut gen_cmd = Command::new(gen_exe);
  gen_cmd
    .arg(format!("--seed={}", seed))
    .args(gen_args)
    .stdout(Stdio::piped()) // Pipe stdout
    .stderr(Stdio::piped()) // Pipe stderr
    .kill_on_drop(true);

  tracing::debug!(cmd = ?gen_cmd, "Spawning generator");
  let mut gen_child = gen_cmd.spawn().context("Failed to spawn generator")?;

  // Take pipes from generator
  let gen_stdout = gen_child
    .stdout
    .take()
    .context("Failed to pipe generator stdout")?;
  let gen_stderr = gen_child
    .stderr
    .take()
    .context("Failed to pipe generator stderr")?;

  let functions_arg = format!("--functions={}", functions.join(","));
  let mut sorter_cmd = Command::new(sorter_exe);
  let gen_stdout_try: Stdio = gen_stdout.try_into()?;
  sorter_cmd
    .stdin(gen_stdout_try) // Pipe generator's stdout into sorter's stdin
    .stdout(Stdio::piped()) // Pipe sorter's stdout
    .stderr(Stdio::piped()) // Pipe sorter's stderr
    .arg(&functions_arg)
    .kill_on_drop(true);

  tracing::debug!(cmd = ?sorter_cmd, "Spawning sorter");
  let mut sorter_child = sorter_cmd.spawn().context("Failed to spawn sorter")?;

  // Take pipes from sorter
  let sorter_stdout = sorter_child
    .stdout
    .take()
    .context("Failed to pipe sorter stdout")?;
  let sorter_stderr = sorter_child
    .stderr
    .take()
    .context("Failed to pipe sorter stderr")?;

  let lang_clone = language.to_string();

  let stdout_task = tokio::spawn(
    async move { process_sorter_stdout(sorter_stdout, &lang_clone).await }
      .instrument(tracing::info_span!("stdout_handler", lang = %language)),
  );

  let gen_stderr_task = tokio::spawn(
    read_and_log_stderr(gen_stderr, "generator")
      .instrument(tracing::info_span!("stderr_handler", target = "generator")),
  );

  let sorter_stderr_task = tokio::spawn(
    read_and_log_stderr(sorter_stderr, "sorter")
      .instrument(tracing::info_span!("stderr_handler", target = "sorter")),
  );

  let (gen_status, sorter_status) = tokio::try_join!(gen_child.wait(), sorter_child.wait())
    .context("Failed to wait for child processes")?;

  let _ = stdout_task.await??;
  gen_stderr_task.await??;
  sorter_stderr_task.await??;

  if !gen_status.success() {
    tracing::error!(code = ?gen_status.code(), "Generator process failed");
  }
  if !sorter_status.success() {
    tracing::error!(code = ?sorter_status.code(), "Sorter process failed");
  }

  Ok(())
}

/// Reads lines from the sorter's stdout, parses them, and prints them as JSON.
async fn process_sorter_stdout<R: AsyncRead + Unpin>(stream: R, language: &str) -> Result<()> {
  let mut reader = BufReader::new(stream).lines();

  while let Some(line) = reader
    .next_line()
    .await
    .context("Failed to read sorter stdout")?
  {
    if line.is_empty() {
      continue;
    }

    match parse_native_line(&line, language) {
      Ok(result) => {
        let json_result = serde_json::to_string(&result)?;
        println!("{}", json_result);
      }
      Err(e) => {
        tracing::warn!(?line, error = %e, "Warning: Malformed output line");
      }
    }
  }
  Ok(())
}

/// Reads lines from a process's stderr and logs them.
async fn read_and_log_stderr<R: AsyncRead + Unpin>(stream: R, target: &'static str) -> Result<()> {
  let mut reader = BufReader::new(stream).lines();

  while let Some(line) = reader.next_line().await.context("Failed to read stderr")? {
    tracing::warn!(target, "{}", line);
  }
  Ok(())
}

/// Parses a single line of `id,func,duration` CSV.
fn parse_native_line(line: &str, language: &str) -> Result<BenchmarkResult> {
  let parts: Vec<&str> = line.split(',').collect();

  if parts.len() != 3 {
    anyhow::bail!("Expected 3 CSV parts, got {}: {}", parts.len(), line);
  }

  let id = parts[0].to_string();
  let function_name = parts[1].to_string();
  let duration = parts[2]
    .parse::<u64>()
    .context(format!("Failed to parse duration '{}'", parts[2]))?;

  Ok(BenchmarkResult {
    id,
    language: language.to_string(),
    function_name,
    duration,
  })
}
