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
use crate::config::ResolvedConfig;
use crate::config::ResolvedGenerator;
use crate::config::ResolvedTask;
use crate::error::BenchmarkError;
use crate::manifest::ComponentType;
use base64::Engine;
use serde::Serialize;

use std::process::Stdio;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncRead;
use tokio::io::BufReader;
use tokio::process::Child;
use tokio::process::Command;
use tracing::Instrument;

#[derive(Debug, Serialize)]
struct BenchmarkMeta {
  task_index: usize,

  executor: String,

  #[serde(rename = "args", skip_serializing_if = "Vec::is_empty")]
  task_args: Vec<String>,

  rep_index: usize,
  #[serde(skip_serializing_if = "serde_json::Map::is_empty")]
  attributes: serde_json::Map<String, serde_json::Value>,
}

/// Main benchmark runner.
///
/// Takes a fully resolved `Config` and executes the benchmark plan.
/// It handles spawning the generator (if any) and all executor processes (tasks),
/// piping data, and logging results.
pub async fn run_benchmarks(
  ResolvedConfig {
    generator: gen_cmd_args,
    tasks,
  }: ResolvedConfig,
) -> Result<(), BenchmarkError> {
  let gen_info = if let Some(ResolvedGenerator {
    seed,
    command_args: gen_cmd,
    ..
  }) = &gen_cmd_args
  {
    format!(
      "seed = {}, dir = {:?}, generator = {}, args = {:?}",
      seed,
      gen_cmd.working_dir,
      gen_cmd.command.display(),
      gen_cmd.args
    )
  } else {
    "generator = none".to_string()
  };

  let max_reps = tasks.iter().map(|t| t.effective_reps).max().unwrap_or(1);

  let span = tracing::info_span!(
    "run_benchmarks",
    %gen_info
  );

  async {
    tracing::info!("--- Starting Benchmark Pipeline ---");
    for rep_index in 0..max_reps {
      for task in tasks.iter().enumerate() {
        let reps = task.1.effective_reps;
        if rep_index >= reps {
          continue;
        }

        let executor = task.1.executor.clone();
        let exec_span = tracing::info_span!("run_executor", executor = %executor);

        let result = async {
          tracing::info!(
            "Running natively for: {} (rep_index={} out of {} reps)...",
            executor,
            rep_index,
            reps
          );

          match run_pipeline(gen_cmd_args.as_ref(), task, rep_index).await {
            Ok(_) => {
              tracing::info!(
                "Finished running pipeline: {} (rep_index {})",
                executor,
                rep_index
              );
              Ok(())
            }
            Err(e) => {
              tracing::error!(
                error = %e,
                "Pipeline failed for executor: {} (rep_index {})",
                executor,
                rep_index
              );
              Err(e)
            }
          }
        }
        .instrument(exec_span)
        .await;

        result?
      }
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
  generator_cfg: Option<&ResolvedGenerator>,
  (
    task_index,
    ResolvedTask {
      executor: executor_name,
      args: task_args,
      command_args,
      effective_attributes,
      effective_reps,
    },
  ): (usize, &ResolvedTask),
  rep_index: usize,
) -> Result<(), BenchmarkError> {
  let mut gen_child_handle: Option<Child> = None;
  let mut gen_stderr_handle: Option<tokio::task::JoinHandle<Result<(), BenchmarkError>>> = None;

  // --- Configure Executor Command ---
  let mut exec_cmd = Command::new(&command_args.command);
  exec_cmd
    .args(&command_args.args) // Add base args from manifest/override
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true);

  if let Some(dir) = &command_args.working_dir {
    exec_cmd.current_dir(dir);
  }

  exec_cmd
    .env("IMPALAB_COMPONENT_NAME", executor_name)
    .env("IMPALAB_TASK_INDEX", task_index.to_string())
    .env("IMPALAB_REP_INDEX", rep_index.to_string())
    .env("IMPALAB_REPS", effective_reps.to_string())
    .env(
      "IMPALAB_ATTRIBUTES",
      serde_json::to_string(&effective_attributes).unwrap(), // unwrapping here is safe because `effective_attributes` is a `serde_json::Map` with string keys
    );

  // --- Configure Generator (if provided) ---
  if let Some(ResolvedGenerator {
    name,
    seed,
    command_args: gen_command_args,
  }) = generator_cfg
  {
    // --- Pipelined Mode ---
    let mut gen_cmd = Command::new(&gen_command_args.command);
    gen_cmd
      .args(&gen_command_args.args)
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .kill_on_drop(true);

    if let Some(dir) = &gen_command_args.working_dir {
      gen_cmd.current_dir(dir);
    }

    gen_cmd
      .env("IMPALAB_COMPONENT_NAME", name)
      .env("IMPALAB_SEED", seed.to_string())
      .env(
        "IMPALAB_ATTRIBUTES",
        serde_json::to_string(&effective_attributes).unwrap(),
      );

    tracing::debug!(gen_dir = ?gen_command_args.working_dir, "Generator directory");
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
      read_and_log_stderr(gen_stderr, ComponentType::Generator).instrument(
        tracing::info_span!("stderr_handler", component_type = ?ComponentType::Generator),
      ),
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
  let meta = BenchmarkMeta {
    task_index,
    executor: executor_name.clone(),
    task_args: task_args.clone(),
    rep_index,
    attributes: effective_attributes.clone(),
  };
  let stdout_task = tokio::spawn(
    async move { process_executor_stdout(exec_stdout, &meta).await }
      .instrument(tracing::info_span!("stdout_handler", executor = %executor_name)),
  );

  let exec_stderr_task = tokio::spawn(
    read_and_log_stderr(exec_stderr, ComponentType::Executor)
      .instrument(tracing::info_span!("stderr_handler", component_type = ?ComponentType::Executor)),
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

fn extract_gen_meta(token: &str) -> Result<Option<serde_json::Value>, BenchmarkError> {
  if let Some(encoded) = token.strip_prefix("meta:") {
    if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(encoded) {
      match serde_json::from_slice(&decoded) {
        Ok(v) => Ok(Some(v)),
        Err(e) => Err(BenchmarkError::MalformedJSON {
          context: "gen_meta".to_string(),
          raw_segment: token.to_string(),
          source: e,
        }),
      }
    } else {
      Ok(None)
    }
  } else {
    Ok(None)
  }
}

/// Reads lines from the executor's stdout, parses them, and prints them as JSON.
async fn process_executor_stdout<R: AsyncRead + Unpin>(
  stream: R,
  meta: &BenchmarkMeta,
) -> Result<(), BenchmarkError> {
  /// The structure of a single benchmark result, used for JSON serialization.
  #[derive(Debug, Serialize)]
  struct BenchmarkResult<'a> {
    #[serde(flatten)]
    meta: &'a BenchmarkMeta,

    data_token: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    gen_meta: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    exec_meta: Option<serde_json::Value>,

    metric: serde_json::Number,
  }

  let mut reader = BufReader::new(stream).lines();
  while let Some(line) = reader
    .next_line()
    .await
    .map_err(BenchmarkError::ReadExecStdout)?
  {
    if line.is_empty() {
      continue;
    }

    match parse_native_line(&line) {
      Ok((metric, data_token, exec_meta)) => {
        let gen_meta =
          extract_gen_meta(&data_token).map_err(|e| BenchmarkError::MalformedExecOutput {
            line: line.clone(),
            source: Box::new(e),
          })?;

        let result = BenchmarkResult {
          meta,
          gen_meta,
          exec_meta,
          data_token,
          metric,
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
  component_type: ComponentType,
) -> Result<(), BenchmarkError> {
  let mut reader = BufReader::new(stream).lines();

  while let Some(line) = reader
    .next_line()
    .await
    .map_err(|e| BenchmarkError::ReadStderr {
      component_type: component_type.clone(),
      source: e,
    })?
  {
    tracing::warn!(component_type = ?component_type, "{}", line);
  }
  Ok(())
}

/// Parses a single line of `metric|data_token[|exec_meta]` pipe-delimited format.
fn parse_native_line(
  line: &str,
) -> Result<(serde_json::Number, String, Option<serde_json::Value>), BenchmarkError> {
  let parts: Vec<&str> = line.splitn(3, '|').collect();

  if parts.len() < 2 {
    return Err(BenchmarkError::PipeParts {
      parts: parts.len(),
      line: line.to_string(),
    });
  }

  let data_token = parts[1].to_string();
  let metric = serde_json::from_str::<serde_json::Number>(parts[0]).map_err(|e| {
    BenchmarkError::ParseMetric {
      metric: parts[0].to_string(),
      source: e,
    }
  })?;

  let exec_meta = if parts.len() == 3 {
    Some(
      serde_json::from_str(parts[2]).map_err(|e| BenchmarkError::MalformedJSON {
        context: "exec_meta".to_string(),
        raw_segment: parts[2].to_string(),
        source: e,
      })?,
    )
  } else {
    None
  };

  Ok((metric, data_token, exec_meta))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_parse_native_line_valid() {
    let (metric, id, meta) = parse_native_line("45000|run_123").unwrap();
    assert_eq!(id, "run_123");
    assert_eq!(metric, serde_json::Number::from(45000));
    assert!(meta.is_none());
  }

  #[test]
  fn test_parse_native_line_valid_float() {
    let (metric, id, meta) = parse_native_line("45.52|run_123").unwrap();
    assert_eq!(id, "run_123");
    assert_eq!(metric, serde_json::Number::from_f64(45.52).unwrap());
    assert!(meta.is_none());
  }

  #[test]
  fn test_parse_native_line_with_meta() {
    let (metric, id, meta) =
      parse_native_line(r#"450|run_1|{"converged":true,"iters":10}"#).unwrap();
    assert_eq!(id, "run_1");
    assert_eq!(metric, serde_json::Number::from(450));
    let meta = meta.unwrap();
    assert_eq!(meta["converged"], true);
    assert_eq!(meta["iters"], 10);
  }

  #[test]
  fn test_parse_native_line_with_malformed_meta() {
    let res = parse_native_line(r#"450|run_1|{"bad":true"#);

    assert!(matches!(res, Err(BenchmarkError::MalformedJSON { .. })));
  }

  #[test]
  fn test_parse_native_line_newline_failure() {
    let res = parse_native_line("450|run_1|{");

    match res {
      Err(BenchmarkError::MalformedJSON {
        context,
        raw_segment,
        ..
      }) => {
        assert_eq!(context, "exec_meta");
        assert_eq!(raw_segment, "{");
      }
      _ => panic!("Expected MalformedJSON"),
    }
  }

  #[test]
  fn test_parse_native_line_nested_array() {
    let (metric, id, meta) = parse_native_line(r#"450|run_1|[1, 2, {"a": "b"}]"#).unwrap();
    assert_eq!(id, "run_1");
    assert_eq!(metric, serde_json::Number::from(450));
    assert!(meta.unwrap().is_array());
  }

  #[test]
  fn test_parse_native_line_with_nested_pipes_in_meta() {
    let (metric, id, meta) = parse_native_line(r#"450|run_1|{"msg":"foo|bar"}"#).unwrap();
    assert_eq!(id, "run_1");
    assert_eq!(metric, serde_json::Number::from(450));
    assert_eq!(meta.unwrap()["msg"], "foo|bar");
  }

  #[test]
  fn test_extract_gen_meta() {
    // Valid JSON
    let json_str = r#"{"size": 100}"#;
    let encoded = base64::engine::general_purpose::STANDARD.encode(json_str);
    let id = format!("meta:{}", encoded);
    let meta = extract_gen_meta(&id).unwrap();
    assert_eq!(meta.unwrap()["size"], 100);

    // Plain string
    let meta = extract_gen_meta("run_1").unwrap();
    assert!(meta.is_none());

    // Invalid Base64
    let meta = extract_gen_meta("meta:!@#$").unwrap();
    assert!(meta.is_none());

    // Invalid JSON
    let encoded = base64::engine::general_purpose::STANDARD.encode("not_json");
    let id = format!("meta:{}", encoded);
    let res = extract_gen_meta(&id);
    assert!(matches!(res, Err(BenchmarkError::MalformedJSON { .. })));
  }

  #[test]
  fn test_parse_native_line_malformed_parts_too_few() {
    let res = parse_native_line("45000");
    assert!(matches!(
      res,
      Err(BenchmarkError::PipeParts { parts: 1, .. })
    ));
  }

  #[test]
  fn test_parse_native_line_malformed_invalid_metric() {
    let res = parse_native_line("fast|run_123");
    assert!(matches!(res, Err(BenchmarkError::ParseMetric { .. })));
  }
}
