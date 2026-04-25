//! Configuration validation rules.
//!
//! This module validates configuration for correctness:
//! - Steps must have either template or command
//! - depends_on must reference existing steps
//! - Workflows must reference existing steps
//! - No circular dependencies allowed
//! - Var names must be valid identifiers
//! - Var names must not collide with builtin variables
//! - Computed vars must have non-empty commands

use crate::config::schema::{BivvyConfig, VarDefinition};
use crate::error::{BivvyError, Result};
use std::collections::HashSet;

/// Built-in interpolation variable names that user vars must not shadow.
const BUILTIN_VAR_NAMES: &[&str] = &["bivvy_version", "project_name", "project_root"];

/// Validation error with context.
#[derive(Debug, Clone)]
pub struct ValidationError {
    /// Rule identifier
    pub rule: String,
    /// Human-readable error message
    pub message: String,
    /// Step name if error is step-specific
    pub step: Option<String>,
    /// Workflow name if error is workflow-specific
    pub workflow: Option<String>,
}

/// Validate a configuration and return all errors.
///
/// This function collects all validation errors rather than stopping
/// at the first one, allowing users to fix multiple issues at once.
pub fn validate_config(config: &BivvyConfig) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    errors.extend(validate_vars(config));
    errors.extend(validate_steps(config));
    errors.extend(validate_workflows(config));
    errors.extend(validate_dependencies(config));

    errors
}

/// Check whether a var name is a valid identifier.
///
/// Valid names start with a letter or underscore and contain only
/// ASCII letters, digits, and underscores.
fn is_valid_var_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let mut chars = name.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Validate user-defined variable declarations.
fn validate_vars(config: &BivvyConfig) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    for (name, def) in &config.vars {
        // Empty or invalid var name
        if !is_valid_var_name(name) {
            errors.push(ValidationError {
                rule: "invalid-var-name".to_string(),
                message: format!(
                    "Variable '{}' has an invalid name. \
                     Names must start with a letter or underscore and \
                     contain only letters, digits, and underscores.",
                    name
                ),
                step: None,
                workflow: None,
            });
        }

        // Collision with builtin variables
        if BUILTIN_VAR_NAMES.contains(&name.as_str()) {
            errors.push(ValidationError {
                rule: "builtin-var-collision".to_string(),
                message: format!(
                    "Variable '{}' collides with a built-in variable. \
                     Choose a different name.",
                    name
                ),
                step: None,
                workflow: None,
            });
        }

        // Computed vars must have a non-empty command
        if let VarDefinition::Computed { command } = def {
            if command.trim().is_empty() {
                errors.push(ValidationError {
                    rule: "empty-var-command".to_string(),
                    message: format!("Computed variable '{}' has an empty command.", name),
                    step: None,
                    workflow: None,
                });
            }
        }
    }

    errors
}

/// Validate step definitions.
fn validate_steps(config: &BivvyConfig) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    for (name, step) in &config.steps {
        // Step must have either template or command
        if step.template.is_none() && step.execution.command.is_none() {
            errors.push(ValidationError {
                rule: "missing-command".to_string(),
                message: format!("Step '{}' must have either 'template' or 'command'", name),
                step: Some(name.clone()),
                workflow: None,
            });
        }

        // Validate depends_on references
        for dep in &step.depends_on {
            if !config.steps.contains_key(dep) {
                errors.push(ValidationError {
                    rule: "unknown-step".to_string(),
                    message: format!("Step '{}' depends on '{}' which does not exist", name, dep),
                    step: Some(name.clone()),
                    workflow: None,
                });
            }
        }
    }

    errors
}

/// Validate workflow definitions.
fn validate_workflows(config: &BivvyConfig) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    for (name, workflow) in &config.workflows {
        // Validate step references
        for step_name in &workflow.steps {
            if !config.steps.contains_key(step_name) {
                errors.push(ValidationError {
                    rule: "unknown-workflow-step".to_string(),
                    message: format!(
                        "Workflow '{}' references step '{}' which does not exist",
                        name, step_name
                    ),
                    step: None,
                    workflow: Some(name.clone()),
                });
            }
        }

        // Validate override references
        for step_name in workflow.overrides.keys() {
            if !workflow.steps.contains(step_name) {
                errors.push(ValidationError {
                    rule: "unknown-override-step".to_string(),
                    message: format!(
                        "Workflow '{}' has override for '{}' which is not in the workflow",
                        name, step_name
                    ),
                    step: None,
                    workflow: Some(name.clone()),
                });
            }
        }
    }

    errors
}

/// Validate step dependencies for cycles.
fn validate_dependencies(config: &BivvyConfig) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let mut visited = HashSet::new();
    let mut rec_stack = HashSet::new();
    let mut path = Vec::new();

    for step_name in config.steps.keys() {
        if let Some(cycle) =
            detect_cycle(step_name, config, &mut visited, &mut rec_stack, &mut path)
        {
            errors.push(ValidationError {
                rule: "circular-dependency".to_string(),
                message: format!("Circular dependency detected: {}", cycle),
                step: Some(step_name.clone()),
                workflow: None,
            });
            break; // Only report one cycle
        }
    }

    errors
}

fn detect_cycle(
    step: &str,
    config: &BivvyConfig,
    visited: &mut HashSet<String>,
    rec_stack: &mut HashSet<String>,
    path: &mut Vec<String>,
) -> Option<String> {
    if rec_stack.contains(step) {
        // Found cycle - format it
        let cycle_start = path.iter().position(|s| s == step).unwrap();
        let cycle: Vec<_> = path[cycle_start..].to_vec();
        return Some(format!("{} -> {}", cycle.join(" -> "), step));
    }

    if visited.contains(step) {
        return None;
    }

    visited.insert(step.to_string());
    rec_stack.insert(step.to_string());
    path.push(step.to_string());

    if let Some(step_config) = config.steps.get(step) {
        for dep in &step_config.depends_on {
            if let Some(cycle) = detect_cycle(dep, config, visited, rec_stack, path) {
                return Some(cycle);
            }
        }
    }

    path.pop();
    rec_stack.remove(step);
    None
}

/// Validate and return Result (for convenience).
///
/// # Errors
///
/// Returns `ConfigValidationError` if any validation rules fail.
pub fn validate(config: &BivvyConfig) -> Result<()> {
    let errors = validate_config(config);

    if errors.is_empty() {
        Ok(())
    } else {
        let messages: Vec<_> = errors.iter().map(|e| e.message.clone()).collect();
        Err(BivvyError::ConfigValidationError {
            message: messages.join("; "),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::{
        ExecutionConfig, StepConfig, StepOverride, VarDefinition, WorkflowConfig,
    };

    #[test]
    fn validates_step_has_command_or_template() {
        let mut config = BivvyConfig::default();
        config
            .steps
            .insert("empty".to_string(), StepConfig::default());

        let errors = validate_config(&config);
        assert!(errors.iter().any(|e| e.rule == "missing-command"));
    }

    #[test]
    fn validates_depends_on_exists() {
        let mut config = BivvyConfig::default();
        let step = StepConfig {
            execution: ExecutionConfig {
                command: Some("echo test".to_string()),
                ..Default::default()
            },
            depends_on: vec!["nonexistent".to_string()],
            ..Default::default()
        };
        config.steps.insert("test".to_string(), step);

        let errors = validate_config(&config);
        assert!(errors.iter().any(|e| e.rule == "unknown-step"));
    }

    #[test]
    fn validates_workflow_step_exists() {
        let mut config = BivvyConfig::default();
        let workflow = WorkflowConfig {
            steps: vec!["nonexistent".to_string()],
            ..Default::default()
        };
        config.workflows.insert("default".to_string(), workflow);

        let errors = validate_config(&config);
        assert!(errors.iter().any(|e| e.rule == "unknown-workflow-step"));
    }

    #[test]
    fn detects_circular_dependency() {
        let mut config = BivvyConfig::default();

        let step_a = StepConfig {
            execution: ExecutionConfig {
                command: Some("a".to_string()),
                ..Default::default()
            },
            depends_on: vec!["b".to_string()],
            ..Default::default()
        };

        let step_b = StepConfig {
            execution: ExecutionConfig {
                command: Some("b".to_string()),
                ..Default::default()
            },
            depends_on: vec!["a".to_string()],
            ..Default::default()
        };

        config.steps.insert("a".to_string(), step_a);
        config.steps.insert("b".to_string(), step_b);

        let errors = validate_config(&config);
        assert!(errors.iter().any(|e| e.rule == "circular-dependency"));
    }

    #[test]
    fn valid_config_returns_no_errors() {
        let mut config = BivvyConfig::default();

        let step_a = StepConfig {
            execution: ExecutionConfig {
                command: Some("a".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };

        let step_b = StepConfig {
            execution: ExecutionConfig {
                command: Some("b".to_string()),
                ..Default::default()
            },
            depends_on: vec!["a".to_string()],
            ..Default::default()
        };

        config.steps.insert("a".to_string(), step_a);
        config.steps.insert("b".to_string(), step_b);

        let workflow = WorkflowConfig {
            steps: vec!["a".to_string(), "b".to_string()],
            ..Default::default()
        };
        config.workflows.insert("default".to_string(), workflow);

        let errors = validate_config(&config);
        assert!(errors.is_empty());
    }

    #[test]
    fn validates_workflow_override_in_workflow() {
        let mut config = BivvyConfig::default();

        let step = StepConfig {
            execution: ExecutionConfig {
                command: Some("test".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };
        config.steps.insert("test".to_string(), step);

        let mut overrides = std::collections::HashMap::new();
        overrides.insert("other".to_string(), StepOverride::default());

        let workflow = WorkflowConfig {
            steps: vec!["test".to_string()],
            overrides,
            ..Default::default()
        };
        config.workflows.insert("default".to_string(), workflow);

        let errors = validate_config(&config);
        assert!(errors.iter().any(|e| e.rule == "unknown-override-step"));
    }

    #[test]
    fn validate_returns_result() {
        let config = BivvyConfig::default();
        assert!(validate(&config).is_ok());

        let mut bad_config = BivvyConfig::default();
        bad_config
            .steps
            .insert("empty".to_string(), StepConfig::default());
        assert!(validate(&bad_config).is_err());
    }

    // --- Var validation tests ---

    #[test]
    fn validates_empty_var_name() {
        let mut config = BivvyConfig::default();
        config
            .vars
            .insert("".to_string(), VarDefinition::Static("val".to_string()));

        let errors = validate_config(&config);
        assert!(errors.iter().any(|e| e.rule == "invalid-var-name"));
    }

    #[test]
    fn validates_var_name_with_spaces() {
        let mut config = BivvyConfig::default();
        config.vars.insert(
            "my var".to_string(),
            VarDefinition::Static("val".to_string()),
        );

        let errors = validate_config(&config);
        assert!(errors.iter().any(|e| e.rule == "invalid-var-name"));
    }

    #[test]
    fn validates_var_name_starting_with_digit() {
        let mut config = BivvyConfig::default();
        config.vars.insert(
            "9lives".to_string(),
            VarDefinition::Static("val".to_string()),
        );

        let errors = validate_config(&config);
        assert!(errors.iter().any(|e| e.rule == "invalid-var-name"));
    }

    #[test]
    fn validates_var_name_with_special_chars() {
        let mut config = BivvyConfig::default();
        config.vars.insert(
            "my-var".to_string(),
            VarDefinition::Static("val".to_string()),
        );

        let errors = validate_config(&config);
        assert!(errors.iter().any(|e| e.rule == "invalid-var-name"));
    }

    #[test]
    fn accepts_valid_var_names() {
        let mut config = BivvyConfig::default();
        config.vars.insert(
            "my_var".to_string(),
            VarDefinition::Static("val".to_string()),
        );
        config.vars.insert(
            "_private".to_string(),
            VarDefinition::Static("val".to_string()),
        );
        config.vars.insert(
            "VERSION2".to_string(),
            VarDefinition::Static("val".to_string()),
        );

        let errors = validate_config(&config);
        assert!(!errors.iter().any(|e| e.rule == "invalid-var-name"));
    }

    #[test]
    fn validates_builtin_var_collision_bivvy_version() {
        let mut config = BivvyConfig::default();
        config.vars.insert(
            "bivvy_version".to_string(),
            VarDefinition::Static("1.0".to_string()),
        );

        let errors = validate_config(&config);
        assert!(errors.iter().any(|e| e.rule == "builtin-var-collision"));
    }

    #[test]
    fn validates_builtin_var_collision_project_name() {
        let mut config = BivvyConfig::default();
        config.vars.insert(
            "project_name".to_string(),
            VarDefinition::Static("test".to_string()),
        );

        let errors = validate_config(&config);
        assert!(errors.iter().any(|e| e.rule == "builtin-var-collision"));
    }

    #[test]
    fn validates_builtin_var_collision_project_root() {
        let mut config = BivvyConfig::default();
        config.vars.insert(
            "project_root".to_string(),
            VarDefinition::Static("/tmp".to_string()),
        );

        let errors = validate_config(&config);
        assert!(errors.iter().any(|e| e.rule == "builtin-var-collision"));
    }

    #[test]
    fn validates_computed_var_empty_command() {
        let mut config = BivvyConfig::default();
        config.vars.insert(
            "empty_cmd".to_string(),
            VarDefinition::Computed {
                command: "".to_string(),
            },
        );

        let errors = validate_config(&config);
        assert!(errors.iter().any(|e| e.rule == "empty-var-command"));
    }

    #[test]
    fn validates_computed_var_whitespace_only_command() {
        let mut config = BivvyConfig::default();
        config.vars.insert(
            "ws_cmd".to_string(),
            VarDefinition::Computed {
                command: "   \t  ".to_string(),
            },
        );

        let errors = validate_config(&config);
        assert!(errors.iter().any(|e| e.rule == "empty-var-command"));
    }

    #[test]
    fn accepts_valid_computed_var() {
        let mut config = BivvyConfig::default();
        config.vars.insert(
            "version".to_string(),
            VarDefinition::Computed {
                command: "echo 1.0.0".to_string(),
            },
        );

        let errors = validate_config(&config);
        assert!(!errors.iter().any(|e| e.rule == "empty-var-command"));
        assert!(!errors.iter().any(|e| e.rule == "invalid-var-name"));
        assert!(!errors.iter().any(|e| e.rule == "builtin-var-collision"));
    }

    #[test]
    fn no_var_errors_for_empty_vars() {
        let config = BivvyConfig::default();
        let errors = validate_config(&config);
        assert!(!errors.iter().any(|e| e.rule.contains("var")));
    }

    #[test]
    fn is_valid_var_name_helper() {
        assert!(is_valid_var_name("abc"));
        assert!(is_valid_var_name("_abc"));
        assert!(is_valid_var_name("a1"));
        assert!(is_valid_var_name("A_B_C"));
        assert!(!is_valid_var_name(""));
        assert!(!is_valid_var_name("1abc"));
        assert!(!is_valid_var_name("a b"));
        assert!(!is_valid_var_name("a-b"));
        assert!(!is_valid_var_name("a.b"));
    }
}
