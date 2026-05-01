//! Detects secrets referenced from configs that lack a resolution path.
//!
//! Bivvy resolves a secret by running its `command:` handler and capturing
//! stdout. A `SecretConfig` without a non-empty `command:` (e.g. an empty
//! string slipped past the type system, or after merge stripping) cannot
//! produce a value at runtime.
//!
//! This rule fires when a step references a secret (`${secrets.<name>}`)
//! that exists in the secrets map but whose `command:` is blank.

use std::collections::HashSet;

use crate::config::interpolation::extract_variables;
use crate::config::BivvyConfig;
use crate::lint::{LintDiagnostic, LintRule, RuleId, Severity};

/// Detects secrets referenced without a runtime resolution path.
pub struct SecretWithoutHandlerRule;

impl LintRule for SecretWithoutHandlerRule {
    fn id(&self) -> RuleId {
        RuleId::new("secret-without-handler")
    }

    fn name(&self) -> &str {
        "Secret Without Handler"
    }

    fn description(&self) -> &str {
        "Detects secrets referenced by a step but whose command handler is empty"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, config: &BivvyConfig) -> Vec<LintDiagnostic> {
        // Find every secret name referenced via `${secrets.<name>}` in any
        // string field on the config.
        let referenced = referenced_secret_names(config);
        if referenced.is_empty() {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for name in &referenced {
            if let Some(secret) = config.secrets.get(name) {
                if secret.command.trim().is_empty() {
                    diagnostics.push(
                        LintDiagnostic::new(
                            self.id(),
                            self.default_severity(),
                            format!(
                                "Secret '{}' is referenced but its command handler is empty — \
                                 nothing will resolve `${{secrets.{}}}` at runtime",
                                name, name
                            ),
                        )
                        .with_suggestion(format!(
                            "Add a command for '{}' under `secrets:`, e.g. `command: op read \"op://Vault/{}\"`",
                            name, name
                        )),
                    );
                }
            }
        }

        diagnostics.sort_by(|a, b| a.message.cmp(&b.message));
        diagnostics
    }
}

/// Walk the merged config and collect every `secrets.<name>` reference.
fn referenced_secret_names(config: &BivvyConfig) -> HashSet<String> {
    let mut found = HashSet::new();

    let mut collect = |s: &str| {
        for var in extract_variables(s) {
            if let Some((ns, key)) = var.split_once('.') {
                if ns == "secrets" {
                    found.insert(key.to_string());
                }
            }
        }
    };

    for step in config.steps.values() {
        if let Some(ref cmd) = step.execution.command {
            collect(cmd);
        }
        for val in step.env_vars.env.values() {
            collect(val);
        }
        for hook in step.hooks.before.iter().chain(step.hooks.after.iter()) {
            collect(hook);
        }
    }
    for val in config.settings.env_vars.env.values() {
        collect(val);
    }
    for workflow in config.workflows.values() {
        for val in workflow.env.values() {
            collect(val);
        }
    }

    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::{ExecutionConfig, SecretConfig, StepConfig};
    use std::collections::HashMap;

    fn config_referencing_secret(secret_name: &str, command: &str) -> BivvyConfig {
        let mut steps = HashMap::new();
        steps.insert(
            "fetch".to_string(),
            StepConfig {
                execution: ExecutionConfig {
                    command: Some(format!("curl -H \"Auth: ${{secrets.{}}}\"", secret_name)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let mut secrets = HashMap::new();
        secrets.insert(
            secret_name.to_string(),
            SecretConfig {
                command: command.to_string(),
            },
        );
        BivvyConfig {
            steps,
            secrets,
            ..Default::default()
        }
    }

    #[test]
    fn flags_secret_with_empty_handler() {
        let rule = SecretWithoutHandlerRule;
        let config = config_referencing_secret("api_key", "");

        let diags = rule.check(&config);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, RuleId::new("secret-without-handler"));
        assert_eq!(diags[0].severity, Severity::Warning);
        assert!(diags[0].message.contains("api_key"));
        assert!(diags[0].message.contains("empty"));
        assert!(diags[0].suggestion.is_some());
    }

    #[test]
    fn flags_whitespace_only_handler() {
        let rule = SecretWithoutHandlerRule;
        let config = config_referencing_secret("token", "   \t  ");

        let diags = rule.check(&config);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("token"));
    }

    #[test]
    fn ignores_secret_with_command() {
        let rule = SecretWithoutHandlerRule;
        let config = config_referencing_secret("api_key", "op read api_key");

        let diags = rule.check(&config);
        assert!(diags.is_empty(), "got {:?}", diags);
    }

    #[test]
    fn ignores_unreferenced_secret() {
        let rule = SecretWithoutHandlerRule;
        let mut config = BivvyConfig::default();
        config.secrets.insert(
            "leftover".to_string(),
            SecretConfig {
                command: String::new(),
            },
        );

        let diags = rule.check(&config);
        // Not referenced anywhere, so nothing to warn about — this rule
        // only catches the case where a step depends on a runtime-empty
        // secret.
        assert!(diags.is_empty());
    }

    #[test]
    fn suggestion_proposes_command_field() {
        let rule = SecretWithoutHandlerRule;
        let config = config_referencing_secret("api_key", "");

        let diags = rule.check(&config);
        let suggestion = diags[0].suggestion.as_ref().unwrap();
        assert!(suggestion.contains("api_key"));
        assert!(suggestion.contains("command"));
    }
}
