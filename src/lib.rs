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

//! # Impalab
//!
//! `impalab` is a language-agnostic framework for orchestrating micro-benchmarks.
//! It allows you to define, build, and run benchmark components written in any language,
//! piping data from a generator to one or more algorithm implementations.
//!
//! This crate contains the main library logic for the `impa` CLI, but its
//! core modules (`builder`, `config`, `benchmark`) could be used independently.
//!
//! ## Core Modules
//!
//! * [`builder`]: Contains logic for the `impa build` command. It discovers
//!   `impafile.toml` files, runs optional build steps, and creates the
//!   `impa_manifest.json`.
//! * [`config`]: Handles parsing the `RunArgs` from the CLI, applying overrides,
//!   and resolving all component paths from the manifest to create a `Config` struct.
//! * [`benchmark`]: Contains the `run_benchmarks` function which executes the
//!   generator and algorithm processes, handling `stdin`/`stdout` piping.
//! * [`cli`]: Defines the `clap`-based command-line interface.
//! * [`command`]: Defines the shared `CommandArgs` struct.
//! * [`error`]: Defines the custom error types for the library.
//! * [`logging`]: Provides the `setup_tracing` utility.

pub mod benchmark;
pub mod builder;
pub mod cli;
pub mod command;
pub mod config;
pub mod error;
pub mod logging;
