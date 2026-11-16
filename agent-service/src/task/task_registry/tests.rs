//! Tests for task validation, ownership, migration, and persistence.

use super::*;
use chrono::{DateTime, Utc};
use std::{fs, path::PathBuf};
use uuid::Uuid;

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
