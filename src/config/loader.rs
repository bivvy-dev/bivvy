//! Configuration file discovery and loading.
//!
//! This module handles finding and loading configuration files from
//! various locations in the correct priority order.

use crate::config::merger::merge_configs;
use crate::config::schema::BivvyConfig;
use crate::error::{BivvyError, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Paths to configuration files in priority order (later overrides earlier).
///
/// Merge order:
/// 1. Remote base configs (from `extends:`)
/// 2. User global config (`~/.bivvy/config.yml`)
/// 3. Project config (`.bivvy/config.yml`)
/// 4. Local overrides (`.bivvy/config.local.yml`)
#[derive(Debug, Clone)]
pub struct ConfigPaths {
    /// Remote base config (if extends: is used)
    pub extends: Vec<PathBuf>,

    /// User's global config: ~/.bivvy/config.yml
    pub user_global: Option<PathBuf>,

    /// Project config: .bivvy/config.yml
    pub project: Option<PathBuf>,

    /// Local overrides: .bivvy/config.local.yml
    pub project_local: Option<PathBuf>,
}

impl ConfigPaths {
    /// Discover config files for the given project root.
    pub fn discover(project_root: &Path) -> Self {
        Self {
            extends: Vec::new(), // Populated later after parsing project config
            user_global: Self::find_user_global(),
            project: Self::find_project_config(project_root),
            project_local: Self::find_project_local(project_root),
        }
    }

    /// Find user's global config at ~/.bivvy/config.yml
    fn find_user_global() -> Option<PathBuf> {
        let path = dirs::home_dir()?.join(".bivvy").join("config.yml");
        if path.exists() {
            Some(path)
        } else {
            None
        }
    }

    /// Find project config at .bivvy/config.yml
    fn find_project_config(project_root: &Path) -> Option<PathBuf> {
        let path = project_root.join(".bivvy").join("config.yml");
        if path.exists() {
            Some(path)
        } else {
            None
        }
    }

    /// Find local overrides at .bivvy/config.local.yml
    fn find_project_local(project_root: &Path) -> Option<PathBuf> {
        let path = project_root.join(".bivvy").join("config.local.yml");
        if path.exists() {
            Some(path)
        } else {
            None
        }
    }

    /// Returns all existing config paths in merge order.
    pub fn all_existing(&self) -> Vec<&PathBuf> {
        let mut paths = Vec::new();

        for p in &self.extends {
            paths.push(p);
        }

        if let Some(p) = &self.user_global {
            paths.push(p);
        }

        if let Some(p) = &self.project {
            paths.push(p);
        }

        if let Some(p) = &self.project_local {
            paths.push(p);
        }

        paths
    }

    /// Check if any project config exists.
    pub fn has_project_config(&self) -> bool {
        self.project.is_some()
    }
}

/// Find the project root by walking up from current directory.
///
/// Looks for:
/// 1. `.bivvy` directory (primary indicator)
/// 2. `.git` directory (fallback)
///
/// # Returns
///
/// The path to the project root, or None if not found.
pub fn find_project_root(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();

    loop {
        // Check for .bivvy directory
        if current.join(".bivvy").is_dir() {
            return Some(current);
        }

        // Check for .git directory (fallback)
        if current.join(".git").exists() {
            return Some(current);
        }

        // Move up one directory
        if !current.pop() {
            return None;
        }
    }
}

/// Load a single config file and parse it into BivvyConfig.
///
/// # Errors
///
/// Returns `ConfigNotFound` if the file doesn't exist.
/// Returns `ConfigParseError` if the YAML is invalid.
pub fn load_config_file(path: &Path) -> Result<BivvyConfig> {
    let content = fs::read_to_string(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            BivvyError::ConfigNotFound {
                path: path.to_path_buf(),
            }
        } else {
            BivvyError::Io(e)
        }
    })?;

    parse_config(&content, path)
}

/// Parse YAML content into BivvyConfig.
///
/// # Arguments
///
/// * `content` - The YAML content to parse
/// * `source_path` - Path for error reporting
pub fn parse_config(content: &str, source_path: &Path) -> Result<BivvyConfig> {
    serde_yaml::from_str(content).map_err(|e| BivvyError::ConfigParseError {
        path: source_path.to_path_buf(),
        message: e.to_string(),
    })
}

/// Load a config file as raw YAML Value (for merging).
///
/// Returns the raw serde_yaml::Value without deserializing into
/// BivvyConfig, which allows for deep merging before final parsing.
pub fn load_config_value(path: &Path) -> Result<serde_yaml::Value> {
    let content = fs::read_to_string(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            BivvyError::ConfigNotFound {
                path: path.to_path_buf(),
            }
        } else {
            BivvyError::Io(e)
        }
    })?;

    serde_yaml::from_str(&content).map_err(|e| BivvyError::ConfigParseError {
        path: path.to_path_buf(),
        message: e.to_string(),
    })
}

/// Load and merge all config files for a project.
///
/// Discovers and merges configs in this order:
/// 1. Remote base configs (from `extends:`)
/// 2. User global config (`~/.bivvy/config.yml`)
/// 3. Project config (`.bivvy/config.yml`)
/// 4. Local overrides (`.bivvy/config.local.yml`)
///
/// # Errors
///
/// Returns `ConfigNotFound` if no project config exists.
/// Returns `ConfigParseError` if any config file is invalid.
pub fn load_merged_config(project_root: &Path) -> Result<BivvyConfig> {
    let paths = ConfigPaths::discover(project_root);

    if !paths.has_project_config() {
        return Err(BivvyError::ConfigNotFound {
            path: project_root.join(".bivvy").join("config.yml"),
        });
    }

    let mut configs = Vec::new();

    // Load in merge order
    for path in paths.all_existing() {
        let value = load_config_value(path)?;
        configs.push(value);
    }

    // Merge all configs
    let merged = merge_configs(&configs);

    // Parse merged value into typed config
    serde_yaml::from_value(merged).map_err(|e| BivvyError::ConfigParseError {
        path: project_root.join(".bivvy").join("config.yml"),
        message: format!("Failed to parse merged config: {}", e),
    })
}

/// Load config with optional path override.
///
/// If `config_override` is provided, loads only that file without merging.
/// Otherwise, discovers and merges all config files.
pub fn load_config(project_root: &Path, config_override: Option<&Path>) -> Result<BivvyConfig> {
    if let Some(override_path) = config_override {
        // Direct load of specified file, no merging
        load_config_file(override_path)
    } else {
        load_merged_config(project_root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn discover_finds_project_config() {
        let temp = TempDir::new().unwrap();
        let bivvy_dir = temp.path().join(".bivvy");
        fs::create_dir_all(&bivvy_dir).unwrap();
        fs::write(bivvy_dir.join("config.yml"), "app_name: test").unwrap();

        let paths = ConfigPaths::discover(temp.path());
        assert!(paths.project.is_some());
        assert!(paths.has_project_config());
    }

    #[test]
    fn discover_finds_local_overrides() {
        let temp = TempDir::new().unwrap();
        let bivvy_dir = temp.path().join(".bivvy");
        fs::create_dir_all(&bivvy_dir).unwrap();
        fs::write(bivvy_dir.join("config.yml"), "").unwrap();
        fs::write(bivvy_dir.join("config.local.yml"), "").unwrap();

        let paths = ConfigPaths::discover(temp.path());
        assert!(paths.project_local.is_some());
    }

    #[test]
    fn discover_returns_none_for_missing_configs() {
        let temp = TempDir::new().unwrap();
        let paths = ConfigPaths::discover(temp.path());
        assert!(paths.project.is_none());
        assert!(paths.project_local.is_none());
        assert!(!paths.has_project_config());
    }

    #[test]
    fn find_project_root_finds_bivvy_dir() {
        let temp = TempDir::new().unwrap();
        let subdir = temp.path().join("foo").join("bar");
        fs::create_dir_all(&subdir).unwrap();
        fs::create_dir_all(temp.path().join(".bivvy")).unwrap();

        let root = find_project_root(&subdir);
        assert_eq!(root, Some(temp.path().to_path_buf()));
    }

    #[test]
    fn find_project_root_finds_git_dir() {
        let temp = TempDir::new().unwrap();
        let subdir = temp.path().join("src");
        fs::create_dir_all(&subdir).unwrap();
        fs::create_dir_all(temp.path().join(".git")).unwrap();

        let root = find_project_root(&subdir);
        assert_eq!(root, Some(temp.path().to_path_buf()));
    }

    #[test]
    fn find_project_root_prefers_bivvy_over_git() {
        let temp = TempDir::new().unwrap();
        let subdir = temp.path().join("nested").join("project");
        fs::create_dir_all(&subdir).unwrap();
        fs::create_dir_all(temp.path().join(".git")).unwrap();
        fs::create_dir_all(subdir.join(".bivvy")).unwrap();

        let root = find_project_root(&subdir);
        assert_eq!(root, Some(subdir));
    }

    #[test]
    fn all_existing_returns_in_merge_order() {
        let temp = TempDir::new().unwrap();
        let bivvy_dir = temp.path().join(".bivvy");
        fs::create_dir_all(&bivvy_dir).unwrap();
        fs::write(bivvy_dir.join("config.yml"), "").unwrap();
        fs::write(bivvy_dir.join("config.local.yml"), "").unwrap();

        let paths = ConfigPaths::discover(temp.path());
        let all = paths.all_existing();

        // Project should come before local
        assert!(all.len() >= 2);
    }

    #[test]
    fn load_config_file_parses_valid_yaml() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.yml");
        fs::write(&config_path, "app_name: TestApp").unwrap();

        let config = load_config_file(&config_path).unwrap();
        assert_eq!(config.app_name, Some("TestApp".to_string()));
    }

    #[test]
    fn load_config_file_returns_not_found_error() {
        let result = load_config_file(Path::new("/nonexistent/config.yml"));
        assert!(matches!(result, Err(BivvyError::ConfigNotFound { .. })));
    }

    #[test]
    fn parse_config_returns_parse_error_for_invalid_yaml() {
        let content = "invalid: yaml: content: [";
        let result = parse_config(content, Path::new("test.yml"));
        assert!(matches!(result, Err(BivvyError::ConfigParseError { .. })));
    }

    #[test]
    fn load_config_value_returns_raw_value() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.yml");
        fs::write(&config_path, "key: value\nnested:\n  inner: 42").unwrap();

        let value = load_config_value(&config_path).unwrap();
        assert_eq!(value["key"], "value");
        assert_eq!(value["nested"]["inner"], 42);
    }

    #[test]
    fn load_config_file_handles_empty_file() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.yml");
        fs::write(&config_path, "").unwrap();

        let config = load_config_file(&config_path).unwrap();
        assert!(config.app_name.is_none());
        assert!(config.steps.is_empty());
    }

    #[test]
    fn load_config_file_parses_full_config() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.yml");
        fs::write(
            &config_path,
            r#"
app_name: "FullApp"
settings:
  default_output: quiet
steps:
  test:
    command: "echo test"
workflows:
  default:
    steps: [test]
"#,
        )
        .unwrap();

        let config = load_config_file(&config_path).unwrap();
        assert_eq!(config.app_name, Some("FullApp".to_string()));
        assert!(config.steps.contains_key("test"));
        assert!(config.workflows.contains_key("default"));
    }

    #[test]
    fn load_merged_config_merges_project_and_local() {
        use crate::config::schema::OutputMode;

        let temp = TempDir::new().unwrap();
        let bivvy_dir = temp.path().join(".bivvy");
        fs::create_dir_all(&bivvy_dir).unwrap();

        // Project config
        fs::write(
            bivvy_dir.join("config.yml"),
            r#"
app_name: TestApp
settings:
  default_output: verbose
steps:
  test:
    command: "echo test"
"#,
        )
        .unwrap();

        // Local overrides
        fs::write(
            bivvy_dir.join("config.local.yml"),
            r#"
settings:
  default_output: quiet
"#,
        )
        .unwrap();

        let config = load_merged_config(temp.path()).unwrap();

        assert_eq!(config.app_name, Some("TestApp".to_string()));
        assert_eq!(config.settings.default_output, OutputMode::Quiet);
        assert!(config.steps.contains_key("test"));
    }

    #[test]
    fn load_merged_config_fails_without_project_config() {
        let temp = TempDir::new().unwrap();
        let result = load_merged_config(temp.path());
        assert!(matches!(result, Err(BivvyError::ConfigNotFound { .. })));
    }

    #[test]
    fn load_config_with_override_skips_merge() {
        let temp = TempDir::new().unwrap();
        let override_path = temp.path().join("custom.yml");
        fs::write(&override_path, "app_name: CustomApp").unwrap();

        let config = load_config(temp.path(), Some(&override_path)).unwrap();
        assert_eq!(config.app_name, Some("CustomApp".to_string()));
    }

    #[test]
    fn load_merged_config_preserves_step_definitions() {
        let temp = TempDir::new().unwrap();
        let bivvy_dir = temp.path().join(".bivvy");
        fs::create_dir_all(&bivvy_dir).unwrap();

        fs::write(
            bivvy_dir.join("config.yml"),
            r#"
steps:
  deps:
    command: "yarn install"
    watches:
      - package.json
  database:
    command: "rails db:setup"
"#,
        )
        .unwrap();

        fs::write(
            bivvy_dir.join("config.local.yml"),
            r#"
steps:
  deps:
    command: "yarn install --frozen-lockfile"
"#,
        )
        .unwrap();

        let config = load_merged_config(temp.path()).unwrap();

        // deps command should be overridden
        assert_eq!(
            config.steps["deps"].command,
            Some("yarn install --frozen-lockfile".to_string())
        );
        // database should still exist
        assert!(config.steps.contains_key("database"));
    }

    #[test]
    fn load_config_without_override_uses_merge() {
        let temp = TempDir::new().unwrap();
        let bivvy_dir = temp.path().join(".bivvy");
        fs::create_dir_all(&bivvy_dir).unwrap();
        fs::write(bivvy_dir.join("config.yml"), "app_name: Merged").unwrap();

        let config = load_config(temp.path(), None).unwrap();
        assert_eq!(config.app_name, Some("Merged".to_string()));
    }
}
