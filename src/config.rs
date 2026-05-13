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
use crate::manifest::ComponentType;
use crate::manifest::ManifestComponent;

use crate::figment_ext::*;

use serde::Deserialize;
use serde::Serialize;

use std::collections::HashMap;
use std::io::IsTerminal;
use std::io::Read;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
struct RawConfig {
  generator: Option<RawGenerator>,
  tasks: Option<Vec<Task>>,
  #[serde(default)]
  components: HashMap<String, ManifestComponent>,
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
        Ok(mut cmd) => {
          let seed = generator_cfg.seed.unwrap_or_else(rand::random);
          tracing::info!(seed, "Using generator seed");
          cmd.run.args.push(format!("--seed={}", seed));
          cmd.run.args.extend(generator_cfg.args.to_owned());
          resolved_generator = Some(cmd);
        }
        Err(e) => errors.push(e),
      }
    }

    let mut resolved_tasks = Vec::new();
    if let Some(tasks) = self.tasks.as_ref() {
      for task in tasks {
        match self.resolve_component(&task.executor_name, ComponentType::Executor, root_dir) {
          Ok(mut cmd) => {
            cmd.run.args.extend(task.args.clone());
            resolved_tasks.push(ResolvedTask {
              raw_task: task.clone(),
              component: cmd,
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

#[derive(Debug, Deserialize, Serialize, Clone, Hash)]
pub struct Task {
  #[serde(rename = "executor")]
  pub executor_name: String,

  #[serde(default)]
  pub args: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedTask {
  pub raw_task: Task,
  pub component: ManifestComponent,
}

#[derive(Debug, Clone)]
pub struct ResolvedConfig {
  pub generator: Option<ManifestComponent>,
  pub tasks: Vec<ResolvedTask>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
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
    };

    let resolved = raw.resolve_all(std::path::Path::new(".")).unwrap();
    assert!(resolved.generator.is_some());
    assert_eq!(resolved.tasks.len(), 1);
    assert_eq!(
      resolved.tasks[0].component.run.args,
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
      }]),
      components: HashMap::new(),
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
      }]),
      components,
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
}
