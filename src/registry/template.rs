//! Template schema definitions.
//!
//! Templates are reusable step definitions that can be referenced
//! from configuration files.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A reusable step template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    /// Template name (must be unique within source)
    pub name: String,

    /// Human-readable description
    pub description: String,

    /// Category for organization (e.g., "ruby", "node", "common")
    pub category: String,

    /// Semantic version
    #[serde(default = "default_version")]
    pub version: String,

    /// Minimum Bivvy version required
    pub min_bivvy_version: Option<String>,

    /// Platforms this template supports
    #[serde(default = "default_platforms")]
    pub platforms: Vec<Platform>,

    /// Detection rules to suggest this template
    #[serde(default)]
    pub detects: Vec<Detection>,

    /// Input contracts for this template
    #[serde(default)]
    pub inputs: HashMap<String, TemplateInput>,

    /// The step configuration this template provides
    pub step: TemplateStep,

    /// Environment impact after this step runs
    pub environment_impact: Option<EnvironmentImpact>,
}

fn default_version() -> String {
    "1.0.0".to_string()
}

fn default_platforms() -> Vec<Platform> {
    vec![Platform::MacOS, Platform::Linux, Platform::Windows]
}

/// Supported platforms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    MacOS,
    Linux,
    Windows,
}

impl Platform {
    /// Check if current platform matches.
    pub fn is_current(&self) -> bool {
        match self {
            Platform::MacOS => cfg!(target_os = "macos"),
            Platform::Linux => cfg!(target_os = "linux"),
            Platform::Windows => cfg!(target_os = "windows"),
        }
    }
}

/// Detection rule for auto-suggesting templates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Detection {
    /// File that indicates this template applies
    pub file: Option<String>,

    /// Command that must succeed (exit 0)
    pub command: Option<String>,
}

/// Template input contract for parameterized templates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateInput {
    /// Human-readable description
    pub description: String,

    /// Input type
    #[serde(rename = "type")]
    pub input_type: InputType,

    /// Whether this input is required
    #[serde(default)]
    pub required: bool,

    /// Default value if not provided
    pub default: Option<serde_yaml::Value>,

    /// Valid values for enum type
    #[serde(default)]
    pub values: Vec<String>,
}

impl TemplateInput {
    /// Validate a provided value against this input contract.
    pub fn validate(&self, name: &str, value: Option<&serde_yaml::Value>) -> Result<(), String> {
        match value {
            None => {
                if self.required && self.default.is_none() {
                    Err(format!("Required input '{}' is missing", name))
                } else {
                    Ok(())
                }
            }
            Some(v) => self.validate_type(name, v),
        }
    }

    fn validate_type(&self, name: &str, value: &serde_yaml::Value) -> Result<(), String> {
        match self.input_type {
            InputType::String => {
                if !value.is_string() {
                    return Err(format!("Input '{}' must be a string", name));
                }
            }
            InputType::Number => {
                if !value.is_number() {
                    return Err(format!("Input '{}' must be a number", name));
                }
            }
            InputType::Boolean => {
                if !value.is_bool() {
                    return Err(format!("Input '{}' must be a boolean", name));
                }
            }
            InputType::Enum => {
                let s = value
                    .as_str()
                    .ok_or_else(|| format!("Input '{}' must be a string (enum)", name))?;
                if !self.values.contains(&s.to_string()) {
                    return Err(format!(
                        "Input '{}' must be one of: {}",
                        name,
                        self.values.join(", ")
                    ));
                }
            }
        }
        Ok(())
    }

    /// Get effective value (provided or default).
    pub fn effective_value<'a>(
        &'a self,
        provided: Option<&'a serde_yaml::Value>,
    ) -> Option<&'a serde_yaml::Value> {
        provided.or(self.default.as_ref())
    }
}

/// Types of template inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InputType {
    String,
    Number,
    Boolean,
    Enum,
}

/// Step configuration provided by a template.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TemplateStep {
    /// Step title (supports interpolation)
    pub title: Option<String>,

    /// Step description
    pub description: Option<String>,

    /// Command to execute
    pub command: Option<String>,

    /// Completed check configuration
    pub completed_check: Option<crate::config::CompletedCheck>,

    /// Environment variables
    #[serde(default)]
    pub env: HashMap<String, String>,

    /// Files to watch for change detection
    #[serde(default)]
    pub watches: Vec<String>,
}

/// Environment impact after a step runs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct EnvironmentImpact {
    /// Whether this step modifies PATH
    #[serde(default)]
    pub modifies_path: bool,

    /// Shell config files that may be modified
    #[serde(default)]
    pub shell_files: Vec<String>,

    /// Paths that will be added to PATH
    #[serde(default)]
    pub path_additions: Vec<String>,

    /// Note to display to user
    pub note: Option<String>,
}

/// Source of a template (for priority ordering).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TemplateSource {
    /// Project-local templates (.bivvy/templates/)
    Project,
    /// User-local templates (~/.bivvy/templates/)
    User,
    /// Remote templates (URL-based)
    Remote {
        /// Priority for remote sources (lower is higher priority)
        priority: u32,
    },
    /// Built-in templates (embedded in binary)
    Builtin,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_template() {
        let yaml = r#"
name: test
description: "A test template"
category: common
step:
  command: "echo test"
"#;
        let template: Template = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(template.name, "test");
        assert_eq!(template.version, "1.0.0"); // default
        assert_eq!(template.platforms.len(), 3); // all platforms by default
    }

    #[test]
    fn parse_full_template() {
        let yaml = r#"
name: yarn
description: "Install Node.js dependencies using Yarn"
category: node
version: "2.0.0"
platforms: [macos, linux]
detects:
  - file: yarn.lock
  - file: package.json
step:
  title: "Install Node dependencies"
  command: "yarn install"
  completed_check:
    type: command_succeeds
    command: "yarn check --verify-tree"
  env:
    NODE_ENV: development
  watches:
    - yarn.lock
    - package.json
environment_impact:
  path_additions:
    - "./node_modules/.bin"
  note: "Node binaries are available in node_modules/.bin"
"#;
        let template: Template = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(template.name, "yarn");
        assert_eq!(template.version, "2.0.0");
        assert_eq!(template.platforms.len(), 2);
        assert_eq!(template.detects.len(), 2);
        assert!(template.step.command.is_some());
    }

    #[test]
    fn platform_is_current_matches() {
        // At least one should match (test runs on some platform)
        let platforms = [Platform::MacOS, Platform::Linux, Platform::Windows];
        assert!(platforms.iter().any(|p| p.is_current()));
    }

    #[test]
    fn template_source_ordering() {
        // Project > User > Remote > Builtin (lower is higher priority)
        assert!(TemplateSource::Project < TemplateSource::User);
        assert!(TemplateSource::User < TemplateSource::Remote { priority: 10 });
        assert!(TemplateSource::Remote { priority: 10 } < TemplateSource::Builtin);
    }

    #[test]
    fn parse_template_with_inputs() {
        let yaml = r#"
name: database-setup
description: "Setup database"
category: common
inputs:
  database_name:
    description: "Name of the database"
    type: string
    required: true
  environment:
    description: "Target environment"
    type: enum
    values: [development, test, staging]
    default: development
step:
  command: "rails db:setup"
"#;
        let template: Template = serde_yaml::from_str(yaml).unwrap();
        assert!(template.inputs.contains_key("database_name"));
        assert!(template.inputs["database_name"].required);
        assert_eq!(template.inputs["environment"].values.len(), 3);
    }

    #[test]
    fn validate_required_input_missing() {
        let input = TemplateInput {
            description: "test".to_string(),
            input_type: InputType::String,
            required: true,
            default: None,
            values: vec![],
        };
        assert!(input.validate("test", None).is_err());
    }

    #[test]
    fn validate_required_input_with_default() {
        let input = TemplateInput {
            description: "test".to_string(),
            input_type: InputType::String,
            required: true,
            default: Some(serde_yaml::Value::String("default".to_string())),
            values: vec![],
        };
        assert!(input.validate("test", None).is_ok());
    }

    #[test]
    fn validate_string_type() {
        let input = TemplateInput {
            description: "test".to_string(),
            input_type: InputType::String,
            required: false,
            default: None,
            values: vec![],
        };

        let valid = serde_yaml::Value::String("hello".to_string());
        let invalid = serde_yaml::Value::Number(42.into());

        assert!(input.validate("test", Some(&valid)).is_ok());
        assert!(input.validate("test", Some(&invalid)).is_err());
    }

    #[test]
    fn validate_boolean_type() {
        let input = TemplateInput {
            description: "test".to_string(),
            input_type: InputType::Boolean,
            required: false,
            default: None,
            values: vec![],
        };

        let valid = serde_yaml::Value::Bool(true);
        let invalid = serde_yaml::Value::String("true".to_string());

        assert!(input.validate("test", Some(&valid)).is_ok());
        assert!(input.validate("test", Some(&invalid)).is_err());
    }

    #[test]
    fn validate_enum_type() {
        let input = TemplateInput {
            description: "test".to_string(),
            input_type: InputType::Enum,
            required: false,
            default: None,
            values: vec!["a".to_string(), "b".to_string()],
        };

        let valid = serde_yaml::Value::String("a".to_string());
        let invalid = serde_yaml::Value::String("c".to_string());

        assert!(input.validate("test", Some(&valid)).is_ok());
        assert!(input.validate("test", Some(&invalid)).is_err());
    }

    #[test]
    fn effective_value_prefers_provided() {
        let input = TemplateInput {
            description: "test".to_string(),
            input_type: InputType::String,
            required: false,
            default: Some(serde_yaml::Value::String("default".to_string())),
            values: vec![],
        };

        let provided = serde_yaml::Value::String("provided".to_string());
        assert_eq!(
            input.effective_value(Some(&provided)).unwrap().as_str(),
            Some("provided")
        );
        assert_eq!(
            input.effective_value(None).unwrap().as_str(),
            Some("default")
        );
    }
}
