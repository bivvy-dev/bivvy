//! Validates that check-related fields on steps are not conflicting.
//!
//! Steps may use `completed_check` (legacy), `check` (new, singular),
//! or `checks` (new, multiple) — but not more than one of these at a time.

use crate::config::BivvyConfig;
use crate::lint::{LintDiagnostic, LintRule, RuleId, Severity};

/// Ensures that `completed_check`, `check`, and `checks` are mutually exclusive
/// on each step.
pub struct CheckFieldsMutualExclusivityRule;

impl LintRule for CheckFieldsMutualExclusivityRule {
    fn id(&self) -> RuleId {
        RuleId::new("check-fields-exclusive")
    }

    fn name(&self) -> &str {
        "Check Fields Mutual Exclusivity"
    }

    fn description(&self) -> &str {
        "Ensures steps do not mix completed_check, check, and checks fields"
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, config: &BivvyConfig) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();

        for (name, step) in &config.steps {
            let has_completed = step.execution.completed_check.is_some();
            let has_check = step.execution.check.is_some();
            let has_checks = !step.execution.checks.is_empty();

            let count = [has_completed, has_check, has_checks]
                .iter()
                .filter(|&&v| v)
                .count();

            if count > 1 {
                let mut fields = Vec::new();
                if has_completed {
                    fields.push("completed_check");
                }
                if has_check {
                    fields.push("check");
                }
                if has_checks {
                    fields.push("checks");
                }
                diagnostics.push(LintDiagnostic::new(
                    self.id(),
                    self.default_severity(),
                    format!(
                        "Step '{}' has multiple check fields ({}). Use only one.",
                        name,
                        fields.join(", ")
                    ),
                ));
            }

            // Check for precondition field conflicts
            let has_legacy_precondition = step.execution.precondition.is_some();
            let has_new_precondition = step.execution.new_precondition.is_some();
            if has_legacy_precondition && has_new_precondition {
                diagnostics.push(LintDiagnostic::new(
                    self.id(),
                    self.default_severity(),
                    format!(
                        "Step '{}' has both 'precondition' (legacy) and 'new_precondition'. Use only one.",
                        name,
                    ),
                ));
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::Check;
    use crate::config::{CompletedCheck, StepConfig};
    use std::collections::HashMap;

    fn config_with_step(step: StepConfig) -> BivvyConfig {
        let mut steps = HashMap::new();
        steps.insert("test_step".to_string(), step);
        BivvyConfig {
            steps,
            ..Default::default()
        }
    }

    #[test]
    fn no_check_fields_produces_no_diagnostic() {
        let config = config_with_step(StepConfig::default());
        let rule = CheckFieldsMutualExclusivityRule;
        let diagnostics = rule.check(&config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn single_completed_check_is_fine() {
        let mut step = StepConfig::default();
        step.execution.completed_check = Some(CompletedCheck::CommandSucceeds {
            command: "true".to_string(),
        });
        let config = config_with_step(step);
        let rule = CheckFieldsMutualExclusivityRule;
        assert!(rule.check(&config).is_empty());
    }

    #[test]
    fn single_new_check_is_fine() {
        let mut step = StepConfig::default();
        step.execution.check = Some(Check::Presence {
            name: None,
            target: Some("node_modules".to_string()),
            kind: None,
            command: None,
        });
        let config = config_with_step(step);
        let rule = CheckFieldsMutualExclusivityRule;
        assert!(rule.check(&config).is_empty());
    }

    #[test]
    fn completed_check_and_new_check_conflicts() {
        let mut step = StepConfig::default();
        step.execution.completed_check = Some(CompletedCheck::CommandSucceeds {
            command: "true".to_string(),
        });
        step.execution.check = Some(Check::Presence {
            name: None,
            target: Some("node_modules".to_string()),
            kind: None,
            command: None,
        });
        let config = config_with_step(step);
        let rule = CheckFieldsMutualExclusivityRule;
        let diagnostics = rule.check(&config);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("completed_check"));
        assert!(diagnostics[0].message.contains("check"));
    }

    #[test]
    fn check_and_checks_conflicts() {
        let mut step = StepConfig::default();
        step.execution.check = Some(Check::Presence {
            name: None,
            target: Some("node_modules".to_string()),
            kind: None,
            command: None,
        });
        step.execution.checks = vec![Check::Execution {
            name: None,
            command: "true".to_string(),
            validation: Default::default(),
        }];
        let config = config_with_step(step);
        let rule = CheckFieldsMutualExclusivityRule;
        let diagnostics = rule.check(&config);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("check, checks"));
    }
}
