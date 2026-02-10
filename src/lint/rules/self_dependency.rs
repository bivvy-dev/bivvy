//! Self-dependency detection.
//!
//! This rule detects steps that depend on themselves.

use crate::config::BivvyConfig;
use crate::lint::{LintDiagnostic, LintRule, RuleId, Severity};

/// Detects steps that depend on themselves.
pub struct SelfDependencyRule;

impl LintRule for SelfDependencyRule {
    fn id(&self) -> RuleId {
        RuleId::new("self-dependency")
    }

    fn name(&self) -> &str {
        "Self Dependency"
    }

    fn description(&self) -> &str {
        "Detects steps that depend on themselves"
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, config: &BivvyConfig) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();

        for (step_name, step_config) in &config.steps {
            if step_config.depends_on.contains(step_name) {
                diagnostics.push(LintDiagnostic::new(
                    self.id(),
                    self.default_severity(),
                    format!("Step '{}' depends on itself", step_name),
                ));
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StepConfig;
    use std::collections::HashMap;

    #[test]
    fn detects_self_dependency() {
        let rule = SelfDependencyRule;

        let mut steps = HashMap::new();
        steps.insert(
            "a".to_string(),
            StepConfig {
                command: Some("echo a".to_string()),
                depends_on: vec!["a".to_string()],
                ..Default::default()
            },
        );
        let config = BivvyConfig {
            steps,
            ..Default::default()
        };

        let diagnostics = rule.check(&config);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("depends on itself"));
    }

    #[test]
    fn passes_without_self_dependency() {
        let rule = SelfDependencyRule;

        let mut steps = HashMap::new();
        steps.insert(
            "a".to_string(),
            StepConfig {
                command: Some("echo a".to_string()),
                depends_on: vec!["b".to_string()],
                ..Default::default()
            },
        );
        steps.insert(
            "b".to_string(),
            StepConfig {
                command: Some("echo b".to_string()),
                ..Default::default()
            },
        );
        let config = BivvyConfig {
            steps,
            ..Default::default()
        };

        let diagnostics = rule.check(&config);

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn detects_self_dependency_with_other_deps() {
        let rule = SelfDependencyRule;

        let mut steps = HashMap::new();
        steps.insert(
            "a".to_string(),
            StepConfig {
                command: Some("echo a".to_string()),
                depends_on: vec!["b".to_string(), "a".to_string()],
                ..Default::default()
            },
        );
        steps.insert(
            "b".to_string(),
            StepConfig {
                command: Some("echo b".to_string()),
                ..Default::default()
            },
        );
        let config = BivvyConfig {
            steps,
            ..Default::default()
        };

        let diagnostics = rule.check(&config);

        assert_eq!(diagnostics.len(), 1);
    }
}
