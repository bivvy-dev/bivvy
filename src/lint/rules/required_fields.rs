//! Required fields validation.
//!
//! This rule ensures that required configuration fields are present.

use crate::config::BivvyConfig;
use crate::lint::{LintDiagnostic, LintRule, RuleId, Severity};

/// Validates that required configuration fields are present.
pub struct RequiredFieldsRule;

impl LintRule for RequiredFieldsRule {
    fn id(&self) -> RuleId {
        RuleId::new("required-fields")
    }

    fn name(&self) -> &str {
        "Required Fields"
    }

    fn description(&self) -> &str {
        "Ensures all required configuration fields are present"
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, config: &BivvyConfig) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();

        if config.app_name.is_none() {
            diagnostics.push(LintDiagnostic::new(
                self.id(),
                self.default_severity(),
                "Missing required field: app_name",
            ));
        }

        if config.workflows.is_empty() {
            diagnostics.push(LintDiagnostic::new(
                self.id(),
                Severity::Warning,
                "No workflows defined",
            ));
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WorkflowConfig;

    #[test]
    fn detects_missing_app_name() {
        let rule = RequiredFieldsRule;
        let config = BivvyConfig::default();

        let diagnostics = rule.check(&config);

        assert!(diagnostics.iter().any(|d| d.message.contains("app_name")));
    }

    #[test]
    fn passes_with_app_name() {
        let rule = RequiredFieldsRule;
        let config = BivvyConfig {
            app_name: Some("test-app".to_string()),
            ..Default::default()
        };

        let diagnostics = rule.check(&config);

        assert!(!diagnostics.iter().any(|d| d.message.contains("app_name")));
    }

    #[test]
    fn warns_when_no_workflows() {
        let rule = RequiredFieldsRule;
        let config = BivvyConfig::default();

        let diagnostics = rule.check(&config);

        assert!(diagnostics
            .iter()
            .any(|d| d.message.contains("No workflows")));
    }

    #[test]
    fn no_workflow_warning_when_workflows_exist() {
        let rule = RequiredFieldsRule;
        let mut workflows = std::collections::HashMap::new();
        workflows.insert(
            "default".to_string(),
            WorkflowConfig {
                steps: vec!["test".to_string()],
                ..Default::default()
            },
        );
        let config = BivvyConfig {
            app_name: Some("test".to_string()),
            workflows,
            ..Default::default()
        };

        let diagnostics = rule.check(&config);

        assert!(!diagnostics
            .iter()
            .any(|d| d.message.contains("No workflows")));
    }
}
