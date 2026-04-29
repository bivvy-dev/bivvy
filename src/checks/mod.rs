//! Check evaluation system for Bivvy.
//!
//! Checks report facts about the external world: "does this file exist?",
//! "does this command succeed?", "has this target changed?" They never
//! query internal state or decide whether a step should run.
//!
//! # Check Types
//!
//! - [`Check::Presence`] — file, binary, or custom presence detection
//! - [`Check::Execution`] — run a command and validate the result
//! - [`Check::Change`] — detect changes from a stored baseline
//! - [`Check::All`] / [`Check::Any`] — combinators
//!
//! # Evaluation
//!
//! All checks go through [`CheckEvaluator::evaluate`], which produces a
//! [`CheckResult`] with a [`CheckOutcome`]. The evaluator has no access
//! to the state store or UI — it's pure evaluation.

pub mod change;
pub mod evaluator;
pub mod execution;
pub mod presence;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A check that evaluates external-world state.
///
/// Checks are configured in step definitions via the `check`/`checks` fields.
/// They report facts about the environment — they do not access bivvy's
/// internal execution history.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Check {
    /// Confirms the existence of a file, binary, or other resource.
    ///
    /// Also accepts the deprecated `type: file_exists` via serde alias.
    #[serde(alias = "file_exists")]
    Presence {
        /// Optional name for referencing this check from `satisfied_when`.
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,

        /// What to check for: a file path or binary name.
        #[serde(skip_serializing_if = "Option::is_none")]
        target: Option<String>,

        /// Subtype: `file`, `binary`, or `custom`.
        /// Inferred from context if omitted.
        #[serde(skip_serializing_if = "Option::is_none")]
        kind: Option<PresenceKind>,

        /// Command for custom presence checks.
        #[serde(skip_serializing_if = "Option::is_none")]
        command: Option<String>,
    },

    /// Runs a command and validates the result.
    ///
    /// Also accepts the deprecated `type: command_succeeds` via serde alias.
    #[serde(alias = "command_succeeds")]
    Execution {
        /// Optional name for referencing this check.
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,

        /// Shell command to execute.
        command: String,

        /// How to validate the result (default: `success`).
        #[serde(default)]
        validation: ValidationMode,
    },

    /// Detects whether a target has changed from a stored baseline.
    Change {
        /// Optional name for referencing this check.
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,

        /// What to hash — a file path, glob pattern, or command.
        target: String,

        /// Target type: `file` (default), `glob`, or `command`.
        #[serde(default)]
        kind: ChangeKind,

        /// What a detected change means: `proceed`, `fail`, or `require`.
        #[serde(default)]
        on_change: OnChange,

        /// Step to flag as required when `on_change: require` and change is detected.
        /// Ignored unless `on_change` is `require`.
        #[serde(skip_serializing_if = "Option::is_none")]
        require_step: Option<String>,

        /// When/how the baseline is established: `each_run` (default) or `first_run`.
        #[serde(default)]
        baseline: BaselineConfig,

        /// Named snapshot baseline. When set, comparison is against this named snapshot
        /// instead of the run-based baseline. Equivalent to the spec's `snapshot:<slug>`.
        #[serde(skip_serializing_if = "Option::is_none")]
        baseline_snapshot: Option<String>,

        /// Git ref baseline. When set, comparison is against the content at this git ref.
        /// Equivalent to the spec's `git:<ref>`.
        #[serde(skip_serializing_if = "Option::is_none")]
        baseline_git: Option<String>,

        /// Max total size of target before bivvy refuses to hash.
        /// Integer in bytes, or `null` for no limit. Default: 50 MB.
        #[serde(default = "default_size_limit")]
        size_limit: SizeLimit,

        /// Snapshot scope: project (default) or workflow.
        #[serde(default)]
        scope: SnapshotScope,
    },

    /// All checks must pass.
    All {
        /// Optional name for the combinator.
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,

        /// Checks that must all pass.
        checks: Vec<Check>,
    },

    /// Any check passing is sufficient.
    Any {
        /// Optional name for the combinator.
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,

        /// Checks where at least one must pass.
        checks: Vec<Check>,
    },
}

impl Check {
    /// Returns the check type as a string (for event logging).
    pub fn type_name(&self) -> &'static str {
        match self {
            Check::Presence { .. } => "presence",
            Check::Execution { .. } => "execution",
            Check::Change { .. } => "change",
            Check::All { .. } => "all",
            Check::Any { .. } => "any",
        }
    }

    /// Returns the optional name of this check.
    pub fn name(&self) -> Option<&str> {
        match self {
            Check::Presence { name, .. }
            | Check::Execution { name, .. }
            | Check::Change { name, .. }
            | Check::All { name, .. }
            | Check::Any { name, .. } => name.as_deref(),
        }
    }

    /// Returns true if this check or any nested sub-check has a name.
    ///
    /// Used to avoid unnecessary evaluation when collecting named check results
    /// for cross-step ref resolution.
    pub fn has_named_checks(&self) -> bool {
        if self.name().is_some() {
            return true;
        }
        match self {
            Check::All { checks, .. } | Check::Any { checks, .. } => {
                checks.iter().any(|c| c.has_named_checks())
            }
            _ => false,
        }
    }

    /// Compute a short hash of this check's configuration for snapshot key isolation.
    ///
    /// Two checks with different configs (different targets, on_change values, etc.)
    /// will produce different hashes, ensuring their baselines don't collide.
    pub fn config_hash(&self) -> String {
        use sha2::{Digest, Sha256};
        let yaml = serde_yaml::to_string(self).unwrap_or_default();
        let hash = Sha256::digest(yaml.as_bytes());
        hex::encode(hash)[..8].to_string()
    }
}

/// Subtype for presence checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PresenceKind {
    /// File or directory existence.
    File,
    /// Binary on `$PATH`.
    Binary,
    /// Custom command that exits 0 if present.
    Custom,
}

/// How to validate an execution check result.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ValidationMode {
    /// Command exits with code 0.
    #[default]
    Success,
    /// Command exits 0 AND produces non-empty stdout.
    Truthy,
    /// Command exits 0 AND produces empty stdout (or exits non-zero).
    Falsy,
}

/// Target type for change checks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    /// Hash a single file.
    #[default]
    File,
    /// Hash all files matching a glob pattern.
    Glob,
    /// Hash the stdout of a command.
    Command,
}

/// What a detected change means for a change check.
///
/// In YAML:
/// ```yaml
/// on_change: proceed       # step should run when target changed
/// on_change: fail          # check fails when target changed
/// on_change: require       # flags require_step as needed when target changed
/// ```
///
/// When `on_change: require`, the `require_step` field on the Change check
/// specifies which step to flag. The check itself always passes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OnChange {
    /// Change detected = check passes (step should run).
    /// No change = check fails (step has no reason to run).
    #[default]
    Proceed,
    /// Change detected = check fails (something unstable changed).
    /// No change = check passes (stability maintained).
    Fail,
    /// Change detected = the step named in `require_step` is flagged as required.
    /// The check itself always passes regardless of change.
    Require,
}

/// When/how the change check baseline is established.
///
/// In YAML:
/// ```yaml
/// baseline: each_run       # updated after each successful run (default)
/// baseline: first_run      # established once, never updated
/// ```
///
/// For named snapshots or git refs, use `baseline_snapshot` or `baseline_git`
/// fields on the Change check instead.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BaselineConfig {
    /// Baseline updated after each successful run (default).
    #[default]
    EachRun,
    /// Baseline established on first evaluation, never updated.
    FirstRun,
}

/// Maximum total size of files a change check will hash.
///
/// Configured as an integer (bytes) or `null`/omitted for no limit.
/// The default is 50 MB (52428800 bytes).
///
/// In YAML:
/// ```yaml
/// size_limit: 52428800       # explicit byte count
/// size_limit: null           # no limit
/// # omitted → defaults to 50 MB
/// ```
#[derive(Debug, Clone, PartialEq, Eq, JsonSchema)]
#[schemars(transparent)]
pub struct SizeLimit {
    /// Maximum size in bytes. `None` means no limit.
    pub max_bytes: Option<u64>,
}

impl SizeLimit {
    /// 50 MB default.
    pub const DEFAULT_BYTES: u64 = 50 * 1024 * 1024;

    /// No size limit.
    pub fn none() -> Self {
        Self { max_bytes: None }
    }

    /// Limit in bytes.
    pub fn bytes(n: u64) -> Self {
        Self { max_bytes: Some(n) }
    }
}

impl Default for SizeLimit {
    fn default() -> Self {
        Self::bytes(Self::DEFAULT_BYTES)
    }
}

impl Serialize for SizeLimit {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.max_bytes {
            Some(n) => serializer.serialize_u64(n),
            None => serializer.serialize_none(),
        }
    }
}

impl<'de> Deserialize<'de> for SizeLimit {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value: Option<u64> = Option::deserialize(deserializer)?;
        Ok(match value {
            Some(n) => SizeLimit::bytes(n),
            None => SizeLimit::none(),
        })
    }
}

fn default_size_limit() -> SizeLimit {
    SizeLimit::default()
}

/// Snapshot scope for change check baselines.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotScope {
    /// Per-project, shared across workflows (default).
    #[default]
    Project,
    /// Per-workflow, isolated to the workflow this step runs in.
    Workflow,
}

/// Result of evaluating a check.
#[derive(Debug, Clone)]
pub struct CheckResult {
    /// The outcome of the check evaluation.
    pub outcome: CheckOutcome,
    /// Human-readable description of what was checked.
    pub description: String,
    /// Optional details (e.g., which file was missing, what command output was).
    pub details: Option<String>,
}

impl CheckResult {
    /// Create a passing result.
    pub fn passed(description: impl Into<String>) -> Self {
        Self {
            outcome: CheckOutcome::Passed,
            description: description.into(),
            details: None,
        }
    }

    /// Create a failing result.
    pub fn failed(description: impl Into<String>, details: impl Into<String>) -> Self {
        Self {
            outcome: CheckOutcome::Failed,
            description: description.into(),
            details: Some(details.into()),
        }
    }

    /// Create an indeterminate result.
    pub fn indeterminate(description: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            outcome: CheckOutcome::Indeterminate(reason.into()),
            description: description.into(),
            details: None,
        }
    }

    /// Whether the check passed.
    pub fn passed_check(&self) -> bool {
        matches!(self.outcome, CheckOutcome::Passed)
    }
}

/// Outcome of a check evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckOutcome {
    /// The check passed (condition is met).
    Passed,
    /// The check failed (condition is not met).
    Failed,
    /// The check could not be evaluated (e.g., no baseline for change detection).
    Indeterminate(String),
}

impl CheckOutcome {
    /// Returns the outcome as a string (for event logging).
    pub fn as_str(&self) -> &str {
        match self {
            CheckOutcome::Passed => "passed",
            CheckOutcome::Failed => "failed",
            CheckOutcome::Indeterminate(_) => "indeterminate",
        }
    }
}

/// Truncate a string to a maximum length, appending "..." if truncated.
pub(crate) fn truncate_display(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

/// A satisfaction condition entry in `satisfied_when`.
///
/// Can be either an inline check or a reference to a named check.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum SatisfactionCondition {
    /// Reference to a named check by name (same step or `step_name.check_name`).
    Ref {
        /// The referenced check name.
        #[serde(rename = "ref")]
        check_ref: String,
    },
    /// An inline check definition.
    Check(Check),
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Check enum serde tests ---

    #[test]
    fn presence_check_deserializes_with_file_kind() {
        let yaml = r#"
            type: presence
            target: node_modules
            kind: file
        "#;
        let check: Check = serde_yaml::from_str(yaml).unwrap();
        match &check {
            Check::Presence { target, kind, .. } => {
                assert_eq!(target.as_deref(), Some("node_modules"));
                assert_eq!(*kind, Some(PresenceKind::File));
            }
            _ => panic!("Expected Presence check"),
        }
    }

    #[test]
    fn presence_check_deserializes_with_binary_kind() {
        let yaml = r#"
            type: presence
            target: rustc
            kind: binary
        "#;
        let check: Check = serde_yaml::from_str(yaml).unwrap();
        match &check {
            Check::Presence { target, kind, .. } => {
                assert_eq!(target.as_deref(), Some("rustc"));
                assert_eq!(*kind, Some(PresenceKind::Binary));
            }
            _ => panic!("Expected Presence check"),
        }
    }

    #[test]
    fn presence_check_deserializes_custom_with_command() {
        let yaml = r#"
            type: presence
            kind: custom
            command: "pg_isready -q"
        "#;
        let check: Check = serde_yaml::from_str(yaml).unwrap();
        match &check {
            Check::Presence {
                kind,
                command,
                target,
                ..
            } => {
                assert_eq!(*kind, Some(PresenceKind::Custom));
                assert_eq!(command.as_deref(), Some("pg_isready -q"));
                assert!(target.is_none());
            }
            _ => panic!("Expected Presence check"),
        }
    }

    #[test]
    fn presence_check_with_name() {
        let yaml = r#"
            type: presence
            name: deps_installed
            target: node_modules
        "#;
        let check: Check = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(check.name(), Some("deps_installed"));
    }

    #[test]
    fn execution_check_deserializes_with_defaults() {
        let yaml = r#"
            type: execution
            command: "bundle check"
        "#;
        let check: Check = serde_yaml::from_str(yaml).unwrap();
        match &check {
            Check::Execution {
                command,
                validation,
                ..
            } => {
                assert_eq!(command, "bundle check");
                assert_eq!(*validation, ValidationMode::Success);
            }
            _ => panic!("Expected Execution check"),
        }
    }

    #[test]
    fn execution_check_deserializes_truthy_validation() {
        let yaml = r#"
            type: execution
            command: "git status --porcelain"
            validation: truthy
        "#;
        let check: Check = serde_yaml::from_str(yaml).unwrap();
        match &check {
            Check::Execution { validation, .. } => {
                assert_eq!(*validation, ValidationMode::Truthy);
            }
            _ => panic!("Expected Execution check"),
        }
    }

    #[test]
    fn execution_check_deserializes_falsy_validation() {
        let yaml = r#"
            type: execution
            command: "git diff --cached --quiet"
            validation: falsy
        "#;
        let check: Check = serde_yaml::from_str(yaml).unwrap();
        match &check {
            Check::Execution { validation, .. } => {
                assert_eq!(*validation, ValidationMode::Falsy);
            }
            _ => panic!("Expected Execution check"),
        }
    }

    #[test]
    fn change_check_deserializes_with_proceed() {
        let yaml = r#"
            type: change
            target: Gemfile.lock
            on_change: proceed
        "#;
        let check: Check = serde_yaml::from_str(yaml).unwrap();
        match &check {
            Check::Change {
                target,
                kind,
                on_change,
                baseline,
                ..
            } => {
                assert_eq!(target, "Gemfile.lock");
                assert_eq!(*kind, ChangeKind::File);
                assert_eq!(*on_change, OnChange::Proceed);
                assert_eq!(*baseline, BaselineConfig::EachRun);
            }
            _ => panic!("Expected Change check"),
        }
    }

    #[test]
    fn change_check_deserializes_with_fail() {
        let yaml = r#"
            type: change
            target: .env.example
            on_change: fail
        "#;
        let check: Check = serde_yaml::from_str(yaml).unwrap();
        match &check {
            Check::Change { on_change, .. } => {
                assert_eq!(*on_change, OnChange::Fail);
            }
            _ => panic!("Expected Change check"),
        }
    }

    #[test]
    fn change_check_deserializes_glob_kind() {
        let yaml = r#"
            type: change
            target: "db/migrate/*.rb"
            kind: glob
            on_change: proceed
        "#;
        let check: Check = serde_yaml::from_str(yaml).unwrap();
        match &check {
            Check::Change { kind, .. } => {
                assert_eq!(*kind, ChangeKind::Glob);
            }
            _ => panic!("Expected Change check"),
        }
    }

    #[test]
    fn change_check_deserializes_command_kind() {
        let yaml = r#"
            type: change
            target: "ruby --version"
            kind: command
            on_change: fail
            baseline: first_run
        "#;
        let check: Check = serde_yaml::from_str(yaml).unwrap();
        match &check {
            Check::Change { kind, baseline, .. } => {
                assert_eq!(*kind, ChangeKind::Command);
                assert_eq!(*baseline, BaselineConfig::FirstRun);
            }
            _ => panic!("Expected Change check"),
        }
    }

    #[test]
    fn change_check_deserializes_workflow_scope() {
        let yaml = r#"
            type: change
            target: Gemfile.lock
            on_change: proceed
            scope: workflow
        "#;
        let check: Check = serde_yaml::from_str(yaml).unwrap();
        match &check {
            Check::Change { scope, .. } => {
                assert_eq!(*scope, SnapshotScope::Workflow);
            }
            _ => panic!("Expected Change check"),
        }
    }

    #[test]
    fn change_check_default_scope_is_project() {
        let yaml = r#"
            type: change
            target: Gemfile.lock
            on_change: proceed
        "#;
        let check: Check = serde_yaml::from_str(yaml).unwrap();
        match &check {
            Check::Change { scope, .. } => {
                assert_eq!(*scope, SnapshotScope::Project);
            }
            _ => panic!("Expected Change check"),
        }
    }

    #[test]
    fn change_check_default_size_limit_is_50mb() {
        let yaml = r#"
            type: change
            target: Gemfile.lock
            on_change: proceed
        "#;
        let check: Check = serde_yaml::from_str(yaml).unwrap();
        match &check {
            Check::Change { size_limit, .. } => {
                assert_eq!(*size_limit, SizeLimit::default());
            }
            _ => panic!("Expected Change check"),
        }
    }

    #[test]
    fn all_combinator_deserializes() {
        let yaml = r#"
            type: all
            checks:
              - type: presence
                target: node_modules
              - type: execution
                command: "yarn check"
        "#;
        let check: Check = serde_yaml::from_str(yaml).unwrap();
        match &check {
            Check::All { checks, .. } => {
                assert_eq!(checks.len(), 2);
                assert!(matches!(checks[0], Check::Presence { .. }));
                assert!(matches!(checks[1], Check::Execution { .. }));
            }
            _ => panic!("Expected All check"),
        }
    }

    #[test]
    fn any_combinator_deserializes() {
        let yaml = r#"
            type: any
            checks:
              - type: presence
                target: ".ruby-version"
              - type: presence
                target: ".tool-versions"
        "#;
        let check: Check = serde_yaml::from_str(yaml).unwrap();
        match &check {
            Check::Any { checks, .. } => {
                assert_eq!(checks.len(), 2);
            }
            _ => panic!("Expected Any check"),
        }
    }

    #[test]
    fn nested_combinators_deserialize() {
        let yaml = r#"
            type: all
            checks:
              - type: presence
                target: node_modules
              - type: any
                checks:
                  - type: presence
                    target: ".ruby-version"
                  - type: presence
                    target: ".tool-versions"
        "#;
        let check: Check = serde_yaml::from_str(yaml).unwrap();
        match &check {
            Check::All { checks, .. } => {
                assert_eq!(checks.len(), 2);
                assert!(matches!(checks[1], Check::Any { .. }));
            }
            _ => panic!("Expected All check"),
        }
    }

    #[test]
    fn check_serializes_roundtrip() {
        let check = Check::Presence {
            name: Some("deps".to_string()),
            target: Some("node_modules".to_string()),
            kind: Some(PresenceKind::File),
            command: None,
        };
        let yaml = serde_yaml::to_string(&check).unwrap();
        let deserialized: Check = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(deserialized.name(), Some("deps"));
    }

    // --- CheckResult tests ---

    #[test]
    fn check_result_passed() {
        let result = CheckResult::passed("node_modules exists");
        assert!(result.passed_check());
        assert_eq!(result.outcome, CheckOutcome::Passed);
        assert_eq!(result.description, "node_modules exists");
        assert!(result.details.is_none());
    }

    #[test]
    fn check_result_failed() {
        let result = CheckResult::failed("node_modules not found", "Expected at ./node_modules");
        assert!(!result.passed_check());
        assert_eq!(result.outcome, CheckOutcome::Failed);
        assert_eq!(
            result.details.as_deref(),
            Some("Expected at ./node_modules")
        );
    }

    #[test]
    fn check_result_indeterminate() {
        let result = CheckResult::indeterminate("Gemfile.lock change check", "No baseline exists");
        assert!(!result.passed_check());
        assert_eq!(
            result.outcome,
            CheckOutcome::Indeterminate("No baseline exists".to_string())
        );
    }

    // --- SatisfactionCondition tests ---

    #[test]
    fn satisfaction_ref_deserializes() {
        let yaml = r#"
            ref: deps_installed
        "#;
        let cond: SatisfactionCondition = serde_yaml::from_str(yaml).unwrap();
        match cond {
            SatisfactionCondition::Ref { check_ref } => {
                assert_eq!(check_ref, "deps_installed");
            }
            _ => panic!("Expected Ref"),
        }
    }

    #[test]
    fn satisfaction_cross_step_ref_deserializes() {
        let yaml = r#"
            ref: install_deps.deps_present
        "#;
        let cond: SatisfactionCondition = serde_yaml::from_str(yaml).unwrap();
        match cond {
            SatisfactionCondition::Ref { check_ref } => {
                assert_eq!(check_ref, "install_deps.deps_present");
            }
            _ => panic!("Expected Ref"),
        }
    }

    #[test]
    fn satisfaction_inline_check_deserializes() {
        let yaml = r#"
            type: presence
            target: node_modules
        "#;
        let cond: SatisfactionCondition = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(
            cond,
            SatisfactionCondition::Check(Check::Presence { .. })
        ));
    }

    // --- OnChange serde tests ---

    #[test]
    fn on_change_proceed_deserializes() {
        let yaml = "\"proceed\"";
        let on_change: OnChange = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(on_change, OnChange::Proceed);
    }

    #[test]
    fn on_change_fail_deserializes() {
        let yaml = "\"fail\"";
        let on_change: OnChange = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(on_change, OnChange::Fail);
    }

    // --- Validation mode tests ---

    #[test]
    fn validation_mode_default_is_success() {
        assert_eq!(ValidationMode::default(), ValidationMode::Success);
    }

    // --- BaselineConfig tests ---

    #[test]
    fn baseline_default_is_each_run() {
        assert_eq!(BaselineConfig::default(), BaselineConfig::EachRun);
    }

    // --- SizeLimit tests ---

    #[test]
    fn size_limit_default_is_50mb() {
        assert_eq!(SizeLimit::default().max_bytes, Some(50 * 1024 * 1024));
    }

    // --- Serde roundtrip tests for OnChange, BaselineConfig, SizeLimit ---

    #[test]
    fn on_change_require_serializes_as_string() {
        let require = OnChange::Require;
        let yaml = serde_yaml::to_string(&require).unwrap();
        assert_eq!(yaml.trim(), "require");
        let back: OnChange = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back, require);
    }

    #[test]
    fn on_change_require_deserializes_from_string() {
        let on_change: OnChange = serde_yaml::from_str("require").unwrap();
        assert_eq!(on_change, OnChange::Require);
    }

    #[test]
    fn size_limit_serializes_as_integer() {
        let limit = SizeLimit::bytes(1024);
        let yaml = serde_yaml::to_string(&limit).unwrap();
        assert_eq!(yaml.trim(), "1024");
    }

    #[test]
    fn size_limit_none_serializes_as_null() {
        let limit = SizeLimit::none();
        let yaml = serde_yaml::to_string(&limit).unwrap();
        assert_eq!(yaml.trim(), "null");
    }

    #[test]
    fn size_limit_deserializes_from_integer() {
        let limit: SizeLimit = serde_yaml::from_str("1024").unwrap();
        assert_eq!(limit.max_bytes, Some(1024));
    }

    #[test]
    fn size_limit_deserializes_from_null() {
        let limit: SizeLimit = serde_yaml::from_str("null").unwrap();
        assert_eq!(limit.max_bytes, None);
    }

    #[test]
    fn full_change_check_yaml_roundtrip() {
        let yaml = r#"
            type: change
            target: Gemfile.lock
            on_change: require
            require_step: bundle_install
            baseline_snapshot: v1.0
            size_limit: 52428800
            scope: project
        "#;
        let check: Check = serde_yaml::from_str(yaml).unwrap();
        match &check {
            Check::Change {
                target,
                on_change,
                require_step,
                baseline_snapshot,
                size_limit,
                scope,
                ..
            } => {
                assert_eq!(target, "Gemfile.lock");
                assert_eq!(*on_change, OnChange::Require);
                assert_eq!(require_step.as_deref(), Some("bundle_install"));
                assert_eq!(baseline_snapshot.as_deref(), Some("v1.0"));
                assert_eq!(size_limit.max_bytes, Some(52428800));
                assert_eq!(*scope, SnapshotScope::Project);
            }
            _ => panic!("Expected Change check"),
        }
    }

    #[test]
    fn change_check_with_baseline_git() {
        let yaml = r#"
            type: change
            target: schema.rb
            on_change: fail
            baseline_git: main
        "#;
        let check: Check = serde_yaml::from_str(yaml).unwrap();
        match &check {
            Check::Change {
                baseline_git,
                on_change,
                ..
            } => {
                assert_eq!(baseline_git.as_deref(), Some("main"));
                assert_eq!(*on_change, OnChange::Fail);
            }
            _ => panic!("Expected Change check"),
        }
    }

    // --- PresenceKind inference helper tests ---

    #[test]
    fn presence_check_without_kind_deserializes() {
        let yaml = r#"
            type: presence
            target: node_modules
        "#;
        let check: Check = serde_yaml::from_str(yaml).unwrap();
        match &check {
            Check::Presence { kind, .. } => {
                assert!(kind.is_none(), "Kind should be None when not specified");
            }
            _ => panic!("Expected Presence check"),
        }
    }

    // --- Check list (implicit all) deserialization ---

    #[test]
    fn check_list_deserializes_as_vec() {
        let yaml = r#"
            - type: presence
              target: node_modules
            - type: execution
              command: "yarn check"
        "#;
        let checks: Vec<Check> = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(checks.len(), 2);
        assert!(matches!(checks[0], Check::Presence { .. }));
        assert!(matches!(checks[1], Check::Execution { .. }));
    }

    // --- SatisfactionCondition list deserialization ---

    #[test]
    fn satisfaction_condition_list_deserializes() {
        let yaml = r#"
            - ref: deps_installed
            - type: presence
              target: vendor/bundle
        "#;
        let conditions: Vec<SatisfactionCondition> = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(conditions.len(), 2);
        assert!(matches!(conditions[0], SatisfactionCondition::Ref { .. }));
        assert!(matches!(
            conditions[1],
            SatisfactionCondition::Check(Check::Presence { .. })
        ));
    }

    // --- Change check with all fields ---

    #[test]
    fn change_check_with_all_fields() {
        let yaml = r#"
            type: change
            name: lockfile_changed
            target: Gemfile.lock
            kind: file
            on_change: proceed
            baseline: each_run
            scope: workflow
        "#;
        let check: Check = serde_yaml::from_str(yaml).unwrap();
        match &check {
            Check::Change {
                name,
                target,
                kind,
                on_change,
                baseline,
                scope,
                ..
            } => {
                assert_eq!(name.as_deref(), Some("lockfile_changed"));
                assert_eq!(target, "Gemfile.lock");
                assert_eq!(*kind, ChangeKind::File);
                assert_eq!(*on_change, OnChange::Proceed);
                assert_eq!(*baseline, BaselineConfig::EachRun);
                assert_eq!(*scope, SnapshotScope::Workflow);
            }
            _ => panic!("Expected Change check"),
        }
    }

    // --- has_named_checks tests ---

    #[test]
    fn has_named_checks_true_for_named_check() {
        let check = Check::Presence {
            name: Some("deps".to_string()),
            target: Some("node_modules".to_string()),
            kind: Some(PresenceKind::File),
            command: None,
        };
        assert!(check.has_named_checks());
    }

    #[test]
    fn has_named_checks_false_for_unnamed_check() {
        let check = Check::Presence {
            name: None,
            target: Some("node_modules".to_string()),
            kind: Some(PresenceKind::File),
            command: None,
        };
        assert!(!check.has_named_checks());
    }

    #[test]
    fn has_named_checks_recurses_into_combinators() {
        let check = Check::All {
            name: None,
            checks: vec![
                Check::Presence {
                    name: None,
                    target: Some("a".to_string()),
                    kind: None,
                    command: None,
                },
                Check::Presence {
                    name: Some("b_exists".to_string()),
                    target: Some("b".to_string()),
                    kind: None,
                    command: None,
                },
            ],
        };
        assert!(check.has_named_checks());
    }

    #[test]
    fn has_named_checks_false_for_all_unnamed_in_combinator() {
        let check = Check::All {
            name: None,
            checks: vec![
                Check::Presence {
                    name: None,
                    target: Some("a".to_string()),
                    kind: None,
                    command: None,
                },
                Check::Presence {
                    name: None,
                    target: Some("b".to_string()),
                    kind: None,
                    command: None,
                },
            ],
        };
        assert!(!check.has_named_checks());
    }
}
