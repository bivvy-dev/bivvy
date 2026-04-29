//! Environment variable layering.
//!
//! This module provides priority-based environment variable management
//! with source tracking for debugging.

use crate::config::environment::load_env_file;
use crate::config::schema::BivvyConfig;
use crate::error::Result;
use std::collections::HashMap;
use std::path::Path;

/// Represents a layer of environment variables.
///
/// # Example
///
/// ```
/// use bivvy::config::EnvLayer;
///
/// let mut layer = EnvLayer::new("config.yml");
/// layer.set("DATABASE_URL", "postgres://localhost/db");
/// layer.set("DEBUG", "true");
///
/// assert_eq!(layer.vars.get("DATABASE_URL").map(String::as_str), Some("postgres://localhost/db"));
/// assert_eq!(layer.source, "config.yml");
/// ```
#[derive(Debug, Clone, Default)]
pub struct EnvLayer {
    /// Variables in this layer.
    pub vars: HashMap<String, String>,
    /// Source of this layer (for debugging).
    pub source: String,
}

impl EnvLayer {
    /// Create a new layer with the given source name.
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            vars: HashMap::new(),
            source: source.into(),
        }
    }

    /// Add a variable to this layer.
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.vars.insert(key.into(), value.into());
    }

    /// Check if this layer has a variable.
    pub fn contains(&self, key: &str) -> bool {
        self.vars.contains_key(key)
    }

    /// Get the number of variables in this layer.
    pub fn len(&self) -> usize {
        self.vars.len()
    }

    /// Check if this layer is empty.
    pub fn is_empty(&self) -> bool {
        self.vars.is_empty()
    }
}

/// Manages layered environment variables.
///
/// Variables from higher layers override variables from lower layers.
/// The first layer pushed has lowest priority, the last has highest.
///
/// # Example
///
/// ```
/// use bivvy::config::{EnvLayer, EnvLayerStack};
///
/// let mut stack = EnvLayerStack::new();
///
/// // Base layer (lowest priority)
/// let mut base = EnvLayer::new("base");
/// base.set("KEY", "base_value");
/// base.set("BASE_ONLY", "from_base");
/// stack.push(base);
///
/// // Override layer (higher priority)
/// let mut overlay = EnvLayer::new("overlay");
/// overlay.set("KEY", "override_value");
/// stack.push(overlay);
///
/// assert_eq!(stack.get("KEY"), Some("override_value"));
/// assert_eq!(stack.get("BASE_ONLY"), Some("from_base"));
/// assert_eq!(stack.source_of("KEY"), Some("overlay"));
/// ```
pub struct EnvLayerStack {
    /// Layers from lowest to highest priority.
    layers: Vec<EnvLayer>,
}

impl EnvLayerStack {
    /// Create a new empty stack.
    pub fn new() -> Self {
        Self { layers: vec![] }
    }

    /// Add a layer with the given priority.
    ///
    /// Later layers have higher priority.
    pub fn push(&mut self, layer: EnvLayer) {
        self.layers.push(layer);
    }

    /// Get the resolved value for a variable.
    ///
    /// Returns the value from the highest priority layer that contains the key.
    pub fn get(&self, key: &str) -> Option<&str> {
        // Higher index = higher priority
        for layer in self.layers.iter().rev() {
            if let Some(value) = layer.vars.get(key) {
                return Some(value);
            }
        }
        None
    }

    /// Get all resolved variables.
    ///
    /// Higher priority layers override lower priority layers.
    pub fn resolve(&self) -> HashMap<String, String> {
        let mut result = HashMap::new();
        for layer in &self.layers {
            result.extend(layer.vars.clone());
        }
        result
    }

    /// Get the source of a variable's value.
    ///
    /// Returns the source of the highest priority layer that contains the key.
    pub fn source_of(&self, key: &str) -> Option<&str> {
        for layer in self.layers.iter().rev() {
            if layer.vars.contains_key(key) {
                return Some(&layer.source);
            }
        }
        None
    }

    /// Get all layers for inspection.
    pub fn layers(&self) -> &[EnvLayer] {
        &self.layers
    }

    /// Get the number of layers.
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }
}

impl Default for EnvLayerStack {
    fn default() -> Self {
        Self::new()
    }
}

/// Build the YAML-defined env layer stack for a given workflow.
///
/// Layers are pushed in priority order (lowest → highest):
///
/// 1. `settings.env_vars.env_file` (loaded from disk, relative to project_root)
/// 2. `settings.env_vars.env`
/// 3. `workflows.<name>.env_file` (loaded from disk, relative to project_root)
/// 4. `workflows.<name>.env`
///
/// The returned stack is the YAML-derived "base" env. Step-level `env_file`
/// and `env`, plus the parent process environment, are layered on top of this
/// inside the step executor — the process env always wins.
///
/// If the named workflow does not exist in the config, only the global layers
/// are included.
///
/// # Errors
///
/// Returns an error if any specified env_file cannot be read or parsed.
pub fn build_yaml_env_stack(
    config: &BivvyConfig,
    workflow_name: &str,
    project_root: &Path,
) -> Result<EnvLayerStack> {
    let mut stack = EnvLayerStack::new();

    if let Some(ref env_file) = config.settings.env_vars.env_file {
        let resolved = project_root.join(env_file);
        let file_env = load_env_file(&resolved)?;
        let mut layer = EnvLayer::new("settings.env_vars.env_file");
        for (k, v) in file_env {
            layer.set(k, v);
        }
        stack.push(layer);
    }

    if !config.settings.env_vars.env.is_empty() {
        let mut layer = EnvLayer::new("settings.env_vars.env");
        for (k, v) in &config.settings.env_vars.env {
            layer.set(k, v);
        }
        stack.push(layer);
    }

    if let Some(workflow) = config.workflows.get(workflow_name) {
        if let Some(ref env_file) = workflow.env_file {
            let resolved = project_root.join(env_file);
            let file_env = load_env_file(&resolved)?;
            let mut layer = EnvLayer::new(format!("workflows.{}.env_file", workflow_name));
            for (k, v) in file_env {
                layer.set(k, v);
            }
            stack.push(layer);
        }

        if !workflow.env.is_empty() {
            let mut layer = EnvLayer::new(format!("workflows.{}.env", workflow_name));
            for (k, v) in &workflow.env {
                layer.set(k, v);
            }
            stack.push(layer);
        }
    }

    Ok(stack)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn higher_layers_override_lower() {
        let mut stack = EnvLayerStack::new();

        let mut base = EnvLayer::new("base");
        base.set("KEY", "base");
        stack.push(base);

        let mut local = EnvLayer::new("local");
        local.set("KEY", "override");
        stack.push(local);

        assert_eq!(stack.get("KEY"), Some("override"));
    }

    #[test]
    fn tracks_variable_source() {
        let mut stack = EnvLayerStack::new();

        let mut layer = EnvLayer::new("config.yml");
        layer.set("KEY", "value");
        stack.push(layer);

        assert_eq!(stack.source_of("KEY"), Some("config.yml"));
    }

    #[test]
    fn resolve_merges_all_layers() {
        let mut stack = EnvLayerStack::new();

        let mut layer1 = EnvLayer::new("layer1");
        layer1.set("A", "1");
        stack.push(layer1);

        let mut layer2 = EnvLayer::new("layer2");
        layer2.set("B", "2");
        stack.push(layer2);

        let resolved = stack.resolve();
        assert_eq!(resolved.get("A"), Some(&"1".to_string()));
        assert_eq!(resolved.get("B"), Some(&"2".to_string()));
    }

    #[test]
    fn missing_key_returns_none() {
        let stack = EnvLayerStack::new();
        assert_eq!(stack.get("MISSING"), None);
        assert_eq!(stack.source_of("MISSING"), None);
    }

    #[test]
    fn layer_contains_check() {
        let mut layer = EnvLayer::new("test");
        layer.set("KEY", "value");

        assert!(layer.contains("KEY"));
        assert!(!layer.contains("OTHER"));
    }

    #[test]
    fn layer_len_and_is_empty() {
        let mut layer = EnvLayer::new("test");
        assert!(layer.is_empty());
        assert_eq!(layer.len(), 0);

        layer.set("KEY", "value");
        assert!(!layer.is_empty());
        assert_eq!(layer.len(), 1);
    }

    #[test]
    fn stack_layer_count() {
        let mut stack = EnvLayerStack::new();
        assert_eq!(stack.layer_count(), 0);

        stack.push(EnvLayer::new("layer1"));
        stack.push(EnvLayer::new("layer2"));
        assert_eq!(stack.layer_count(), 2);
    }

    #[test]
    fn default_stack_is_empty() {
        let stack = EnvLayerStack::default();
        assert_eq!(stack.layer_count(), 0);
    }

    #[test]
    fn multiple_overrides() {
        let mut stack = EnvLayerStack::new();

        let mut layer1 = EnvLayer::new("layer1");
        layer1.set("KEY", "value1");
        stack.push(layer1);

        let mut layer2 = EnvLayer::new("layer2");
        layer2.set("KEY", "value2");
        stack.push(layer2);

        let mut layer3 = EnvLayer::new("layer3");
        layer3.set("KEY", "value3");
        stack.push(layer3);

        assert_eq!(stack.get("KEY"), Some("value3"));
        assert_eq!(stack.source_of("KEY"), Some("layer3"));
    }

    mod build_yaml_env_stack {
        use super::*;
        use std::fs;
        use tempfile::TempDir;

        fn parse(yaml: &str) -> BivvyConfig {
            serde_yaml::from_str(yaml).unwrap()
        }

        #[test]
        fn empty_config_produces_empty_stack() {
            let config = parse("app_name: test\n");
            let temp = TempDir::new().unwrap();
            let stack = build_yaml_env_stack(&config, "default", temp.path()).unwrap();
            assert_eq!(stack.layer_count(), 0);
        }

        #[test]
        fn settings_env_layer_is_pushed() {
            let config = parse(
                r#"
app_name: test
settings:
  env:
    GLOBAL_KEY: global_value
"#,
            );
            let temp = TempDir::new().unwrap();
            let stack = build_yaml_env_stack(&config, "default", temp.path()).unwrap();
            assert_eq!(stack.get("GLOBAL_KEY"), Some("global_value"));
            assert_eq!(stack.source_of("GLOBAL_KEY"), Some("settings.env_vars.env"));
        }

        #[test]
        fn workflow_env_overrides_settings_env() {
            let config = parse(
                r#"
app_name: test
settings:
  env:
    DATABASE_URL: from_settings
workflows:
  default:
    steps: []
    env:
      DATABASE_URL: from_workflow
"#,
            );
            let temp = TempDir::new().unwrap();
            let stack = build_yaml_env_stack(&config, "default", temp.path()).unwrap();
            assert_eq!(stack.get("DATABASE_URL"), Some("from_workflow"));
            assert_eq!(
                stack.source_of("DATABASE_URL"),
                Some("workflows.default.env")
            );
        }

        #[test]
        fn settings_env_file_layer_loads_from_disk() {
            let temp = TempDir::new().unwrap();
            fs::write(
                temp.path().join(".env.global"),
                "GLOBAL_URL=https://global\n",
            )
            .unwrap();
            let config = parse(
                r#"
app_name: test
settings:
  env_file: .env.global
"#,
            );
            let stack = build_yaml_env_stack(&config, "default", temp.path()).unwrap();
            assert_eq!(stack.get("GLOBAL_URL"), Some("https://global"));
            assert_eq!(
                stack.source_of("GLOBAL_URL"),
                Some("settings.env_vars.env_file"),
            );
        }

        #[test]
        fn settings_env_overrides_settings_env_file() {
            let temp = TempDir::new().unwrap();
            fs::write(temp.path().join(".env.global"), "PORT=8080\n").unwrap();
            let config = parse(
                r#"
app_name: test
settings:
  env_file: .env.global
  env:
    PORT: "9090"
"#,
            );
            let stack = build_yaml_env_stack(&config, "default", temp.path()).unwrap();
            assert_eq!(stack.get("PORT"), Some("9090"));
            assert_eq!(stack.source_of("PORT"), Some("settings.env_vars.env"));
        }

        #[test]
        fn workflow_env_file_layer_loads_from_disk() {
            let temp = TempDir::new().unwrap();
            fs::write(temp.path().join(".env.ci"), "CI_TOKEN=abc123\n").unwrap();
            let config = parse(
                r#"
app_name: test
workflows:
  ci:
    steps: []
    env_file: .env.ci
"#,
            );
            let stack = build_yaml_env_stack(&config, "ci", temp.path()).unwrap();
            assert_eq!(stack.get("CI_TOKEN"), Some("abc123"));
            assert_eq!(stack.source_of("CI_TOKEN"), Some("workflows.ci.env_file"));
        }

        #[test]
        fn workflow_env_overrides_workflow_env_file() {
            let temp = TempDir::new().unwrap();
            fs::write(temp.path().join(".env.ci"), "MODE=fast\n").unwrap();
            let config = parse(
                r#"
app_name: test
workflows:
  ci:
    steps: []
    env_file: .env.ci
    env:
      MODE: thorough
"#,
            );
            let stack = build_yaml_env_stack(&config, "ci", temp.path()).unwrap();
            assert_eq!(stack.get("MODE"), Some("thorough"));
            assert_eq!(stack.source_of("MODE"), Some("workflows.ci.env"));
        }

        #[test]
        fn unknown_workflow_only_includes_global_layers() {
            let config = parse(
                r#"
app_name: test
settings:
  env:
    GLOBAL_KEY: global_value
workflows:
  ci:
    steps: []
    env:
      CI_ONLY: ci_value
"#,
            );
            let temp = TempDir::new().unwrap();
            let stack = build_yaml_env_stack(&config, "default", temp.path()).unwrap();
            assert_eq!(stack.get("GLOBAL_KEY"), Some("global_value"));
            assert_eq!(stack.get("CI_ONLY"), None);
        }

        #[test]
        fn missing_settings_env_file_is_an_error() {
            let config = parse(
                r#"
app_name: test
settings:
  env_file: .env.does-not-exist
"#,
            );
            let temp = TempDir::new().unwrap();
            let result = build_yaml_env_stack(&config, "default", temp.path());
            assert!(result.is_err());
        }

        #[test]
        fn missing_workflow_env_file_is_an_error() {
            let config = parse(
                r#"
app_name: test
workflows:
  ci:
    steps: []
    env_file: .env.missing
"#,
            );
            let temp = TempDir::new().unwrap();
            let result = build_yaml_env_stack(&config, "ci", temp.path());
            assert!(result.is_err());
        }
    }
}
