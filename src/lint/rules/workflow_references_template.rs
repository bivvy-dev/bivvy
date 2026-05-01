//! Detects template paths used in place of step aliases.
//!
//! When a user pastes a template path (e.g. `rust/version-bump`) into a
//! workflow's `steps:` list rather than the alias they registered the
//! step under, the workflow looks valid until run-time. This rule warns
//! whenever a workflow step name contains a `/` — a strong indicator
//! the template path was used instead of the alias.

use crate::config::BivvyConfig;
use crate::lint::{LintDiagnostic, LintRule, RuleId, Severity};

/// Detects workflow step entries that look like template paths.
pub struct WorkflowReferencesTemplateNotStepRule;

impl LintRule for WorkflowReferencesTemplateNotStepRule {
    fn id(&self) -> RuleId {
        RuleId::new("workflow-references-template-not-step")
    }

    fn name(&self) -> &str {
        "Workflow References Template, Not Step"
    }

    fn description(&self) -> &str {
        "Detects workflow steps that look like template paths (e.g. category/name) instead of step aliases"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, config: &BivvyConfig) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();

        for (workflow_name, workflow) in &config.workflows {
            for step_name in &workflow.steps {
                if step_name.contains('/') {
                    diagnostics.push(
                        LintDiagnostic::new(
                            self.id(),
                            self.default_severity(),
                            format!(
                                "Workflow '{}' references '{}', which looks like a template path; \
                                 workflows must reference step aliases defined in `steps:`",
                                workflow_name, step_name
                            ),
                        )
                        .with_suggestion(format!(
                            "bivvy add {} --as <step-name> --workflow {}",
                            step_name, workflow_name
                        )),
                    );
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

    fn config_with_workflow(name: &str, steps: Vec<&str>) -> BivvyConfig {
        let mut workflows = HashMap::new();
        workflows.insert(
            name.to_string(),
            WorkflowConfig {
                steps: steps.into_iter().map(String::from).collect(),
                ..Default::default()
            },
        );
        BivvyConfig {
            workflows,
            ..Default::default()
        }
    }

    #[test]
    fn flags_template_path_in_workflow() {
        let rule = WorkflowReferencesTemplateNotStepRule;
        let config = config_with_workflow("release", vec!["rust/version-bump", "publish"]);

        let diags = rule.check(&config);
        assert_eq!(diags.len(), 1);
        assert_eq!(
            diags[0].rule_id,
            RuleId::new("workflow-references-template-not-step")
        );
        assert_eq!(diags[0].severity, Severity::Warning);
        assert!(diags[0].message.contains("rust/version-bump"));
        assert!(diags[0].message.contains("release"));
    }

    #[test]
    fn suggestion_uses_bivvy_add_template() {
        let rule = WorkflowReferencesTemplateNotStepRule;
        let config = config_with_workflow("release", vec!["rust/version-bump"]);

        let diags = rule.check(&config);
        let suggestion = diags[0].suggestion.as_ref().unwrap();
        assert!(suggestion.contains("bivvy add rust/version-bump"));
        assert!(suggestion.contains("--as"));
        assert!(suggestion.contains("--workflow release"));
    }

    #[test]
    fn ignores_valid_step_names() {
        let rule = WorkflowReferencesTemplateNotStepRule;
        let mut steps = HashMap::new();
        steps.insert(
            "version-bump".to_string(),
            StepConfig {
                execution: ExecutionConfig {
                    command: Some("cargo bump".to_string()),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let mut config = config_with_workflow("release", vec!["version-bump", "publish"]);
        config.steps = steps;

        let diags = rule.check(&config);
        assert!(diags.is_empty());
    }

    #[test]
    fn flags_each_template_path_separately() {
        let rule = WorkflowReferencesTemplateNotStepRule;
        let config = config_with_workflow("ci", vec!["node/install", "rust/test", "fmt"]);

        let diags = rule.check(&config);
        assert_eq!(diags.len(), 2);
        let messages: Vec<&str> = diags.iter().map(|d| d.message.as_str()).collect();
        assert!(messages.iter().any(|m| m.contains("node/install")));
        assert!(messages.iter().any(|m| m.contains("rust/test")));
    }
}
