//! Undefined workflow force step detection.
//!
//! This rule detects references to undefined steps in a workflow's
//! `force` list.

use crate::config::BivvyConfig;
use crate::lint::{LintDiagnostic, LintRule, RuleId, Severity};

/// Detects references to undefined steps in workflow `force` lists.
pub struct UndefinedWorkflowForceRule;

impl LintRule for UndefinedWorkflowForceRule {
    fn id(&self) -> RuleId {
        RuleId::new("undefined-workflow-force")
    }

    fn name(&self) -> &str {
        "Undefined Workflow Force Step"
    }

    fn description(&self) -> &str {
        "Ensures all steps named in a workflow's `force` list exist"
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, config: &BivvyConfig) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();

        for (workflow_name, workflow) in &config.workflows {
            for step_name in &workflow.force {
                if !config.steps.contains_key(step_name) {
                    diagnostics.push(LintDiagnostic::new(
                        self.id(),
                        self.default_severity(),
                        format!(
                            "Workflow '{}' force list references undefined step '{}'",
                            workflow_name, step_name
                        ),
                    ));
                }
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ExecutionConfig, StepConfig, WorkflowConfig};
    use std::collections::HashMap;

    #[test]
    fn detects_undefined_step_in_workflow_force() {
        let rule = UndefinedWorkflowForceRule;

        let mut steps = HashMap::new();
        steps.insert(
            "build".to_string(),
            StepConfig {
                execution: ExecutionConfig {
                    command: Some("cargo build".to_string()),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let mut workflows = HashMap::new();
        workflows.insert(
            "default".to_string(),
            WorkflowConfig {
                steps: vec!["build".to_string()],
                force: vec!["nonexistent".to_string()],
                ..Default::default()
            },
        );
        let config = BivvyConfig {
            steps,
            workflows,
            ..Default::default()
        };

        let diagnostics = rule.check(&config);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("nonexistent"));
        assert!(diagnostics[0].message.contains("default"));
    }

    #[test]
    fn passes_when_all_force_entries_exist() {
        let rule = UndefinedWorkflowForceRule;

        let mut steps = HashMap::new();
        steps.insert(
            "build".to_string(),
            StepConfig {
                execution: ExecutionConfig {
                    command: Some("cargo build".to_string()),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        steps.insert(
            "test".to_string(),
            StepConfig {
                execution: ExecutionConfig {
                    command: Some("cargo test".to_string()),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let mut workflows = HashMap::new();
        workflows.insert(
            "default".to_string(),
            WorkflowConfig {
                steps: vec!["build".to_string(), "test".to_string()],
                force: vec!["build".to_string(), "test".to_string()],
                ..Default::default()
            },
        );
        let config = BivvyConfig {
            steps,
            workflows,
            ..Default::default()
        };

        let diagnostics = rule.check(&config);

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn empty_force_list_passes() {
        let rule = UndefinedWorkflowForceRule;

        let mut workflows = HashMap::new();
        workflows.insert(
            "default".to_string(),
            WorkflowConfig {
                force: vec![],
                ..Default::default()
            },
        );
        let config = BivvyConfig {
            workflows,
            ..Default::default()
        };

        let diagnostics = rule.check(&config);

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn reports_each_undefined_entry_separately() {
        let rule = UndefinedWorkflowForceRule;

        let mut workflows = HashMap::new();
        workflows.insert(
            "default".to_string(),
            WorkflowConfig {
                force: vec!["missing_a".to_string(), "missing_b".to_string()],
                ..Default::default()
            },
        );
        let config = BivvyConfig {
            workflows,
            ..Default::default()
        };

        let diagnostics = rule.check(&config);

        assert_eq!(diagnostics.len(), 2);
    }
}
