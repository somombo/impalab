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
use assert_cmd::Command;
use assert_cmd::cargo;
use predicates::prelude::*;
use tempfile::tempdir;

use fs_extra::dir::CopyOptions;
use fs_extra::dir::copy;
use std::fs;

use serde_json::Value;

#[test]
fn test_build_no_components() {
  let temp = tempdir().unwrap();

  let mut cmd = Command::new(cargo::cargo_bin!("impa"));
  cmd
    .arg("build")
    .arg("--components-dir")
    .arg(temp.path())
    .arg("--root-dir")
    .arg(temp.path())
    .arg("--manifest-filename")
    .arg("test_manifest.json")
    .env("RUST_LOG", "info")
    .env("NO_COLOR", "1");

  cmd
    .assert()
    .success()
    .stderr(predicate::str::contains("Build manifest written"));
}

#[test]
fn test_run_no_manifest_or_overrides() {
  let temp = tempdir().unwrap();
  let config_path = temp.path().join("config.json");
  fs::write(
    &config_path,
    r#"{
    "tasks": [
      {"executor": "test-executor", "args": ["test-arg"]}
    ]
  }"#,
  )
  .unwrap();

  let mut cmd = Command::new(cargo::cargo_bin!("impa"));

  cmd
    .arg("run")
    .arg("--set")
    .arg("generator.name=none")
    .arg("--config")
    .arg(&config_path)
    .arg("--manifest-filename")
    .arg("non_existent_manifest.json")
    .env("NO_COLOR", "1");

  cmd.assert().failure().stderr(predicate::str::contains(
    "Component resolution graph validation failed:",
  ));
}

#[test]
fn test_build_and_run_e2e() {
  // Setup: Create temp dir and copy fixtures
  let temp = tempdir().unwrap();
  let components_dir = temp.path().join("components");
  fs::create_dir_all(&components_dir).unwrap();

  // Copy our ./tests/fixtures dir into the temp components_dir
  let options = CopyOptions::new();
  copy("tests/fixtures", temp.path(), &options).unwrap();
  fs::rename(temp.path().join("fixtures"), &components_dir).unwrap();

  // --- Test `impa build` ---

  let mut build_cmd = Command::new(cargo::cargo_bin!("impa"));
  build_cmd
    .arg("build")
    .arg("--components-dir")
    .arg(&components_dir)
    .arg("--root-dir")
    .arg(temp.path())
    .arg("--manifest-filename")
    .arg("e2e_manifest.json")
    .env("RUST_LOG", "info")
    .env("NO_COLOR", "1");

  // Assert build success
  build_cmd
    .assert()
    .success()
    .stderr(predicate::str::contains("Build Process Complete"));

  {
    let manifest_path = temp.path().join("e2e_manifest.json");
    // Verify manifest content
    let manifest_content = fs::read_to_string(&manifest_path).unwrap();
    let manifest_json: Value = serde_json::from_str(&manifest_content).unwrap();

    assert_eq!(
      manifest_json["components"]["py-gen-e2e"]["command"],
      "python3"
    );
    assert_eq!(
      manifest_json["components"]["python-e2e"]["command"],
      "python3"
    );
  }

  // --- Test `impa run` ---
  let config_path = temp.path().join("config.json");
  fs::write(
    &config_path,
    r#"{
    "tasks": [
      {"executor": "python-e2e", "args": ["test_func_1"]},
      {"executor": "python-e2e", "args": ["test_func_2", "--foo=true", "--bars=-100"]}
    ]
  }"#,
  )
  .unwrap();

  let mut run_cmd = Command::new(cargo::cargo_bin!("impa"));
  run_cmd
    .arg("run")
    .arg("--set")
    .arg("generator.name=py-gen-e2e")
    .arg("--set")
    .arg("generator.seed=42")
    .arg("--root-dir")
    .arg(temp.path())
    .arg("--manifest-filename")
    .arg("e2e_manifest.json")
    .arg("--config")
    .arg(&config_path)
    // .env("CLICOLOR", "0")
    .env("NO_COLOR", "1");

  // Assert run success and check the JSONL output
  run_cmd
    .assert()
    .success()
    .stdout(
      predicate::str::contains(r#"{"task_index":0,"executor":"python-e2e","args":["test_func_1"],"labels":{},"data_id":"test_case_1","duration":1234}"#)
    )
    .stdout(
      predicate::str::contains(r#"{"task_index":1,"executor":"python-e2e","args":["test_func_2","--foo=true","--bars=-100"],"labels":{},"data_id":"test_case_1","duration":12}"#)
    );
}

#[test]
fn test_build_and_run_e2e_stdin_config() {
  // Setup: Create temp dir and copy fixtures
  let temp = tempdir().unwrap();
  let components_dir = temp.path().join("components");
  fs::create_dir_all(&components_dir).unwrap();

  // Copy our ./tests/fixtures dir into the temp components_dir
  let options = CopyOptions::new();
  copy("tests/fixtures", temp.path(), &options).unwrap();
  fs::rename(temp.path().join("fixtures"), &components_dir).unwrap();

  // --- Test `impa build` ---

  let mut build_cmd = Command::new(cargo::cargo_bin!("impa"));
  build_cmd
    .arg("build")
    .arg("--components-dir")
    .arg(&components_dir)
    .arg("--root-dir")
    .arg(temp.path())
    .arg("--manifest-filename")
    .arg("e2e_manifest.json")
    .env("RUST_LOG", "info")
    .env("NO_COLOR", "1");

  // Assert build success
  build_cmd
    .assert()
    .success()
    .stderr(predicate::str::contains("Build Process Complete"));

  {
    let manifest_path = temp.path().join("e2e_manifest.json");
    // Verify manifest content
    let manifest_content = fs::read_to_string(&manifest_path).unwrap();
    let manifest_json: Value = serde_json::from_str(&manifest_content).unwrap();

    assert_eq!(
      manifest_json["components"]["py-gen-e2e"]["command"],
      "python3"
    );
    assert_eq!(
      manifest_json["components"]["python-e2e"]["command"],
      "python3"
    );
  }

  // --- Test `impa run` ---
  let config_str = r#"{
    "tasks": [
      {"executor": "python-e2e", "args": ["test_func_1"]},
      {"executor": "python-e2e", "args": ["test_func_2", "--foo=true", "--bars=-100"]}
    ]
  }"#;

  let mut run_cmd = Command::new(cargo::cargo_bin!("impa"));
  run_cmd
    .arg("run")
    .arg("--set")
    .arg("generator.name=py-gen-e2e")
    .arg("--set")
    .arg("generator.seed=42")
    .arg("--root-dir")
    .arg(temp.path())
    .arg("--manifest-filename")
    .arg("e2e_manifest.json")
    .arg("--config")
    .arg("-")
    // .env("CLICOLOR", "0")
    .env("NO_COLOR", "1")
    .write_stdin(config_str);

  // Assert run success and check the JSONL output
  run_cmd
    .assert()
    .success()
    .stdout(
      predicate::str::contains(r#"{"task_index":0,"executor":"python-e2e","args":["test_func_1"],"labels":{},"data_id":"test_case_1","duration":1234}"#)
    )
    .stdout(
      predicate::str::contains(r#"{"task_index":1,"executor":"python-e2e","args":["test_func_2","--foo=true","--bars=-100"],"labels":{},"data_id":"test_case_1","duration":12}"#)
    );
}

#[test]
fn test_build_with_filters() {
  let temp = tempdir().unwrap();
  let components_dir = temp.path().join("components");
  fs::create_dir_all(&components_dir).unwrap();

  let options = CopyOptions::new();
  copy("tests/fixtures", temp.path(), &options).unwrap();
  fs::rename(temp.path().join("fixtures"), &components_dir).unwrap();

  // Test --include
  let mut include_cmd = Command::new(cargo::cargo_bin!("impa"));
  include_cmd
    .arg("build")
    .arg("--components-dir")
    .arg(&components_dir)
    .arg("--root-dir")
    .arg(temp.path())
    .arg("--manifest-filename")
    .arg("include_manifest.json")
    .arg("--include")
    .arg("py-gen-e2e")
    .env("NO_COLOR", "1");

  include_cmd.assert().success();

  let manifest_path = temp.path().join("include_manifest.json");
  let manifest_content = fs::read_to_string(&manifest_path).unwrap();
  let manifest_json: Value = serde_json::from_str(&manifest_content).unwrap();

  assert!(manifest_json["components"].get("py-gen-e2e").is_some());
  assert!(manifest_json["components"].get("python-e2e").is_none());

  // Test --exclude
  let mut exclude_cmd = Command::new(cargo::cargo_bin!("impa"));
  exclude_cmd
    .arg("build")
    .arg("--components-dir")
    .arg(&components_dir)
    .arg("--root-dir")
    .arg(temp.path())
    .arg("--manifest-filename")
    .arg("exclude_manifest.json")
    .arg("--exclude")
    .arg("py-gen-e2e")
    .env("NO_COLOR", "1");

  exclude_cmd.assert().success();

  let manifest_path = temp.path().join("exclude_manifest.json");
  let manifest_content = fs::read_to_string(&manifest_path).unwrap();
  let manifest_json: Value = serde_json::from_str(&manifest_content).unwrap();

  assert!(manifest_json["components"].get("py-gen-e2e").is_none());
  assert!(manifest_json["components"].get("python-e2e").is_some());
}

#[test]
fn test_labels_e2e() {
  // Setup: Create temp dir and copy fixtures
  let temp = tempdir().unwrap();
  let components_dir = temp.path().join("components");
  fs::create_dir_all(&components_dir).unwrap();

  // Copy our ./tests/fixtures dir into the temp components_dir
  let options = CopyOptions::new();
  copy("tests/fixtures", temp.path(), &options).unwrap();
  fs::rename(temp.path().join("fixtures"), &components_dir).unwrap();

  // --- Test `impa build` ---
  let mut build_cmd = Command::new(cargo::cargo_bin!("impa"));
  build_cmd
    .arg("build")
    .arg("--components-dir")
    .arg(&components_dir)
    .arg("--root-dir")
    .arg(temp.path())
    .arg("--manifest-filename")
    .arg("e2e_manifest.json")
    .env("RUST_LOG", "info")
    .env("NO_COLOR", "1");

  build_cmd.assert().success();

  // --- Test `impa run` ---
  let config_str = r#"{
    "labels": {"global": "foo"},
    "tasks": [
      {"executor": "python-e2e", "args": ["test_func_1"], "labels": {"task": "bar"}}
    ]
  }"#;

  let mut run_cmd = Command::new(cargo::cargo_bin!("impa"));
  run_cmd
    .arg("run")
    .arg("--set")
    .arg("generator.name=py-gen-e2e")
    .arg("--set")
    .arg("generator.seed=42")
    .arg("--root-dir")
    .arg(temp.path())
    .arg("--manifest-filename")
    .arg("e2e_manifest.json")
    .arg("--config")
    .arg("-")
    .env("NO_COLOR", "1")
    .write_stdin(config_str);

  // Assert run success and check the JSONL output
  // Labels should be merged: {"global": "foo", "task": "bar"}
  run_cmd
    .assert()
    .success()
    .stdout(predicate::str::contains(r#""global":"foo""#))
    .stdout(predicate::str::contains(r#""task":"bar""#));
}
