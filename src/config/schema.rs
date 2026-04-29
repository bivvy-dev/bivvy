//! Configuration schema definitions for Bivvy.
//!
//! This module contains all the struct definitions that map to
//! the YAML configuration file format.

use crate::checks::{Check, SatisfactionCondition};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Helper for `serde_yaml::Value` fields — accepts any valid YAML/JSON value.
fn any_value_schema(_: &mut schemars::generate::SchemaGenerator) -> schemars::Schema {
    true.into()
}

/// Helper for `HashMap<String, serde_yaml::Value>` — object with any values.
fn any_value_map_schema(_: &mut schemars::generate::SchemaGenerator) -> schemars::Schema {
    let mut map = serde_json::Map::new();
    map.insert(
        "type".to_string(),
        serde_json::Value::String("object".to_string()),
    );
    map.insert(
        "additionalProperties".to_string(),
        serde_json::Value::Bool(true),
    );
    map.into()
}

/// Root configuration structure for bivvy.yml
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
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

/// JSONL event logging settings.
///
/// Controls whether structured event logs are written to `~/.bivvy/logs/`
/// and how long they are retained.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct LoggingSettings {
    /// Enable JSONL event logging. When false, no log files are written.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub logging: bool,

    /// Maximum age of log files in days. Files older than this are deleted
    /// during cleanup. Default: 30.
    #[serde(
        default = "default_log_retention_days",
        skip_serializing_if = "is_default_log_retention_days"
    )]
    pub log_retention_days: u32,

    /// Maximum total size of log files in megabytes. When exceeded, oldest
    /// files are deleted first. Default: 500.
    #[serde(
        default = "default_log_retention_mb",
        skip_serializing_if = "is_default_log_retention_mb"
    )]
    pub log_retention_mb: u64,
}

impl Default for LoggingSettings {
    fn default() -> Self {
        Self {
            logging: true,
            log_retention_days: default_log_retention_days(),
            log_retention_mb: default_log_retention_mb(),
        }
    }
}

impl LoggingSettings {
    /// Convert to a [`RetentionPolicy`](crate::logging::RetentionPolicy).
    pub fn to_retention_policy(&self) -> crate::logging::RetentionPolicy {
        crate::logging::RetentionPolicy {
            max_age_days: self.log_retention_days,
            max_size_mb: self.log_retention_mb,
        }
    }
}

fn default_log_retention_days() -> u32 {
    30
}

fn is_default_log_retention_days(v: &u32) -> bool {
    *v == default_log_retention_days()
}

fn default_log_retention_mb() -> u64 {
    500
}

fn is_default_log_retention_mb(v: &u64) -> bool {
    *v == default_log_retention_mb()
}

/// Execution-related settings (parallelism, history, updates).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

    /// Use the diagnostic funnel pipeline for step failure recovery.
    ///
    /// When true, step failures are analyzed by a multi-stage pipeline that
    /// produces ranked resolution candidates. When false, the legacy pattern
    /// registry is used (single fix per error). Default: true.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub diagnostic_funnel: bool,

    /// Enable automatic background updates.
    ///
    /// When true, bivvy checks for new versions in the background after each
    /// run and automatically installs updates so the next invocation uses the
    /// latest version. Set to false to disable (users can still run `bivvy update`).
    #[serde(default = "default_auto_update")]
    pub auto_update: bool,

    /// Global default rerun window for all steps.
    /// Steps can override this with their own `rerun_window` field.
    /// Accepts duration strings: `"4h"`, `"30m"`, `"7d"`, `"0"`/`"never"`, `"forever"`.
    /// Default: `"4h"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_rerun_window: Option<String>,
}

impl Default for ExecutionSettings {
    fn default() -> Self {
        Self {
            parallel: false,
            max_parallel: default_max_parallel(),
            history_retention: default_history_retention(),
            diagnostic_funnel: true,
            auto_update: default_auto_update(),
            default_rerun_window: None,
        }
    }
}

/// Environment variable settings (global env, env files, secret patterns).
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
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
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct EnvironmentProfileSettings {
    /// Default environment name (used when no --env flag and no auto-detection)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_environment: Option<String>,

    /// Named environment configurations
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub environments: HashMap<String, EnvironmentConfig>,
}

/// Default values for step behavior flags.
///
/// These serve as project-wide (or system-wide) defaults. Step-level settings
/// override these, and workflow `step_overrides` override step-level settings.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct DefaultsSettings {
    /// Default output mode: verbose, quiet, silent
    #[serde(default = "default_output")]
    pub output: OutputMode,

    /// Whether steps auto-run when the pipeline determines they need to run.
    /// When false, the user is prompted before each step executes.
    /// Default: true.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub auto_run: bool,

    /// Whether to prompt the user before re-running a step that was recently
    /// completed (within the rerun window). When false, recently-completed
    /// steps are silently skipped. Default: false.
    #[serde(default, skip_serializing_if = "is_false")]
    pub prompt_on_rerun: bool,

    /// Default rerun window for all steps.
    /// Steps can override this with their own `rerun_window` field.
    /// Accepts duration strings: `"4h"`, `"30m"`, `"7d"`, `"0"`/`"never"`, `"forever"`.
    /// Default: `"4h"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rerun_window: Option<String>,
}

impl Default for DefaultsSettings {
    fn default() -> Self {
        Self {
            output: default_output(),
            auto_run: true,
            prompt_on_rerun: false,
            rerun_window: None,
        }
    }
}

impl DefaultsSettings {
    fn is_default(&self) -> bool {
        matches!(self.output, OutputMode::Verbose)
            && self.auto_run
            && !self.prompt_on_rerun
            && self.rerun_window.is_none()
    }
}

/// Global settings that apply to all workflows and steps.
///
/// Uses `#[serde(flatten)]` on sub-structs so the YAML surface stays flat
/// (e.g., `settings.parallel` in YAML, not `settings.execution.parallel`).
///
/// `deny_unknown_fields` is applied via the `schemars`-only attribute because
/// serde's variant is incompatible with `#[serde(flatten)]`. The schema
/// therefore rejects unknown fields in editors, while runtime deserialization
/// continues to silently ignore them (preserving backward compatibility).
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
#[schemars(deny_unknown_fields)]
pub struct Settings {
    /// JSONL event logging settings (enable/disable, retention)
    #[serde(flatten)]
    pub logging: LoggingSettings,

    /// Execution-related settings (parallelism, history, updates)
    #[serde(flatten)]
    pub execution: ExecutionSettings,

    /// Environment variable settings (global env, env files, secrets)
    #[serde(flatten)]
    pub env_vars: EnvVarSettings,

    /// Environment profile settings (named environments, default)
    #[serde(flatten)]
    pub environment_profiles: EnvironmentProfileSettings,

    /// Default values for step behavior flags.
    #[serde(default, skip_serializing_if = "DefaultsSettings::is_default")]
    pub defaults: DefaultsSettings,
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
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum OutputMode {
    #[default]
    Verbose,
    Quiet,
    Silent,
}

/// Fields related to what the step actually runs and how.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ExecutionConfig {
    /// Shell command to execute
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,

    /// Single check (new `Check` enum).
    /// Mutually exclusive with `checks`.
    ///
    /// Also accepts the deprecated `completed_check` field name via serde alias.
    #[serde(skip_serializing_if = "Option::is_none", alias = "completed_check")]
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
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct BehaviorConfig {
    /// Whether user can skip this step via `--skip` on the CLI.
    /// Does NOT trigger interactive prompts — use `confirm` for that.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub skippable: bool,

    /// Step must run, cannot be skipped
    #[serde(default, skip_serializing_if = "is_false")]
    pub required: bool,

    /// Always prompt the user before running this step.
    /// Default: false. When true, the step will never auto-run — the user
    /// must explicitly confirm.
    #[serde(default, skip_serializing_if = "is_false")]
    pub confirm: bool,

    /// Whether this step auto-runs when the pipeline determines it needs to run.
    /// `None` means "use the global default" (`settings.defaults.auto_run`).
    /// `Some(true)` = auto-run, `Some(false)` = prompt user before running.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_run: Option<bool>,

    /// Ask before re-running completed steps.
    /// `None` means "use the global default" (`settings.defaults.prompt_on_rerun`).
    /// `Some(true)` = prompt before re-running, `Some(false)` = silently skip.
    ///
    /// Also accepts the deprecated `prompt_if_complete` field name via serde alias.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "prompt_if_complete"
    )]
    pub prompt_on_rerun: Option<bool>,

    /// Continue workflow if this step fails
    #[serde(default, skip_serializing_if = "is_false")]
    pub allow_failure: bool,

    /// Mark step as handling sensitive data
    #[serde(default, skip_serializing_if = "is_false")]
    pub sensitive: bool,

    /// How long a previous successful run counts as "recent enough" to
    /// consider this step satisfied by execution history alone.
    /// Accepts duration strings: `"4h"`, `"30m"`, `"7d"`, `"0"`/`"never"`, `"forever"`.
    /// If not set, uses the global `default_rerun_window` setting (default: `"4h"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rerun_window: Option<String>,

    /// Always re-run this step, bypassing its `check`, `checks`, and
    /// `satisfied_when` evaluation. Equivalent to listing the step in
    /// `--force` on every run. Preconditions still apply.
    #[serde(default, skip_serializing_if = "is_false")]
    pub force: bool,
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        Self {
            skippable: true,
            required: false,
            confirm: false,
            auto_run: None,
            prompt_on_rerun: None,
            allow_failure: false,
            sensitive: false,
            rerun_window: None,
            force: false,
        }
    }
}

/// Before/after lifecycle hooks.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
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
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
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
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
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
///
/// `deny_unknown_fields` is applied via the `schemars`-only attribute because
/// serde's variant is incompatible with `#[serde(flatten)]`. The schema
/// therefore rejects unknown fields in editors, while runtime deserialization
/// continues to silently ignore them (preserving backward compatibility).
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
#[schemars(deny_unknown_fields)]
pub struct StepConfig {
    /// Reference to a template (mutually exclusive with inline config)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,

    /// Template inputs (when using template)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    #[schemars(schema_with = "any_value_map_schema")]
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
    ///
    /// The canonical YAML field name is `tools`. The old name `requires` is
    /// accepted as an alias for backward compatibility.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        rename = "tools",
        alias = "requires"
    )]
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

    /// Deprecated `watches` field. Accepted for backward compatibility and
    /// converted to change checks during config loading. Users should migrate
    /// to `check: { type: change, target: ... }`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub watches: Vec<String>,
}

fn default_true() -> bool {
    true
}

/// Prompt configuration for interactive input
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
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
    #[schemars(schema_with = "any_value_schema")]
    pub default: Option<serde_yaml::Value>,
}

/// Type of interactive prompt
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PromptType {
    Select,
    Multiselect,
    Confirm,
    Input,
}

/// Option for select/multiselect prompts
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PromptOption {
    /// Display label
    pub label: String,
    /// Value to use when selected
    pub value: String,
}

/// Step-specific output configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StepOutputConfig {
    /// Output mode for this step
    pub default: Option<OutputMode>,
}

/// Configuration for a named workflow
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
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

    /// Override auto_run for all steps in this workflow.
    /// Individual step overrides take precedence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_run_steps: Option<bool>,

    /// Workflow-level environment variables
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,

    /// Workflow-level env file
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env_file: Option<PathBuf>,

    /// Steps to always force when this workflow runs. Same effect as
    /// passing `--force <step>` on the CLI for each entry. Step names
    /// must reference steps defined in the configuration.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub force: Vec<String>,

    /// Force every step in this workflow, bypassing all checks and
    /// step-level configuration. Equivalent to passing `--force-all`
    /// every time this workflow runs.
    #[serde(default, skip_serializing_if = "is_false")]
    pub force_all: bool,
}

/// Per-step overrides within a workflow
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct StepOverride {
    /// Skip prompts, just run
    #[serde(default, skip_serializing_if = "is_false")]
    pub skip_prompt: bool,

    /// Override required flag
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,

    /// Override auto_run flag.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_run: Option<bool>,

    /// Override prompt_on_rerun flag.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_on_rerun: Option<bool>,

    /// Override rerun_window.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rerun_window: Option<String>,
}

/// Workflow-level settings overrides
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct WorkflowSettings {
    /// Force non-interactive mode for this workflow
    #[serde(default, skip_serializing_if = "is_false")]
    pub non_interactive: bool,
}

/// Remote template source configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
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
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CacheConfig {
    /// Time-to-live (e.g., "7d", "24h")
    pub ttl: String,

    /// Cache strategy: etag, git
    #[serde(default)]
    pub strategy: CacheStrategy,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum CacheStrategy {
    #[default]
    Etag,
    Git,
}

/// Authentication for remote template sources
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AuthConfig {
    /// Auth type: bearer, header
    #[serde(rename = "type")]
    pub auth_type: AuthType,

    /// Environment variable containing the token
    pub token_env: String,

    /// Custom header name (for header auth type)
    pub header: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AuthType {
    Bearer,
    Header,
}

/// Config inheritance source
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExtendsConfig {
    /// URL to base config
    pub url: String,
}

/// Secret configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
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
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
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
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
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
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
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
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
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

    /// Override requirements.
    /// The canonical YAML field name is `tools`. `requires` is accepted as an alias.
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "tools",
        alias = "requires"
    )]
    pub requires: Option<Vec<String>>,

    /// Override retry count
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry: Option<u32>,

    /// Override confirm
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirm: Option<bool>,

    /// Override auto_run
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_run: Option<bool>,

    /// Override rerun_window
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rerun_window: Option<String>,
}

impl BivvyConfig {
    /// Migrate deprecated fields to their modern equivalents.
    ///
    /// Converts:
    /// - `watches: [path, ...]` → change checks on the step
    ///
    /// Returns deprecation warning messages for any fields that were migrated.
    /// Should be called after deserialization to ensure backward compatibility.
    pub fn migrate_deprecated_fields(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();

        for (step_name, step) in &mut self.steps {
            if !step.watches.is_empty() {
                warnings.push(format!(
                    "Step '{}': 'watches' is deprecated. Use 'check: {{ type: change, target: ... }}' instead.",
                    step_name
                ));

                // Convert each watch path into a change check
                let change_checks: Vec<Check> = step
                    .watches
                    .drain(..)
                    .map(|target| Check::Change {
                        name: None,
                        target,
                        kind: crate::checks::ChangeKind::default(),
                        on_change: crate::checks::OnChange::default(),
                        require_step: None,
                        baseline: crate::checks::BaselineConfig::default(),
                        baseline_snapshot: None,
                        baseline_git: None,
                        size_limit: crate::checks::SizeLimit::default(),
                        scope: crate::checks::SnapshotScope::default(),
                    })
                    .collect();

                // Merge into existing checks
                step.execution.checks.extend(change_checks);
            }
        }

        warnings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_has_defaults() {
        let config: BivvyConfig = serde_yaml::from_str("").unwrap();
        assert_eq!(config.settings.defaults.output, OutputMode::Verbose);
        assert_eq!(config.settings.execution.max_parallel, 4);
        assert_eq!(config.settings.execution.history_retention, 50);
        assert!(config.settings.logging.logging);
        assert_eq!(config.settings.logging.log_retention_days, 30);
        assert_eq!(config.settings.logging.log_retention_mb, 500);
        assert!(config.steps.is_empty());
        assert!(config.workflows.is_empty());
    }

    #[test]
    fn parses_minimal_config() {
        let yaml = r#"
app_name: "MyApp"
settings:
  defaults:
    output: quiet
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.app_name, Some("MyApp".to_string()));
        assert_eq!(config.settings.defaults.output, OutputMode::Quiet);
    }

    #[test]
    fn logging_settings_parse_from_yaml() {
        let yaml = r#"
settings:
  logging: false
  log_retention_days: 7
  log_retention_mb: 100
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(!config.settings.logging.logging);
        assert_eq!(config.settings.logging.log_retention_days, 7);
        assert_eq!(config.settings.logging.log_retention_mb, 100);
    }

    #[test]
    fn logging_settings_to_retention_policy() {
        let settings = LoggingSettings {
            logging: true,
            log_retention_days: 14,
            log_retention_mb: 250,
        };
        let policy = settings.to_retention_policy();
        assert_eq!(policy.max_age_days, 14);
        assert_eq!(policy.max_size_mb, 250);
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
  env:
    RAILS_ENV: development
    DEBUG: "true"
  parallel: true
  max_parallel: 8
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            config.settings.env_vars.env.get("RAILS_ENV"),
            Some(&"development".to_string())
        );
        assert!(config.settings.execution.parallel);
        assert_eq!(config.settings.execution.max_parallel, 8);
    }

    #[test]
    fn ignores_removed_logging_and_log_path_fields() {
        let yaml = r#"
settings:
  logging: true
  log_path: "logs/bivvy.log"
  defaults:
    output: verbose
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        // logging and log_path are silently ignored; defaults.output still works
        assert_eq!(config.settings.defaults.output, OutputMode::Verbose);
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
        assert!(step.behavior.prompt_on_rerun.is_none());
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
            "None prompt_on_rerun should be omitted"
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
        assert!(!yaml.contains("tools"), "empty tools should be omitted");
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
    fn step_config_tools_is_canonical_name() {
        let yaml = r#"
            command: "bundle install"
            tools:
              - ruby
              - postgres-server
        "#;
        let config: StepConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.requires, vec!["ruby", "postgres-server"]);
    }

    #[test]
    fn step_config_tools_serializes_as_tools() {
        let config = StepConfig {
            requires: vec!["ruby".to_string()],
            ..Default::default()
        };
        let yaml = serde_yaml::to_string(&config).unwrap();
        assert!(yaml.contains("tools:"), "should serialize as 'tools'");
        assert!(
            !yaml.contains("requires:"),
            "should not serialize as 'requires'"
        );
    }

    #[test]
    fn behavior_config_confirm_defaults_false() {
        let yaml = r#"
            command: "echo hello"
        "#;
        let config: StepConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(!config.behavior.confirm);
    }

    #[test]
    fn behavior_config_confirm_parses() {
        let yaml = r#"
            command: "bin/deploy"
            confirm: true
        "#;
        let config: StepConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.behavior.confirm);
    }

    #[test]
    fn behavior_config_rerun_window_defaults_none() {
        let yaml = r#"
            command: "echo hello"
        "#;
        let config: StepConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.behavior.rerun_window.is_none());
    }

    #[test]
    fn behavior_config_rerun_window_parses() {
        let yaml = r#"
            command: "yarn install"
            rerun_window: "24h"
        "#;
        let config: StepConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.behavior.rerun_window, Some("24h".to_string()));
    }

    #[test]
    fn execution_settings_default_rerun_window() {
        let yaml = r#"
            default_rerun_window: "8h"
        "#;
        let settings: ExecutionSettings = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(settings.default_rerun_window, Some("8h".to_string()));
    }

    #[test]
    fn behavior_config_confirm_omitted_when_false() {
        let config = BehaviorConfig::default();
        let yaml = serde_yaml::to_string(&config).unwrap();
        assert!(!yaml.contains("confirm"), "false confirm should be omitted");
    }

    #[test]
    fn behavior_config_rerun_window_omitted_when_none() {
        let config = BehaviorConfig::default();
        let yaml = serde_yaml::to_string(&config).unwrap();
        assert!(
            !yaml.contains("rerun_window"),
            "None rerun_window should be omitted"
        );
    }

    #[test]
    fn behavior_config_auto_run_defaults_none() {
        let yaml = r#"
            command: "echo hello"
        "#;
        let config: StepConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.behavior.auto_run.is_none());
    }

    #[test]
    fn behavior_config_auto_run_parses_false() {
        let yaml = r#"
            command: "yarn install"
            auto_run: false
        "#;
        let config: StepConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.behavior.auto_run, Some(false));
    }

    #[test]
    fn behavior_config_auto_run_parses_true() {
        let yaml = r#"
            command: "yarn install"
            auto_run: true
        "#;
        let config: StepConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.behavior.auto_run, Some(true));
    }

    #[test]
    fn behavior_config_auto_run_omitted_when_none() {
        let config = BehaviorConfig::default();
        let yaml = serde_yaml::to_string(&config).unwrap();
        assert!(
            !yaml.contains("auto_run"),
            "None auto_run should be omitted"
        );
    }

    #[test]
    fn behavior_config_force_defaults_false() {
        let yaml = r#"
            command: "echo hello"
        "#;
        let config: StepConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(!config.behavior.force);
    }

    #[test]
    fn behavior_config_force_parses_true() {
        let yaml = r#"
            command: "bin/migrate"
            force: true
        "#;
        let config: StepConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.behavior.force);
    }

    #[test]
    fn behavior_config_force_omitted_when_false() {
        let config = BehaviorConfig::default();
        let yaml = serde_yaml::to_string(&config).unwrap();
        assert!(
            !yaml.contains("force"),
            "false force should be omitted from serialized output"
        );
    }

    #[test]
    fn defaults_settings_parses() {
        let yaml = r#"
settings:
  defaults:
    auto_run: false
    prompt_on_rerun: true
    rerun_window: "8h"
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(!config.settings.defaults.auto_run);
        assert!(config.settings.defaults.prompt_on_rerun);
        assert_eq!(
            config.settings.defaults.rerun_window,
            Some("8h".to_string())
        );
    }

    #[test]
    fn defaults_settings_has_correct_defaults() {
        let yaml = r#"
app_name: test
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.settings.defaults.auto_run);
        assert!(!config.settings.defaults.prompt_on_rerun);
        assert!(config.settings.defaults.rerun_window.is_none());
    }

    #[test]
    fn defaults_settings_omitted_when_default() {
        let settings = Settings::default();
        let yaml = serde_yaml::to_string(&settings).unwrap();
        assert!(
            !yaml.contains("defaults"),
            "default DefaultsSettings should be omitted"
        );
    }

    #[test]
    fn workflow_auto_run_steps_parses() {
        let yaml = r#"
app_name: test
steps:
  build:
    command: "cargo build"
workflows:
  ci:
    steps: [build]
    auto_run_steps: false
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.workflows["ci"].auto_run_steps, Some(false));
    }

    #[test]
    fn workflow_force_list_parses() {
        let yaml = r#"
app_name: test
steps:
  install:
    command: "yarn install"
  build:
    command: "yarn build"
workflows:
  refresh:
    steps: [install, build]
    force: [install]
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            config.workflows["refresh"].force,
            vec!["install".to_string()]
        );
        assert!(!config.workflows["refresh"].force_all);
    }

    #[test]
    fn workflow_force_all_parses_true() {
        let yaml = r#"
app_name: test
steps:
  install:
    command: "yarn install"
workflows:
  fresh:
    steps: [install]
    force_all: true
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.workflows["fresh"].force_all);
        assert!(config.workflows["fresh"].force.is_empty());
    }

    #[test]
    fn workflow_force_defaults_empty() {
        let yaml = r#"
app_name: test
steps:
  install:
    command: "yarn install"
workflows:
  default:
    steps: [install]
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.workflows["default"].force.is_empty());
        assert!(!config.workflows["default"].force_all);
    }

    #[test]
    fn workflow_auto_run_steps_defaults_none() {
        let yaml = r#"
app_name: test
steps:
  build:
    command: "cargo build"
workflows:
  default:
    steps: [build]
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.workflows["default"].auto_run_steps.is_none());
    }

    #[test]
    fn step_override_auto_run_parses() {
        let yaml = r#"
auto_run: false
"#;
        let overrides: StepOverride = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(overrides.auto_run, Some(false));
    }

    #[test]
    fn step_environment_override_auto_run_parses() {
        let yaml = r#"
auto_run: true
"#;
        let overrides: StepEnvironmentOverride = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(overrides.auto_run, Some(true));
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

    // --- Backward compatibility tests ---

    #[test]
    fn completed_check_alias_parses_as_check() {
        let yaml = r#"
steps:
  install:
    command: "cargo install"
    completed_check:
      type: presence
      target: "target/release/bivvy"
"#;
        let config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        let step = &config.steps["install"];
        assert!(step.execution.check.is_some());
        assert!(matches!(step.execution.check, Some(Check::Presence { .. })));
    }

    #[test]
    fn file_exists_alias_parses_as_presence() {
        let yaml = r#"
type: file_exists
target: "Gemfile.lock"
"#;
        let check: Check = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(check, Check::Presence { .. }));
        assert_eq!(check.type_name(), "presence");
    }

    #[test]
    fn command_succeeds_alias_parses_as_execution() {
        let yaml = r#"
type: command_succeeds
command: "ruby --version"
"#;
        let check: Check = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(check, Check::Execution { .. }));
        assert_eq!(check.type_name(), "execution");
    }

    #[test]
    fn prompt_if_complete_alias_parses_as_prompt_on_rerun() {
        let yaml = r#"
command: "cargo build"
prompt_if_complete: false
"#;
        let step: StepConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(step.behavior.prompt_on_rerun, Some(false));
    }

    #[test]
    fn watches_field_parses_and_migrates_to_change_checks() {
        let yaml = r#"
steps:
  build:
    command: "cargo build"
    watches:
      - Cargo.toml
      - Cargo.lock
"#;
        let mut config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        let warnings = config.migrate_deprecated_fields();

        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("'watches' is deprecated"));

        let step = &config.steps["build"];
        assert_eq!(step.execution.checks.len(), 2);
        assert!(step.watches.is_empty());

        // Verify the checks are change-type with the right targets
        for (i, expected_target) in ["Cargo.toml", "Cargo.lock"].iter().enumerate() {
            match &step.execution.checks[i] {
                Check::Change { target, .. } => assert_eq!(target, expected_target),
                other => panic!("Expected Change check, got {:?}", other),
            }
        }
    }

    #[test]
    fn watches_migration_merges_with_existing_checks() {
        let yaml = r#"
steps:
  build:
    command: "cargo build"
    checks:
      - type: presence
        target: "target/debug/bivvy"
    watches:
      - Cargo.toml
"#;
        let mut config: BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        config.migrate_deprecated_fields();

        let step = &config.steps["build"];
        assert_eq!(step.execution.checks.len(), 2);
        assert!(matches!(step.execution.checks[0], Check::Presence { .. }));
        assert!(matches!(step.execution.checks[1], Check::Change { .. }));
    }
}
