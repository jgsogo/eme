use std::{collections::HashSet, path::Path};

use agent_config::{ConfigError, read_toml_dir};
use chrono::{DateTime, FixedOffset, Utc};
use chrono_tz::Tz;
use cron::Schedule;
use serde::Deserialize;

use crate::ScheduledTask;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskDefinition {
    name: String,
    #[serde(default)]
    description: String,
    agent: String,
    prompt: String,
    #[serde(default)]
    schedule: Option<String>,
    #[serde(default)]
    at: Option<DateTime<FixedOffset>>,
    #[serde(default)]
    timezone: Option<String>,
    #[serde(default = "default_enabled")]
    enabled: bool,
}

fn default_enabled() -> bool {
    true
}

/// Loads and validates task definitions from a TOML directory.
pub struct TaskRegistry {
    tasks: Vec<ScheduledTask>,
}

impl TaskRegistry {
    /// Load tasks and validate their agent references.
    ///
    /// A directory with no TOML files is a valid empty task registry.
    pub fn load(
        directory: impl AsRef<Path>,
        agent_names: impl IntoIterator<Item = String>,
    ) -> Result<Self, ConfigError> {
        let directory = directory.as_ref();
        let agents: HashSet<String> = agent_names.into_iter().collect();
        let loaded = read_toml_dir::<TaskDefinition>(directory)?;
        let mut names = HashSet::new();
        let mut tasks = Vec::new();
        let now = Utc::now();

        for (path, definition) in loaded {
            if definition.name.trim().is_empty() {
                return Err(ConfigError::invalid(
                    "task",
                    Some(path),
                    "task name cannot be empty",
                ));
            }
            if !names.insert(definition.name.clone()) {
                return Err(ConfigError::duplicate(
                    "task",
                    definition.name,
                    Some(path),
                ));
            }
            if definition.agent.trim().is_empty() {
                return Err(ConfigError::invalid(
                    "task",
                    Some(path),
                    "agent name cannot be empty",
                ));
            }
            if !agents.contains(&definition.agent) {
                return Err(ConfigError::invalid(
                    "task",
                    Some(path),
                    format!("task '{}' references unknown agent '{}'", definition.name, definition.agent),
                ));
            }
            if definition.prompt.trim().is_empty() {
                return Err(ConfigError::invalid(
                    "task",
                    Some(path),
                    "task prompt cannot be empty",
                ));
            }

            let has_schedule = definition.schedule.is_some();
            let has_at = definition.at.is_some();
            if has_schedule == has_at {
                return Err(ConfigError::invalid(
                    "task",
                    Some(path),
                    "exactly one of 'schedule' or 'at' must be configured",
                ));
            }

            let timezone_name = definition.timezone.as_deref().unwrap_or("UTC");
            let timezone: Tz = timezone_name.parse().map_err(|error| {
                ConfigError::invalid(
                    "task",
                    Some(path.clone()),
                    format!("invalid timezone '{timezone_name}': {error}"),
                )
            })?;

            let schedule = match definition.schedule {
                Some(expression) => Some(expression.parse::<Schedule>().map_err(|error| {
                    ConfigError::invalid(
                        "task",
                        Some(path.clone()),
                        format!("invalid cron expression '{expression}': {error}"),
                    )
                })?),
                None => {
                    if definition.timezone.is_some() {
                        return Err(ConfigError::invalid(
                            "task",
                            Some(path),
                            "'timezone' can only be used with a recurring 'schedule'; one-time 'at' values must include their UTC offset",
                        ));
                    }
                    None
                }
            };

            if !definition.enabled {
                continue;
            }

            let next_run = match (&schedule, definition.at) {
                (Some(schedule), None) => {
                    let local_now = now.with_timezone(&timezone);
                    schedule
                        .after(&local_now)
                        .next()
                        .map(|date| date.with_timezone(&Utc))
                        .ok_or_else(|| {
                            ConfigError::invalid(
                                "task",
                                Some(path.clone()),
                                "cron expression has no future occurrence",
                            )
                        })?
                }
                (None, Some(at)) => {
                    let at = at.with_timezone(&Utc);
                    if at <= now {
                        tracing::warn!(
                            task = %definition.name,
                            scheduled_at = %at,
                            "ignoring one-time task whose scheduled time has passed"
                        );
                        continue;
                    }
                    at
                }
                _ => unreachable!("schedule/at exclusivity was validated above"),
            };

            tracing::info!(
                task = %definition.name,
                agent = %definition.agent,
                next_run = %next_run,
                description = %definition.description,
                "loaded scheduled task"
            );

            tasks.push(ScheduledTask {
                name: definition.name,
                agent: definition.agent,
                prompt: definition.prompt,
                schedule,
                timezone,
                next_run,
            });
        }

        Ok(Self { tasks })
    }

    pub(crate) fn into_tasks(self) -> Vec<ScheduledTask> {
        self.tasks
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    use chrono::Utc;

    use super::TaskRegistry;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "agent-task-registry-test-{}",
                uuid::Uuid::new_v4()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn write(&self, name: &str, contents: &str) {
            fs::write(self.0.join(name), contents).unwrap();
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn agent_names() -> impl Iterator<Item = String> {
        ["assistant"].into_iter().map(str::to_owned)
    }

    #[test]
    fn loads_recurring_task_and_calculates_next_run_in_configured_timezone() {
        let directory = TestDirectory::new();
        directory.write(
            "daily.toml",
            r#"
name = "daily"
description = "Daily report"
agent = "assistant"
prompt = "Prepare the daily report"
schedule = "0 0 8 * * *"
timezone = "Europe/Madrid"
"#,
        );

        let registry = TaskRegistry::load(directory.path(), agent_names()).unwrap();
        let tasks = registry.into_tasks();

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name, "daily");
        assert!(tasks[0].next_run > Utc::now());
        assert_eq!(tasks[0].timezone.to_string(), "Europe/Madrid");
    }

    #[test]
    fn loads_one_time_task_with_explicit_offset() {
        let directory = TestDirectory::new();
        let at = (Utc::now() + chrono::Duration::hours(2)).to_rfc3339();
        directory.write(
            "once.toml",
            &format!(
                r#"
name = "once"
agent = "assistant"
prompt = "Do the thing"
at = "{at}"
"#
            ),
        );

        let registry = TaskRegistry::load(directory.path(), agent_names()).unwrap();
        assert_eq!(registry.into_tasks().len(), 1);
    }

    #[test]
    fn rejects_unknown_agents_and_invalid_cron_expressions() {
        let directory = TestDirectory::new();
        directory.write(
            "unknown.toml",
            r#"
name = "unknown"
agent = "missing"
prompt = "Do the thing"
schedule = "0 0 8 * * *"
"#,
        );
        assert!(TaskRegistry::load(directory.path(), agent_names())
            .err()
            .unwrap()
            .to_string()
            .contains("unknown agent"));

        fs::remove_file(directory.path().join("unknown.toml")).unwrap();
        directory.write(
            "invalid.toml",
            r#"
name = "invalid"
agent = "assistant"
prompt = "Do the thing"
schedule = "not a cron expression"
"#,
        );
        assert!(TaskRegistry::load(directory.path(), agent_names())
            .err()
            .unwrap()
            .to_string()
            .contains("invalid cron expression"));
    }

    #[test]
    fn requires_exactly_one_schedule_mode_and_rejects_empty_prompts() {
        let directory = TestDirectory::new();
        directory.write(
            "invalid.toml",
            r#"
name = "invalid"
agent = "assistant"
prompt = " "
schedule = "0 0 8 * * *"
at = "2026-11-20T09:00:00+01:00"
"#,
        );
        assert!(TaskRegistry::load(directory.path(), agent_names())
            .err()
            .unwrap()
            .to_string()
            .contains("exactly one"));

        fs::remove_file(directory.path().join("invalid.toml")).unwrap();
        directory.write(
            "empty.toml",
            r#"
name = "empty"
agent = "assistant"
prompt = " "
schedule = "0 0 8 * * *"
"#,
        );
        assert!(TaskRegistry::load(directory.path(), agent_names())
            .err()
            .unwrap()
            .to_string()
            .contains("prompt cannot be empty"));
    }

    #[test]
    fn ignores_disabled_and_expired_one_time_tasks() {
        let directory = TestDirectory::new();
        directory.write(
            "disabled.toml",
            r#"
name = "disabled"
agent = "assistant"
prompt = "Do not run"
schedule = "0 0 8 * * *"
enabled = false
"#,
        );
        directory.write(
            "expired.toml",
            r#"
name = "expired"
agent = "assistant"
prompt = "Do not run"
at = "2020-01-01T00:00:00Z"
"#,
        );

        let registry = TaskRegistry::load(directory.path(), agent_names()).unwrap();
        assert!(registry.into_tasks().is_empty());
    }

    #[test]
    fn accepts_an_empty_task_directory() {
        let directory = TestDirectory::new();
        let registry = TaskRegistry::load(directory.path(), agent_names()).unwrap();
        assert!(registry.into_tasks().is_empty());
    }
}
