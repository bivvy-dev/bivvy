//! Detects steps that are defined but never referenced by any workflow.
//!
//! A step is "used" when it appears in some workflow's `steps:` list, or
//! when it is reached transitively through `depends_on` from a step that
//! is used. Steps that are not reachable from any workflow are likely
//! dead code.
//!
//! When no workflows are defined the rule is vacuous and emits nothing.

use std::collections::HashSet;

use crate::config::BivvyConfig;
use crate::lint::{LintDiagnostic, LintRule, RuleId, Severity};

/// Detects steps that are defined but never referenced.
pub struct UnusedStepRule;

impl LintRule for UnusedStepRule {
    fn id(&self) -> RuleId {
        RuleId::new("unused-step")
    }

    fn name(&self) -> &str {
        "Unused Step"
    }

    fn description(&self) -> &str {
        "Detects steps that are defined but never referenced by a workflow"
    }

    fn default_severity(&self) -> Severity {
        Severity::Hint
    }

    fn check(&self, config: &BivvyConfig) -> Vec<LintDiagnostic> {
        if config.workflows.is_empty() {
            return Vec::new();
        }

        // Walk all workflows and collect everything reachable through
        // depends_on.
        let mut reachable: HashSet<String> = HashSet::new();
        for workflow in config.workflows.values() {
            for step_name in &workflow.steps {
                walk(config, step_name, &mut reachable);
            }
            // Also count steps used in workflow.force / workflow.overrides.
            for step_name in &workflow.force {
                walk(config, step_name, &mut reachable);
            }
            for step_name in workflow.overrides.keys() {
                walk(config, step_name, &mut reachable);
            }
        }

        let mut diagnostics: Vec<LintDiagnostic> = config
            .steps
            .keys()
            .filter(|name| !reachable.contains(*name))
            .map(|name| {
                LintDiagnostic::new(
                    self.id(),
                    self.default_severity(),
                    format!("Step '{}' is defined but never used by any workflow", name),
                )
                .with_suggestion(format!(
                    "Add '{}' to a workflow's `steps:` list, or remove the step",
                    name
                ))
            })
            .collect();

        // Stable order for deterministic output.
        diagnostics.sort_by(|a, b| a.message.cmp(&b.message));
        diagnostics
    }
}

fn walk(config: &BivvyConfig, name: &str, seen: &mut HashSet<String>) {
    if !seen.insert(name.to_string()) {
        return;
    }
    if let Some(step) = config.steps.get(name) {
        for dep in &step.depends_on {
            walk(config, dep, seen);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ExecutionConfig, StepConfig, WorkflowConfig};
    use std::collections::HashMap;

    fn step(command: &str, deps: &[&str]) -> StepConfig {
        StepConfig {
            execution: ExecutionConfig {
                command: Some(command.to_string()),
                ..Default::default()
            },
            depends_on: deps.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn flags_step_not_in_any_workflow() {
        let rule = UnusedStepRule;
        let mut steps = HashMap::new();
        steps.insert("used".to_string(), step("cargo build", &[]));
        steps.insert("orphan".to_string(), step("cargo install", &[]));

        let mut workflows = HashMap::new();
        workflows.insert(
            "default".to_string(),
            WorkflowConfig {
                steps: vec!["used".to_string()],
                ..Default::default()
            },
        );

        let config = BivvyConfig {
            steps,
            workflows,
            ..Default::default()
        };

        let diags = rule.check(&config);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, RuleId::new("unused-step"));
        assert_eq!(diags[0].severity, Severity::Hint);
        assert!(diags[0].message.contains("orphan"));
    }

    #[test]
    fn ignores_steps_reached_through_depends_on() {
        let rule = UnusedStepRule;
        let mut steps = HashMap::new();
        steps.insert("a".to_string(), step("cargo build", &["b"]));
        steps.insert("b".to_string(), step("cargo test", &["c"]));
        steps.insert("c".to_string(), step("cargo fmt", &[]));

        let mut workflows = HashMap::new();
        workflows.insert(
            "default".to_string(),
            WorkflowConfig {
                steps: vec!["a".to_string()],
                ..Default::default()
            },
        );

        let config = BivvyConfig {
            steps,
            workflows,
            ..Default::default()
        };

        let diags = rule.check(&config);
        assert!(diags.is_empty());
    }

    #[test]
    fn skips_when_no_workflows_defined() {
        let rule = UnusedStepRule;
        let mut steps = HashMap::new();
        steps.insert("orphan".to_string(), step("cargo build", &[]));

        let config = BivvyConfig {
            steps,
            ..Default::default()
        };

        let diags = rule.check(&config);
        assert!(diags.is_empty());
    }

    #[test]
    fn suggestion_includes_step_name() {
        let rule = UnusedStepRule;
        let mut steps = HashMap::new();
        steps.insert("orphan".to_string(), step("cargo build", &[]));
        let mut workflows = HashMap::new();
        workflows.insert("default".to_string(), WorkflowConfig::default());

        let config = BivvyConfig {
            steps,
            workflows,
            ..Default::default()
        };

        let diags = rule.check(&config);
        let suggestion = diags[0].suggestion.as_ref().unwrap();
        assert!(suggestion.contains("orphan"));
        assert!(suggestion.contains("workflow"));
    }

    #[test]
    fn step_referenced_only_through_workflow_force_is_not_flagged() {
        let rule = UnusedStepRule;
        let mut steps = HashMap::new();
        steps.insert("primary".to_string(), step("cargo build", &[]));
        steps.insert("forced".to_string(), step("cargo clean", &[]));

        let mut workflows = HashMap::new();
        workflows.insert(
            "default".to_string(),
            WorkflowConfig {
                steps: vec!["primary".to_string()],
                force: vec!["forced".to_string()],
                ..Default::default()
            },
        );

        let config = BivvyConfig {
            steps,
            workflows,
            ..Default::default()
        };

        let diags = rule.check(&config);
        assert!(diags.is_empty(), "got {:?}", diags);
    }
}
