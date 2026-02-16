//! Resolved step ready for execution.
//!
//! A ResolvedStep combines template defaults with config overrides,
//! producing a fully-specified step that can be executed.

use crate::config::schema::StepEnvironmentOverride;
use crate::config::{CompletedCheck, StepConfig};
use crate::registry::template::Template;
use std::collections::HashMap;

/// A fully resolved step ready for execution.
#[derive(Debug, Clone)]
pub struct ResolvedStep {
    /// Step name (key from config).
    pub name: String,

    /// Display title.
    pub title: String,

    /// Description.
    pub description: Option<String>,

    /// Command to execute.
    pub command: String,

    /// Dependencies.
    pub depends_on: Vec<String>,

    /// Completion check.
    pub completed_check: Option<CompletedCheck>,

    /// Whether step can be skipped.
    pub skippable: bool,

    /// Whether step is required.
    pub required: bool,

    /// Prompt before re-running completed.
    pub prompt_if_complete: bool,

    /// Continue on failure.
    pub allow_failure: bool,

    /// Retry count.
    pub retry: u32,

    /// Environment variables.
    pub env: HashMap<String, String>,

    /// Files to watch.
    pub watches: Vec<String>,

    /// Before hooks.
    pub before: Vec<String>,

    /// After hooks.
    pub after: Vec<String>,

    /// Sensitive step.
    pub sensitive: bool,

    /// Requires sudo.
    pub requires_sudo: bool,

    /// System-level prerequisites this step requires.
    pub requires: Vec<String>,

    /// Restrict this step to specific environments.
    /// Empty means "run in all environments".
    pub only_environments: Vec<String>,
}

impl ResolvedStep {
    /// Create from a step config that uses a template.
    ///
    /// When `environment` is `Some`, applies matching per-environment overrides
    /// from the step config after the base resolution.
    pub fn from_template(
        name: &str,
        template: &Template,
        config: &StepConfig,
        _inputs: &HashMap<String, serde_yaml::Value>,
        environment: Option<&str>,
    ) -> Self {
        let step = &template.step;

        let mut resolved = Self {
            name: name.to_string(),
            title: config
                .title
                .clone()
                .or_else(|| step.title.clone())
                .unwrap_or_else(|| name.to_string()),
            description: config
                .description
                .clone()
                .or_else(|| step.description.clone()),
            command: config
                .command
                .clone()
                .or_else(|| step.command.clone())
                .unwrap_or_default(),
            depends_on: config.depends_on.clone(),
            completed_check: config
                .completed_check
                .clone()
                .or_else(|| step.completed_check.clone()),
            skippable: config.skippable,
            required: config.required,
            prompt_if_complete: config.prompt_if_complete,
            allow_failure: config.allow_failure,
            retry: config.retry,
            env: merge_env(&step.env, &config.env),
            watches: if config.watches.is_empty() {
                step.watches.clone()
            } else {
                config.watches.clone()
            },
            before: config.before.clone(),
            after: config.after.clone(),
            sensitive: config.sensitive,
            requires_sudo: config.requires_sudo,
            requires: merge_requires(&step.requires, &config.requires),
            only_environments: config.only_environments.clone(),
        };

        if let Some(env_name) = environment {
            if let Some(overrides) = config.environments.get(env_name) {
                resolved.apply_environment_overrides(overrides);
            }
        }

        resolved
    }

    /// Create from an inline step config (no template).
    ///
    /// When `environment` is `Some`, applies matching per-environment overrides
    /// from the step config after the base resolution.
    pub fn from_config(name: &str, config: &StepConfig, environment: Option<&str>) -> Self {
        let mut resolved = Self {
            name: name.to_string(),
            title: config.title.clone().unwrap_or_else(|| name.to_string()),
            description: config.description.clone(),
            command: config.command.clone().unwrap_or_default(),
            depends_on: config.depends_on.clone(),
            completed_check: config.completed_check.clone(),
            skippable: config.skippable,
            required: config.required,
            prompt_if_complete: config.prompt_if_complete,
            allow_failure: config.allow_failure,
            retry: config.retry,
            env: config.env.clone(),
            watches: config.watches.clone(),
            before: config.before.clone(),
            after: config.after.clone(),
            sensitive: config.sensitive,
            requires_sudo: config.requires_sudo,
            requires: config.requires.clone(),
            only_environments: config.only_environments.clone(),
        };

        if let Some(env_name) = environment {
            if let Some(overrides) = config.environments.get(env_name) {
                resolved.apply_environment_overrides(overrides);
            }
        }

        resolved
    }

    /// Apply per-environment overrides to this resolved step.
    ///
    /// Only fields that are `Some` in the override will replace the base values.
    /// The `env` field supports add/override (`Some(val)`) and removal (`None`).
    pub fn apply_environment_overrides(&mut self, overrides: &StepEnvironmentOverride) {
        if let Some(t) = &overrides.title {
            self.title = t.clone();
        }
        if let Some(d) = &overrides.description {
            self.description = Some(d.clone());
        }
        if let Some(cmd) = &overrides.command {
            self.command = cmd.clone();
        }
        for (k, v) in &overrides.env {
            match v {
                Some(val) => {
                    self.env.insert(k.clone(), val.clone());
                }
                None => {
                    self.env.remove(k);
                }
            }
        }
        if let Some(check) = &overrides.completed_check {
            self.completed_check = Some(check.clone());
        }
        if let Some(v) = overrides.skippable {
            self.skippable = v;
        }
        if let Some(v) = overrides.allow_failure {
            self.allow_failure = v;
        }
        if let Some(v) = overrides.requires_sudo {
            self.requires_sudo = v;
        }
        if let Some(v) = overrides.sensitive {
            self.sensitive = v;
        }
        if let Some(hooks) = &overrides.before {
            self.before = hooks.clone();
        }
        if let Some(hooks) = &overrides.after {
            self.after = hooks.clone();
        }
        if let Some(deps) = &overrides.depends_on {
            self.depends_on = deps.clone();
        }
        if let Some(reqs) = &overrides.requires {
            self.requires = reqs.clone();
        }
        if let Some(w) = &overrides.watches {
            self.watches = w.clone();
        }
        if let Some(r) = overrides.retry {
            self.retry = r;
        }
    }
}

fn merge_requires(template_requires: &[String], config_requires: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for r in template_requires.iter().chain(config_requires.iter()) {
        if seen.insert(r.clone()) {
            result.push(r.clone());
        }
    }
    result
}

fn merge_env(
    template_env: &HashMap<String, String>,
    config_env: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut result = template_env.clone();
    result.extend(config_env.iter().map(|(k, v)| (k.clone(), v.clone())));
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::template::TemplateStep;

    fn make_template() -> Template {
        Template {
            name: "test".to_string(),
            description: "Test template".to_string(),
            category: "test".to_string(),
            version: "1.0.0".to_string(),
            min_bivvy_version: None,
            platforms: vec![],
            detects: vec![],
            inputs: HashMap::new(),
            step: TemplateStep {
                title: Some("Template Title".to_string()),
                description: Some("Template desc".to_string()),
                command: Some("template command".to_string()),
                completed_check: None,
                env: {
                    let mut env = HashMap::new();
                    env.insert("TEMPLATE_VAR".to_string(), "from_template".to_string());
                    env
                },
                watches: vec!["template.lock".to_string()],
                requires: vec![],
            },
            environment_impact: None,
        }
    }

    #[test]
    fn from_template_uses_template_defaults() {
        let template = make_template();
        let config = StepConfig::default();

        let resolved =
            ResolvedStep::from_template("test", &template, &config, &HashMap::new(), None);

        assert_eq!(resolved.title, "Template Title");
        assert_eq!(resolved.command, "template command");
        assert!(resolved.env.contains_key("TEMPLATE_VAR"));
    }

    #[test]
    fn from_template_config_overrides_template() {
        let template = make_template();
        let config = StepConfig {
            title: Some("Custom Title".to_string()),
            command: Some("custom command".to_string()),
            ..Default::default()
        };

        let resolved =
            ResolvedStep::from_template("test", &template, &config, &HashMap::new(), None);

        assert_eq!(resolved.title, "Custom Title");
        assert_eq!(resolved.command, "custom command");
    }

    #[test]
    fn from_template_merges_env() {
        let template = make_template();
        let mut config = StepConfig::default();
        config
            .env
            .insert("CONFIG_VAR".to_string(), "from_config".to_string());

        let resolved =
            ResolvedStep::from_template("test", &template, &config, &HashMap::new(), None);

        assert_eq!(
            resolved.env.get("TEMPLATE_VAR"),
            Some(&"from_template".to_string())
        );
        assert_eq!(
            resolved.env.get("CONFIG_VAR"),
            Some(&"from_config".to_string())
        );
    }

    #[test]
    fn from_template_config_env_overrides_template_env() {
        let template = make_template();
        let mut config = StepConfig::default();
        config
            .env
            .insert("TEMPLATE_VAR".to_string(), "overridden".to_string());

        let resolved =
            ResolvedStep::from_template("test", &template, &config, &HashMap::new(), None);

        assert_eq!(
            resolved.env.get("TEMPLATE_VAR"),
            Some(&"overridden".to_string())
        );
    }

    #[test]
    fn from_config_works_without_template() {
        let config = StepConfig {
            title: Some("Inline Step".to_string()),
            command: Some("echo inline".to_string()),
            ..Default::default()
        };

        let resolved = ResolvedStep::from_config("inline", &config, None);

        assert_eq!(resolved.title, "Inline Step");
        assert_eq!(resolved.command, "echo inline");
    }

    #[test]
    fn from_config_uses_name_as_default_title() {
        let config = StepConfig::default();
        let resolved = ResolvedStep::from_config("step_name", &config, None);

        assert_eq!(resolved.title, "step_name");
    }

    #[test]
    fn from_template_uses_template_watches_when_config_empty() {
        let template = make_template();
        let config = StepConfig::default();

        let resolved =
            ResolvedStep::from_template("test", &template, &config, &HashMap::new(), None);

        assert_eq!(resolved.watches, vec!["template.lock".to_string()]);
    }

    #[test]
    fn from_template_uses_config_watches_when_provided() {
        let template = make_template();
        let config = StepConfig {
            watches: vec!["config.lock".to_string()],
            ..Default::default()
        };

        let resolved =
            ResolvedStep::from_template("test", &template, &config, &HashMap::new(), None);

        assert_eq!(resolved.watches, vec!["config.lock".to_string()]);
    }

    #[test]
    fn resolved_step_carries_requires_from_config() {
        let config = StepConfig {
            command: Some("bundle install".to_string()),
            requires: vec!["ruby".to_string(), "postgres-server".to_string()],
            ..Default::default()
        };

        let resolved = ResolvedStep::from_config("deps", &config, None);

        assert_eq!(resolved.requires, vec!["ruby", "postgres-server"]);
    }

    #[test]
    fn resolved_step_merges_template_and_config_requires() {
        let mut template = make_template();
        template.step.requires = vec!["node".to_string()];

        let config = StepConfig {
            requires: vec!["postgres-server".to_string()],
            ..Default::default()
        };

        let resolved =
            ResolvedStep::from_template("test", &template, &config, &HashMap::new(), None);

        assert_eq!(resolved.requires, vec!["node", "postgres-server"]);
    }

    #[test]
    fn resolved_step_deduplicates_merged_requires() {
        let mut template = make_template();
        template.step.requires = vec!["ruby".to_string(), "node".to_string()];

        let config = StepConfig {
            requires: vec!["node".to_string(), "postgres-server".to_string()],
            ..Default::default()
        };

        let resolved =
            ResolvedStep::from_template("test", &template, &config, &HashMap::new(), None);

        assert_eq!(resolved.requires, vec!["ruby", "node", "postgres-server"]);
    }

    #[test]
    fn resolved_step_requires_defaults_empty() {
        let config = StepConfig::default();
        let resolved = ResolvedStep::from_config("test", &config, None);

        assert!(resolved.requires.is_empty());
    }

    #[test]
    fn step_environment_override_defaults_all_none() {
        let overrides = StepEnvironmentOverride::default();
        assert!(overrides.title.is_none());
        assert!(overrides.description.is_none());
        assert!(overrides.command.is_none());
        assert!(overrides.env.is_empty());
        assert!(overrides.completed_check.is_none());
        assert!(overrides.skippable.is_none());
        assert!(overrides.allow_failure.is_none());
        assert!(overrides.requires_sudo.is_none());
        assert!(overrides.sensitive.is_none());
        assert!(overrides.before.is_none());
        assert!(overrides.after.is_none());
        assert!(overrides.depends_on.is_none());
        assert!(overrides.requires.is_none());
        assert!(overrides.watches.is_none());
        assert!(overrides.retry.is_none());
    }

    #[test]
    fn apply_environment_overrides_replaces_command() {
        let config = StepConfig {
            command: Some("echo base".to_string()),
            ..Default::default()
        };
        let mut resolved = ResolvedStep::from_config("test", &config, None);

        let overrides = StepEnvironmentOverride {
            command: Some("echo ci".to_string()),
            ..Default::default()
        };
        resolved.apply_environment_overrides(&overrides);

        assert_eq!(resolved.command, "echo ci");
    }

    #[test]
    fn apply_environment_overrides_replaces_requires() {
        let config = StepConfig {
            command: Some("echo test".to_string()),
            requires: vec!["ruby".to_string(), "postgres-server".to_string()],
            ..Default::default()
        };
        let mut resolved = ResolvedStep::from_config("test", &config, None);

        let overrides = StepEnvironmentOverride {
            requires: Some(vec!["ruby".to_string()]),
            ..Default::default()
        };
        resolved.apply_environment_overrides(&overrides);

        assert_eq!(resolved.requires, vec!["ruby"]);
    }

    #[test]
    fn apply_environment_overrides_adds_env_var() {
        let config = StepConfig {
            command: Some("echo test".to_string()),
            env: {
                let mut env = HashMap::new();
                env.insert("BASE_VAR".to_string(), "base".to_string());
                env
            },
            ..Default::default()
        };
        let mut resolved = ResolvedStep::from_config("test", &config, None);

        let overrides = StepEnvironmentOverride {
            env: {
                let mut env = HashMap::new();
                env.insert("CI".to_string(), Some("true".to_string()));
                env
            },
            ..Default::default()
        };
        resolved.apply_environment_overrides(&overrides);

        assert_eq!(resolved.env.get("BASE_VAR"), Some(&"base".to_string()));
        assert_eq!(resolved.env.get("CI"), Some(&"true".to_string()));
    }

    #[test]
    fn apply_environment_overrides_removes_env_var() {
        let config = StepConfig {
            command: Some("echo test".to_string()),
            env: {
                let mut env = HashMap::new();
                env.insert("DEBUG".to_string(), "true".to_string());
                env.insert("KEEP".to_string(), "yes".to_string());
                env
            },
            ..Default::default()
        };
        let mut resolved = ResolvedStep::from_config("test", &config, None);

        let overrides = StepEnvironmentOverride {
            env: {
                let mut env = HashMap::new();
                env.insert("DEBUG".to_string(), None);
                env
            },
            ..Default::default()
        };
        resolved.apply_environment_overrides(&overrides);

        assert!(!resolved.env.contains_key("DEBUG"));
        assert_eq!(resolved.env.get("KEEP"), Some(&"yes".to_string()));
    }

    #[test]
    fn apply_environment_overrides_ignores_none_fields() {
        let config = StepConfig {
            title: Some("Original".to_string()),
            command: Some("echo original".to_string()),
            ..Default::default()
        };
        let mut resolved = ResolvedStep::from_config("test", &config, None);

        let overrides = StepEnvironmentOverride::default();
        resolved.apply_environment_overrides(&overrides);

        assert_eq!(resolved.title, "Original");
        assert_eq!(resolved.command, "echo original");
    }

    #[test]
    fn from_config_applies_environment_overrides() {
        let mut environments = HashMap::new();
        environments.insert(
            "ci".to_string(),
            StepEnvironmentOverride {
                command: Some("echo ci-mode".to_string()),
                ..Default::default()
            },
        );
        let config = StepConfig {
            command: Some("echo dev-mode".to_string()),
            environments,
            ..Default::default()
        };

        let resolved = ResolvedStep::from_config("test", &config, Some("ci"));
        assert_eq!(resolved.command, "echo ci-mode");
    }

    #[test]
    fn from_config_no_override_for_unknown_environment() {
        let mut environments = HashMap::new();
        environments.insert(
            "ci".to_string(),
            StepEnvironmentOverride {
                command: Some("echo ci-mode".to_string()),
                ..Default::default()
            },
        );
        let config = StepConfig {
            command: Some("echo dev-mode".to_string()),
            environments,
            ..Default::default()
        };

        let resolved = ResolvedStep::from_config("test", &config, Some("staging"));
        assert_eq!(resolved.command, "echo dev-mode");
    }

    #[test]
    fn from_config_none_environment_skips_overrides() {
        let mut environments = HashMap::new();
        environments.insert(
            "ci".to_string(),
            StepEnvironmentOverride {
                command: Some("echo ci-mode".to_string()),
                ..Default::default()
            },
        );
        let config = StepConfig {
            command: Some("echo dev-mode".to_string()),
            environments,
            ..Default::default()
        };

        let resolved = ResolvedStep::from_config("test", &config, None);
        assert_eq!(resolved.command, "echo dev-mode");
    }

    #[test]
    fn from_template_applies_environment_overrides() {
        let template = make_template();
        let mut environments = HashMap::new();
        environments.insert(
            "ci".to_string(),
            StepEnvironmentOverride {
                command: Some("echo ci-command".to_string()),
                ..Default::default()
            },
        );
        let config = StepConfig {
            environments,
            ..Default::default()
        };

        let resolved =
            ResolvedStep::from_template("test", &template, &config, &HashMap::new(), Some("ci"));
        assert_eq!(resolved.command, "echo ci-command");
    }

    #[test]
    fn from_template_none_environment_uses_base() {
        let template = make_template();
        let mut environments = HashMap::new();
        environments.insert(
            "ci".to_string(),
            StepEnvironmentOverride {
                command: Some("echo ci-command".to_string()),
                ..Default::default()
            },
        );
        let config = StepConfig {
            environments,
            ..Default::default()
        };

        let resolved =
            ResolvedStep::from_template("test", &template, &config, &HashMap::new(), None);
        assert_eq!(resolved.command, "template command");
    }

    #[test]
    fn from_config_propagates_only_environments() {
        let config = StepConfig {
            command: Some("echo test".to_string()),
            only_environments: vec!["ci".to_string(), "staging".to_string()],
            ..Default::default()
        };

        let resolved = ResolvedStep::from_config("test", &config, None);
        assert_eq!(resolved.only_environments, vec!["ci", "staging"]);
    }

    #[test]
    fn from_config_empty_only_environments() {
        let config = StepConfig::default();
        let resolved = ResolvedStep::from_config("test", &config, None);
        assert!(resolved.only_environments.is_empty());
    }

    #[test]
    fn from_template_propagates_only_environments() {
        let template = make_template();
        let config = StepConfig {
            only_environments: vec!["ci".to_string()],
            ..Default::default()
        };

        let resolved =
            ResolvedStep::from_template("test", &template, &config, &HashMap::new(), None);
        assert_eq!(resolved.only_environments, vec!["ci"]);
    }

    // --- 7B: Resolution override tests ---

    #[test]
    fn resolved_step_env_overrides_title() {
        let config = StepConfig {
            title: Some("Base Title".to_string()),
            command: Some("echo test".to_string()),
            ..Default::default()
        };
        let mut resolved = ResolvedStep::from_config("test", &config, None);

        let overrides = StepEnvironmentOverride {
            title: Some("CI".to_string()),
            ..Default::default()
        };
        resolved.apply_environment_overrides(&overrides);

        assert_eq!(resolved.title, "CI");
    }

    #[test]
    fn resolved_step_env_overrides_description() {
        let config = StepConfig {
            command: Some("echo test".to_string()),
            description: Some("Base desc".to_string()),
            ..Default::default()
        };
        let mut resolved = ResolvedStep::from_config("test", &config, None);

        let overrides = StepEnvironmentOverride {
            description: Some("CI desc".to_string()),
            ..Default::default()
        };
        resolved.apply_environment_overrides(&overrides);

        assert_eq!(resolved.description, Some("CI desc".to_string()));
    }

    #[test]
    fn resolved_step_env_overrides_skippable() {
        let config = StepConfig {
            command: Some("echo test".to_string()),
            skippable: false,
            ..Default::default()
        };
        let mut resolved = ResolvedStep::from_config("test", &config, None);
        assert!(!resolved.skippable);

        let overrides = StepEnvironmentOverride {
            skippable: Some(true),
            ..Default::default()
        };
        resolved.apply_environment_overrides(&overrides);

        assert!(resolved.skippable);
    }

    #[test]
    fn resolved_step_env_overrides_completed_check() {
        let config = StepConfig {
            command: Some("echo test".to_string()),
            completed_check: Some(crate::config::CompletedCheck::FileExists {
                path: "base.txt".to_string(),
            }),
            ..Default::default()
        };
        let mut resolved = ResolvedStep::from_config("test", &config, None);

        let overrides = StepEnvironmentOverride {
            completed_check: Some(crate::config::CompletedCheck::CommandSucceeds {
                command: "true".to_string(),
            }),
            ..Default::default()
        };
        resolved.apply_environment_overrides(&overrides);

        assert!(matches!(
            resolved.completed_check,
            Some(crate::config::CompletedCheck::CommandSucceeds { .. })
        ));
    }

    #[test]
    fn resolved_step_env_overrides_depends_on() {
        let config = StepConfig {
            command: Some("echo test".to_string()),
            depends_on: vec!["a".to_string(), "b".to_string()],
            ..Default::default()
        };
        let mut resolved = ResolvedStep::from_config("test", &config, None);

        let overrides = StepEnvironmentOverride {
            depends_on: Some(vec!["x".to_string()]),
            ..Default::default()
        };
        resolved.apply_environment_overrides(&overrides);

        assert_eq!(resolved.depends_on, vec!["x"]);
    }

    #[test]
    fn resolved_step_env_overrides_watches() {
        let config = StepConfig {
            command: Some("echo test".to_string()),
            watches: vec!["Gemfile".to_string(), "Gemfile.lock".to_string()],
            ..Default::default()
        };
        let mut resolved = ResolvedStep::from_config("test", &config, None);

        let overrides = StepEnvironmentOverride {
            watches: Some(vec!["package.json".to_string()]),
            ..Default::default()
        };
        resolved.apply_environment_overrides(&overrides);

        assert_eq!(resolved.watches, vec!["package.json"]);
    }

    #[test]
    fn resolved_step_env_overrides_retry() {
        let config = StepConfig {
            command: Some("echo test".to_string()),
            ..Default::default()
        };
        let mut resolved = ResolvedStep::from_config("test", &config, None);
        assert_eq!(resolved.retry, 0);

        let overrides = StepEnvironmentOverride {
            retry: Some(3),
            ..Default::default()
        };
        resolved.apply_environment_overrides(&overrides);

        assert_eq!(resolved.retry, 3);
    }

    #[test]
    fn resolved_step_env_overrides_existing_env_var() {
        let config = StepConfig {
            command: Some("echo test".to_string()),
            env: {
                let mut env = HashMap::new();
                env.insert("RAILS_ENV".to_string(), "development".to_string());
                env
            },
            ..Default::default()
        };
        let mut resolved = ResolvedStep::from_config("test", &config, None);

        let overrides = StepEnvironmentOverride {
            env: {
                let mut env = HashMap::new();
                env.insert("RAILS_ENV".to_string(), Some("test".to_string()));
                env
            },
            ..Default::default()
        };
        resolved.apply_environment_overrides(&overrides);

        assert_eq!(resolved.env.get("RAILS_ENV"), Some(&"test".to_string()));
    }
}
