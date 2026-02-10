//! Undefined template validation.
//!
//! This rule validates that template references in steps resolve to actual templates.

use crate::config::BivvyConfig;
use crate::lint::{LintDiagnostic, LintRule, RuleId, Severity};
use crate::registry::Registry;

/// Validates that template references resolve.
pub struct UndefinedTemplateRule {
    registry: Registry,
}

impl UndefinedTemplateRule {
    /// Create a new undefined template rule with the given registry.
    pub fn new(registry: Registry) -> Self {
        Self { registry }
    }
}

impl LintRule for UndefinedTemplateRule {
    fn id(&self) -> RuleId {
        RuleId::new("undefined-template")
    }

    fn name(&self) -> &str {
        "Undefined Template"
    }

    fn description(&self) -> &str {
        "Ensures all template references exist in the registry"
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, config: &BivvyConfig) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();

        for (step_name, step_config) in &config.steps {
            if let Some(ref template_name) = step_config.template {
                if !self.registry.has(template_name) {
                    diagnostics.push(LintDiagnostic::new(
                        self.id(),
                        self.default_severity(),
                        format!(
                            "Step '{}' references undefined template '{}'",
                            step_name, template_name
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
    use crate::config::StepConfig;
    use std::collections::HashMap;

    #[test]
    fn detects_undefined_template() {
        let registry = Registry::new(None).unwrap();
        let rule = UndefinedTemplateRule::new(registry);

        let mut steps = HashMap::new();
        steps.insert(
            "test".to_string(),
            StepConfig {
                template: Some("nonexistent-template-xyz".to_string()),
                ..Default::default()
            },
        );
        let config = BivvyConfig {
            steps,
            ..Default::default()
        };

        let diagnostics = rule.check(&config);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("nonexistent-template-xyz"));
    }

    #[test]
    fn passes_with_builtin_template() {
        let registry = Registry::new(None).unwrap();
        let rule = UndefinedTemplateRule::new(registry);

        let mut steps = HashMap::new();
        steps.insert(
            "test".to_string(),
            StepConfig {
                template: Some("brew".to_string()), // This is a built-in template
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
    fn passes_with_no_template() {
        let registry = Registry::new(None).unwrap();
        let rule = UndefinedTemplateRule::new(registry);

        let mut steps = HashMap::new();
        steps.insert(
            "test".to_string(),
            StepConfig {
                command: Some("echo hello".to_string()),
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
