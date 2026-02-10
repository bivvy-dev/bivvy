//! Undefined dependency detection.
//!
//! This rule detects references to undefined steps in depends_on.

use crate::config::BivvyConfig;
use crate::lint::{LintDiagnostic, LintRule, RuleId, Severity};

/// Detects references to undefined steps in depends_on.
pub struct UndefinedDependencyRule;

impl LintRule for UndefinedDependencyRule {
    fn id(&self) -> RuleId {
        RuleId::new("undefined-dependency")
    }

    fn name(&self) -> &str {
        "Undefined Dependency"
    }

    fn description(&self) -> &str {
        "Ensures all depends_on references exist"
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, config: &BivvyConfig) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();

        for (step_name, step_config) in &config.steps {
            for dep in &step_config.depends_on {
                if !config.steps.contains_key(dep) {
                    diagnostics.push(LintDiagnostic::new(
                        self.id(),
                        self.default_severity(),
                        format!("Step '{}' depends on undefined step '{}'", step_name, dep),
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
    use crate::config::StepConfig;
    use std::collections::HashMap;

    #[test]
    fn detects_undefined_dependency() {
        let rule = UndefinedDependencyRule;

        let mut steps = HashMap::new();
        steps.insert(
            "a".to_string(),
            StepConfig {
                command: Some("echo a".to_string()),
                depends_on: vec!["nonexistent".to_string()],
                ..Default::default()
            },
        );
        let config = BivvyConfig {
            steps,
            ..Default::default()
        };

        let diagnostics = rule.check(&config);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("nonexistent"));
    }

    #[test]
    fn passes_with_valid_dependencies() {
        let rule = UndefinedDependencyRule;

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
    fn detects_multiple_undefined_dependencies() {
        let rule = UndefinedDependencyRule;

        let mut steps = HashMap::new();
        steps.insert(
            "a".to_string(),
            StepConfig {
                command: Some("echo a".to_string()),
                depends_on: vec!["missing1".to_string(), "missing2".to_string()],
                ..Default::default()
            },
        );
        let config = BivvyConfig {
            steps,
            ..Default::default()
        };

        let diagnostics = rule.check(&config);

        assert_eq!(diagnostics.len(), 2);
    }

    #[test]
    fn no_false_positives_with_no_dependencies() {
        let rule = UndefinedDependencyRule;

        let mut steps = HashMap::new();
        steps.insert(
            "a".to_string(),
            StepConfig {
                command: Some("echo a".to_string()),
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
}
