//! Tests for task validation, ownership, migration, and persistence.

use super::*;
use chrono::Utc;
use std::{fs, path::PathBuf};
use uuid::Uuid;

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
    let persisted = fs::read_to_string(directory.join(format!("{}.toml", tasks[0].id))).unwrap();
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

#[test]
fn task_definitions_can_be_updated_and_enabled_without_cross_user_access() {
    let directory = temp_dir();
    let registry = TaskRegistry::load(&directory).unwrap();
    registry
        .validate_agents(["assistant".to_owned(), "researcher".to_owned()])
        .unwrap();
    let task = registry
        .add(
            "telegram:10",
            NewTask {
                name: "daily".into(),
                description: "old description".into(),
                agent: "assistant".into(),
                prompt: "old prompt".into(),
                schedule: Some("0 0 8 * * *".into()),
                at: None,
                timezone: Some("Europe/Madrid".into()),
            },
        )
        .unwrap();

    let updated = registry
        .update(
            "telegram:10",
            task.id,
            NewTask {
                name: "weekly".into(),
                description: "updated description".into(),
                agent: "researcher".into(),
                prompt: "updated prompt".into(),
                schedule: Some("0 0 9 * * MON".into()),
                at: None,
                timezone: Some("Europe/Madrid".into()),
            },
            true,
        )
        .unwrap();
    assert_eq!(updated.name, "weekly");
    assert_eq!(updated.agent, "researcher");
    assert_eq!(updated.prompt, "updated prompt");

    assert!(matches!(
        registry.set_enabled("telegram:11", task.id, false),
        Err(TaskError::NotFound)
    ));
    let disabled = registry.set_enabled("telegram:10", task.id, false).unwrap();
    assert!(!disabled.enabled);
    assert!(disabled.next_run.is_none());
    let enabled = registry.set_enabled("telegram:10", task.id, true).unwrap();
    assert!(enabled.enabled);
    assert!(enabled.next_run.is_some());

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn completed_one_time_tasks_are_retained_as_disabled_definitions() {
    let directory = temp_dir();
    let registry = TaskRegistry::load(&directory).unwrap();
    registry.validate_agents(["assistant".to_owned()]).unwrap();
    let task = registry
        .add(
            "telegram:12",
            NewTask {
                name: "once".into(),
                description: String::new(),
                agent: "assistant".into(),
                prompt: "do it".into(),
                schedule: None,
                at: Some((Utc::now() + chrono::Duration::hours(1)).fixed_offset()),
                timezone: None,
            },
        )
        .unwrap();

    registry.disable_after_execution(task.id);
    let retained = registry.list("telegram:12");
    assert_eq!(retained.len(), 1);
    assert_eq!(retained[0].id, task.id);
    assert!(!retained[0].enabled);
    assert!(retained[0].next_run.is_none());

    drop(registry);
    let reloaded = TaskRegistry::load(&directory).unwrap();
    assert_eq!(reloaded.list("telegram:12").len(), 1);
    assert!(!reloaded.list("telegram:12")[0].enabled);
    fs::remove_dir_all(directory).unwrap();
}
