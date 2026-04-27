//! Configuration schema definitions for Bivvy.
//!
//! This module contains all the struct definitions that map to
//! the YAML configuration file format.

use crate::checks::{Check, SatisfactionCondition};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Root configuration structure for bivvy.yml
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BivvyConfig {
    /// Application name (for display purposes)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_name: Option<String>,

    /// Global settings
    pub settings: Settings,

    /// Remote template sources
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub template_sources: Vec<TemplateSource>,

    /// Step definitions
    #[serde(default)]
    pub steps: HashMap<String, StepConfig>,

    /// Workflow definitions
    #[serde(default)]
    pub workflows: HashMap<String, WorkflowConfig>,

    /// Secrets configuration
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub secrets: HashMap<String, SecretConfig>,

    /// Config inheritance
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extends: Option<Vec<ExtendsConfig>>,

    /// Custom requirement definitions
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub requirements: HashMap<String, CustomRequirement>,

    /// User-defined variables for interpolation.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub vars: HashMap<String, VarDefinition>,
}

/// Output-related settings (verbosity, logging).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputSettings {
    /// Default output mode: verbose, quiet, silent
    #[serde(default = "default_output")]
    pub default_output: OutputMode,

    /// Enable logging to file
    #[serde(default, skip_serializing_if = "is_false")]
    pub logging: bool,

    /// Log file path (relative to project root)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_path: Option<PathBuf>,
}

impl Default for OutputSettings {
    fn default() -> Self {
        Self {
            default_output: default_output(),
            logging: false,
            log_path: None,
        }
    }
}

/// Execution-related settings (parallelism, history, updates).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ExecutionSettings {
    /// Enable parallel execution
    #[serde(default, skip_serializing_if = "is_false")]
    pub parallel: bool,

    /// Maximum concurrent steps
    #[serde(
        default = "default_max_parallel",
        skip_serializing_if = "is_default_max_parallel"
    )]
    pub max_parallel: usize,

    /// History retention count
    #[serde(
        default = "default_history_retention",
        skip_serializing_if = "is_default_history_retention"
    )]
    pub history_retention: usize,

    /// Enable automatic background updates.
    ///
    /// When true, bivvy checks for new versions in the background after each
    /// run and automatically installs updates so the next invocation uses the
    /// latest version. Set to false to disable (users can still run `bivvy update`).
    #[serde(default = "default_auto_update")]
    pub auto_update: bool,
}

impl Default for ExecutionSettings {
    fn default() -> Self {
        Self {
            parallel: false,
            max_parallel: default_max_parallel(),
            history_retention: default_history_retention(),
            auto_update: default_auto_update(),
        }
    }
}

/// Environment variable settings (global env, env files, secret patterns).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct EnvVarSettings {
    /// Global environment variables
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,

    /// Global env file to load
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env_file: Option<PathBuf>,

    /// Additional secret patterns to mask
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secret_env: Vec<String>,
}

/// Environment profile settings (named environments, default environment).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct EnvironmentProfileSettings {
    /// Default environment name (used when no --env flag and no auto-detection)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_environment: Option<String>,

    /// Named environment configurations
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub environments: HashMap<String, EnvironmentConfig>,
}

/// Global settings that apply to all workflows and steps.
///
/// Uses `#[serde(flatten)]` on sub-structs so the YAML surface stays flat
/// (e.g., `settings.default_output` in YAML, not `settings.output.default_output`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Output-related settings (verbosity, logging)
    #[serde(flatten)]
    pub output: OutputSettings,

    /// Execution-related settings (parallelism, history, updates)
    #[serde(flatten)]
    pub execution: ExecutionSettings,

    /// Environment variable settings (global env, env files, secrets)
    #[serde(flatten)]
    pub env_vars: EnvVarSettings,

    /// Environment profile settings (named environments, default)
    #[serde(flatten)]
    pub environment_profiles: EnvironmentProfileSettings,
}

fn default_output() -> OutputMode {
    OutputMode::Verbose
}

fn default_max_parallel() -> usize {
    4
}

fn is_default_max_parallel(v: &usize) -> bool {
    *v == default_max_parallel()
}

fn default_history_retention() -> usize {
    50
}

fn is_default_history_retention(v: &usize) -> bool {
    *v == default_history_retention()
}

fn default_auto_update() -> bool {
    true
}

fn is_false(v: &bool) -> bool {
    !v
}

fn is_true(v: &bool) -> bool {
    *v
}

fn is_zero(v: &u32) -> bool {
    *v == 0
}

/// Output verbosity mode
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputMode {
    #[default]
    Verbose,
    Quiet,
    Silent,
}

/// Fields related to what the step actually runs and how.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ExecutionConfig {
    /// Shell command to execute
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,

    /// Single check (new `Check` enum).
    /// Mutually exclusive with `checks`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check: Option<Check>,

    /// Multiple checks (implicit `all`).
    /// Mutually exclusive with `check`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<Check>,

    /// Precondition that must pass before the step runs.
    /// Unlike `check` (which skips when passing), a precondition
    /// *fails* the step when it does not pass. `--force` does not bypass preconditions.
    /// Uses the same check types as `check`/`checks` (presence, execution, change, combinators).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub precondition: Option<Check>,

    /// Number of retries on failure
    #[serde(default, skip_serializing_if = "is_zero")]
    pub retry: u32,

    /// Step requires sudo/elevated permissions
    #[serde(default, skip_serializing_if = "is_false")]
    pub requires_sudo: bool,
}

/// Fields related to environment variable management.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct EnvironmentVarsConfig {
    /// Step-specific environment variables
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,

    /// Env file to load for this step
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env_file: Option<PathBuf>,

    /// Don't fail if env_file is missing
    #[serde(default, skip_serializing_if = "is_false")]
    pub env_file_optional: bool,

    /// Required environment variables (fail if missing)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_env: Vec<String>,
}

/// Fields controlling step lifecycle behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BehaviorConfig {
    /// Whether user can skip this step
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub skippable: bool,

    /// Step must run, cannot be skipped
    #[serde(default, skip_serializing_if = "is_false")]
    pub required: bool,

    /// Ask before re-running completed steps.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub prompt_on_rerun: bool,

    /// Continue workflow if this step fails
    #[serde(default, skip_serializing_if = "is_false")]
    pub allow_failure: bool,

    /// Mark step as handling sensitive data
    #[serde(default, skip_serializing_if = "is_false")]
    pub sensitive: bool,
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        Self {
            skippable: true,
            required: false,
            prompt_on_rerun: true,
            allow_failure: false,
            sensitive: false,
        }
    }
}

/// Before/after lifecycle hooks.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct HookConfig {
    /// Commands to run before the step
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub before: Vec<String>,

    /// Commands to run after the step succeeds
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<String>,
}

/// Step-specific output and prompt settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct StepOutputSettings {
    /// Step output settings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<StepOutputConfig>,

    /// Interactive prompts within this step
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prompts: Vec<PromptConfig>,
}

/// Environment-scoping and override settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct EnvironmentScopingConfig {
    /// Per-environment overrides for this step.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub environments: HashMap<String, StepEnvironmentOverride>,

    /// Restrict this step to specific environments.
    /// Empty list (default) means "run in all environments".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub only_environments: Vec<String>,
}

/// Configuration for a single setup step
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct StepConfig {
    /// Reference to a template (mutually exclusive with inline config)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,

    /// Template inputs (when using template)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub inputs: HashMap<String, serde_yaml::Value>,

    /// Step title (for display)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Step description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Steps that must complete before this one
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,

    /// System-level prerequisites this step requires (e.g., ruby, node, postgres-server).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,

    /// Declarative satisfaction conditions.
    /// If all conditions pass, the step's purpose is already fulfilled.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub satisfied_when: Vec<SatisfactionCondition>,

    /// Execution settings (command, checks, retry, sudo)
    #[serde(flatten)]
    pub execution: ExecutionConfig,

    /// Environment variable settings (env, env_file, required_env)
    #[serde(flatten)]
    pub env_vars: EnvironmentVarsConfig,

    /// Behavior settings (skippable, required, prompt_on_rerun, allow_failure, sensitive)
    #[serde(flatten)]
    pub behavior: BehaviorConfig,

    /// Lifecycle hooks (before, after)
    #[serde(flatten)]
    pub hooks: HookConfig,

    /// Output and prompt settings
    #[serde(flatten)]
    pub output_settings: StepOutputSettings,

    /// Environment scoping and overrides
    #[serde(flatten)]
    pub scoping: EnvironmentScopingConfig,
}

fn default_true() -> bool {
    true
}

/// Prompt configuration for interactive input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptConfig {
    /// Unique key for this prompt (used in interpolation)
    pub key: String,

    /// Question to display
    pub question: String,

    /// Prompt type: select, multiselect, confirm, input
    #[serde(rename = "type")]
    pub prompt_type: PromptType,

    /// Options for select/multiselect
    #[serde(default)]
    pub options: Vec<PromptOption>,

    /// Default value
    pub default: Option<serde_yaml::Value>,
}

/// Type of interactive prompt
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PromptType {
    Select,
    Multiselect,
    Confirm,
    Input,
}

/// Option for select/multiselect prompts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptOption {
    /// Display label
    pub label: String,
    /// Value to use when selected
    pub value: String,
}

/// Step-specific output configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepOutputConfig {
    /// Output mode for this step
    pub default: Option<OutputMode>,

    /// Enable logging for this step
    pub logging: Option<bool>,
}

/// Configuration for a named workflow
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkflowConfig {
    /// Human-readable description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Ordered list of step names to execute
    #[serde(default)]
    pub steps: Vec<String>,

    /// Step-specific overrides for this workflow
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub overrides: HashMap<String, StepOverride>,

    /// Workflow-level settings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<WorkflowSettings>,

    /// Workflow-level environment variables
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,

    /// Workflow-level env file
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env_file: Option<PathBuf>,
}

/// Per-step overrides within a workflow
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct StepOverride {
    /// Skip prompts, just run
    #[serde(default, skip_serializing_if = "is_false")]
    pub skip_prompt: bool,

    /// Override required flag
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,

    /// Override prompt_on_rerun flag.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_on_rerun: Option<bool>,
}

/// Workflow-level settings overrides
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkflowSettings {
    /// Force non-interactive mode for this workflow
    #[serde(default, skip_serializing_if = "is_false")]
    pub non_interactive: bool,
}

/// Remote template source configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateSource {
    /// URL to template repository or file
    pub url: String,

    /// Priority (lower = higher priority)
    #[serde(default = "default_priority")]
    pub priority: u32,

    /// Network timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout: u64,

    /// Cache configuration
    pub cache: Option<CacheConfig>,

    /// Authentication configuration
    pub auth: Option<AuthConfig>,
}

fn default_priority() -> u32 {
    100
}

fn default_timeout() -> u64 {
    30
}

/// Cache configuration for remote templates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Time-to-live (e.g., "7d", "24h")
    pub ttl: String,

    /// Cache strategy: etag, git
    #[serde(default)]
    pub strategy: CacheStrategy,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CacheStrategy {
    #[default]
    Etag,
    Git,
}

/// Authentication for remote template sources
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Auth type: bearer, header
    #[serde(rename = "type")]
    pub auth_type: AuthType,

    /// Environment variable containing the token
    pub token_env: String,

    /// Custom header name (for header auth type)
    pub header: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthType {
    Bearer,
    Header,
}

/// Config inheritance source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtendsConfig {
    /// URL to base config
    pub url: String,
}

/// Secret configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretConfig {
    /// Command to fetch the secret
    pub command: String,
}

/// A variable definition: either a static value or a shell-computed value.
///
/// In YAML, static values are plain strings and computed values use
/// `{ command: "..." }` syntax:
///
/// ```yaml
/// vars:
///   app_name: "bivvy"                       # static
///   version:
///     command: "cat VERSION"                 # computed
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VarDefinition {
    /// Computed from a shell command's stdout (trimmed).
    Computed {
        /// Shell command to run
        command: String,
    },
    /// A static string value.
    Static(String),
}

/// A project-specific requirement definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomRequirement {
    /// How to check if this requirement is satisfied
    pub check: CustomRequirementCheck,

    /// Template to use for installation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_template: Option<String>,

    /// Human-readable install instructions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_hint: Option<String>,
}

/// Check type for a custom requirement.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CustomRequirementCheck {
    /// Check if a command succeeds (exit code 0)
    CommandSucceeds {
        /// Command to run
        command: String,
    },

    /// Check if a file or directory exists
    FileExists {
        /// Path to check
        path: String,
    },

    /// Check if a service is reachable
    ServiceReachable {
        /// Command to check reachability (e.g., curl health endpoint)
        command: String,
    },
}

/// Configuration for a named environment.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct EnvironmentConfig {
    /// Rules for auto-detecting this environment.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub detect: Vec<EnvironmentDetectRule>,

    /// Default workflow to run in this environment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_workflow: Option<String>,

    /// Requirements that are assumed to be satisfied in this environment.
    /// These skip gap detection checks when this environment is active.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provided_requirements: Vec<String>,
}

/// A rule for auto-detecting an environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentDetectRule {
    /// Environment variable name to check.
    pub env: String,

    /// If set, the variable must equal this value. If absent, just checks presence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

/// Per-environment overrides for a step.
///
/// All fields are `Option` — only specified fields override the base step.
/// The `env` field uses `HashMap<String, Option<String>>`:
/// `Some(val)` = set/override, `None` = remove the key from the base env.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct StepEnvironmentOverride {
    /// Override step title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Override step description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Override step command
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,

    /// Override/remove env vars (None value = remove key)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, Option<String>>,

    /// Override check
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check: Option<Check>,

    /// Override precondition
    #[serde(skip_serializing_if = "Option::is_none")]
    pub precondition: Option<Check>,

    /// Override skippable
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skippable: Option<bool>,

    /// Override allow_failure
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_failure: Option<bool>,

    /// Override requires_sudo
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_sudo: Option<bool>,

    /// Override sensitive
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sensitive: Option<bool>,

    /// Override before hooks
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<Vec<String>>,

    /// Override after hooks
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<Vec<String>>,

    /// Override dependencies
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depends_on: Option<Vec<String>>,

    /// Override requirements
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires: Option<Vec<String>>,

    /// Override retry count
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_has_defaults() {
        let config: BivvyConfig = serde_yaml::from_str("").unwrap();
        assert_eq!(config.settings.output.default_output, OutputMode::Verbose);
        assert_eq!(config.settings.execution.max_parallel, 4);
        assert_eq!(config.settings.execution.history_retention, 50);
        assert!(config.steps.is_empty());
        assert!(config.workflows.is_empty());
    }

    #[test]
    fn parses_minimal_config() {
        let yaml = r#"
app_name: "MyApp"
settings:
  default_output: quiet
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.app_name, Some("MyApp".to_string()));
        assert_eq!(config.settings.output.default_output, OutputMode::Quiet);
    }

    #[test]
    fn parses_template_sources() {
        let yaml = r#"
template_sources:
  - url: "https://example.com/templates"
    priority: 10
    timeout: 60
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.template_sources.len(), 1);
        assert_eq!(config.template_sources[0].priority, 10);
        assert_eq!(config.template_sources[0].timeout, 60);
    }

    #[test]
    fn parses_settings_with_env() {
        let yaml = r#"
settings:
  logging: true
  log_path: "logs/bivvy.log"
  env:
    RAILS_ENV: development
    DEBUG: "true"
  parallel: true
  max_parallel: 8
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.settings.output.logging);
        assert_eq!(
            config.settings.output.log_path,
            Some(PathBuf::from("logs/bivvy.log"))
        );
        assert_eq!(
            config.settings.env_vars.env.get("RAILS_ENV"),
            Some(&"development".to_string())
        );
        assert!(config.settings.execution.parallel);
        assert_eq!(config.settings.execution.max_parallel, 8);
    }

    #[test]
    fn parses_extends_config() {
        let yaml = r#"
extends:
  - url: "https://example.com/base-config.yml"
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.extends.is_some());
        assert_eq!(config.extends.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn parses_secrets_config() {
        let yaml = r#"
secrets:
  db_password:
    command: "op read op://vault/database/password"
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.secrets.contains_key("db_password"));
    }

    #[test]
    fn parses_step_with_template() {
        let yaml = r#"
steps:
  node_deps:
    template: yarn-install
    inputs:
      frozen: true
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        let step = &config.steps["node_deps"];
        assert_eq!(step.template, Some("yarn-install".to_string()));
    }

    #[test]
    fn parses_step_with_inline_config() {
        let yaml = r#"
steps:
  custom:
    title: "Custom Step"
    command: "echo hello"
    depends_on: [other]
    env:
      MY_VAR: "value"
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        let step = &config.steps["custom"];
        assert_eq!(step.title, Some("Custom Step".to_string()));
        assert_eq!(step.execution.command, Some("echo hello".to_string()));
        assert_eq!(step.depends_on, vec!["other"]);
    }

    #[test]
    fn step_defaults_are_correct() {
        let yaml = r#"
steps:
  minimal:
    command: "echo test"
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        let step = &config.steps["minimal"];
        assert!(step.behavior.skippable);
        assert!(!step.behavior.required);
        assert!(step.behavior.prompt_on_rerun);
        assert!(!step.behavior.allow_failure);
        assert_eq!(step.execution.retry, 0);
    }

    #[test]
    fn parses_step_with_prompts() {
        let yaml = r#"
steps:
  interactive:
    command: "yarn ${install_mode}"
    prompts:
      - key: install_mode
        question: "How to install?"
        type: select
        options:
          - label: "Normal"
            value: "install"
          - label: "Frozen"
            value: "install --frozen-lockfile"
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        let step = &config.steps["interactive"];
        assert_eq!(step.output_settings.prompts.len(), 1);
        assert_eq!(step.output_settings.prompts[0].key, "install_mode");
        assert_eq!(step.output_settings.prompts[0].options.len(), 2);
    }

    #[test]
    fn parses_step_with_hooks() {
        let yaml = r#"
steps:
  database:
    command: "rails db:setup"
    before:
      - echo "Starting..."
    after:
      - echo "Done!"
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        let step = &config.steps["database"];
        assert_eq!(step.hooks.before.len(), 1);
        assert_eq!(step.hooks.after.len(), 1);
    }

    #[test]
    fn step_config_defaults_to_empty_hooks() {
        let yaml = r#"
            command: "echo hello"
        "#;
        let config: StepConfig = serde_yaml::from_str(yaml).unwrap();

        assert!(config.hooks.before.is_empty());
        assert!(config.hooks.after.is_empty());
    }

    #[test]
    fn step_config_parses_before_hooks() {
        let yaml = r#"
            command: "echo main"
            before:
              - "echo pre-1"
              - "echo pre-2"
        "#;
        let config: StepConfig = serde_yaml::from_str(yaml).unwrap();

        assert_eq!(config.hooks.before.len(), 2);
        assert_eq!(config.hooks.before[0], "echo pre-1");
    }

    #[test]
    fn step_config_parses_after_hooks() {
        let yaml = r#"
            command: "echo main"
            after:
              - "echo post-1"
        "#;
        let config: StepConfig = serde_yaml::from_str(yaml).unwrap();

        assert_eq!(config.hooks.after.len(), 1);
    }

    #[test]
    fn step_config_parses_both_hooks() {
        let yaml = r#"
            command: "bin/rails db:setup"
            before:
              - "echo Starting database setup..."
              - "./scripts/pre-db-check.sh"
            after:
              - "echo Database setup complete"
        "#;
        let config: StepConfig = serde_yaml::from_str(yaml).unwrap();

        assert_eq!(config.hooks.before.len(), 2);
        assert_eq!(config.hooks.after.len(), 1);
    }

    #[test]
    fn parses_workflow_definition() {
        let yaml = r#"
workflows:
  default:
    description: "Full development setup"
    steps: [brew, deps, database]
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        let workflow = &config.workflows["default"];
        assert_eq!(
            workflow.description,
            Some("Full development setup".to_string())
        );
        assert_eq!(workflow.steps, vec!["brew", "deps", "database"]);
    }

    #[test]
    fn parses_workflow_with_overrides() {
        let yaml = r#"
workflows:
  onboarding:
    steps: [deps, database, seeds]
    overrides:
      seeds:
        required: true
        skip_prompt: true
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        let workflow = &config.workflows["onboarding"];
        let override_ = &workflow.overrides["seeds"];
        assert_eq!(override_.required, Some(true));
        assert!(override_.skip_prompt);
    }

    #[test]
    fn parses_workflow_with_env() {
        let yaml = r#"
workflows:
  ci:
    steps: [deps, test]
    env:
      CI: "true"
      RAILS_ENV: test
    settings:
      non_interactive: true
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        let workflow = &config.workflows["ci"];
        assert_eq!(workflow.env.get("CI"), Some(&"true".to_string()));
        assert!(workflow.settings.as_ref().unwrap().non_interactive);
    }

    #[test]
    fn parses_multiple_workflows() {
        let yaml = r#"
workflows:
  default:
    steps: [deps, database]
  ci:
    steps: [deps, test]
  reset:
    description: "Clean slate"
    steps: [clean, deps, database, seeds]
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.workflows.len(), 3);
        assert!(config.workflows.contains_key("default"));
        assert!(config.workflows.contains_key("ci"));
        assert!(config.workflows.contains_key("reset"));
    }

    #[test]
    fn parses_presence_check() {
        let yaml = r#"
steps:
  deps:
    command: "yarn install"
    check:
      type: presence
      target: "node_modules"
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        let check = config.steps["deps"].execution.check.as_ref().unwrap();
        assert!(matches!(check, Check::Presence { target: Some(t), .. } if t == "node_modules"));
    }

    #[test]
    fn parses_execution_check() {
        let yaml = r#"
steps:
  deps:
    command: "bundle install"
    check:
      type: execution
      command: "bundle check"
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        let check = config.steps["deps"].execution.check.as_ref().unwrap();
        assert!(matches!(check, Check::Execution { command, .. } if command == "bundle check"));
    }

    #[test]
    fn parses_all_check() {
        let yaml = r#"
steps:
  deps:
    command: "yarn install"
    check:
      type: all
      checks:
        - type: presence
          target: "node_modules"
        - type: execution
          command: "yarn check"
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        let check = config.steps["deps"].execution.check.as_ref().unwrap();
        if let Check::All { checks, .. } = check {
            assert_eq!(checks.len(), 2);
        } else {
            panic!("Expected All check");
        }
    }

    #[test]
    fn parses_any_check() {
        let yaml = r#"
steps:
  env:
    command: "cp .env.example .env"
    check:
      type: any
      checks:
        - type: presence
          target: ".env"
        - type: presence
          target: ".env.local"
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        let check = config.steps["env"].execution.check.as_ref().unwrap();
        assert!(matches!(check, Check::Any { .. }));
    }

    #[test]
    fn serialize_omits_default_values() {
        let config: BivvyConfig = serde_yaml::from_str(
            r#"
app_name: "TestApp"
steps:
  hello:
    command: "echo hello"
workflows:
  default:
    steps: [hello]
"#,
        )
        .unwrap();

        let yaml = serde_yaml::to_string(&config).unwrap();

        // Should include explicitly set fields
        assert!(yaml.contains("app_name"));
        assert!(yaml.contains("command"));

        // Should NOT include default/empty fields
        assert!(
            !yaml.contains("template_sources"),
            "empty template_sources should be omitted"
        );
        assert!(!yaml.contains("secrets"), "empty secrets should be omitted");
        assert!(
            !yaml.contains("requirements"),
            "empty requirements should be omitted"
        );
        assert!(!yaml.contains("extends"), "None extends should be omitted");
        assert!(
            !yaml.contains("log_path"),
            "None log_path should be omitted"
        );
        assert!(!yaml.contains("logging"), "false logging should be omitted");
        assert!(
            !yaml.contains("parallel"),
            "false parallel should be omitted"
        );
        assert!(
            !yaml.contains("max_parallel"),
            "default max_parallel should be omitted"
        );
        assert!(
            !yaml.contains("history_retention"),
            "default history_retention should be omitted"
        );
        assert!(
            !yaml.contains("secret_env"),
            "empty secret_env should be omitted"
        );
        assert!(
            !yaml.contains("depends_on"),
            "empty depends_on should be omitted"
        );
        assert!(
            !yaml.contains("skippable"),
            "default true skippable should be omitted"
        );
        assert!(
            !yaml.contains("prompt_on_rerun"),
            "default true prompt_on_rerun should be omitted"
        );
        assert!(
            !yaml.contains("required"),
            "false required should be omitted"
        );
        assert!(
            !yaml.contains("allow_failure"),
            "false allow_failure should be omitted"
        );
        assert!(!yaml.contains("retry"), "zero retry should be omitted");
        assert!(
            !yaml.contains("sensitive"),
            "false sensitive should be omitted"
        );
        assert!(
            !yaml.contains("requires_sudo"),
            "false requires_sudo should be omitted"
        );
        assert!(!yaml.contains("before"), "empty before should be omitted");
        assert!(!yaml.contains("after"), "empty after should be omitted");
        assert!(!yaml.contains("prompts"), "empty prompts should be omitted");
        assert!(
            !yaml.contains("requires"),
            "empty requires should be omitted"
        );
        assert!(
            !yaml.contains("environments"),
            "empty environments should be omitted"
        );
        assert!(
            !yaml.contains("only_environments"),
            "empty only_environments should be omitted"
        );
        assert!(
            !yaml.contains("default_environment"),
            "None default_environment should be omitted"
        );
        assert!(
            !yaml.contains("overrides"),
            "empty overrides should be omitted"
        );
    }

    #[test]
    fn serialize_includes_non_default_values() {
        let config: BivvyConfig = serde_yaml::from_str(
            r#"
steps:
  test:
    command: "echo test"
    required: true
    skippable: false
    retry: 3
    allow_failure: true
    sensitive: true
    depends_on: [other]
    before: ["echo pre"]
    after: ["echo post"]
settings:
  logging: true
  parallel: true
  max_parallel: 8
"#,
        )
        .unwrap();

        let yaml = serde_yaml::to_string(&config).unwrap();

        assert!(
            yaml.contains("required"),
            "non-default required should be present"
        );
        assert!(
            yaml.contains("skippable"),
            "non-default skippable should be present"
        );
        assert!(yaml.contains("retry"), "non-zero retry should be present");
        assert!(
            yaml.contains("allow_failure"),
            "true allow_failure should be present"
        );
        assert!(
            yaml.contains("sensitive"),
            "true sensitive should be present"
        );
        assert!(
            yaml.contains("depends_on"),
            "non-empty depends_on should be present"
        );
        assert!(
            yaml.contains("before"),
            "non-empty before should be present"
        );
        assert!(yaml.contains("after"), "non-empty after should be present");
        assert!(yaml.contains("logging"), "true logging should be present");
        assert!(yaml.contains("parallel"), "true parallel should be present");
        assert!(
            yaml.contains("max_parallel"),
            "non-default max_parallel should be present"
        );
    }

    #[test]
    fn step_config_requires_defaults_empty() {
        let yaml = r#"
            command: "echo hello"
        "#;
        let config: StepConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.requires.is_empty());
    }

    #[test]
    fn step_config_requires_parses() {
        let yaml = r#"
            command: "bundle install"
            requires:
              - ruby
              - postgres-server
        "#;
        let config: StepConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.requires, vec!["ruby", "postgres-server"]);
    }

    #[test]
    fn custom_requirement_parses() {
        let yaml = r#"
requirements:
  internal-cli:
    check:
      type: command_succeeds
      command: "internal-cli --version"
    install_hint: "Download from https://internal.company.com/cli"
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.requirements.contains_key("internal-cli"));
        let req = &config.requirements["internal-cli"];
        assert!(matches!(
            &req.check,
            CustomRequirementCheck::CommandSucceeds { command } if command == "internal-cli --version"
        ));
        assert_eq!(
            req.install_hint,
            Some("Download from https://internal.company.com/cli".to_string())
        );
        assert!(req.install_template.is_none());
    }

    #[test]
    fn custom_requirement_service_check_parses() {
        let yaml = r#"
requirements:
  minio:
    check:
      type: service_reachable
      command: "curl -sf http://localhost:9000/minio/health/live"
    install_hint: "Run: docker compose up -d minio"
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        let req = &config.requirements["minio"];
        assert!(matches!(
            &req.check,
            CustomRequirementCheck::ServiceReachable { command } if command.contains("curl")
        ));
    }

    #[test]
    fn custom_requirement_with_install_template() {
        let yaml = r#"
requirements:
  libvips:
    check:
      type: command_succeeds
      command: "vips --version"
    install_template: libvips
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        let req = &config.requirements["libvips"];
        assert_eq!(req.install_template, Some("libvips".to_string()));
    }

    #[test]
    fn custom_requirement_file_exists_check() {
        let yaml = r#"
requirements:
  local-config:
    check:
      type: file_exists
      path: "/etc/myapp/config.yml"
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        let req = &config.requirements["local-config"];
        assert!(matches!(
            &req.check,
            CustomRequirementCheck::FileExists { path } if path == "/etc/myapp/config.yml"
        ));
    }

    #[test]
    fn empty_requirements_defaults() {
        let yaml = r#"
app_name: "TestApp"
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.requirements.is_empty());
    }

    #[test]
    fn settings_default_environment_defaults_none() {
        let config: BivvyConfig = serde_yaml::from_str("").unwrap();
        assert!(config
            .settings
            .environment_profiles
            .default_environment
            .is_none());
        assert!(config.settings.environment_profiles.environments.is_empty());
    }

    #[test]
    fn parses_default_environment() {
        let yaml = r#"
settings:
  default_environment: staging
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            config.settings.environment_profiles.default_environment,
            Some("staging".to_string())
        );
    }

    #[test]
    fn parses_environment_config() {
        let yaml = r#"
settings:
  environments:
    ci:
      detect:
        - env: CI
        - env: GITHUB_ACTIONS
      default_workflow: ci
      provided_requirements:
        - docker
        - node
    staging:
      detect:
        - env: DEPLOY_ENV
          value: staging
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.settings.environment_profiles.environments.len(), 2);

        let ci = &config.settings.environment_profiles.environments["ci"];
        assert_eq!(ci.detect.len(), 2);
        assert_eq!(ci.detect[0].env, "CI");
        assert!(ci.detect[0].value.is_none());
        assert_eq!(ci.default_workflow, Some("ci".to_string()));
        assert_eq!(ci.provided_requirements, vec!["docker", "node"]);

        let staging = &config.settings.environment_profiles.environments["staging"];
        assert_eq!(staging.detect.len(), 1);
        assert_eq!(staging.detect[0].env, "DEPLOY_ENV");
        assert_eq!(staging.detect[0].value, Some("staging".to_string()));
        assert!(staging.default_workflow.is_none());
        assert!(staging.provided_requirements.is_empty());
    }

    #[test]
    fn parses_step_environments_override() {
        let yaml = r#"
steps:
  database:
    command: "rails db:setup"
    environments:
      ci:
        command: "rails db:schema:load"
        env:
          RAILS_ENV: test
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        let step = &config.steps["database"];
        assert_eq!(step.scoping.environments.len(), 1);

        let ci_override = &step.scoping.environments["ci"];
        assert_eq!(
            ci_override.command,
            Some("rails db:schema:load".to_string())
        );
        assert_eq!(
            ci_override.env.get("RAILS_ENV"),
            Some(&Some("test".to_string()))
        );
    }

    #[test]
    fn parses_step_only_environments() {
        let yaml = r#"
steps:
  seeds:
    command: "rails db:seed"
    only_environments:
      - development
      - staging
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        let step = &config.steps["seeds"];
        assert_eq!(
            step.scoping.only_environments,
            vec!["development", "staging"]
        );
    }

    #[test]
    fn step_only_environments_defaults_empty() {
        let yaml = r#"
            command: "echo hello"
        "#;
        let config: StepConfig = serde_yaml::from_str(yaml).unwrap();
        // Empty means "run in all environments"
        assert!(config.scoping.only_environments.is_empty());
    }

    #[test]
    fn step_environments_defaults_empty() {
        let yaml = r#"
            command: "echo hello"
        "#;
        let config: StepConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.scoping.environments.is_empty());
    }

    #[test]
    fn environment_config_defaults() {
        let config: EnvironmentConfig = serde_yaml::from_str("").unwrap();
        assert!(config.detect.is_empty());
        assert!(config.default_workflow.is_none());
        assert!(config.provided_requirements.is_empty());
    }

    #[test]
    fn parses_nested_composite_checks() {
        let yaml = r#"
steps:
  complex:
    command: "echo complex"
    check:
      type: all
      checks:
        - type: presence
          target: "required.txt"
        - type: any
          checks:
            - type: presence
              target: "option_a.txt"
            - type: presence
              target: "option_b.txt"
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        let check = config.steps["complex"].execution.check.as_ref().unwrap();
        if let Check::All { checks, .. } = check {
            assert_eq!(checks.len(), 2);
            assert!(matches!(&checks[1], Check::Any { .. }));
        } else {
            panic!("Expected All check");
        }
    }

    // --- 7A: Schema override parsing tests ---

    #[test]
    fn environment_override_depends_on_parses() {
        let yaml = r#"
            depends_on: [a, b]
        "#;
        let o: StepEnvironmentOverride = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(o.depends_on, Some(vec!["a".to_string(), "b".to_string()]));
    }

    #[test]
    fn environment_override_title_parses() {
        let yaml = r#"
            title: "CI Title"
        "#;
        let o: StepEnvironmentOverride = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(o.title, Some("CI Title".to_string()));
    }

    // --- Vars parsing tests ---

    #[test]
    fn parses_static_var() {
        let yaml = r#"
vars:
  name: "bivvy"
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(
            config.vars.get("name"),
            Some(VarDefinition::Static(v)) if v == "bivvy"
        ));
    }

    #[test]
    fn parses_computed_var() {
        let yaml = r#"
vars:
  v:
    command: "echo 1"
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(
            config.vars.get("v"),
            Some(VarDefinition::Computed { command }) if command == "echo 1"
        ));
    }

    #[test]
    fn parses_mixed_vars() {
        let yaml = r#"
vars:
  app_name: "bivvy"
  version:
    command: "echo 1.0.0"
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.vars.len(), 2);
        assert!(matches!(
            config.vars.get("app_name"),
            Some(VarDefinition::Static(_))
        ));
        assert!(matches!(
            config.vars.get("version"),
            Some(VarDefinition::Computed { .. })
        ));
    }

    #[test]
    fn empty_vars_defaults() {
        let config: BivvyConfig = serde_yaml::from_str("").unwrap();
        assert!(config.vars.is_empty());
    }

    #[test]
    fn serialize_omits_empty_vars() {
        let config: BivvyConfig = serde_yaml::from_str("app_name: test").unwrap();
        let yaml = serde_yaml::to_string(&config).unwrap();
        assert!(!yaml.contains("vars"), "empty vars should be omitted");
    }

    // --- Precondition parsing tests ---

    #[test]
    fn parses_precondition_execution() {
        let yaml = r#"
steps:
  release:
    command: "git tag v1"
    precondition:
      type: execution
      command: "test $(git branch --show-current) = main"
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        let step = &config.steps["release"];
        assert!(matches!(
            step.execution.precondition,
            Some(Check::Execution { .. })
        ));
    }

    #[test]
    fn parses_precondition_all() {
        let yaml = r#"
steps:
  release:
    command: "git tag v1"
    precondition:
      type: all
      checks:
        - type: execution
          command: "test $(git branch --show-current) = main"
        - type: execution
          command: "git diff --quiet"
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        let step = &config.steps["release"];
        if let Some(Check::All { checks, .. }) = &step.execution.precondition {
            assert_eq!(checks.len(), 2);
        } else {
            panic!("Expected All precondition");
        }
    }

    #[test]
    fn precondition_defaults_none() {
        let yaml = r#"
            command: "echo hello"
        "#;
        let config: StepConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.execution.precondition.is_none());
    }

    #[test]
    fn serialize_omits_none_precondition() {
        let config: BivvyConfig = serde_yaml::from_str(
            r#"
steps:
  hello:
    command: "echo hello"
"#,
        )
        .unwrap();
        let yaml = serde_yaml::to_string(&config).unwrap();
        assert!(
            !yaml.contains("precondition"),
            "None precondition should be omitted"
        );
    }

    #[test]
    fn environment_override_bare_null_parses_as_none() {
        let yaml = "command:";
        let o: StepEnvironmentOverride = serde_yaml::from_str(yaml).unwrap();
        assert!(o.command.is_none());
    }

    #[test]
    fn environment_override_empty_string_is_not_null() {
        let yaml = r#"command: """#;
        let o: StepEnvironmentOverride = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(o.command, Some("".to_string()));
    }
}
