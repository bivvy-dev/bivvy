//! Detects environments defined in settings but never referenced.
//!
//! An environment is "live" when it appears in any of:
//! - a workflow's `env:` field key (no — that's just env vars, skip)
//! - a step's `only_environments:` list
//! - a step's `environments:` override map
//! - any environment's `default_workflow:` is irrelevant — that
//!   doesn't make ANOTHER environment live
//! - the global `default_environment:` setting
//!
//! Built-in environments (`ci`, `docker`, `codespace`, `development`)
//! are always considered live regardless of references because Bivvy's
//! detection logic uses them as fallbacks.

use std::collections::HashSet;

use crate::config::BivvyConfig;
use crate::lint::{LintDiagnostic, LintRule, RuleId, Severity};

/// Built-in environment names that are always considered live.
///
/// Mirrors `BUILTIN_ENVIRONMENTS` in `valid_environments.rs`.
const BUILTIN_ENVIRONMENTS: &[&str] = &["ci", "docker", "codespace", "development"];

/// Detects custom environments that are defined but never referenced.
pub struct DeadEnvironmentRule;

impl LintRule for DeadEnvironmentRule {
    fn id(&self) -> RuleId {
        RuleId::new("dead-environment")
    }

    fn name(&self) -> &str {
        "Dead Environment"
    }

    fn description(&self) -> &str {
        "Detects environments defined in settings but never referenced anywhere"
    }

    fn default_severity(&self) -> Severity {
        Severity::Hint
    }

    fn check(&self, config: &BivvyConfig) -> Vec<LintDiagnostic> {
        let defined: HashSet<String> = config
            .settings
            .environment_profiles
            .environments
            .keys()
            .cloned()
            .collect();
        if defined.is_empty() {
            return Vec::new();
        }

        // Collect every name referenced anywhere.
        let mut referenced: HashSet<String> = HashSet::new();

        if let Some(ref default) = config.settings.environment_profiles.default_environment {
            referenced.insert(default.clone());
        }

        for step in config.steps.values() {
            for env_name in &step.scoping.only_environments {
                referenced.insert(env_name.clone());
            }
            for env_name in step.scoping.environments.keys() {
                referenced.insert(env_name.clone());
            }
        }

        // Built-in names are always considered live so we don't flag
        // a custom override that shadows a built-in (handled by a
        // separate rule).
        for builtin in BUILTIN_ENVIRONMENTS {
            referenced.insert((*builtin).to_string());
        }

        let mut diagnostics: Vec<LintDiagnostic> = defined
            .into_iter()
            .filter(|name| !referenced.contains(name))
            .map(|name| {
                LintDiagnostic::new(
                    self.id(),
                    self.default_severity(),
                    format!(
                        "Environment '{}' is defined in settings.environments but never referenced",
                        name
                    ),
                )
                .with_suggestion(format!(
                    "Reference '{}' from settings.default_environment, a step's only_environments, \
                     or a step's environments override — or remove it",
                    name
                ))
            })
            .collect();

        diagnostics.sort_by(|a, b| a.message.cmp(&b.message));
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::{
        EnvironmentConfig, EnvironmentScopingConfig, ExecutionConfig, StepConfig,
        StepEnvironmentOverride,
    };
    use std::collections::HashMap;

    fn config_with_envs(names: &[&str]) -> BivvyConfig {
        let mut envs = HashMap::new();
        for name in names {
            envs.insert((*name).to_string(), EnvironmentConfig::default());
        }
        let mut config = BivvyConfig::default();
        config.settings.environment_profiles.environments = envs;
        config
    }

    #[test]
    fn flags_unreferenced_custom_environment() {
        let rule = DeadEnvironmentRule;
        let config = config_with_envs(&["staging"]);

        let diags = rule.check(&config);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, RuleId::new("dead-environment"));
        assert_eq!(diags[0].severity, Severity::Hint);
        assert!(diags[0].message.contains("staging"));
    }

    #[test]
    fn ignores_envs_referenced_by_default_environment_setting() {
        let rule = DeadEnvironmentRule;
        let mut config = config_with_envs(&["staging"]);
        config.settings.environment_profiles.default_environment = Some("staging".to_string());

        let diags = rule.check(&config);
        assert!(diags.is_empty());
    }

    #[test]
    fn ignores_envs_referenced_by_only_environments() {
        let rule = DeadEnvironmentRule;
        let mut config = config_with_envs(&["staging"]);
        let mut steps = HashMap::new();
        steps.insert(
            "deploy".to_string(),
            StepConfig {
                execution: ExecutionConfig {
                    command: Some("kubectl apply".to_string()),
                    ..Default::default()
                },
                scoping: EnvironmentScopingConfig {
                    only_environments: vec!["staging".to_string()],
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        config.steps = steps;

        let diags = rule.check(&config);
        assert!(diags.is_empty());
    }

    #[test]
    fn ignores_envs_referenced_by_step_overrides() {
        let rule = DeadEnvironmentRule;
        let mut config = config_with_envs(&["staging"]);
        let mut step_envs = HashMap::new();
        step_envs.insert(
            "staging".to_string(),
            StepEnvironmentOverride {
                command: Some("kubectl apply -n staging".to_string()),
                ..Default::default()
            },
        );
        let mut steps = HashMap::new();
        steps.insert(
            "deploy".to_string(),
            StepConfig {
                execution: ExecutionConfig {
                    command: Some("kubectl apply".to_string()),
                    ..Default::default()
                },
                scoping: EnvironmentScopingConfig {
                    environments: step_envs,
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        config.steps = steps;

        let diags = rule.check(&config);
        assert!(diags.is_empty());
    }

    #[test]
    fn skips_when_no_environments_defined() {
        let rule = DeadEnvironmentRule;
        let config = BivvyConfig::default();
        let diags = rule.check(&config);
        assert!(diags.is_empty());
    }

    #[test]
    fn does_not_flag_builtin_named_environment() {
        let rule = DeadEnvironmentRule;
        // A custom environment named after a builtin is already handled by
        // the shadows rule — and the builtin name is in the always-live
        // set, so this rule produces no diagnostic for it.
        let config = config_with_envs(&["ci"]);
        let diags = rule.check(&config);
        assert!(diags.is_empty());
    }

    #[test]
    fn suggestion_describes_resolution_paths() {
        let rule = DeadEnvironmentRule;
        let config = config_with_envs(&["staging"]);

        let diags = rule.check(&config);
        let suggestion = diags[0].suggestion.as_ref().unwrap();
        assert!(suggestion.contains("default_environment"));
        assert!(suggestion.contains("only_environments"));
    }
}
