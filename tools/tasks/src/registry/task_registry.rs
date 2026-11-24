//! Persistent, user-scoped task registry operations.

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use cron::Schedule;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};
use tokio::sync::watch;
use uuid::Uuid;

use super::{Inner, LEGACY_TASK_OWNER, NewTask, StoredTask, Task, TaskError};

#[derive(Clone)]
pub struct TaskRegistry {
    inner: Arc<Inner>,
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
            let persisted_next_run = old.next_run;
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
            task.next_run = if !task.enabled {
                None
            } else if let Some(next) = persisted_next_run {
                Some(next)
            } else if task.schedule.is_none() {
                // Recover legacy one-time tasks whose due time passed before next_run
                // was persisted by treating their original "at" value as pending.
                task.at.map(|at| at.with_timezone(&Utc))
            } else {
                next_run(&task, Utc::now())?
            };
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
            if path != canonical
                || !raw.contains("user_id")
                || !raw.contains("id")
                || persisted_next_run.is_none()
            {
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
    /// Migrates tasks created under a transport-specific owner to the stable user identity.
    pub fn migrate_owner(&self, old_user: &str, new_user: &str) -> Result<(), TaskError> {
        if old_user == new_user {
            return Ok(());
        }
        let mut tasks = self.inner.tasks.write().map_err(lock_error)?;
        let mut migrating = tasks
            .values()
            .filter(|task| task.user_id == old_user)
            .cloned()
            .collect::<Vec<_>>();
        if migrating.is_empty() {
            return Ok(());
        }
        for task in &migrating {
            if tasks.values().any(|other| {
                other.user_id == new_user && other.name == task.name && other.id != task.id
            }) {
                return Err(TaskError::Invalid(format!(
                    "cannot migrate task '{}': a task with that name already exists",
                    task.name
                )));
            }
        }
        for task in &mut migrating {
            task.user_id = new_user.to_owned();
            self.persist(task)?;
        }
        for task in migrating {
            tasks.insert(task.id, task);
        }
        self.notify();
        Ok(())
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
    /// Replaces a task definition owned by the authenticated user.
    pub fn update(
        &self,
        user: &str,
        id: Uuid,
        task: NewTask,
        enabled: bool,
    ) -> Result<Task, TaskError> {
        if user.trim().is_empty() {
            return Err(TaskError::Invalid("authenticated user is missing".into()));
        }
        let mut tasks = self.inner.tasks.write().map_err(lock_error)?;
        if !tasks.get(&id).is_some_and(|value| value.user_id == user) {
            return Err(TaskError::NotFound);
        }
        let mut value = Task {
            id,
            user_id: user.into(),
            name: task.name,
            description: task.description,
            agent: task.agent,
            prompt: task.prompt,
            schedule: task.schedule,
            at: task.at,
            timezone: task.timezone,
            enabled,
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
        if tasks
            .values()
            .any(|other| other.id != id && other.user_id == user && other.name == value.name)
        {
            return Err(TaskError::Invalid(
                "task name already exists for this user".into(),
            ));
        }
        value.next_run = next_run(&value, Utc::now())?;
        if value.enabled && value.schedule.is_none() && value.next_run.is_none() {
            return Err(TaskError::Invalid(
                "one-time task must be scheduled in the future".into(),
            ));
        }
        self.persist(&value)?;
        tasks.insert(id, value.clone());
        self.notify();
        Ok(value)
    }

    /// Enables or disables a task without changing its definition.
    pub fn set_enabled(&self, user: &str, id: Uuid, enabled: bool) -> Result<Task, TaskError> {
        let current = self
            .inner
            .tasks
            .read()
            .map_err(lock_error)?
            .get(&id)
            .filter(|task| task.user_id == user)
            .cloned()
            .ok_or(TaskError::NotFound)?;
        self.update(
            user,
            id,
            NewTask {
                name: current.name,
                description: current.description,
                agent: current.agent,
                prompt: current.prompt,
                schedule: current.schedule,
                at: current.at,
                timezone: current.timezone,
            },
            enabled,
        )
    }

    pub(crate) fn set_next_run(&self, id: Uuid, next: DateTime<Utc>) -> Result<(), TaskError> {
        let mut tasks = self.inner.tasks.write().map_err(lock_error)?;
        let task = tasks.get_mut(&id).ok_or(TaskError::NotFound)?;
        task.next_run = Some(next);
        // Update memory even if persistence fails so a later registry refresh does not
        // resubmit an occurrence that the run manager has already accepted.
        self.persist(task)
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
    pub(crate) fn remove_after_execution(&self, id: Uuid) -> Result<(), TaskError> {
        let mut tasks = self.inner.tasks.write().map_err(lock_error)?;
        let Some(task) = tasks.get(&id).cloned() else {
            return Ok(());
        };
        match fs::remove_file(self.path(id)) {
            Ok(()) => {
                tasks.remove(&id);
                self.notify();
                Ok(())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                tasks.remove(&id);
                self.notify();
                Ok(())
            }
            Err(error) => {
                // The run has already been accepted. If deletion fails, disable the task
                // in memory and on disk where possible so it is not resubmitted in-process.
                let mut completed = task;
                completed.enabled = false;
                completed.next_run = None;
                tasks.insert(id, completed.clone());
                let persist_result = self.persist(&completed);
                self.notify();
                persist_result?;
                Err(storage_error(error))
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
        let stored = StoredTask {
            id: Some(t.id),
            user_id: Some(t.user_id.clone()),
            name: t.name.clone(),
            description: t.description.clone(),
            agent: t.agent.clone(),
            prompt: t.prompt.clone(),
            schedule: t.schedule.clone(),
            at: t.at,
            timezone: t.timezone.clone(),
            enabled: t.enabled,
            next_run: t.next_run,
        };
        fs::write(
            &tmp,
            toml::to_string_pretty(&stored).map_err(storage_error)?,
        )
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
