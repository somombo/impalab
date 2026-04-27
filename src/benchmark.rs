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
use crate::config::DataGen;
use crate::config::RootedManifest;
use crate::config::Task;
use crate::config::Tasks;
use crate::error::BenchmarkError;
use crate::manifest::CommandArgs;
use crate::manifest::ManifestComponent;
use seahash::SeaHasher;
use serde::Serialize;
use std::hash::Hash;
use std::hash::Hasher;
use std::process::Stdio;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncRead;
use tokio::io::BufReader;
use tokio::process::Child;
use tokio::process::Command;
use tracing::Instrument;

/// The structure of a single benchmark result, used for JSON serialization.
#[derive(Debug, Serialize)]
struct BenchmarkResult<'a, 'b> {
  task_index: usize,
  task_hash: &'a str,

  #[serde(flatten)]
  task: &'b Task,

  data_id: String,
  duration: u64,
}

/// Main benchmark runner.
///
/// Takes a fully resolved `Config` and executes the benchmark plan.
/// It handles spawning the generator (if any) and all executor processes (tasks),
/// piping data, and logging results.
pub async fn run_benchmarks(
  data_gen: Option<DataGen>,
  tasks: Tasks,
  manifest: RootedManifest,
) -> Result<(), BenchmarkError> {
  // Resolve Generator (Priority: Override -> Manifest -> Fail)
  let gen_cmd_args = manifest.resolve_generator(data_gen)?;

  let gen_info = if let Some(gen_cmd) = &gen_cmd_args {
    format!(
      "dir = {:?}, generator = {}, args = {:?}",
      gen_cmd.run.working_dir,
      gen_cmd.run.command.display(),
      gen_cmd.run.args
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
    for enum_task in tasks.into_iter().enumerate() {
      let executor = enum_task.1.executor_name.clone();
      let lang_span = tracing::info_span!("run_language", executor = %executor);

      // Resolve Executor (Priority: Override -> Manifest -> Fail)
      let exec_cmd_args = manifest.resolve_executor(&enum_task.1)?;

      let result = async {
        tracing::info!("Running natively for: {}...", executor);

        match run_pipeline(gen_cmd_args.as_ref(), exec_cmd_args, enum_task).await {
          Ok(_) => {
            tracing::info!("Finished running pipeline: {}", executor);
            Ok(())
          }
          Err(e) => {
            tracing::error!(error = %e, "Pipeline failed for language: {}", executor);
            Err(e) // Propagate the error
          }
        }
      }
      .instrument(lang_span)
      .await;

      result?
    }
    tracing::info!("--- Benchmark run complete ---");
    Ok(())
  }
  .instrument(span)
  .await
}

/// Spawns and manages the generator -> executor pipeline for one language.
/// Handles both pipelined and self-contained (no generator) runs.
async fn run_pipeline(
  generator_cmd_args: Option<&ManifestComponent>,
  ManifestComponent {
    run:
      CommandArgs {
        command: exec_cmd_path,
        args: exec_args,
        working_dir: exec_dir,
      },
    ..
  }: ManifestComponent,
  (task_index, task): (usize, Task),
) -> Result<(), BenchmarkError> {
  let mut gen_child_handle: Option<Child> = None;
  let mut gen_stderr_handle: Option<tokio::task::JoinHandle<Result<(), BenchmarkError>>> = None;

  // --- Configure Executor Command ---
  let mut exec_cmd = Command::new(exec_cmd_path);
  exec_cmd
    .args(exec_args) // Add base args from manifest/override
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true);

  if let Some(dir) = exec_dir {
    exec_cmd.current_dir(dir);
  }

  // --- Configure Generator (if provided) ---
  if let Some(ManifestComponent {
    run:
      CommandArgs {
        args: gen_args,
        command: gen_cmd_path,
        working_dir: gen_dir,
      },
    ..
  }) = generator_cmd_args
  {
    // --- Pipelined Mode ---
    let mut gen_cmd = Command::new(gen_cmd_path);
    gen_cmd
      .args(gen_args)
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .kill_on_drop(true);

    if let Some(dir) = gen_dir {
      gen_cmd.current_dir(dir);
    }

    tracing::debug!(gen_dir = ?gen_dir, "Generator directory");
    tracing::debug!(cmd = ?gen_cmd, "Spawning generator");
    let mut gen_child = gen_cmd.spawn().map_err(BenchmarkError::SpawnGenerator)?;

    // Take pipes from generator
    let gen_stdout = gen_child
      .stdout
      .take()
      .ok_or(BenchmarkError::PipeGenStdout)?;
    let gen_stderr = gen_child
      .stderr
      .take()
      .ok_or(BenchmarkError::PipeGenStderr)?;

    // Pipe generator's stdout into executor's stdin
    let gen_stdout_try: Stdio = gen_stdout
      .try_into()
      .map_err(BenchmarkError::ConvertGenStdout)?;
    exec_cmd.stdin(gen_stdout_try);

    // Spawn task to log generator's stderr
    gen_stderr_handle = Some(tokio::spawn(
      read_and_log_stderr(gen_stderr, "generator")
        .instrument(tracing::info_span!("stderr_handler", target = "generator")),
    ));

    gen_child_handle = Some(gen_child);
  } else {
    // --- Self-Contained Mode ---
    tracing::debug!("Running executor in self-contained mode (no generator)");
    exec_cmd.stdin(Stdio::null());
  }

  // --- Spawn Executor Process ---
  tracing::debug!(cmd = ?exec_cmd, "Spawning executor component");
  let mut exec_child = exec_cmd.spawn().map_err(BenchmarkError::SpawnExecutor)?;

  let exec_stdout = exec_child
    .stdout
    .take()
    .ok_or(BenchmarkError::PipeExecStdout)?;
  let exec_stderr = exec_child
    .stderr
    .take()
    .ok_or(BenchmarkError::PipeExecStderr)?;

  // --- Concurrently process all IO ---
  let executor_name = &task.executor_name;
  let task_ = task.clone();
  let stdout_task = tokio::spawn(
    async move { process_executor_stdout(exec_stdout, task_index, &task_).await }
      .instrument(tracing::info_span!("stdout_handler", executor = %executor_name)),
  );

  let exec_stderr_task = tokio::spawn(
    read_and_log_stderr(exec_stderr, "executor")
      .instrument(tracing::info_span!("stderr_handler", target = "executor")),
  );

  // --- Wait for processes to exit ---
  let (gen_status, exec_status) = if let Some(mut gen_child) = gen_child_handle {
    // Pipelined mode: Wait on both

    let (gen_res, exec_res) =
      tokio::try_join!(gen_child.wait(), exec_child.wait()).map_err(BenchmarkError::WaitChild)?;
    (Some(gen_res), exec_res)
  } else {
    // Self-contained mode: Wait only on executor
    let exec_res = exec_child.wait().await.map_err(BenchmarkError::WaitExec)?;
    (None, exec_res)
  };

  // --- Wait for IO tasks to finish ---
  if let Some(handle) = gen_stderr_handle {
    handle.await.map_err(BenchmarkError::GenStderrTask)??;
  }

  stdout_task.await.map_err(BenchmarkError::StdoutTask)??;
  exec_stderr_task
    .await
    .map_err(BenchmarkError::ExecStderrTask)??;

  // --- Check exit statuses ---
  if let Some(gen_status) = gen_status
    && !gen_status.success()
  {
    tracing::error!(code = ?gen_status.code(), "Generator process failed");
    return Err(BenchmarkError::GeneratorProcessFailed {
      code: gen_status.code(),
    });
  }
  if !exec_status.success() {
    tracing::error!(code = ?exec_status.code(), "Executor process failed");
    return Err(BenchmarkError::ExecutorProcessFailed {
      code: exec_status.code(),
    });
  }

  Ok(())
}

impl Task {
  pub fn get_hash(&self) -> String {
    let mut hasher = SeaHasher::new();
    self.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
  }
}
/// Reads lines from the executor's stdout, parses them, and prints them as JSON.
async fn process_executor_stdout<R: AsyncRead + Unpin>(
  stream: R,
  task_index: usize,
  task: &Task,
) -> Result<(), BenchmarkError> {
  let mut reader = BufReader::new(stream).lines();
  let task_hash = task.get_hash();
  while let Some(line) = reader
    .next_line()
    .await
    .map_err(BenchmarkError::ReadExecStdout)?
  {
    if line.is_empty() {
      continue;
    }

    match parse_native_line(&line) {
      Ok((data_id, duration)) => {
        let result = BenchmarkResult {
          task_index,
          task_hash: &task_hash,
          task,
          data_id,
          duration,
        };
        let json_result =
          serde_json::to_string(&result).map_err(BenchmarkError::SerializeResult)?;
        tracing::debug!(parse_native_line = json_result, "Enriched Output");
        println!("{}", json_result);
      }
      Err(e) => {
        let wrapped_err = BenchmarkError::MalformedExecOutput {
          line: line.clone(),
          source: Box::new(e),
        };
        tracing::error!(?line, error = %wrapped_err, "Error: Malformed output line from executor");
        return Err(wrapped_err);
      }
    }
  }
  Ok(())
}

/// Reads lines from a process's stderr and logs them.
async fn read_and_log_stderr<R: AsyncRead + Unpin>(
  stream: R,
  target: &'static str,
) -> Result<(), BenchmarkError> {
  let mut reader = BufReader::new(stream).lines();

  while let Some(line) = reader
    .next_line()
    .await
    .map_err(|e| BenchmarkError::ReadStderr { target, source: e })?
  {
    tracing::warn!(target, "{}", line);
  }
  Ok(())
}

/// Parses a single line of `data_id,duration` CSV.
fn parse_native_line(line: &str) -> Result<(String, u64), BenchmarkError> {
  let parts: Vec<&str> = line.split(',').collect();

  if parts.len() != 2 {
    return Err(BenchmarkError::CsvParts {
      parts: parts.len(),
      line: line.to_string(),
    });
  }

  let data_id = parts[0].to_string();
  let duration = parts[1]
    .parse::<u64>()
    .map_err(|e| BenchmarkError::ParseDuration {
      duration: parts[1].to_string(),
      source: e,
    })?;

  Ok((data_id, duration))
}
