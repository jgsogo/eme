use chrono::{DateTime, FixedOffset, Utc};
use chrono_tz::Tz;
use cron::Schedule;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};
use tokio::sync::watch;
use uuid::Uuid;

pub const LEGACY_TASK_OWNER: &str = "telegram:118716073";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub id: Uuid,
    pub user_id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub agent: String,
    pub prompt: String,
    #[serde(default)]
    pub schedule: Option<String>,
    #[serde(default)]
    pub at: Option<DateTime<FixedOffset>>,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(skip)]
    pub next_run: Option<DateTime<Utc>>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewTask {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub agent: String,
    pub prompt: String,
    #[serde(default)]
    pub schedule: Option<String>,
    #[serde(default)]
    pub at: Option<DateTime<FixedOffset>>,
    #[serde(default)]
    pub timezone: Option<String>,
}
fn default_enabled() -> bool {
    true
}

#[derive(Debug, thiserror::Error)]
pub enum TaskError {
    #[error("task not found")]
    NotFound,
    #[error("invalid task: {0}")]
    Invalid(String),
    #[error("task storage error: {0}")]
    Storage(String),
}
#[derive(Clone)]
pub struct TaskRegistry {
    inner: Arc<Inner>,
}
struct Inner {
    directory: PathBuf,
    agents: RwLock<Vec<String>>,
    tasks: RwLock<HashMap<Uuid, Task>>,
    changed: watch::Sender<u64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredTask {
    #[serde(default)]
    id: Option<Uuid>,
    #[serde(default)]
    user_id: Option<String>,
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
impl TaskRegistry {
    /// Loads tasks and migrates ownerless definitions to the designated legacy user.
    pub fn load(directory: impl AsRef<Path>) -> Result<Self, TaskError> {
        let directory = directory.as_ref().to_path_buf();
        fs::create_dir_all(&directory).map_err(storage_error)?;
        let (changed, _) = watch::channel(0);
        let registry = Self {
            inner: Arc::new(Inner {
                directory: directory.clone(),
                agents: RwLock::new(Vec::new()),
                tasks: RwLock::new(HashMap::new()),
                changed,
            }),
        };
        let mut tasks = HashMap::new();
        let mut names = std::collections::HashSet::new();
        for entry in fs::read_dir(&directory).map_err(storage_error)? {
            let path = entry.map_err(storage_error)?.path();
            if path.extension().and_then(|x| x.to_str()) != Some("toml") {
                continue;
            }
            let raw = fs::read_to_string(&path).map_err(storage_error)?;
            let old: StoredTask = toml::from_str(&raw)
                .map_err(|e| TaskError::Invalid(format!("{}: {e}", path.display())))?;
            let id = old.id.unwrap_or_else(Uuid::new_v4);
            let user_id = old.user_id.unwrap_or_else(|| LEGACY_TASK_OWNER.into());
            if user_id.trim().is_empty() {
                return Err(TaskError::Invalid("user_id cannot be empty".into()));
            }
            let mut task = Task {
                id,
                user_id,
                name: old.name,
                description: old.description,
                agent: old.agent,
                prompt: old.prompt,
                schedule: old.schedule,
                at: old.at,
                timezone: old.timezone,
                enabled: old.enabled,
                next_run: None,
            };
            validate(&task)?;
            task.next_run = next_run(&task, Utc::now())?;
            if !names.insert((task.user_id.clone(), task.name.clone())) {
                return Err(TaskError::Invalid(format!(
                    "duplicate task name '{}' for user '{}'",
                    task.name, task.user_id
                )));
            }
            if tasks.insert(id, task.clone()).is_some() {
                return Err(TaskError::Invalid(format!("duplicate task id {id}")));
            }
            let canonical = directory.join(format!("{id}.toml"));
            if path != canonical || !raw.contains("user_id") || !raw.contains("id") {
                registry.persist(&task)?;
                if path != canonical {
                    fs::remove_file(path).map_err(storage_error)?;
                }
            }
        }
        *registry.inner.tasks.write().map_err(lock_error)? = tasks;
        Ok(registry)
    }
    pub fn validate_agents(
        &self,
        names: impl IntoIterator<Item = String>,
    ) -> Result<(), TaskError> {
        let names: Vec<_> = names.into_iter().collect();
        for task in self.inner.tasks.read().map_err(lock_error)?.values() {
            if !names.contains(&task.agent) {
                return Err(TaskError::Invalid(format!(
                    "task '{}' references unknown agent '{}'",
                    task.name, task.agent
                )));
            }
        }
        *self.inner.agents.write().map_err(lock_error)? = names;
        Ok(())
    }
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.inner.changed.subscribe()
    }
    pub fn snapshot(&self) -> Vec<Task> {
        self.inner
            .tasks
            .read()
            .expect("task lock poisoned")
            .values()
            .cloned()
            .collect()
    }
    pub fn list(&self, user: &str) -> Vec<Task> {
        let mut result: Vec<_> = self
            .snapshot()
            .into_iter()
            .filter(|t| t.user_id == user)
            .collect();
        result.sort_by(|a, b| a.name.cmp(&b.name));
        result
    }
    pub fn add(&self, user: &str, task: NewTask) -> Result<Task, TaskError> {
        if user.trim().is_empty() {
            return Err(TaskError::Invalid("authenticated user is missing".into()));
        }
        let mut value = Task {
            id: Uuid::new_v4(),
            user_id: user.into(),
            name: task.name,
            description: task.description,
            agent: task.agent,
            prompt: task.prompt,
            schedule: task.schedule,
            at: task.at,
            timezone: task.timezone,
            enabled: true,
            next_run: None,
        };
        validate(&value)?;
        if !self
            .inner
            .agents
            .read()
            .map_err(lock_error)?
            .contains(&value.agent)
        {
            return Err(TaskError::Invalid(format!(
                "unknown agent '{}'",
                value.agent
            )));
        }
        value.next_run = next_run(&value, Utc::now())?;
        if value.next_run.is_none() {
            return Err(TaskError::Invalid(
                "one-time task must be scheduled in the future".into(),
            ));
        }
        let mut tasks = self.inner.tasks.write().map_err(lock_error)?;
        if tasks
            .values()
            .any(|t| t.user_id == user && t.name == value.name)
        {
            return Err(TaskError::Invalid(
                "task name already exists for this user".into(),
            ));
        }
        self.persist(&value)?;
        tasks.insert(value.id, value.clone());
        self.notify();
        Ok(value)
    }
    pub(crate) fn set_next_run(&self, id: Uuid, next: DateTime<Utc>) {
        if let Ok(mut tasks) = self.inner.tasks.write() {
            if let Some(task) = tasks.get_mut(&id) {
                task.next_run = Some(next);
            }
        }
    }
    pub fn remove(&self, user: &str, id: Uuid) -> Result<(), TaskError> {
        let mut tasks = self.inner.tasks.write().map_err(lock_error)?;
        if !tasks.get(&id).is_some_and(|t| t.user_id == user) {
            return Err(TaskError::NotFound);
        }
        fs::remove_file(self.path(id)).map_err(storage_error)?;
        tasks.remove(&id);
        self.notify();
        Ok(())
    }
    pub(crate) fn remove_after_execution(&self, id: Uuid) {
        if let Ok(mut tasks) = self.inner.tasks.write() {
            if tasks.remove(&id).is_some() {
                let _ = fs::remove_file(self.path(id));
                self.notify();
            }
        }
    }
    fn path(&self, id: Uuid) -> PathBuf {
        self.inner.directory.join(format!("{id}.toml"))
    }
    fn persist(&self, t: &Task) -> Result<(), TaskError> {
        let path = self.path(t.id);
        let tmp = self
            .inner
            .directory
            .join(format!(".{}.tmp", Uuid::new_v4()));
        fs::write(&tmp, toml::to_string_pretty(t).map_err(storage_error)?)
            .map_err(storage_error)?;
        if let Err(e) = fs::rename(&tmp, &path) {
            let _ = fs::remove_file(tmp);
            return Err(storage_error(e));
        }
        Ok(())
    }
    fn notify(&self) {
        self.inner.changed.send_modify(|v| *v = v.wrapping_add(1));
    }
}
fn validate(t: &Task) -> Result<(), TaskError> {
    if t.name.trim().is_empty() || t.agent.trim().is_empty() || t.prompt.trim().is_empty() {
        return Err(TaskError::Invalid(
            "name, agent and prompt are required".into(),
        ));
    }
    if t.schedule.is_some() == t.at.is_some() {
        return Err(TaskError::Invalid(
            "exactly one of schedule or at is required".into(),
        ));
    }
    if let Some(s) = &t.schedule {
        s.parse::<Schedule>()
            .map_err(|e| TaskError::Invalid(format!("invalid cron: {e}")))?;
    }
    if let Some(z) = &t.timezone {
        if t.schedule.is_none() {
            return Err(TaskError::Invalid("timezone requires schedule".into()));
        }
        z.parse::<Tz>()
            .map_err(|e| TaskError::Invalid(format!("invalid timezone: {e}")))?;
    }
    Ok(())
}
fn next_run(t: &Task, now: DateTime<Utc>) -> Result<Option<DateTime<Utc>>, TaskError> {
    if !t.enabled {
        return Ok(None);
    }
    if let Some(s) = &t.schedule {
        let schedule = s.parse::<Schedule>().map_err(storage_error)?;
        let tz: Tz = t
            .timezone
            .as_deref()
            .unwrap_or("UTC")
            .parse()
            .map_err(storage_error)?;
        return Ok(schedule
            .after(&now.with_timezone(&tz))
            .next()
            .map(|d| d.with_timezone(&Utc)));
    }
    Ok(t.at.map(|d| d.with_timezone(&Utc)).filter(|d| *d > now))
}
fn storage_error(e: impl std::fmt::Display) -> TaskError {
    TaskError::Storage(e.to_string())
}
fn lock_error<T>(_: std::sync::PoisonError<T>) -> TaskError {
    TaskError::Storage("task registry lock poisoned".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    fn temp_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!("task-registry-{}", Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn migrates_legacy_task_to_the_default_owner_and_uuid_filename() {
        let directory = temp_dir();
        fs::write(
            directory.join("daily.toml"),
            r#"
name = "daily"
agent = "assistant"
prompt = "prepare report"
schedule = "0 0 8 * * *"
"#,
        )
        .unwrap();

        let registry = TaskRegistry::load(&directory).unwrap();
        let tasks = registry.list(LEGACY_TASK_OWNER);
        assert_eq!(tasks.len(), 1);
        assert!(directory.join(format!("{}.toml", tasks[0].id)).exists());
        assert!(!directory.join("daily.toml").exists());
        let persisted =
            fs::read_to_string(directory.join(format!("{}.toml", tasks[0].id))).unwrap();
        assert!(persisted.contains("user_id = \"telegram:118716073\""));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn task_listing_and_removal_are_scoped_to_the_authenticated_user() {
        let directory = temp_dir();
        let registry = TaskRegistry::load(&directory).unwrap();
        registry.validate_agents(["assistant".to_owned()]).unwrap();
        let task = registry
            .add(
                "telegram:1",
                NewTask {
                    name: "daily".into(),
                    description: String::new(),
                    agent: "assistant".into(),
                    prompt: "prepare report".into(),
                    schedule: Some("0 0 8 * * *".into()),
                    at: None,
                    timezone: Some("Europe/Madrid".into()),
                },
            )
            .unwrap();

        assert_eq!(registry.list("telegram:1").len(), 1);
        assert!(registry.list("telegram:2").is_empty());
        assert!(matches!(
            registry.remove("telegram:2", task.id),
            Err(TaskError::NotFound)
        ));
        assert!(registry.remove("telegram:1", task.id).is_ok());
        assert!(registry.list("telegram:1").is_empty());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn persists_tasks_across_registry_restarts() {
        let directory = temp_dir();
        let registry = TaskRegistry::load(&directory).unwrap();
        registry.validate_agents(["assistant".to_owned()]).unwrap();
        let task = registry
            .add(
                "telegram:7",
                NewTask {
                    name: "once".into(),
                    description: "one-time job".into(),
                    agent: "assistant".into(),
                    prompt: "do it".into(),
                    schedule: None,
                    at: Some((Utc::now() + chrono::Duration::hours(1)).fixed_offset()),
                    timezone: None,
                },
            )
            .unwrap();
        drop(registry);

        let reloaded = TaskRegistry::load(&directory).unwrap();
        assert_eq!(reloaded.list("telegram:7").len(), 1);
        assert_eq!(reloaded.list("telegram:7")[0].id, task.id);
        assert_eq!(reloaded.list("telegram:7")[0].prompt, "do it");
        fs::remove_dir_all(directory).unwrap();
    }
}
