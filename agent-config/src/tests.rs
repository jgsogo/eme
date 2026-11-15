use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

use super::{ConfigError, read_toml, read_toml_dir};

fn test_directory() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let path = std::env::temp_dir().join(format!(
        "agent-config-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn reads_a_toml_file() {
    let directory = test_directory();
    let path = directory.join("valid.toml");
    fs::write(&path, "name = 'example'").unwrap();

    assert_eq!(
        read_toml::<BTreeMap<String, String>>(&path).unwrap(),
        BTreeMap::from([("name".into(), "example".into())])
    );

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn reports_parse_errors_with_the_source_path() {
    let directory = test_directory();
    let path = directory.join("invalid.toml");
    fs::write(&path, "name = [").unwrap();

    let error = read_toml::<BTreeMap<String, String>>(&path).unwrap_err();
    assert!(matches!(
        error,
        ConfigError::Parse { path: error_path, .. } if error_path == path
    ));

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn loads_toml_directories_in_sorted_order_and_ignores_other_files() {
    let directory = test_directory();
    fs::write(directory.join("b.toml"), "name = 'b'").unwrap();
    fs::write(directory.join("a.toml"), "name = 'a'").unwrap();
    fs::write(directory.join("notes.txt"), "ignored").unwrap();

    let loaded = read_toml_dir::<BTreeMap<String, String>>(&directory).unwrap();
    let names = loaded
        .into_iter()
        .map(|(_, fixture)| fixture["name"].clone())
        .collect::<Vec<_>>();

    assert_eq!(names, ["a", "b"]);

    fs::remove_dir_all(directory).unwrap();
}
