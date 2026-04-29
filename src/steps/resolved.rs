//! Resolved step ready for execution.
//!
//! A ResolvedStep combines template defaults with config overrides,
//! producing a fully-specified step that can be executed.

use crate::checks::{Check, SatisfactionCondition};
use crate::config::schema::{PromptConfig, StepEnvironmentOverride};
use crate::config::StepConfig;
use crate::registry::template::Template;
use std::collections::HashMap;
use std::path::PathBuf;

/// Resolved execution fields (command, checks, retry, sudo).
#[derive(Debug, Clone, Default)]
pub struct ResolvedExecution {
    /// Command to execute.
    pub command: String,

    /// Single check (new `Check` enum).
    pub check: Option<Check>,

    /// Multiple checks (implicit `all`).
    pub checks: Vec<Check>,

    /// Precondition that must pass before running.
    pub precondition: Option<Check>,

    /// Retry count.
    pub retry: u32,

    /// Requires sudo.
    pub requires_sudo: bool,
}

impl ResolvedExecution {
    /// Get the effective check to evaluate.
    ///
    /// Returns `None` if no check is configured.
    /// If `checks` has entries, they are wrapped in an `All` combinator.
    /// If `check` is set, it is returned directly.
    pub fn effective_check(&self) -> Option<Check> {
        if !self.checks.is_empty() {
            Some(Check::All {
                name: None,
                checks: self.checks.clone(),
            })
        } else {
            self.check.clone()
        }
    }

    /// Get the effective precondition.
    pub fn effective_precondition(&self) -> Option<Check> {
        self.precondition.clone()
    }
}

/// Resolved environment variable fields.
#[derive(Debug, Clone, Default)]
pub struct ResolvedEnvironmentVars {
    /// Environment variables.
    pub env: HashMap<String, String>,

    /// Path to env file to load before executing this step.
    pub env_file: Option<PathBuf>,

    /// Don't fail if env_file is missing.
    pub env_file_optional: bool,
}

/// Resolved behavior fields (skippable, required, etc.).
#[derive(Debug, Clone)]
pub struct ResolvedBehavior {
    /// Whether step can be skipped via `--skip`.
    pub skippable: bool,

    /// Whether step is required.
    pub required: bool,

    /// Always prompt the user before running this step.
    pub confirm: bool,

    /// Whether this step auto-runs when the pipeline determines it needs to run.
    pub auto_run: bool,

    /// Prompt before re-running completed.
    pub prompt_on_rerun: bool,

    /// Continue on failure.
    pub allow_failure: bool,

    /// Sensitive step.
    pub sensitive: bool,

    /// How long a previous successful run counts as recent enough.
    pub rerun_window: crate::runner::RerunWindow,

    /// Always re-run this step, bypassing its checks.
    pub force: bool,
}

impl Default for ResolvedBehavior {
    fn default() -> Self {
        Self {
            skippable: true,
            required: false,
            confirm: false,
            auto_run: true,
            prompt_on_rerun: false,
            allow_failure: false,
            sensitive: false,
            rerun_window: crate::runner::RerunWindow::default(),
            force: false,
        }
    }
}

/// Resolved lifecycle hooks.
#[derive(Debug, Clone, Default)]
pub struct ResolvedHooks {
    /// Before hooks.
    pub before: Vec<String>,

    /// After hooks.
    pub after: Vec<String>,
}

/// Resolved output settings.
#[derive(Debug, Clone, Default)]
pub struct ResolvedOutput {
    /// Interactive prompts to execute before this step runs.
    pub prompts: Vec<PromptConfig>,
}

/// Resolved environment scoping.
#[derive(Debug, Clone, Default)]
pub struct ResolvedScoping {
    /// Restrict this step to specific environments.
    /// Empty means "run in all environments".
    pub only_environments: Vec<String>,
}

/// A fully resolved step ready for execution.
#[derive(Debug, Clone)]
pub struct ResolvedStep {
    /// Step name (key from config).
    pub name: String,

    /// Display title.
    pub title: String,

    /// Description.
    pub description: Option<String>,

    /// Dependencies.
    pub depends_on: Vec<String>,

    /// System-level prerequisites this step requires.
    pub requires: Vec<String>,

    /// Resolved template input values for interpolation.
    pub inputs: HashMap<String, String>,

    /// Declarative satisfaction conditions.
    /// If all conditions pass, the step's purpose is already fulfilled.
    pub satisfied_when: Vec<SatisfactionCondition>,

    /// Execution settings (command, checks, watches, retry, sudo).
    pub execution: ResolvedExecution,

    /// Environment variable settings.
    pub env_vars: ResolvedEnvironmentVars,

    /// Behavior settings (skippable, required, etc.).
    pub behavior: ResolvedBehavior,

    /// Lifecycle hooks (before, after).
    pub hooks: ResolvedHooks,

    /// Output settings (prompts).
    pub output: ResolvedOutput,

    /// Environment scoping.
    pub scoping: ResolvedScoping,
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
        inputs: &HashMap<String, serde_yaml::Value>,
        environment: Option<&str>,
    ) -> Self {
        let step = &template.step;

        // Resolve effective input values: provided value > template default
        let resolved_inputs = resolve_template_inputs(&template.inputs, inputs);

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
            depends_on: config.depends_on.clone(),
            requires: merge_requires(&step.requires, &config.requires),
            inputs: resolved_inputs.clone(),
            satisfied_when: config.satisfied_when.clone(),
            execution: ResolvedExecution {
                command: config
                    .execution
                    .command
                    .clone()
                    .or_else(|| step.command.clone())
                    .unwrap_or_default(),
                check: config.execution.check.clone(),
                checks: config.execution.checks.clone(),
                precondition: config.execution.precondition.clone(),
                retry: config.execution.retry,
                requires_sudo: config.execution.requires_sudo,
            },
            env_vars: ResolvedEnvironmentVars {
                env: merge_env(&step.env, &config.env_vars.env),
                env_file: config.env_vars.env_file.clone(),
                env_file_optional: config.env_vars.env_file_optional,
            },
            behavior: ResolvedBehavior {
                skippable: config.behavior.skippable,
                required: config.behavior.required,
                confirm: config.behavior.confirm,
                auto_run: config.behavior.auto_run.unwrap_or(true),
                prompt_on_rerun: config.behavior.prompt_on_rerun.unwrap_or(false),
                allow_failure: config.behavior.allow_failure,
                sensitive: config.behavior.sensitive,
                rerun_window: resolve_rerun_window(config.behavior.rerun_window.as_deref()),
                force: config.behavior.force,
            },
            hooks: ResolvedHooks {
                before: config.hooks.before.clone(),
                after: config.hooks.after.clone(),
            },
            output: ResolvedOutput {
                prompts: merge_template_prompts(
                    &config.output_settings.prompts,
                    &template.inputs,
                    &resolved_inputs,
                ),
            },
            scoping: ResolvedScoping {
                only_environments: config.scoping.only_environments.clone(),
            },
        };

        if let Some(env_name) = environment {
            if let Some(overrides) = config.scoping.environments.get(env_name) {
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
            depends_on: config.depends_on.clone(),
            requires: config.requires.clone(),
            inputs: HashMap::new(),
            satisfied_when: config.satisfied_when.clone(),
            execution: ResolvedExecution {
                command: config.execution.command.clone().unwrap_or_default(),
                check: config.execution.check.clone(),
                checks: config.execution.checks.clone(),
                precondition: config.execution.precondition.clone(),
                retry: config.execution.retry,
                requires_sudo: config.execution.requires_sudo,
            },
            env_vars: ResolvedEnvironmentVars {
                env: config.env_vars.env.clone(),
                env_file: config.env_vars.env_file.clone(),
                env_file_optional: config.env_vars.env_file_optional,
            },
            behavior: ResolvedBehavior {
                skippable: config.behavior.skippable,
                required: config.behavior.required,
                confirm: config.behavior.confirm,
                auto_run: config.behavior.auto_run.unwrap_or(true),
                prompt_on_rerun: config.behavior.prompt_on_rerun.unwrap_or(false),
                allow_failure: config.behavior.allow_failure,
                sensitive: config.behavior.sensitive,
                rerun_window: resolve_rerun_window(config.behavior.rerun_window.as_deref()),
                force: config.behavior.force,
            },
            hooks: ResolvedHooks {
                before: config.hooks.before.clone(),
                after: config.hooks.after.clone(),
            },
            output: ResolvedOutput {
                prompts: config.output_settings.prompts.clone(),
            },
            scoping: ResolvedScoping {
                only_environments: config.scoping.only_environments.clone(),
            },
        };

        if let Some(env_name) = environment {
            if let Some(overrides) = config.scoping.environments.get(env_name) {
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
            self.execution.command = cmd.clone();
        }
        for (k, v) in &overrides.env {
            match v {
                Some(val) => {
                    self.env_vars.env.insert(k.clone(), val.clone());
                }
                None => {
                    self.env_vars.env.remove(k);
                }
            }
        }
        if let Some(check) = &overrides.check {
            self.execution.check = Some(check.clone());
        }
        if let Some(check) = &overrides.precondition {
            self.execution.precondition = Some(check.clone());
        }
        if let Some(v) = overrides.skippable {
            self.behavior.skippable = v;
        }
        if let Some(v) = overrides.allow_failure {
            self.behavior.allow_failure = v;
        }
        if let Some(v) = overrides.requires_sudo {
            self.execution.requires_sudo = v;
        }
        if let Some(v) = overrides.sensitive {
            self.behavior.sensitive = v;
        }
        if let Some(hooks) = &overrides.before {
            self.hooks.before = hooks.clone();
        }
        if let Some(hooks) = &overrides.after {
            self.hooks.after = hooks.clone();
        }
        if let Some(deps) = &overrides.depends_on {
            self.depends_on = deps.clone();
        }
        if let Some(reqs) = &overrides.requires {
            self.requires = reqs.clone();
        }
        if let Some(r) = overrides.retry {
            self.execution.retry = r;
        }
        if let Some(v) = overrides.confirm {
            self.behavior.confirm = v;
        }
        if let Some(v) = overrides.auto_run {
            self.behavior.auto_run = v;
        }
        if let Some(ref w) = overrides.rerun_window {
            self.behavior.rerun_window = resolve_rerun_window(Some(w));
        }
    }
}

/// Resolve a rerun window string to a `RerunWindow`, falling back to the default.
fn resolve_rerun_window(window_str: Option<&str>) -> crate::runner::RerunWindow {
    match window_str {
        Some(s) => s.parse().unwrap_or_default(),
        None => crate::runner::RerunWindow::default(),
    }
}

/// Convert a serde_yaml::Value to a String for interpolation.
fn yaml_value_to_string(value: &serde_yaml::Value) -> String {
    match value {
        serde_yaml::Value::String(s) => s.clone(),
        serde_yaml::Value::Bool(b) => b.to_string(),
        serde_yaml::Value::Number(n) => n.to_string(),
        other => format!("{:?}", other),
    }
}

/// Resolve template inputs: for each input defined in the template,
/// use the provided value from config or fall back to the template default.
fn resolve_template_inputs(
    template_inputs: &HashMap<String, crate::registry::template::TemplateInput>,
    provided: &HashMap<String, serde_yaml::Value>,
) -> HashMap<String, String> {
    let mut resolved = HashMap::new();
    for (name, input_def) in template_inputs {
        let provided_val = provided.get(name);
        if let Some(effective) = input_def.effective_value(provided_val) {
            resolved.insert(name.clone(), yaml_value_to_string(effective));
        }
    }
    resolved
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

/// Merge config-level prompts with template input prompts.
///
/// Template inputs that have a `prompt` defined will generate a PromptConfig
/// entry UNLESS the input was already provided statically (via `inputs:` in
/// config) or the config already defines a prompt with the same key.
/// Config prompts take precedence over template prompts.
fn merge_template_prompts(
    config_prompts: &[PromptConfig],
    template_inputs: &HashMap<String, crate::registry::template::TemplateInput>,
    resolved_inputs: &HashMap<String, String>,
) -> Vec<PromptConfig> {
    let mut prompts = config_prompts.to_vec();

    for (input_name, input_def) in template_inputs {
        let Some(ref template_prompt) = input_def.prompt else {
            continue;
        };

        // Skip if already provided statically
        if resolved_inputs.contains_key(input_name) {
            continue;
        }

        // Skip if config already defines a prompt for this key
        if prompts.iter().any(|p| p.key == *input_name) {
            continue;
        }

        prompts.push(PromptConfig {
            key: input_name.clone(),
            question: template_prompt.question.clone(),
            prompt_type: template_prompt.prompt_type.clone(),
            options: template_prompt.options.clone(),
            default: input_def.default.clone(),
        });
    }

    prompts
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
    use crate::config::{
        BehaviorConfig, EnvironmentScopingConfig, EnvironmentVarsConfig, ExecutionConfig,
        StepOutputSettings,
    };
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
            detectors: vec![],
            inputs: HashMap::new(),
            step: TemplateStep {
                title: Some("Template Title".to_string()),
                description: Some("Template desc".to_string()),
                command: Some("template command".to_string()),
                env: {
                    let mut env = HashMap::new();
                    env.insert("TEMPLATE_VAR".to_string(), "from_template".to_string());
                    env
                },
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
        assert_eq!(resolved.execution.command, "template command");
        assert!(resolved.env_vars.env.contains_key("TEMPLATE_VAR"));
    }

    #[test]
    fn from_template_config_overrides_template() {
        let template = make_template();
        let config = StepConfig {
            title: Some("Custom Title".to_string()),
            execution: ExecutionConfig {
                command: Some("custom command".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };

        let resolved =
            ResolvedStep::from_template("test", &template, &config, &HashMap::new(), None);

        assert_eq!(resolved.title, "Custom Title");
        assert_eq!(resolved.execution.command, "custom command");
    }

    #[test]
    fn from_template_merges_env() {
        let template = make_template();
        let mut config = StepConfig::default();
        config
            .env_vars
            .env
            .insert("CONFIG_VAR".to_string(), "from_config".to_string());

        let resolved =
            ResolvedStep::from_template("test", &template, &config, &HashMap::new(), None);

        assert_eq!(
            resolved.env_vars.env.get("TEMPLATE_VAR"),
            Some(&"from_template".to_string())
        );
        assert_eq!(
            resolved.env_vars.env.get("CONFIG_VAR"),
            Some(&"from_config".to_string())
        );
    }

    #[test]
    fn from_template_config_env_overrides_template_env() {
        let template = make_template();
        let mut config = StepConfig::default();
        config
            .env_vars
            .env
            .insert("TEMPLATE_VAR".to_string(), "overridden".to_string());

        let resolved =
            ResolvedStep::from_template("test", &template, &config, &HashMap::new(), None);

        assert_eq!(
            resolved.env_vars.env.get("TEMPLATE_VAR"),
            Some(&"overridden".to_string())
        );
    }

    #[test]
    fn from_config_works_without_template() {
        let config = StepConfig {
            title: Some("Inline Step".to_string()),
            execution: ExecutionConfig {
                command: Some("echo inline".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };

        let resolved = ResolvedStep::from_config("inline", &config, None);

        assert_eq!(resolved.title, "Inline Step");
        assert_eq!(resolved.execution.command, "echo inline");
    }

    #[test]
    fn from_config_uses_name_as_default_title() {
        let config = StepConfig::default();
        let resolved = ResolvedStep::from_config("step_name", &config, None);

        assert_eq!(resolved.title, "step_name");
    }

    #[test]
    fn resolved_step_carries_requires_from_config() {
        let config = StepConfig {
            execution: ExecutionConfig {
                command: Some("bundle install".to_string()),
                ..Default::default()
            },
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

    // --- Precondition resolution tests ---

    #[test]
    fn resolved_step_includes_precondition() {
        let config = StepConfig {
            execution: ExecutionConfig {
                command: Some("echo test".to_string()),
                precondition: Some(Check::Execution {
                    name: None,
                    command: "exit 0".to_string(),
                    validation: crate::checks::ValidationMode::Success,
                }),
                ..Default::default()
            },
            ..Default::default()
        };

        let resolved = ResolvedStep::from_config("test", &config, None);
        assert!(resolved.execution.precondition.is_some());
        assert!(matches!(
            resolved.execution.precondition,
            Some(Check::Execution { .. })
        ));
    }

    #[test]
    fn resolved_step_precondition_defaults_none() {
        let config = StepConfig::default();
        let resolved = ResolvedStep::from_config("test", &config, None);
        assert!(resolved.execution.precondition.is_none());
    }

    #[test]
    fn resolved_step_environment_override_precondition() {
        let config = StepConfig {
            execution: ExecutionConfig {
                command: Some("echo test".to_string()),
                precondition: Some(Check::Execution {
                    name: None,
                    command: "exit 0".to_string(),
                    validation: crate::checks::ValidationMode::Success,
                }),
                ..Default::default()
            },
            scoping: EnvironmentScopingConfig {
                environments: {
                    let mut envs = HashMap::new();
                    envs.insert(
                        "ci".to_string(),
                        StepEnvironmentOverride {
                            precondition: Some(Check::Execution {
                                name: None,
                                command: "exit 1".to_string(),
                                validation: crate::checks::ValidationMode::Success,
                            }),
                            ..Default::default()
                        },
                    );
                    envs
                },
                ..Default::default()
            },
            ..Default::default()
        };

        let resolved = ResolvedStep::from_config("test", &config, Some("ci"));
        if let Some(Check::Execution { command, .. }) = &resolved.execution.precondition {
            assert_eq!(
                command, "exit 1",
                "environment override should replace precondition"
            );
        } else {
            panic!("Expected Execution precondition");
        }
    }

    #[test]
    fn step_environment_override_defaults_all_none() {
        let overrides = StepEnvironmentOverride::default();
        assert!(overrides.title.is_none());
        assert!(overrides.description.is_none());
        assert!(overrides.command.is_none());
        assert!(overrides.env.is_empty());
        assert!(overrides.check.is_none());
        assert!(overrides.precondition.is_none());
        assert!(overrides.skippable.is_none());
        assert!(overrides.allow_failure.is_none());
        assert!(overrides.requires_sudo.is_none());
        assert!(overrides.sensitive.is_none());
        assert!(overrides.before.is_none());
        assert!(overrides.after.is_none());
        assert!(overrides.depends_on.is_none());
        assert!(overrides.requires.is_none());
        assert!(overrides.retry.is_none());
    }

    #[test]
    fn apply_environment_overrides_replaces_command() {
        let config = StepConfig {
            execution: ExecutionConfig {
                command: Some("echo base".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut resolved = ResolvedStep::from_config("test", &config, None);

        let overrides = StepEnvironmentOverride {
            command: Some("echo ci".to_string()),
            ..Default::default()
        };
        resolved.apply_environment_overrides(&overrides);

        assert_eq!(resolved.execution.command, "echo ci");
    }

    #[test]
    fn apply_environment_overrides_replaces_requires() {
        let config = StepConfig {
            execution: ExecutionConfig {
                command: Some("echo test".to_string()),
                ..Default::default()
            },
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
            execution: ExecutionConfig {
                command: Some("echo test".to_string()),
                ..Default::default()
            },
            env_vars: EnvironmentVarsConfig {
                env: {
                    let mut env = HashMap::new();
                    env.insert("BASE_VAR".to_string(), "base".to_string());
                    env
                },
                ..Default::default()
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

        assert_eq!(
            resolved.env_vars.env.get("BASE_VAR"),
            Some(&"base".to_string())
        );
        assert_eq!(resolved.env_vars.env.get("CI"), Some(&"true".to_string()));
    }

    #[test]
    fn apply_environment_overrides_removes_env_var() {
        let config = StepConfig {
            execution: ExecutionConfig {
                command: Some("echo test".to_string()),
                ..Default::default()
            },
            env_vars: EnvironmentVarsConfig {
                env: {
                    let mut env = HashMap::new();
                    env.insert("DEBUG".to_string(), "true".to_string());
                    env.insert("KEEP".to_string(), "yes".to_string());
                    env
                },
                ..Default::default()
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

        assert!(!resolved.env_vars.env.contains_key("DEBUG"));
        assert_eq!(resolved.env_vars.env.get("KEEP"), Some(&"yes".to_string()));
    }

    #[test]
    fn apply_environment_overrides_ignores_none_fields() {
        let config = StepConfig {
            title: Some("Original".to_string()),
            execution: ExecutionConfig {
                command: Some("echo original".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut resolved = ResolvedStep::from_config("test", &config, None);

        let overrides = StepEnvironmentOverride::default();
        resolved.apply_environment_overrides(&overrides);

        assert_eq!(resolved.title, "Original");
        assert_eq!(resolved.execution.command, "echo original");
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
            execution: ExecutionConfig {
                command: Some("echo dev-mode".to_string()),
                ..Default::default()
            },
            scoping: EnvironmentScopingConfig {
                environments,
                ..Default::default()
            },
            ..Default::default()
        };

        let resolved = ResolvedStep::from_config("test", &config, Some("ci"));
        assert_eq!(resolved.execution.command, "echo ci-mode");
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
            execution: ExecutionConfig {
                command: Some("echo dev-mode".to_string()),
                ..Default::default()
            },
            scoping: EnvironmentScopingConfig {
                environments,
                ..Default::default()
            },
            ..Default::default()
        };

        let resolved = ResolvedStep::from_config("test", &config, Some("staging"));
        assert_eq!(resolved.execution.command, "echo dev-mode");
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
            execution: ExecutionConfig {
                command: Some("echo dev-mode".to_string()),
                ..Default::default()
            },
            scoping: EnvironmentScopingConfig {
                environments,
                ..Default::default()
            },
            ..Default::default()
        };

        let resolved = ResolvedStep::from_config("test", &config, None);
        assert_eq!(resolved.execution.command, "echo dev-mode");
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
            scoping: EnvironmentScopingConfig {
                environments,
                ..Default::default()
            },
            ..Default::default()
        };

        let resolved =
            ResolvedStep::from_template("test", &template, &config, &HashMap::new(), Some("ci"));
        assert_eq!(resolved.execution.command, "echo ci-command");
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
            scoping: EnvironmentScopingConfig {
                environments,
                ..Default::default()
            },
            ..Default::default()
        };

        let resolved =
            ResolvedStep::from_template("test", &template, &config, &HashMap::new(), None);
        assert_eq!(resolved.execution.command, "template command");
    }

    #[test]
    fn from_config_propagates_only_environments() {
        let config = StepConfig {
            execution: ExecutionConfig {
                command: Some("echo test".to_string()),
                ..Default::default()
            },
            scoping: EnvironmentScopingConfig {
                only_environments: vec!["ci".to_string(), "staging".to_string()],
                ..Default::default()
            },
            ..Default::default()
        };

        let resolved = ResolvedStep::from_config("test", &config, None);
        assert_eq!(resolved.scoping.only_environments, vec!["ci", "staging"]);
    }

    #[test]
    fn from_config_empty_only_environments() {
        let config = StepConfig::default();
        let resolved = ResolvedStep::from_config("test", &config, None);
        assert!(resolved.scoping.only_environments.is_empty());
    }

    #[test]
    fn from_template_propagates_only_environments() {
        let template = make_template();
        let config = StepConfig {
            scoping: EnvironmentScopingConfig {
                only_environments: vec!["ci".to_string()],
                ..Default::default()
            },
            ..Default::default()
        };

        let resolved =
            ResolvedStep::from_template("test", &template, &config, &HashMap::new(), None);
        assert_eq!(resolved.scoping.only_environments, vec!["ci"]);
    }

    // --- 7B: Resolution override tests ---

    #[test]
    fn resolved_step_env_overrides_title() {
        let config = StepConfig {
            title: Some("Base Title".to_string()),
            execution: ExecutionConfig {
                command: Some("echo test".to_string()),
                ..Default::default()
            },
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
            execution: ExecutionConfig {
                command: Some("echo test".to_string()),
                ..Default::default()
            },
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
            execution: ExecutionConfig {
                command: Some("echo test".to_string()),
                ..Default::default()
            },
            behavior: BehaviorConfig {
                skippable: false,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut resolved = ResolvedStep::from_config("test", &config, None);
        assert!(!resolved.behavior.skippable);

        let overrides = StepEnvironmentOverride {
            skippable: Some(true),
            ..Default::default()
        };
        resolved.apply_environment_overrides(&overrides);

        assert!(resolved.behavior.skippable);
    }

    #[test]
    fn from_config_auto_run_none_defaults_true() {
        let config = StepConfig {
            execution: ExecutionConfig {
                command: Some("echo test".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };
        let resolved = ResolvedStep::from_config("test", &config, None);
        assert!(resolved.behavior.auto_run);
    }

    #[test]
    fn from_config_auto_run_explicit_false() {
        let config = StepConfig {
            execution: ExecutionConfig {
                command: Some("echo test".to_string()),
                ..Default::default()
            },
            behavior: BehaviorConfig {
                auto_run: Some(false),
                ..Default::default()
            },
            ..Default::default()
        };
        let resolved = ResolvedStep::from_config("test", &config, None);
        assert!(!resolved.behavior.auto_run);
    }

    #[test]
    fn resolved_step_env_overrides_auto_run() {
        let config = StepConfig {
            execution: ExecutionConfig {
                command: Some("echo test".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut resolved = ResolvedStep::from_config("test", &config, None);
        assert!(resolved.behavior.auto_run);

        let overrides = StepEnvironmentOverride {
            auto_run: Some(false),
            ..Default::default()
        };
        resolved.apply_environment_overrides(&overrides);

        assert!(!resolved.behavior.auto_run);
    }

    #[test]
    fn resolved_step_env_overrides_check() {
        let config = StepConfig {
            execution: ExecutionConfig {
                command: Some("echo test".to_string()),
                check: Some(Check::Presence {
                    name: None,
                    target: Some("base.txt".to_string()),
                    kind: Some(crate::checks::PresenceKind::File),
                    command: None,
                }),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut resolved = ResolvedStep::from_config("test", &config, None);

        let overrides = StepEnvironmentOverride {
            check: Some(Check::Execution {
                name: None,
                command: "true".to_string(),
                validation: crate::checks::ValidationMode::Success,
            }),
            ..Default::default()
        };
        resolved.apply_environment_overrides(&overrides);

        assert!(matches!(
            resolved.execution.check,
            Some(Check::Execution { .. })
        ));
    }

    #[test]
    fn resolved_step_env_overrides_depends_on() {
        let config = StepConfig {
            execution: ExecutionConfig {
                command: Some("echo test".to_string()),
                ..Default::default()
            },
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
    fn resolved_step_env_overrides_retry() {
        let config = StepConfig {
            execution: ExecutionConfig {
                command: Some("echo test".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut resolved = ResolvedStep::from_config("test", &config, None);
        assert_eq!(resolved.execution.retry, 0);

        let overrides = StepEnvironmentOverride {
            retry: Some(3),
            ..Default::default()
        };
        resolved.apply_environment_overrides(&overrides);

        assert_eq!(resolved.execution.retry, 3);
    }

    #[test]
    fn resolved_step_env_overrides_existing_env_var() {
        let config = StepConfig {
            execution: ExecutionConfig {
                command: Some("echo test".to_string()),
                ..Default::default()
            },
            env_vars: EnvironmentVarsConfig {
                env: {
                    let mut env = HashMap::new();
                    env.insert("RAILS_ENV".to_string(), "development".to_string());
                    env
                },
                ..Default::default()
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

        assert_eq!(
            resolved.env_vars.env.get("RAILS_ENV"),
            Some(&"test".to_string())
        );
    }

    // --- Template input resolution tests ---

    #[test]
    fn from_template_resolves_provided_inputs() {
        use crate::registry::template::{InputType, TemplateInput};

        let mut template = make_template();
        template.inputs.insert(
            "bump".to_string(),
            TemplateInput {
                description: "Bump type".to_string(),
                input_type: InputType::String,
                required: true,
                default: None,
                values: vec![],
                prompt: None,
            },
        );

        let mut inputs = HashMap::new();
        inputs.insert(
            "bump".to_string(),
            serde_yaml::Value::String("minor".to_string()),
        );

        let config = StepConfig::default();
        let resolved = ResolvedStep::from_template("test", &template, &config, &inputs, None);

        assert_eq!(resolved.inputs.get("bump"), Some(&"minor".to_string()));
    }

    #[test]
    fn from_template_uses_default_when_input_not_provided() {
        use crate::registry::template::{InputType, TemplateInput};

        let mut template = make_template();
        template.inputs.insert(
            "bump".to_string(),
            TemplateInput {
                description: "Bump type".to_string(),
                input_type: InputType::String,
                required: false,
                default: Some(serde_yaml::Value::String("patch".to_string())),
                values: vec![],
                prompt: None,
            },
        );

        let config = StepConfig::default();
        let resolved =
            ResolvedStep::from_template("test", &template, &config, &HashMap::new(), None);

        assert_eq!(resolved.inputs.get("bump"), Some(&"patch".to_string()));
    }

    #[test]
    fn from_template_no_input_when_not_provided_and_no_default() {
        use crate::registry::template::{InputType, TemplateInput};

        let mut template = make_template();
        template.inputs.insert(
            "opt".to_string(),
            TemplateInput {
                description: "Optional".to_string(),
                input_type: InputType::String,
                required: false,
                default: None,
                values: vec![],
                prompt: None,
            },
        );

        let config = StepConfig::default();
        let resolved =
            ResolvedStep::from_template("test", &template, &config, &HashMap::new(), None);

        assert!(!resolved.inputs.contains_key("opt"));
    }

    #[test]
    fn from_config_has_empty_inputs() {
        let config = StepConfig::default();
        let resolved = ResolvedStep::from_config("test", &config, None);
        assert!(resolved.inputs.is_empty());
    }

    #[test]
    fn from_template_carries_prompts() {
        use crate::config::schema::PromptType;

        let template = make_template();
        let config = StepConfig {
            output_settings: StepOutputSettings {
                prompts: vec![PromptConfig {
                    key: "bump".to_string(),
                    question: "Bump type?".to_string(),
                    prompt_type: PromptType::Input,
                    options: vec![],
                    default: None,
                }],
                ..Default::default()
            },
            ..Default::default()
        };

        let resolved =
            ResolvedStep::from_template("test", &template, &config, &HashMap::new(), None);
        assert_eq!(resolved.output.prompts.len(), 1);
        assert_eq!(resolved.output.prompts[0].key, "bump");
    }

    #[test]
    fn from_config_carries_prompts() {
        use crate::config::schema::PromptType;

        let config = StepConfig {
            execution: ExecutionConfig {
                command: Some("echo test".to_string()),
                ..Default::default()
            },
            output_settings: StepOutputSettings {
                prompts: vec![PromptConfig {
                    key: "name".to_string(),
                    question: "Name?".to_string(),
                    prompt_type: PromptType::Input,
                    options: vec![],
                    default: None,
                }],
                ..Default::default()
            },
            ..Default::default()
        };

        let resolved = ResolvedStep::from_config("test", &config, None);
        assert_eq!(resolved.output.prompts.len(), 1);
        assert_eq!(resolved.output.prompts[0].key, "name");
    }

    // --- satisfied_when propagation tests ---

    #[test]
    fn from_config_carries_satisfied_when() {
        use crate::checks::{Check, PresenceKind, SatisfactionCondition};

        let config = StepConfig {
            execution: ExecutionConfig {
                command: Some("echo test".to_string()),
                ..Default::default()
            },
            satisfied_when: vec![SatisfactionCondition::Check(Check::Presence {
                name: None,
                target: Some("node_modules".to_string()),
                kind: Some(PresenceKind::File),
                command: None,
            })],
            ..Default::default()
        };

        let resolved = ResolvedStep::from_config("install_deps", &config, None);
        assert_eq!(resolved.satisfied_when.len(), 1);
    }

    #[test]
    fn from_config_empty_satisfied_when_by_default() {
        let config = StepConfig::default();
        let resolved = ResolvedStep::from_config("test", &config, None);
        assert!(resolved.satisfied_when.is_empty());
    }

    #[test]
    fn from_template_carries_satisfied_when() {
        use crate::checks::SatisfactionCondition;

        let template = make_template();
        let config = StepConfig {
            satisfied_when: vec![SatisfactionCondition::Ref {
                check_ref: "deps_installed".to_string(),
            }],
            ..Default::default()
        };

        let resolved =
            ResolvedStep::from_template("test", &template, &config, &HashMap::new(), None);
        assert_eq!(resolved.satisfied_when.len(), 1);
    }
}
