//! Validates that all `rerun_window` (and `default_rerun_window`)
//! strings parse as valid `RerunWindow` values.
//!
//! At runtime, an unparseable value silently falls back to the default,
//! which masks typos like `"4hh"`. This rule catches them at lint time.

use std::str::FromStr;

use crate::config::BivvyConfig;
use crate::lint::{LintDiagnostic, LintRule, RuleId, Severity};
use crate::runner::RerunWindow;

/// Rule that flags any `rerun_window` value that fails to parse.
pub struct RerunWindowFormatRule;

impl LintRule for RerunWindowFormatRule {
    fn id(&self) -> RuleId {
        RuleId::new("rerun-window-format")
    }

    fn name(&self) -> &str {
        "Rerun Window Format"
    }

    fn description(&self) -> &str {
        "Validates that rerun_window values parse as a duration"
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, config: &BivvyConfig) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();

        // settings.execution.default_rerun_window
        if let Some(value) = config.settings.execution.default_rerun_window.as_deref() {
            if let Err(err) = RerunWindow::from_str(value) {
                diagnostics.push(LintDiagnostic::new(
                    self.id(),
                    self.default_severity(),
                    format!(
                        "settings.default_rerun_window value '{}' is invalid: {}",
                        value, err
                    ),
                ));
            }
        }

        // settings.defaults.rerun_window
        if let Some(value) = config.settings.defaults.rerun_window.as_deref() {
            if let Err(err) = RerunWindow::from_str(value) {
                diagnostics.push(LintDiagnostic::new(
                    self.id(),
                    self.default_severity(),
                    format!(
                        "settings.defaults.rerun_window value '{}' is invalid: {}",
                        value, err
                    ),
                ));
            }
        }

        // Each step's behavior.rerun_window and any environment override.
        for (step_name, step) in &config.steps {
            if let Some(value) = step.behavior.rerun_window.as_deref() {
                if let Err(err) = RerunWindow::from_str(value) {
                    diagnostics.push(LintDiagnostic::new(
                        self.id(),
                        self.default_severity(),
                        format!(
                            "Step '{}' rerun_window value '{}' is invalid: {}",
                            step_name, value, err
                        ),
                    ));
                }
            }

            for (env_name, override_) in &step.scoping.environments {
                if let Some(value) = override_.rerun_window.as_deref() {
                    if let Err(err) = RerunWindow::from_str(value) {
                        diagnostics.push(LintDiagnostic::new(
                            self.id(),
                            self.default_severity(),
                            format!(
                                "Step '{}' environment '{}' rerun_window value '{}' is invalid: {}",
                                step_name, env_name, value, err
                            ),
                        ));
                    }
                }
            }
        }

        // Workflow-level step overrides also carry their own rerun_window
        // (consumed at runtime via `StepOverride.rerun_window`). Validate them
        // so a typo inside `workflows.<wf>.overrides.<step>.rerun_window`
        // surfaces here instead of falling through to the runtime warning.
        for (workflow_name, workflow) in &config.workflows {
            for (step_name, override_) in &workflow.overrides {
                if let Some(value) = override_.rerun_window.as_deref() {
                    if let Err(err) = RerunWindow::from_str(value) {
                        diagnostics.push(LintDiagnostic::new(
                            self.id(),
                            self.default_severity(),
                            format!(
                                "Workflow '{}' override for step '{}' rerun_window value '{}' is invalid: {}",
                                workflow_name, step_name, value, err
                            ),
                        ));
                    }
                }
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::StepEnvironmentOverride;
    use crate::config::StepConfig;

    fn config_with_step(name: &str, step: StepConfig) -> BivvyConfig {
        let mut steps = std::collections::HashMap::new();
        steps.insert(name.to_string(), step);
        BivvyConfig {
            steps,
            ..Default::default()
        }
    }

    #[test]
    fn valid_default_rerun_window_produces_no_diagnostic() {
        let mut config = BivvyConfig::default();
        config.settings.execution.default_rerun_window = Some("4h".to_string());
        let rule = RerunWindowFormatRule;
        assert!(rule.check(&config).is_empty());
    }

    #[test]
    fn invalid_default_rerun_window_is_reported() {
        let mut config = BivvyConfig::default();
        config.settings.execution.default_rerun_window = Some("4hh".to_string());
        let rule = RerunWindowFormatRule;
        let diags = rule.check(&config);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, Severity::Error);
        assert!(diags[0].message.contains("default_rerun_window"));
        assert!(diags[0].message.contains("4hh"));
    }

    #[test]
    fn invalid_defaults_rerun_window_is_reported() {
        let mut config = BivvyConfig::default();
        config.settings.defaults.rerun_window = Some("nope".to_string());
        let rule = RerunWindowFormatRule;
        let diags = rule.check(&config);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("defaults.rerun_window"));
    }

    #[test]
    fn invalid_step_rerun_window_is_reported() {
        let mut step = StepConfig::default();
        step.behavior.rerun_window = Some("4hh".to_string());
        let config = config_with_step("install", step);
        let rule = RerunWindowFormatRule;
        let diags = rule.check(&config);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("install"));
        assert!(diags[0].message.contains("4hh"));
    }

    #[test]
    fn invalid_environment_override_rerun_window_is_reported() {
        let mut step = StepConfig::default();
        step.scoping.environments.insert(
            "ci".to_string(),
            StepEnvironmentOverride {
                rerun_window: Some("definitely-not-a-duration".to_string()),
                ..Default::default()
            },
        );
        let config = config_with_step("install", step);
        let rule = RerunWindowFormatRule;
        let diags = rule.check(&config);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("ci"));
    }

    #[test]
    fn valid_step_rerun_window_produces_no_diagnostic() {
        let mut step = StepConfig::default();
        step.behavior.rerun_window = Some("never".to_string());
        let config = config_with_step("install", step);
        let rule = RerunWindowFormatRule;
        assert!(rule.check(&config).is_empty());
    }

    #[test]
    fn invalid_workflow_override_rerun_window_is_reported() {
        use crate::config::schema::{StepOverride, WorkflowConfig};
        use std::collections::HashMap;

        let mut overrides = HashMap::new();
        overrides.insert(
            "install".to_string(),
            StepOverride {
                rerun_window: Some("4hh".to_string()),
                ..Default::default()
            },
        );
        let mut workflows = HashMap::new();
        workflows.insert(
            "ci".to_string(),
            WorkflowConfig {
                overrides,
                ..Default::default()
            },
        );

        let config = BivvyConfig {
            workflows,
            ..Default::default()
        };
        let rule = RerunWindowFormatRule;
        let diags = rule.check(&config);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, Severity::Error);
        assert!(diags[0].message.contains("ci"));
        assert!(diags[0].message.contains("install"));
        assert!(diags[0].message.contains("4hh"));
    }
}
