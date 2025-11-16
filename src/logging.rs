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
use anyhow::Result;
use std::env;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// Sets up the global tracing subscriber.
///
/// Reads the `BENCH_LOG_FILE` env var.
/// - If set, logs to that file.
/// - If not set, logs to stderr.
///
/// Log level is controlled by the `RUST_LOG` env var (e.g., `RUST_LOG=info`).
pub fn setup_tracing() -> Result<()> {
  let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

  match env::var("BENCH_LOG_FILE") {
    Ok(log_file) if !log_file.is_empty() => {
      // Log to a file
      let file_appender = tracing_appender::rolling::never(".", log_file);
      let (non_blocking_writer, _guard) = tracing_appender::non_blocking(file_appender);

      tracing_subscriber::registry()
        .with(env_filter)
        .with(
          fmt::layer()
            .with_writer(non_blocking_writer)
            .with_ansi(false), // No ANSI colors in files
        )
        .init();
    }
    _ => {
      // Log to stderr
      tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().with_writer(std::io::stderr))
        .init();
    }
  }

  Ok(())
}
