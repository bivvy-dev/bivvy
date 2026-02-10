//! Configuration validation rules.
//!
//! This module validates configuration for correctness:
//! - Steps must have either template or command
//! - depends_on must reference existing steps
//! - Workflows must reference existing steps
//! - No circular dependencies allowed

use crate::config::schema::BivvyConfig;
use crate::error::{BivvyError, Result};
use std::collections::HashSet;

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

    errors.extend(validate_steps(config));
    errors.extend(validate_workflows(config));
    errors.extend(validate_dependencies(config));

    errors
}

/// Validate step definitions.
fn validate_steps(config: &BivvyConfig) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    for (name, step) in &config.steps {
        // Step must have either template or command
        if step.template.is_none() && step.command.is_none() {
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
    use crate::config::schema::{StepConfig, StepOverride, WorkflowConfig};

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
            command: Some("echo test".to_string()),
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
            command: Some("a".to_string()),
            depends_on: vec!["b".to_string()],
            ..Default::default()
        };

        let step_b = StepConfig {
            command: Some("b".to_string()),
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
            command: Some("a".to_string()),
            ..Default::default()
        };

        let step_b = StepConfig {
            command: Some("b".to_string()),
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
            command: Some("test".to_string()),
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
}
