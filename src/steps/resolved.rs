//! Resolved step ready for execution.
//!
//! A ResolvedStep combines template defaults with config overrides,
//! producing a fully-specified step that can be executed.

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
}

impl ResolvedStep {
    /// Create from a step config that uses a template.
    pub fn from_template(
        name: &str,
        template: &Template,
        config: &StepConfig,
        _inputs: &HashMap<String, serde_yaml::Value>,
    ) -> Self {
        let step = &template.step;

        Self {
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
        }
    }

    /// Create from an inline step config (no template).
    pub fn from_config(name: &str, config: &StepConfig) -> Self {
        Self {
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

        let resolved = ResolvedStep::from_template("test", &template, &config, &HashMap::new());

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

        let resolved = ResolvedStep::from_template("test", &template, &config, &HashMap::new());

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

        let resolved = ResolvedStep::from_template("test", &template, &config, &HashMap::new());

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

        let resolved = ResolvedStep::from_template("test", &template, &config, &HashMap::new());

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

        let resolved = ResolvedStep::from_config("inline", &config);

        assert_eq!(resolved.title, "Inline Step");
        assert_eq!(resolved.command, "echo inline");
    }

    #[test]
    fn from_config_uses_name_as_default_title() {
        let config = StepConfig::default();
        let resolved = ResolvedStep::from_config("step_name", &config);

        assert_eq!(resolved.title, "step_name");
    }

    #[test]
    fn from_template_uses_template_watches_when_config_empty() {
        let template = make_template();
        let config = StepConfig::default();

        let resolved = ResolvedStep::from_template("test", &template, &config, &HashMap::new());

        assert_eq!(resolved.watches, vec!["template.lock".to_string()]);
    }

    #[test]
    fn from_template_uses_config_watches_when_provided() {
        let template = make_template();
        let config = StepConfig {
            watches: vec!["config.lock".to_string()],
            ..Default::default()
        };

        let resolved = ResolvedStep::from_template("test", &template, &config, &HashMap::new());

        assert_eq!(resolved.watches, vec!["config.lock".to_string()]);
    }

    #[test]
    fn resolved_step_carries_requires_from_config() {
        let config = StepConfig {
            command: Some("bundle install".to_string()),
            requires: vec!["ruby".to_string(), "postgres-server".to_string()],
            ..Default::default()
        };

        let resolved = ResolvedStep::from_config("deps", &config);

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

        let resolved = ResolvedStep::from_template("test", &template, &config, &HashMap::new());

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

        let resolved = ResolvedStep::from_template("test", &template, &config, &HashMap::new());

        assert_eq!(resolved.requires, vec!["ruby", "node", "postgres-server"]);
    }

    #[test]
    fn resolved_step_requires_defaults_empty() {
        let config = StepConfig::default();
        let resolved = ResolvedStep::from_config("test", &config);

        assert!(resolved.requires.is_empty());
    }
}
