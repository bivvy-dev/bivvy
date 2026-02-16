//! Configuration loading, parsing, and validation for Bivvy.
//!
//! This module handles all aspects of configuration:
//! - Schema definitions in [`schema`]
//! - File discovery and loading in [`loader`]
//! - Deep merging in [`merger`]
//! - Validation in [`validator`]
//! - Variable interpolation in [`interpolation`]
//! - Environment variable handling in [`environment`]
//!
//! # Example
//!
//! ```
//! use bivvy::config::{load_merged_config, validate};
//! use tempfile::TempDir;
//! use std::fs;
//!
//! let temp = TempDir::new().unwrap();
//! let bivvy_dir = temp.path().join(".bivvy");
//! fs::create_dir_all(&bivvy_dir).unwrap();
//! fs::write(bivvy_dir.join("config.yml"), "app_name: test").unwrap();
//!
//! let config = load_merged_config(temp.path()).unwrap();
//! validate(&config).unwrap();
//! assert_eq!(config.app_name, Some("test".to_string()));
//! ```
//!
//! # Configuration File Locations
//!
//! Bivvy discovers and merges configuration in this order:
//! 1. Remote base configs (from `extends:`)
//! 2. User global config (`~/.bivvy/config.yml`)
//! 3. Project config (`.bivvy/config.yml`)
//! 4. Local overrides (`.bivvy/config.local.yml`)

pub mod env_file;
pub mod env_layer;
pub mod environment;
pub mod extends;
pub mod interpolation;
pub mod loader;
pub mod merger;
pub mod remote;
pub mod schema;
pub mod validator;

// Schema re-exports
pub use schema::{
    BivvyConfig, CompletedCheck, CustomRequirement, CustomRequirementCheck, OutputMode,
    PromptConfig, PromptType, SecretConfig, Settings, StepConfig, StepEnvironmentOverride,
    StepOutputConfig, StepOverride, TemplateSource, WorkflowConfig, WorkflowSettings,
};

// Loader re-exports
pub use loader::{
    find_project_root, load_config, load_config_file, load_config_value, load_merged_config,
    load_merged_config_with_resolver, parse_config, ConfigPaths,
};

// Merger re-exports
pub use merger::{deep_merge, merge_configs};

// Validator re-exports
pub use validator::{validate, validate_config, ValidationError};

// Interpolation re-exports
pub use interpolation::{
    extract_variables, has_interpolation, parse_interpolation, resolve_string,
    resolve_string_with_default, InterpolationContext, Segment,
};

// Environment re-exports
pub use environment::{
    is_secret, load_env_file, load_env_file_optional, load_system_env, merge_env, SECRET_PATTERNS,
};

// Remote re-exports
pub use remote::{resolve_auth, AuthHeader, RemoteFetcher};

// Extends re-exports
pub use extends::{validate_extends, ExtendsResolver};

// Env layer re-exports
pub use env_layer::{EnvLayer, EnvLayerStack};

// Env file re-exports
pub use env_file::EnvFileParser;

#[cfg(test)]
mod tests {
    #[test]
    fn serde_yaml_parses_basic_yaml() {
        let yaml = "name: test\nvalue: 42";
        let parsed: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(parsed["name"], "test");
        assert_eq!(parsed["value"], 42);
    }

    #[test]
    fn serde_yaml_handles_nested_structures() {
        let yaml = r#"
          settings:
            output: verbose
            logging: true
          steps:
            - name: first
            - name: second
        "#;
        let parsed: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(parsed["settings"]["output"], "verbose");
        assert_eq!(parsed["steps"][0]["name"], "first");
    }
}
