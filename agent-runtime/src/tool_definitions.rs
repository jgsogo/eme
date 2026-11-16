//! Loads a metadata-only catalogue of tool definitions without executable bindings.

use std::{collections::BTreeMap, path::Path};

use agent_config::{ConfigError, read_toml_dir};

use crate::ToolDefinition;

/// Metadata-only catalogue of tool definitions.
///
/// Executable tools should be bound through `ToolRegistry::register_from_file`,
/// which loads the definition and implementation together. This registry is
/// useful when a caller needs to inspect or discover definitions without
/// constructing executable implementations.
#[derive(Default)]
pub struct ToolDefinitionRegistry {
    definitions: BTreeMap<String, ToolDefinition>,
}

impl ToolDefinitionRegistry {
    pub fn load(data_dir: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let data_dir = data_dir.as_ref();
        let loaded = read_toml_dir::<ToolDefinition>(data_dir)?;

        if loaded.is_empty() {
            return Err(ConfigError::no_files("tool definition", data_dir));
        }

        let mut registry = Self::default();

        for (path, definition) in loaded {
            if definition.name.trim().is_empty() {
                return Err(ConfigError::invalid(
                    "tool definition",
                    Some(path),
                    "tool name cannot be empty",
                ));
            }

            let name = definition.name.clone();
            if registry.definitions.contains_key(&name) {
                return Err(ConfigError::duplicate("tool definition", name, Some(path)));
            }

            registry.definitions.insert(name, definition);
        }

        Ok(registry)
    }

    pub fn get(&self, name: &str) -> Option<&ToolDefinition> {
        self.definitions.get(name)
    }

    pub fn definitions(&self) -> impl Iterator<Item = &ToolDefinition> {
        self.definitions.values()
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use agent_config::ConfigError;

    use super::ToolDefinitionRegistry;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "tool-definitions-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn loads_valid_definitions_and_rejects_invalid_or_duplicate_names() {
        let directory = TestDirectory::new();

        // A valid definition is discoverable by its identifier.
        fs::write(
            directory.0.join("valid.toml"),
            r#"
name = "example"
description = "Example tool"

[parameters]
type = "object"
"#,
        )
        .unwrap();
        let registry = ToolDefinitionRegistry::load(&directory.0).unwrap();
        assert_eq!(registry.get("example").unwrap().description, "Example tool");
        fs::remove_file(directory.0.join("valid.toml")).unwrap();

        // Empty names are invalid definitions; repeated names are duplicates.
        fs::write(
            directory.0.join("invalid.toml"),
            r#"
name = ""
description = "Invalid tool"

[parameters]
type = "object"
"#,
        )
        .unwrap();
        assert!(matches!(
            ToolDefinitionRegistry::load(&directory.0),
            Err(ConfigError::Invalid { .. })
        ));
        fs::remove_file(directory.0.join("invalid.toml")).unwrap();

        let duplicate = r#"
name = "same"
description = "Duplicate tool"

[parameters]
type = "object"
"#;
        fs::write(directory.0.join("a.toml"), duplicate).unwrap();
        fs::write(directory.0.join("b.toml"), duplicate).unwrap();
        assert!(matches!(
            ToolDefinitionRegistry::load(&directory.0),
            Err(ConfigError::Duplicate { .. })
        ));

        // An empty definitions directory is not a usable registry.
        fs::remove_file(directory.0.join("a.toml")).unwrap();
        fs::remove_file(directory.0.join("b.toml")).unwrap();
        assert!(matches!(
            ToolDefinitionRegistry::load(&directory.0),
            Err(ConfigError::NoFilesFound { .. })
        ));
    }
}
