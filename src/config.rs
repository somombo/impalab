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
use crate::cli::RunArgs;
use crate::error::ConfigError;
use crate::manifest::CommandArgs;
use crate::manifest::ComponentType;
use crate::manifest::ManifestComponent;

use crate::figment_ext::*;

use serde::Deserialize;

use std::collections::HashMap;
use std::io::IsTerminal;
use std::io::Read;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Clone, Default)]
struct RawConfig {
  generator: Option<RawGenerator>,
  tasks: Option<Vec<Task>>,
  #[serde(default)]
  components: HashMap<String, ManifestComponent>,
  reps: Option<usize>,
  #[serde(default)]
  labels: HashMap<String, String>,
}

impl RawConfig {
  fn resolve_component(
    &self,
    component_name: &str,
    component_type: ComponentType,
    root_dir: &std::path::Path,
  ) -> Result<ManifestComponent, ConfigError> {
    let Some(cmp) = self.components.get(component_name) else {
      return Err(ConfigError::ComponentNotFound {
        component_name: component_name.to_owned(),
        available: self
          .components
          .iter()
          .filter(|(_, c)| c.component_type == component_type)
          .map(|(k, _)| k.to_owned())
          .collect(),
      });
    };

    tracing::debug!(
      "Using `{:?}` command from manifest '{}'",
      component_type,
      component_name
    );
    if component_type != cmp.component_type {
      return Err(ConfigError::IncorrectComponentType {
        component_name: component_name.to_owned(),
        component_type,
      });
    }

    let mut cmp = cmp.clone();

    if let Some(ref mut wd) = cmp.run.working_dir {
      *wd = root_dir.join(&wd);
    }

    Ok(cmp)
  }

  fn resolve_all(&self, root_dir: &std::path::Path) -> Result<ResolvedConfig, ConfigError> {
    let mut errors = Vec::new();

    let mut resolved_generator = None;
    if let Some(generator_cfg) = self.generator.as_ref() {
      match self.resolve_component(&generator_cfg.name, ComponentType::Generator, root_dir) {
        Ok(mut cmp) => {
          let seed = generator_cfg.seed.unwrap_or_else(rand::random);
          tracing::info!(seed, "Using generator seed");
          cmp.run.args.push(format!("--seed={}", seed));
          cmp.run.args.extend(generator_cfg.args.to_owned());
          resolved_generator = Some(cmp.run);
        }
        Err(e) => errors.push(e),
      }
    }

    let mut resolved_tasks = Vec::new();
    if let Some(tasks) = self.tasks.as_ref() {
      for task in tasks {
        match self.resolve_component(&task.executor_name, ComponentType::Executor, root_dir) {
          Ok(mut cmp) => {
            cmp.run.args.extend(task.args.clone());

            let effective_reps = task.reps.or(self.reps).unwrap_or(1);

            if effective_reps == 0 {
              tracing::warn!(
                "Task with executor '{}' has 0 reps.. Skipping its execution",
                task.executor_name
              );
              continue;
            }

            let mut effective_labels = self.labels.clone();
            effective_labels.extend(task.labels.clone());

            resolved_tasks.push(ResolvedTask {
              executor: task.executor_name.clone(),
              args: task.args.clone(),
              command_args: cmp.run,

              effective_reps,
              effective_labels,
            });
          }
          Err(e) => errors.push(e),
        }
      }
    }

    if !errors.is_empty() {
      return Err(ConfigError::GraphValidationFailed(errors));
    }

    Ok(ResolvedConfig {
      generator: resolved_generator,
      tasks: resolved_tasks,
    })
  }
}

#[derive(Debug, Deserialize, Clone)]
pub struct Task {
  #[serde(rename = "executor")]
  pub executor_name: String,

  #[serde(default)]
  pub args: Vec<String>,

  pub reps: Option<usize>,
  #[serde(default)]
  pub labels: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedTask {
  pub executor: String,
  pub args: Vec<String>,
  pub command_args: CommandArgs,
  pub effective_reps: usize,
  pub effective_labels: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedConfig {
  pub generator: Option<CommandArgs>,
  pub tasks: Vec<ResolvedTask>,
}

#[derive(Debug, Deserialize, Clone)]
struct RawGenerator {
  name: String,
  seed: Option<u64>,
  #[serde(default)]
  args: Vec<String>,
}

enum ConfigSource {
  File(PathBuf),
  String(String),
}

impl RawConfig {
  fn build(
    base_manifest: ConfigSource,
    config_source: Option<ConfigSource>,
    cli_overrides: HashMap<String, String>,
  ) -> Result<Self, ConfigError> {
    let p_base = match base_manifest {
      ConfigSource::File(p) => Figment::from(figment::providers::Json::file(p)),
      ConfigSource::String(s) => Figment::from(figment::providers::Json::string(&s)),
    };

    let p_mid = config_source.map(|src| match src {
      ConfigSource::File(p) => Figment::from(figment::providers::Json::file(p)),
      ConfigSource::String(s) => Figment::from(figment::providers::Json::string(&s)),
    });

    let mut p_top = Figment::new();
    for (k, v) in &cli_overrides {
      p_top = p_top.merge(SingleOverride { key: k, value: v });
    }

    let name_base = p_base.extract_inner::<String>("generator.name").ok();

    let name_mid = p_mid
      .as_ref()
      .and_then(|f| f.extract_inner::<String>("generator.name").ok());
    let name_top = p_top.extract_inner::<String>("generator.name").ok();

    let effective_name = name_top
      .clone()
      .or_else(|| name_mid.clone())
      .or_else(|| name_base.clone());

    let tasks_mid = p_mid
      .as_ref()
      .is_some_and(|f| f.extract_inner::<figment::value::Value>("tasks").is_ok());
    let tasks_top = p_top
      .extract_inner::<figment::value::Value>("tasks")
      .is_ok();

    let mut figment = Figment::new();

    // Base Layer
    {
      let mut base = p_base;
      if let Some(n) = &name_base
        && effective_name.as_ref() != Some(n)
      {
        base = Figment::from(StripKey {
          provider: base,
          key: "generator",
        });
      }
      if tasks_mid || tasks_top {
        base = Figment::from(StripKey {
          provider: base,
          key: "tasks",
        });
      }
      figment = figment.merge(base);
    }

    // Middle Layer
    if let Some(mut mid) = p_mid {
      if let Some(n) = &name_mid
        && effective_name.as_ref() != Some(n)
      {
        mid = Figment::from(StripKey {
          provider: mid,
          key: "generator",
        });
      }
      if tasks_top {
        mid = Figment::from(StripKey {
          provider: mid,
          key: "tasks",
        });
      }
      figment = figment.merge(mid);
    }

    // Top Layer
    figment = figment.merge(p_top);

    let raw: RawConfig = figment
      .extract()
      .map_err(|err| ConfigError::FigmentError(Box::new(err)))?;
    Ok(raw)
  }
}

fn parse_cli_overrides(overrides: &[String]) -> Result<HashMap<String, String>, ConfigError> {
  let mut map = HashMap::new();
  for override_str in overrides {
    let (key, value) = override_str
      .split_once('=')
      .ok_or_else(|| ConfigError::InvalidOverrideFormat(override_str.to_string()))?;

    if key.contains('[') || key.contains(']') {
      return Err(ConfigError::ArrayOverrideNotSupported {
        key: key.to_string(),
      });
    }

    for segment in key.split('.') {
      if segment.parse::<usize>().is_ok() {
        return Err(ConfigError::ArrayOverrideNotSupported {
          key: key.to_string(),
        });
      }
    }

    map.insert(key.to_string(), value.to_string());
  }
  Ok(map)
}

fn read_config_source<F: crate::cli::FileReader>(
  config_path: Option<&std::path::PathBuf>,
  file_reader: &F,
) -> Result<Option<String>, ConfigError> {
  if let Some(path) = config_path {
    if path.as_os_str() == "-" {
      if std::io::stdin().is_terminal() {
        return Err(ConfigError::MissingStdinData);
      }
      let mut buffer = String::new();
      std::io::stdin()
        .read_to_string(&mut buffer)
        .map_err(ConfigError::ReadStdin)?;
      return Ok(Some(buffer));
    } else {
      return file_reader
        .read_to_string(path)
        .map_err(|e| ConfigError::ReadManifest {
          path: path.to_owned(),
          source: e,
        });
    }
  }
  Ok(None)
}

impl TryFrom<RunArgs> for ResolvedConfig {
  type Error = ConfigError;

  fn try_from(
    RunArgs {
      manifest,
      config,
      overrides,
    }: RunArgs,
  ) -> Result<Self, Self::Error> {
    let cli_overrides = parse_cli_overrides(&overrides)?;
    let config_src =
      read_config_source(config.as_ref(), &manifest.file_reader)?.map(ConfigSource::String);

    let raw_config = RawConfig::build(
      ConfigSource::File(manifest.get_path()),
      config_src,
      cli_overrides,
    )?;
    let resolved = raw_config.resolve_all(&manifest.root_dir)?;

    Ok(resolved)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::manifest::CommandArgs;
  use serde_json::json;

  #[test]
  fn test_parse_cli_overrides_valid() {
    let overrides = vec![
      "generator.seed=42".to_string(),
      "components.python.command=python3".to_string(),
    ];
    let map = parse_cli_overrides(&overrides).unwrap();
    assert_eq!(map.get("generator.seed").unwrap(), "42");
    assert_eq!(map.get("components.python.command").unwrap(), "python3");
  }

  #[test]
  fn test_parse_cli_overrides_missing_equals() {
    let overrides = vec!["invalid_format".to_string()];
    let res = parse_cli_overrides(&overrides);
    assert!(matches!(res, Err(ConfigError::InvalidOverrideFormat(_))));
  }

  #[test]
  fn test_parse_cli_overrides_array_bracket_ban() {
    let overrides = vec!["tasks[0].executor=foo".to_string()];
    let res = parse_cli_overrides(&overrides);
    assert!(matches!(
      res,
      Err(ConfigError::ArrayOverrideNotSupported { .. })
    ));
  }

  #[test]
  fn test_parse_cli_overrides_numeric_segment_ban() {
    let overrides = vec!["tasks.0.executor=foo".to_string()];
    let res = parse_cli_overrides(&overrides);
    assert!(matches!(
      res,
      Err(ConfigError::ArrayOverrideNotSupported { .. })
    ));
  }

  #[test]
  fn test_raw_config_build_task_replacement() {
    let base = json!({
      "tasks": [
        { "executor": "exec1", "args": ["arg1"] },
        { "executor": "exec2", "args": ["arg2"] }
      ]
    })
    .to_string();

    let mid = json!({
      "tasks": [
        { "executor": "exec3", "args": ["arg3"] }
      ]
    })
    .to_string();

    let config = RawConfig::build(
      ConfigSource::String(base),
      Some(ConfigSource::String(mid)),
      HashMap::new(),
    )
    .unwrap();

    let tasks = config.tasks.unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].executor_name, "exec3");
  }

  #[test]
  fn test_raw_config_build_generator_smart_merge_identity_retained() {
    let base = json!({
      "generator": { "name": "gen_a", "seed": 42 }
    })
    .to_string();

    let mut overrides = HashMap::new();
    overrides.insert("generator.seed".to_string(), "99".to_string());

    let config = RawConfig::build(ConfigSource::String(base), None, overrides).unwrap();

    let generator_cfg = config.generator.unwrap();
    assert_eq!(generator_cfg.name, "gen_a");
    assert_eq!(generator_cfg.seed, Some(99));
  }

  #[test]
  fn test_raw_config_build_generator_smart_merge_identity_changed() {
    let base = json!({
      "generator": { "name": "gen_a", "seed": 42, "args": ["--slow"] }
    })
    .to_string();

    let mid = json!({
      "generator": { "name": "gen_b" }
    })
    .to_string();

    let config = RawConfig::build(
      ConfigSource::String(base),
      Some(ConfigSource::String(mid)),
      HashMap::new(),
    )
    .unwrap();

    let generator_cfg = config.generator.unwrap();
    assert_eq!(generator_cfg.name, "gen_b");
    assert_eq!(generator_cfg.seed, None);
    assert!(generator_cfg.args.is_empty());
  }

  #[test]
  fn test_raw_config_resolve_all_valid() {
    let raw = RawConfig {
      generator: Some(RawGenerator {
        name: "my-gen".to_string(),
        seed: Some(123),
        args: vec!["--extra".to_string()],
      }),
      tasks: Some(vec![Task {
        executor_name: "my-exec".to_string(),
        args: vec!["run-this".to_string()],
        reps: None,
        labels: HashMap::new(),
      }]),
      components: {
        let mut map = HashMap::new();
        map.insert(
          "my-gen".to_string(),
          ManifestComponent {
            component_type: ComponentType::Generator,
            run: CommandArgs {
              command: PathBuf::from("gen-bin"),
              args: vec![],
              working_dir: None,
            },
          },
        );
        map.insert(
          "my-exec".to_string(),
          ManifestComponent {
            component_type: ComponentType::Executor,
            run: CommandArgs {
              working_dir: None,
              command: PathBuf::from("exec-bin"),
              args: vec!["base-arg".to_string()],
            },
          },
        );
        map
      },
      ..Default::default()
    };

    let resolved = raw.resolve_all(std::path::Path::new(".")).unwrap();
    assert!(resolved.generator.is_some());
    assert_eq!(resolved.tasks.len(), 1);
    assert_eq!(
      resolved.tasks[0].command_args.args,
      vec!["base-arg", "run-this"]
    );
  }

  #[test]
  fn test_raw_config_resolve_all_missing_component() {
    let raw = RawConfig {
      generator: None,
      tasks: Some(vec![Task {
        executor_name: "missing-exec".to_string(),
        args: vec![],
        reps: None,
        labels: HashMap::new(),
      }]),
      components: HashMap::new(),
      ..Default::default()
    };

    let res = raw.resolve_all(std::path::Path::new("."));
    match res {
      Err(ConfigError::GraphValidationFailed(errs)) => {
        assert!(matches!(errs[0], ConfigError::ComponentNotFound { .. }));
      }
      _ => panic!("Expected GraphValidationFailed with ComponentNotFound"),
    }
  }

  #[test]
  fn test_raw_config_resolve_all_type_mismatch() {
    let mut components = HashMap::new();
    components.insert(
      "not-an-executor".to_string(),
      ManifestComponent {
        component_type: ComponentType::Generator,
        run: CommandArgs {
          command: PathBuf::from("bin"),
          args: vec![],
          working_dir: None,
        },
      },
    );

    let raw = RawConfig {
      generator: None,
      tasks: Some(vec![Task {
        executor_name: "not-an-executor".to_string(),
        args: vec![],
        reps: None,
        labels: HashMap::new(),
      }]),
      components,
      ..Default::default()
    };

    let res = raw.resolve_all(std::path::Path::new("."));
    match res {
      Err(ConfigError::GraphValidationFailed(errs)) => {
        assert!(matches!(
          errs[0],
          ConfigError::IncorrectComponentType { .. }
        ));
      }
      _ => panic!("Expected GraphValidationFailed with IncorrectComponentType"),
    }
  }

  #[test]
  fn test_raw_config_resolve_reps_fallback() {
    let mut components = HashMap::new();
    components.insert(
      "exec".to_string(),
      ManifestComponent {
        component_type: ComponentType::Executor,
        run: CommandArgs {
          command: PathBuf::from("bin"),
          args: vec![],
          working_dir: None,
        },
      },
    );

    // Task reps override global reps
    let raw = RawConfig {
      reps: Some(5),
      tasks: Some(vec![Task {
        executor_name: "exec".to_string(),
        args: vec![],
        reps: Some(10),
        labels: HashMap::new(),
      }]),
      components: components.clone(),
      ..Default::default()
    };
    let resolved = raw.resolve_all(std::path::Path::new(".")).unwrap();
    assert_eq!(resolved.tasks[0].effective_reps, 10);

    // Global reps fallback
    let raw = RawConfig {
      reps: Some(5),
      tasks: Some(vec![Task {
        executor_name: "exec".to_string(),
        args: vec![],
        reps: None,
        labels: HashMap::new(),
      }]),
      components: components.clone(),
      ..Default::default()
    };
    let resolved = raw.resolve_all(std::path::Path::new(".")).unwrap();
    assert_eq!(resolved.tasks[0].effective_reps, 5);

    // Default to 1
    let raw = RawConfig {
      reps: None,
      tasks: Some(vec![Task {
        executor_name: "exec".to_string(),
        args: vec![],
        reps: None,
        labels: HashMap::new(),
      }]),
      components: components.clone(),
      ..Default::default()
    };
    let resolved = raw.resolve_all(std::path::Path::new(".")).unwrap();
    assert_eq!(resolved.tasks[0].effective_reps, 1);
  }

  #[test]
  fn test_raw_config_resolve_labels_merge() {
    let mut components = HashMap::new();
    components.insert(
      "exec".to_string(),
      ManifestComponent {
        component_type: ComponentType::Executor,
        run: CommandArgs {
          command: PathBuf::from("bin"),
          args: vec![],
          working_dir: None,
        },
      },
    );

    let mut global_labels = HashMap::new();
    global_labels.insert("env".to_string(), "prod".to_string());
    global_labels.insert("shared".to_string(), "base".to_string());

    let mut task_labels = HashMap::new();
    task_labels.insert("shared".to_string(), "override".to_string());
    task_labels.insert("task-only".to_string(), "value".to_string());

    let raw = RawConfig {
      labels: global_labels,
      tasks: Some(vec![Task {
        executor_name: "exec".to_string(),
        args: vec![],
        reps: None,
        labels: task_labels,
      }]),
      components,
      ..Default::default()
    };

    let resolved = raw.resolve_all(std::path::Path::new(".")).unwrap();
    let labels = &resolved.tasks[0].effective_labels;

    assert_eq!(labels.get("env").unwrap(), "prod");
    assert_eq!(labels.get("shared").unwrap(), "override");
    assert_eq!(labels.get("task-only").unwrap(), "value");
    assert_eq!(labels.len(), 3);
  }

  #[test]
  fn test_resolve_reps_and_labels() {
    let mut components = HashMap::new();
    components.insert(
      "my-exec".to_string(),
      ManifestComponent {
        component_type: ComponentType::Executor,
        run: CommandArgs {
          command: PathBuf::from("exec"),
          args: vec![],
          working_dir: None,
        },
      },
    );

    let mut global_labels = HashMap::new();
    global_labels.insert("env".to_string(), "prod".to_string());
    global_labels.insert("version".to_string(), "1.0".to_string());

    let mut task_labels = HashMap::new();
    task_labels.insert("version".to_string(), "2.0".to_string());
    task_labels.insert("tier".to_string(), "high".to_string());

    let raw = RawConfig {
      generator: None,
      reps: Some(5),
      labels: global_labels,
      tasks: Some(vec![
        Task {
          executor_name: "my-exec".to_string(),
          args: vec![],
          reps: None,
          labels: Default::default(),
        },
        Task {
          executor_name: "my-exec".to_string(),
          args: vec![],
          reps: Some(10),
          labels: task_labels,
        },
      ]),
      components,
    };

    let resolved = raw.resolve_all(std::path::Path::new(".")).unwrap();

    // Task 0 inherits global reps and labels
    assert_eq!(resolved.tasks[0].effective_reps, 5);
    assert_eq!(
      resolved.tasks[0].effective_labels.get("env").unwrap(),
      "prod"
    );
    assert_eq!(
      resolved.tasks[0].effective_labels.get("version").unwrap(),
      "1.0"
    );
    assert_eq!(resolved.tasks[0].effective_labels.len(), 2);

    // Task 1 overrides global reps and merges/overwrites labels
    assert_eq!(resolved.tasks[1].effective_reps, 10);
    assert_eq!(
      resolved.tasks[1].effective_labels.get("env").unwrap(),
      "prod"
    );
    assert_eq!(
      resolved.tasks[1].effective_labels.get("version").unwrap(),
      "2.0"
    );
    assert_eq!(
      resolved.tasks[1].effective_labels.get("tier").unwrap(),
      "high"
    );
    assert_eq!(resolved.tasks[1].effective_labels.len(), 3);
  }
}
