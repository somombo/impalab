use crate::config::CommandArgs;
use crate::config::Config;
use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;
use std::process::Stdio;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncRead;
use tokio::io::BufReader;
use tokio::process::Child;
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
pub async fn run_benchmarks(config: Config) -> Result<()> {
  let gen_info = if let Some(gen_cmd) = &config.generator_command {
    format!(
      "generator = {}, args = {:?}",
      gen_cmd.exe.display(),
      gen_cmd.args
    )
  } else {
    "generator = none".to_string()
  };

  let span = tracing::info_span!(
    "run_benchmarks",
    %gen_info
  );

  async {
    tracing::info!("--- Starting Benchmark Pipeline ---");
    for (language, functions) in &config.algorithms {
      let lang_span = tracing::info_span!("run_language", lang = %language);
      async {
        tracing::info!("Running natively for: {}...", language);

        let Some(algorithm_cmd_args) = config.algorithm_commands.get(language) else {
          tracing::error!(lang = %language, "Internal error: No command found for language. Skipping.");
          return;
        };

        if !algorithm_cmd_args.exe.exists() {
          tracing::warn!(lang = %language, path = %algorithm_cmd_args.exe.display(), "Algorithm executable not found. Skipping.");
          return;
        }

        match run_pipeline(
          config.generator_command.as_ref(),
          algorithm_cmd_args,
          language,
          functions,
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

/// Spawns and manages the generator -> algorithm pipeline for one language.
/// Handles both pipelined and self-contained (no generator) runs.
async fn run_pipeline(
  generator_cmd_args: Option<&CommandArgs>,
  algorithm_cmd_args: &CommandArgs,
  language: &str,
  functions: &[String],
) -> Result<()> {
  let mut gen_child_handle: Option<Child> = None;
  let mut gen_stderr_handle: Option<tokio::task::JoinHandle<Result<()>>> = None;

  // --- Configure Algorithm Command ---
  let mut algo_cmd = Command::new(&algorithm_cmd_args.exe);

  // --- Configure Generator (if provided) ---
  if let Some(CommandArgs {
    args: gen_args,
    exe: gen_exe,
  }) = generator_cmd_args
  {
    // --- Pipelined Mode ---
    let mut gen_cmd = Command::new(gen_exe);
    gen_cmd
      .args(gen_args)
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
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

    // Pipe generator's stdout into algorithm's stdin
    let gen_stdout_try: Stdio = gen_stdout.try_into()?;
    algo_cmd.stdin(gen_stdout_try);

    // Spawn task to log generator's stderr
    gen_stderr_handle = Some(tokio::spawn(
      read_and_log_stderr(gen_stderr, "generator")
        .instrument(tracing::info_span!("stderr_handler", target = "generator")),
    ));

    gen_child_handle = Some(gen_child);
  } else {
    // --- Self-Contained Mode ---
    tracing::debug!("Running algorithm in self-contained mode (no generator)");
    algo_cmd.stdin(Stdio::null());
  }

  let functions_arg = format!("--functions={}", functions.join(","));
  algo_cmd
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .arg(&functions_arg)
    .kill_on_drop(true);

  // --- Spawn Algorithm Process ---
  tracing::debug!(cmd = ?algo_cmd, "Spawning algorithm component");
  let mut algo_child = algo_cmd
    .spawn()
    .context("Failed to spawn algorithm component")?;

  // Take pipes from sorter
  let algo_stdout = algo_child
    .stdout
    .take()
    .context("Failed to pipe algorithm stdout")?;
  let algo_stderr = algo_child
    .stderr
    .take()
    .context("Failed to pipe algorithm stderr")?;

  // --- Concurrently process all IO ---
  let lang_clone = language.to_string();

  let stdout_task = tokio::spawn(
    async move { process_algorithm_stdout(algo_stdout, &lang_clone).await }
      .instrument(tracing::info_span!("stdout_handler", lang = %language)),
  );

  let algo_stderr_task = tokio::spawn(
    read_and_log_stderr(algo_stderr, "algorithm")
      .instrument(tracing::info_span!("stderr_handler", target = "algorithm")),
  );

  // --- Wait for processes to exit ---
  let (gen_status, algo_status) = if let Some(mut gen_child) = gen_child_handle {
    // Pipelined mode: Wait on both

    let (gen_res, algo_res) = tokio::try_join!(gen_child.wait(), algo_child.wait())
      .context("Failed to wait for child processes")?;
    (Some(gen_res), algo_res)
  } else {
    // Self-contained mode: Wait only on algorithm
    let algo_res = algo_child
      .wait()
      .await
      .context("Failed to wait for algorithm process")?;
    (None, algo_res)
  };

  // --- Wait for IO tasks to finish ---
  if let Some(handle) = gen_stderr_handle {
    handle.await??; // Propagate generator stderr task errors
  }

  stdout_task.await??;
  algo_stderr_task.await??;

  // --- Check exit statuses ---
  if let Some(gen_status) = gen_status {
    if !gen_status.success() {
      tracing::error!(code = ?gen_status.code(), "Generator process failed");
    }
  }
  if !algo_status.success() {
    tracing::error!(code = ?algo_status.code(), "Algorithm process failed");
  }

  Ok(())
}

/// Reads lines from the algorithm's stdout, parses them, and prints them as JSON.
async fn process_algorithm_stdout<R: AsyncRead + Unpin>(stream: R, language: &str) -> Result<()> {
  let mut reader = BufReader::new(stream).lines();

  while let Some(line) = reader
    .next_line()
    .await
    .context("Failed to read algorithm stdout")?
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
        tracing::warn!(?line, error = %e, "Warning: Malformed output line from algorithm");
      }
    }
  }
  Ok(())
}

/// Reads lines from a process's stderr and logs them.
async fn read_and_log_stderr<R: AsyncRead + Unpin>(stream: R, target: &'static str) -> Result<()> {
  let mut reader = BufReader::new(stream).lines();

  while let Some(line) = reader
    .next_line()
    .await
    .context(format!("Failed to read {} stderr", target))?
  {
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
