//! Template inputs validation.
//!
//! This rule validates that required template inputs are provided
//! and that input types match their contracts.

use crate::config::BivvyConfig;
use crate::lint::{LintDiagnostic, LintRule, RuleId, Severity};
use crate::registry::Registry;

/// Validates template inputs match contracts.
pub struct TemplateInputsRule {
    registry: Registry,
}

impl TemplateInputsRule {
    /// Create a new template inputs rule with the given registry.
    pub fn new(registry: Registry) -> Self {
        Self { registry }
    }
}

impl LintRule for TemplateInputsRule {
    fn id(&self) -> RuleId {
        RuleId::new("template-inputs")
    }

    fn name(&self) -> &str {
        "Template Inputs"
    }

    fn description(&self) -> &str {
        "Validates template input contracts are satisfied"
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, config: &BivvyConfig) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();

        for (step_name, step_config) in &config.steps {
            if let Some(ref template_name) = step_config.template {
                if let Some(template) = self.registry.get(template_name) {
                    // Check for missing required inputs
                    for (input_name, input_contract) in &template.inputs {
                        if input_contract.required
                            && input_contract.default.is_none()
                            && !step_config.inputs.contains_key(input_name)
                        {
                            diagnostics.push(LintDiagnostic::new(
                                self.id(),
                                self.default_severity(),
                                format!(
                                    "Step '{}' is missing required input '{}' for template '{}'",
                                    step_name, input_name, template_name
                                ),
                            ));
                        }

                        // Validate provided inputs
                        if let Some(value) = step_config.inputs.get(input_name) {
                            if let Err(err) = input_contract.validate(input_name, Some(value)) {
                                diagnostics.push(LintDiagnostic::new(
                                    self.id(),
                                    self.default_severity(),
                                    format!("Step '{}': {}", step_name, err),
                                ));
                            }
                        }
                    }

                    // Check for unknown inputs
                    for input_name in step_config.inputs.keys() {
                        if !template.inputs.contains_key(input_name) {
                            diagnostics.push(LintDiagnostic::new(
                                self.id(),
                                Severity::Warning,
                                format!(
                                    "Step '{}' provides unknown input '{}' for template '{}'",
                                    step_name, input_name, template_name
                                ),
                            ));
                        }
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
    use crate::config::StepConfig;
    use std::collections::HashMap;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_registry_with_template(template_yaml: &str) -> (TempDir, Registry) {
        let temp = TempDir::new().unwrap();
        let templates_dir = temp.path().join(".bivvy").join("templates").join("steps");
        fs::create_dir_all(&templates_dir).unwrap();
        fs::write(templates_dir.join("test-template.yml"), template_yaml).unwrap();

        let registry = Registry::new(Some(temp.path())).unwrap();
        (temp, registry)
    }

    #[test]
    fn detects_missing_required_input() {
        let template_yaml = r#"
name: test-template
description: "Test template"
category: test
inputs:
  required_input:
    description: "A required input"
    type: string
    required: true
step:
  command: "echo ${required_input}"
"#;
        let (_temp, registry) = create_test_registry_with_template(template_yaml);
        let rule = TemplateInputsRule::new(registry);

        let mut steps = HashMap::new();
        steps.insert(
            "test".to_string(),
            StepConfig {
                template: Some("test-template".to_string()),
                ..Default::default()
            },
        );
        let config = BivvyConfig {
            steps,
            ..Default::default()
        };

        let diagnostics = rule.check(&config);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("missing required input"));
    }

    #[test]
    fn passes_with_required_input_provided() {
        let template_yaml = r#"
name: test-template
description: "Test template"
category: test
inputs:
  required_input:
    description: "A required input"
    type: string
    required: true
step:
  command: "echo ${required_input}"
"#;
        let (_temp, registry) = create_test_registry_with_template(template_yaml);
        let rule = TemplateInputsRule::new(registry);

        let mut inputs = HashMap::new();
        inputs.insert(
            "required_input".to_string(),
            serde_yaml::Value::String("provided_value".to_string()),
        );

        let mut steps = HashMap::new();
        steps.insert(
            "test".to_string(),
            StepConfig {
                template: Some("test-template".to_string()),
                inputs,
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
    fn warns_on_unknown_input() {
        let template_yaml = r#"
name: test-template
description: "Test template"
category: test
step:
  command: "echo hello"
"#;
        let (_temp, registry) = create_test_registry_with_template(template_yaml);
        let rule = TemplateInputsRule::new(registry);

        let mut inputs = HashMap::new();
        inputs.insert(
            "unknown_input".to_string(),
            serde_yaml::Value::String("value".to_string()),
        );

        let mut steps = HashMap::new();
        steps.insert(
            "test".to_string(),
            StepConfig {
                template: Some("test-template".to_string()),
                inputs,
                ..Default::default()
            },
        );
        let config = BivvyConfig {
            steps,
            ..Default::default()
        };

        let diagnostics = rule.check(&config);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Severity::Warning);
        assert!(diagnostics[0].message.contains("unknown input"));
    }

    #[test]
    fn detects_wrong_input_type() {
        let template_yaml = r#"
name: test-template
description: "Test template"
category: test
inputs:
  bool_input:
    description: "A boolean input"
    type: boolean
    required: true
step:
  command: "echo ${bool_input}"
"#;
        let (_temp, registry) = create_test_registry_with_template(template_yaml);
        let rule = TemplateInputsRule::new(registry);

        let mut inputs = HashMap::new();
        // Provide a string instead of boolean
        inputs.insert(
            "bool_input".to_string(),
            serde_yaml::Value::String("not a boolean".to_string()),
        );

        let mut steps = HashMap::new();
        steps.insert(
            "test".to_string(),
            StepConfig {
                template: Some("test-template".to_string()),
                inputs,
                ..Default::default()
            },
        );
        let config = BivvyConfig {
            steps,
            ..Default::default()
        };

        let diagnostics = rule.check(&config);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("must be a boolean"));
    }
}
