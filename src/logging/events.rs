//! Structured event types for bivvy session logging.
//!
//! Events are emitted by any bivvy command — not just workflow execution.
//! They are consumed by the [`EventLogger`](super::EventLogger) (JSONL writer),
//! the state recorder, and the presenter independently.
//!
//! # Design
//!
//! Events use owned data rather than references to simplify consumer
//! implementations. Each event carries enough context to be meaningful
//! in isolation (when read from a JSONL log file).

use serde::Serialize;

/// A structured event emitted during a bivvy session.
///
/// Any bivvy operation — running a workflow, running a single step,
/// taking a snapshot, evaluating a check — emits events. The event
/// type is `BivvyEvent`, not `WorkflowEvent`, because events can
/// originate from any command.
///
/// # Consumers
///
/// Events are consumed by:
/// 1. **Event logger** — writes all events to JSONL for debugging/auditing
/// 2. **State recorder** — listens for completion events to update persistent state
/// 3. **Presenter** — listens for events to show real-time progress
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BivvyEvent {
    // --- Session lifecycle ---
    /// A bivvy session has started.
    SessionStarted {
        /// The CLI command being run (e.g., "run", "snapshot", "lint").
        command: String,
        /// Command-line arguments.
        args: Vec<String>,
        /// Bivvy version.
        version: String,
        /// Operating system identifier.
        #[serde(skip_serializing_if = "Option::is_none")]
        os: Option<String>,
        /// Working directory.
        #[serde(skip_serializing_if = "Option::is_none")]
        working_directory: Option<String>,
    },

    /// A bivvy session has ended.
    SessionEnded {
        /// Process exit code.
        exit_code: i32,
        /// Total session duration in milliseconds.
        duration_ms: u64,
    },

    /// Configuration was loaded and parsed.
    ConfigLoaded {
        /// Path to the config file.
        config_path: String,
        /// Parse duration in milliseconds.
        #[serde(skip_serializing_if = "Option::is_none")]
        parse_duration_ms: Option<u64>,
        /// Deprecation warnings emitted during parsing.
        #[serde(skip_serializing_if = "Vec::is_empty")]
        deprecation_warnings: Vec<String>,
    },

    // --- Check evaluation ---
    /// A check was evaluated (any context — workflow, lint, etc.).
    CheckEvaluated {
        /// Step this check belongs to.
        step: String,
        /// Optional check name (if the check was named).
        #[serde(skip_serializing_if = "Option::is_none")]
        check_name: Option<String>,
        /// Type of check evaluated (e.g., "presence", "execution", "change").
        check_type: String,
        /// Outcome: "passed", "failed", or "indeterminate".
        outcome: String,
        /// Human-readable description of what was checked.
        description: String,
        /// Optional details (e.g., error message, file path).
        #[serde(skip_serializing_if = "Option::is_none")]
        details: Option<String>,
        /// Evaluation duration in milliseconds.
        #[serde(skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
    },

    /// A precondition was evaluated.
    PreconditionEvaluated {
        /// Step this precondition belongs to.
        step: String,
        /// Type of check used as precondition.
        check_type: String,
        /// Outcome: "passed", "failed", or "indeterminate".
        outcome: String,
        /// Human-readable description.
        description: String,
    },

    /// Satisfaction conditions for a step were evaluated.
    SatisfactionEvaluated {
        /// Step whose satisfaction was evaluated.
        step: String,
        /// Whether all conditions were satisfied.
        satisfied: bool,
        /// Number of conditions evaluated.
        condition_count: usize,
        /// Number of conditions that passed.
        passed_count: usize,
    },

    // --- Step lifecycle ---
    /// A step was included in the execution plan.
    StepPlanned {
        /// Step name.
        name: String,
        /// Position in execution order (0-based).
        index: usize,
        /// Total number of steps in the plan.
        total: usize,
    },

    /// A step was filtered out of the execution plan.
    StepFilteredOut {
        /// Step name.
        name: String,
        /// Why it was filtered (e.g., "environment", "only_filter", "skip_filter").
        reason: String,
    },

    /// The orchestrator decided what to do with a step.
    StepDecided {
        /// Step name.
        name: String,
        /// Decision: "run", "skip", "prompt", or "block".
        decision: String,
        /// Reason for the decision.
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
        /// Full decision trace with all contributing signals.
        #[serde(skip_serializing_if = "Option::is_none")]
        trace: Option<DecisionTrace>,
    },

    /// A step is about to start executing.
    StepStarting {
        /// Step name.
        name: String,
    },

    /// A line of output from a running step.
    StepOutput {
        /// Step name.
        name: String,
        /// Output stream: "stdout" or "stderr".
        stream: String,
        /// The output line content.
        line: String,
    },

    /// A step finished executing.
    StepCompleted {
        /// Step name.
        name: String,
        /// Whether the step succeeded.
        success: bool,
        /// Exit code (if a command was run).
        #[serde(skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
        /// Execution duration in milliseconds.
        duration_ms: u64,
        /// Error message (if failed).
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },

    /// A step was skipped (not executed).
    StepSkipped {
        /// Step name.
        name: String,
        /// Why it was skipped.
        reason: String,
    },

    /// The terminal outcome of a step. Emitted exactly once per step at the
    /// point the runtime decides its final state.
    ///
    /// This event is the single source of truth for "what happened to step X"
    /// in any post-hoc consumer (`bivvy last`, `bivvy history`, etc.). It is
    /// additive — the older fine-grained events (`StepCompleted`, `StepSkipped`,
    /// `StepFilteredOut`, `StepDecided`, `DependencyBlocked`, ...) still carry
    /// timing and decision-reasoning detail and continue to be emitted.
    StepOutcome {
        /// Step name.
        name: String,
        /// Typed terminal state.
        outcome: StepOutcomeKind,
        /// Human-readable detail (success message, skip reason, block reason).
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
        /// Execution duration in milliseconds (only set when the step ran).
        #[serde(skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
    },

    /// A rerun was detected for a step.
    RerunDetected {
        /// Step name.
        name: String,
        /// When the step last ran (ISO 8601).
        last_run: String,
        /// Time since last run, in a human-readable string (e.g., "2 minutes ago").
        time_since: String,
    },

    /// A step was blocked by an unsatisfied dependency.
    DependencyBlocked {
        /// Step that was blocked.
        name: String,
        /// The dependency step that caused the block.
        blocked_by: String,
        /// Why the dependency blocked this step.
        reason: String,
    },

    /// A requirement gap was detected for a step.
    RequirementGap {
        /// Step name.
        name: String,
        /// The requirement that is missing (e.g., "ruby", "node").
        requirement: String,
        /// Status of the requirement (e.g., "not_found", "wrong_version").
        status: String,
    },

    // --- User interaction ---
    /// The user was prompted for input.
    UserPrompted {
        /// Step context (if the prompt is step-related).
        #[serde(skip_serializing_if = "Option::is_none")]
        step: Option<String>,
        /// The prompt text shown to the user.
        prompt: String,
        /// Options presented (if applicable).
        #[serde(skip_serializing_if = "Vec::is_empty")]
        options: Vec<String>,
    },

    /// The user responded to a prompt.
    UserResponded {
        /// Step context (if the prompt was step-related).
        #[serde(skip_serializing_if = "Option::is_none")]
        step: Option<String>,
        /// The user's input value.
        input: String,
        /// How the user provided input.
        method: InputMethod,
    },

    // --- Snapshots ---
    /// A baseline was established for the first time.
    BaselineEstablished {
        /// Step this baseline belongs to.
        step: String,
        /// What was hashed (file path, glob, command).
        target: String,
        /// The computed hash.
        hash: String,
        /// Scope: "project" or "workflow:<name>".
        scope: String,
    },

    /// A baseline was updated after a successful step execution.
    BaselineUpdated {
        /// Step this baseline belongs to.
        step: String,
        /// What was hashed.
        target: String,
        /// Previous hash value.
        old_hash: String,
        /// New hash value.
        new_hash: String,
    },

    /// A named snapshot was captured via `bivvy snapshot`.
    SnapshotCaptured {
        /// Snapshot slug.
        slug: String,
        /// Step this snapshot belongs to.
        step: String,
        /// What was hashed.
        target: String,
        /// The computed hash.
        hash: String,
    },

    // --- Recovery ---
    /// Recovery flow started for a failed step.
    RecoveryStarted {
        /// Step that failed.
        step: String,
        /// Error that triggered recovery.
        error: String,
    },

    /// A recovery action was taken.
    RecoveryActionTaken {
        /// Step being recovered.
        step: String,
        /// Action: "retry", "fix", "custom_fix", "skip", "shell", "abort".
        action: String,
        /// Command run for fix/custom_fix actions.
        #[serde(skip_serializing_if = "Option::is_none")]
        command: Option<String>,
    },

    // --- Workflow (only when running a workflow) ---
    /// A workflow execution started.
    WorkflowStarted {
        /// Workflow name.
        name: String,
        /// Number of steps in the plan.
        step_count: usize,
    },

    /// A workflow execution completed.
    WorkflowCompleted {
        /// Workflow name.
        name: String,
        /// Whether all steps succeeded.
        success: bool,
        /// Whether the user aborted.
        aborted: bool,
        /// Number of steps that ran.
        steps_run: usize,
        /// Number of steps skipped.
        steps_skipped: usize,
        /// Total duration in milliseconds.
        duration_ms: u64,
    },
}

impl BivvyEvent {
    /// Returns the event type name as used in JSONL output.
    pub fn type_name(&self) -> &'static str {
        match self {
            BivvyEvent::SessionStarted { .. } => "session_started",
            BivvyEvent::SessionEnded { .. } => "session_ended",
            BivvyEvent::ConfigLoaded { .. } => "config_loaded",
            BivvyEvent::CheckEvaluated { .. } => "check_evaluated",
            BivvyEvent::PreconditionEvaluated { .. } => "precondition_evaluated",
            BivvyEvent::SatisfactionEvaluated { .. } => "satisfaction_evaluated",
            BivvyEvent::StepPlanned { .. } => "step_planned",
            BivvyEvent::StepFilteredOut { .. } => "step_filtered_out",
            BivvyEvent::StepDecided { .. } => "step_decided",
            BivvyEvent::StepStarting { .. } => "step_starting",
            BivvyEvent::StepOutput { .. } => "step_output",
            BivvyEvent::StepCompleted { .. } => "step_completed",
            BivvyEvent::StepSkipped { .. } => "step_skipped",
            BivvyEvent::StepOutcome { .. } => "step_outcome",
            BivvyEvent::RerunDetected { .. } => "rerun_detected",
            BivvyEvent::DependencyBlocked { .. } => "dependency_blocked",
            BivvyEvent::RequirementGap { .. } => "requirement_gap",
            BivvyEvent::UserPrompted { .. } => "user_prompted",
            BivvyEvent::UserResponded { .. } => "user_responded",
            BivvyEvent::BaselineEstablished { .. } => "baseline_established",
            BivvyEvent::BaselineUpdated { .. } => "baseline_updated",
            BivvyEvent::SnapshotCaptured { .. } => "snapshot_captured",
            BivvyEvent::RecoveryStarted { .. } => "recovery_started",
            BivvyEvent::RecoveryActionTaken { .. } => "recovery_action_taken",
            BivvyEvent::WorkflowStarted { .. } => "workflow_started",
            BivvyEvent::WorkflowCompleted { .. } => "workflow_completed",
        }
    }
}

/// The terminal state of a step in a workflow run.
///
/// Used by [`BivvyEvent::StepOutcome`] to give post-hoc consumers
/// (`bivvy last`, `bivvy history`) a single typed signal per step that
/// matches the visual state shown by `bivvy run`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepOutcomeKind {
    /// Step ran and succeeded.
    Completed,
    /// Step ran and failed.
    Failed,
    /// Step's check passed — work was already done, no execution needed.
    Satisfied,
    /// User declined to run the step at a prompt.
    Declined,
    /// Step was filtered out before execution by `--skip` or environment scoping.
    FilteredOut,
    /// Step could not run because a dependency failed, was skipped without
    /// satisfying its purpose, or a precondition failed.
    Blocked,
}

impl StepOutcomeKind {
    /// Stable string form used in JSONL serialization. Matches the serde
    /// representation so the same string round-trips through `as_str` and
    /// the JSON tag.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Satisfied => "satisfied",
            Self::Declined => "declined",
            Self::FilteredOut => "filtered_out",
            Self::Blocked => "blocked",
        }
    }
}

impl std::str::FromStr for StepOutcomeKind {
    type Err = ();

    /// Parse the JSON tag form back into a [`StepOutcomeKind`]. Errors
    /// for any string that is not one of the six known variant tags.
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "satisfied" => Ok(Self::Satisfied),
            "declined" => Ok(Self::Declined),
            "filtered_out" => Ok(Self::FilteredOut),
            "blocked" => Ok(Self::Blocked),
            _ => Err(()),
        }
    }
}

/// Every signal that contributed to a [`BivvyEvent::StepDecided`] decision.
///
/// Logged so the full reasoning is reconstructable from the event log.
/// A `CheckResult` is ONE signal among many — this struct captures them all.
#[derive(Debug, Clone, Default, Serialize)]
pub struct DecisionTrace {
    /// Results of evaluating the step's check/checks (if any).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub check_results: Vec<NamedCheckResult>,

    /// Result of evaluating preconditions (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub precondition_result: Option<TraceCheckResult>,

    /// Whether satisfied_when conditions passed (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub satisfaction: Option<SatisfactionResult>,

    /// Status of each dependency (satisfied, ran, failed, not run).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dependency_statuses: Vec<DependencyStatus>,

    /// Whether this is a rerun (and how recently the step last ran).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rerun_status: Option<RerunInfo>,

    /// Whether the step is skippable, required, force-flagged.
    pub behavior_flags: BehaviorFlags,

    /// Requirement gap check results (missing binaries, etc.).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub requirement_gaps: Vec<RequirementGapInfo>,

    /// Environment filter result (included or excluded by environment).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment_filter: Option<FilterResult>,
}

/// A named check result within a decision trace.
#[derive(Debug, Clone, Serialize)]
pub struct NamedCheckResult {
    /// Optional check name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Check type (e.g., "presence", "execution", "change").
    pub check_type: String,
    /// Outcome: "passed", "failed", or "indeterminate".
    pub outcome: String,
    /// Human-readable description.
    pub description: String,
}

/// A check result within a decision trace (for preconditions).
#[derive(Debug, Clone, Serialize)]
pub struct TraceCheckResult {
    /// Check type.
    pub check_type: String,
    /// Outcome: "passed", "failed", or "indeterminate".
    pub outcome: String,
    /// Human-readable description.
    pub description: String,
}

/// Satisfaction evaluation result within a decision trace.
#[derive(Debug, Clone, Serialize)]
pub struct SatisfactionResult {
    /// Whether all conditions were satisfied.
    pub satisfied: bool,
    /// Number of conditions evaluated.
    pub condition_count: usize,
    /// Number of conditions that passed.
    pub passed_count: usize,
}

/// Status of a dependency within a decision trace.
#[derive(Debug, Clone, Serialize)]
pub struct DependencyStatus {
    /// Dependency step name.
    pub step: String,
    /// Status: "satisfied", "completed", "failed", "skipped", "not_run".
    pub status: String,
}

/// Rerun information within a decision trace.
#[derive(Debug, Clone, Serialize)]
pub struct RerunInfo {
    /// When the step last ran (ISO 8601).
    pub last_run: String,
    /// Time since last run, human-readable.
    pub time_since: String,
}

/// Behavior flags within a decision trace.
#[derive(Debug, Clone, Default, Serialize)]
pub struct BehaviorFlags {
    /// Whether the step is skippable.
    pub skippable: bool,
    /// Whether the step is required.
    pub required: bool,
    /// Whether --force was applied to this step.
    pub forced: bool,
    /// Whether auto_run is active for this step.
    pub auto_run: bool,
    /// Whether prompt_on_rerun is active.
    pub prompt_on_rerun: bool,
    /// Whether `confirm: true` is set (always prompt before running).
    pub confirm: bool,
}

/// Requirement gap info within a decision trace.
#[derive(Debug, Clone, Serialize)]
pub struct RequirementGapInfo {
    /// The requirement that is missing.
    pub requirement: String,
    /// Status of the requirement.
    pub status: String,
}

/// Environment filter result within a decision trace.
#[derive(Debug, Clone, Serialize)]
pub struct FilterResult {
    /// Whether the step was included.
    pub included: bool,
    /// The active environment (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_environment: Option<String>,
}

/// How the user provided input in response to a prompt.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum InputMethod {
    /// Single keypress (e.g., 'y', 'n').
    Keypress(char),
    /// Arrow key selection + enter.
    ArrowSelect,
    /// Typed text + enter. Contains the typed string.
    TypedInput(String),
}

/// Trait for consuming bivvy events.
///
/// Any subsystem can register as an event consumer. The three primary
/// consumers are:
///
/// 1. **State recorder** — updates persistent state on step completion
/// 2. **Presenter (UI)** — shows real-time progress and prompts
/// 3. **Event logger** — writes all events to JSONL
///
/// Consumers handle only the events they care about and ignore the rest.
pub trait EventConsumer: Send {
    /// Process an event.
    ///
    /// Implementations should be fast and non-blocking. The event logger
    /// writes synchronously (one JSON line per event is negligible overhead).
    fn on_event(&mut self, event: &BivvyEvent);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_started_serializes_to_json() {
        let event = BivvyEvent::SessionStarted {
            command: "run".to_string(),
            args: vec!["--verbose".to_string()],
            version: "1.9.0".to_string(),
            os: None,
            working_directory: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "session_started");
        assert_eq!(value["command"], "run");
        assert_eq!(value["args"][0], "--verbose");
        assert_eq!(value["version"], "1.9.0");
    }

    #[test]
    fn session_ended_serializes_to_json() {
        let event = BivvyEvent::SessionEnded {
            exit_code: 0,
            duration_ms: 1234,
        };
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "session_ended");
        assert_eq!(value["exit_code"], 0);
        assert_eq!(value["duration_ms"], 1234);
    }

    #[test]
    fn check_evaluated_serializes_with_optional_fields() {
        let event = BivvyEvent::CheckEvaluated {
            step: "install_deps".to_string(),
            check_name: Some("deps_installed".to_string()),
            check_type: "presence".to_string(),
            outcome: "passed".to_string(),
            description: "node_modules exists".to_string(),
            details: None,
            duration_ms: Some(5),
        };
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "check_evaluated");
        assert_eq!(value["step"], "install_deps");
        assert_eq!(value["check_name"], "deps_installed");
        assert_eq!(value["outcome"], "passed");
        // details should be absent when None
        assert!(value.get("details").is_none());
    }

    #[test]
    fn check_evaluated_omits_none_fields() {
        let event = BivvyEvent::CheckEvaluated {
            step: "build".to_string(),
            check_name: None,
            check_type: "execution".to_string(),
            outcome: "failed".to_string(),
            description: "cargo build failed".to_string(),
            details: Some("exit code 1".to_string()),
            duration_ms: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(value.get("check_name").is_none());
        assert!(value.get("duration_ms").is_none());
        assert_eq!(value["details"], "exit code 1");
    }

    #[test]
    fn precondition_evaluated_serializes() {
        let event = BivvyEvent::PreconditionEvaluated {
            step: "db_migrate".to_string(),
            check_type: "execution".to_string(),
            outcome: "passed".to_string(),
            description: "pg_isready -q succeeded".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "precondition_evaluated");
        assert_eq!(value["step"], "db_migrate");
    }

    #[test]
    fn satisfaction_evaluated_serializes() {
        let event = BivvyEvent::SatisfactionEvaluated {
            step: "install_deps".to_string(),
            satisfied: true,
            condition_count: 2,
            passed_count: 2,
        };
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "satisfaction_evaluated");
        assert_eq!(value["satisfied"], true);
        assert_eq!(value["condition_count"], 2);
        assert_eq!(value["passed_count"], 2);
    }

    #[test]
    fn step_planned_serializes() {
        let event = BivvyEvent::StepPlanned {
            name: "build".to_string(),
            index: 2,
            total: 5,
        };
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "step_planned");
        assert_eq!(value["name"], "build");
        assert_eq!(value["index"], 2);
        assert_eq!(value["total"], 5);
    }

    #[test]
    fn step_filtered_out_serializes() {
        let event = BivvyEvent::StepFilteredOut {
            name: "deploy".to_string(),
            reason: "environment".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "step_filtered_out");
        assert_eq!(value["reason"], "environment");
    }

    #[test]
    fn step_decided_serializes_with_reason() {
        let event = BivvyEvent::StepDecided {
            name: "db_migrate".to_string(),
            decision: "block".to_string(),
            reason: Some("dependency_unsatisfied".to_string()),
            trace: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "step_decided");
        assert_eq!(value["decision"], "block");
        assert_eq!(value["reason"], "dependency_unsatisfied");
    }

    #[test]
    fn step_decided_omits_none_reason() {
        let event = BivvyEvent::StepDecided {
            name: "build".to_string(),
            decision: "run".to_string(),
            reason: None,
            trace: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(value.get("reason").is_none());
        assert!(value.get("trace").is_none());
    }

    #[test]
    fn step_decided_serializes_with_trace() {
        let trace = DecisionTrace {
            check_results: vec![NamedCheckResult {
                name: Some("deps_installed".to_string()),
                check_type: "presence".to_string(),
                outcome: "passed".to_string(),
                description: "\u{2713} node_modules exists".to_string(),
            }],
            behavior_flags: BehaviorFlags {
                skippable: true,
                required: false,
                forced: false,
                auto_run: true,
                prompt_on_rerun: true,
                confirm: false,
            },
            satisfaction: Some(SatisfactionResult {
                satisfied: true,
                condition_count: 1,
                passed_count: 1,
            }),
            ..Default::default()
        };
        let event = BivvyEvent::StepDecided {
            name: "install_deps".to_string(),
            decision: "skip".to_string(),
            reason: Some("check_passed".to_string()),
            trace: Some(trace),
        };
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["trace"]["check_results"][0]["outcome"], "passed");
        assert!(value["trace"]["behavior_flags"]["skippable"]
            .as_bool()
            .unwrap());
        assert!(value["trace"]["satisfaction"]["satisfied"]
            .as_bool()
            .unwrap());
    }

    #[test]
    fn step_starting_serializes() {
        let event = BivvyEvent::StepStarting {
            name: "build".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "step_starting");
        assert_eq!(value["name"], "build");
    }

    #[test]
    fn step_output_serializes() {
        let event = BivvyEvent::StepOutput {
            name: "build".to_string(),
            stream: "stdout".to_string(),
            line: "Compiling bivvy v1.9.0".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "step_output");
        assert_eq!(value["stream"], "stdout");
    }

    #[test]
    fn step_completed_serializes_success() {
        let event = BivvyEvent::StepCompleted {
            name: "build".to_string(),
            success: true,
            exit_code: Some(0),
            duration_ms: 5432,
            error: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "step_completed");
        assert_eq!(value["success"], true);
        assert_eq!(value["exit_code"], 0);
        assert!(value.get("error").is_none());
    }

    #[test]
    fn step_completed_serializes_failure() {
        let event = BivvyEvent::StepCompleted {
            name: "build".to_string(),
            success: false,
            exit_code: Some(1),
            duration_ms: 1000,
            error: Some("cargo build failed".to_string()),
        };
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["success"], false);
        assert_eq!(value["error"], "cargo build failed");
    }

    #[test]
    fn step_skipped_serializes() {
        let event = BivvyEvent::StepSkipped {
            name: "deploy".to_string(),
            reason: "user_declined".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "step_skipped");
        assert_eq!(value["reason"], "user_declined");
    }

    #[test]
    fn config_loaded_serializes() {
        let event = BivvyEvent::ConfigLoaded {
            config_path: ".bivvy/config.yml".to_string(),
            parse_duration_ms: Some(12),
            deprecation_warnings: vec!["'completed_check' is deprecated".to_string()],
        };
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "config_loaded");
        assert_eq!(value["config_path"], ".bivvy/config.yml");
        assert_eq!(value["parse_duration_ms"], 12);
        assert_eq!(
            value["deprecation_warnings"][0],
            "'completed_check' is deprecated"
        );
    }

    #[test]
    fn config_loaded_omits_empty_warnings() {
        let event = BivvyEvent::ConfigLoaded {
            config_path: ".bivvy/config.yml".to_string(),
            parse_duration_ms: None,
            deprecation_warnings: vec![],
        };
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(value.get("parse_duration_ms").is_none());
        assert!(value.get("deprecation_warnings").is_none());
    }

    #[test]
    fn rerun_detected_serializes() {
        let event = BivvyEvent::RerunDetected {
            name: "build".to_string(),
            last_run: "2026-04-25T10:00:00Z".to_string(),
            time_since: "2 minutes ago".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "rerun_detected");
        assert_eq!(value["name"], "build");
        assert_eq!(value["last_run"], "2026-04-25T10:00:00Z");
        assert_eq!(value["time_since"], "2 minutes ago");
    }

    #[test]
    fn dependency_blocked_serializes() {
        let event = BivvyEvent::DependencyBlocked {
            name: "db_seed".to_string(),
            blocked_by: "install_deps".to_string(),
            reason: "not satisfied".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "dependency_blocked");
        assert_eq!(value["name"], "db_seed");
        assert_eq!(value["blocked_by"], "install_deps");
    }

    #[test]
    fn requirement_gap_serializes() {
        let event = BivvyEvent::RequirementGap {
            name: "bundle_install".to_string(),
            requirement: "ruby".to_string(),
            status: "not_found".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "requirement_gap");
        assert_eq!(value["requirement"], "ruby");
        assert_eq!(value["status"], "not_found");
    }

    #[test]
    fn session_started_with_os_and_working_directory() {
        let event = BivvyEvent::SessionStarted {
            command: "run".to_string(),
            args: vec![],
            version: "1.9.0".to_string(),
            os: Some("darwin".to_string()),
            working_directory: Some("/home/user/project".to_string()),
        };
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["os"], "darwin");
        assert_eq!(value["working_directory"], "/home/user/project");
    }

    #[test]
    fn user_prompted_serializes_with_options() {
        let event = BivvyEvent::UserPrompted {
            step: Some("install_deps".to_string()),
            prompt: "Install dependencies?".to_string(),
            options: vec!["Yes (y)".to_string(), "No (n)".to_string()],
        };
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "user_prompted");
        assert_eq!(value["prompt"], "Install dependencies?");
        assert_eq!(value["options"][0], "Yes (y)");
    }

    #[test]
    fn user_prompted_omits_empty_options() {
        let event = BivvyEvent::UserPrompted {
            step: None,
            prompt: "Continue?".to_string(),
            options: vec![],
        };
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(value.get("step").is_none());
        assert!(value.get("options").is_none());
    }

    #[test]
    fn user_responded_keypress_serializes() {
        let event = BivvyEvent::UserResponded {
            step: Some("build".to_string()),
            input: "y".to_string(),
            method: InputMethod::Keypress('y'),
        };
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "user_responded");
        assert_eq!(value["input"], "y");
        assert_eq!(value["method"]["type"], "keypress");
        assert_eq!(value["method"]["value"], "y");
    }

    #[test]
    fn user_responded_arrow_select_serializes() {
        let event = BivvyEvent::UserResponded {
            step: None,
            input: "Skip".to_string(),
            method: InputMethod::ArrowSelect,
        };
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["method"]["type"], "arrow_select");
    }

    #[test]
    fn user_responded_typed_input_serializes() {
        let event = BivvyEvent::UserResponded {
            step: None,
            input: "my-project".to_string(),
            method: InputMethod::TypedInput("my-project".to_string()),
        };
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["method"]["type"], "typed_input");
        assert_eq!(value["method"]["value"], "my-project");
    }

    #[test]
    fn baseline_established_serializes() {
        let event = BivvyEvent::BaselineEstablished {
            step: "install_deps".to_string(),
            target: "Gemfile.lock".to_string(),
            hash: "sha256:abc123".to_string(),
            scope: "project".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "baseline_established");
        assert_eq!(value["hash"], "sha256:abc123");
    }

    #[test]
    fn baseline_updated_serializes() {
        let event = BivvyEvent::BaselineUpdated {
            step: "install_deps".to_string(),
            target: "Gemfile.lock".to_string(),
            old_hash: "sha256:abc".to_string(),
            new_hash: "sha256:def".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "baseline_updated");
        assert_eq!(value["old_hash"], "sha256:abc");
        assert_eq!(value["new_hash"], "sha256:def");
    }

    #[test]
    fn snapshot_captured_serializes() {
        let event = BivvyEvent::SnapshotCaptured {
            slug: "v1.0".to_string(),
            step: "install_deps".to_string(),
            target: "Gemfile.lock".to_string(),
            hash: "sha256:abc123".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "snapshot_captured");
        assert_eq!(value["slug"], "v1.0");
    }

    #[test]
    fn recovery_started_serializes() {
        let event = BivvyEvent::RecoveryStarted {
            step: "build".to_string(),
            error: "exit code 1".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "recovery_started");
    }

    #[test]
    fn recovery_action_taken_serializes_with_command() {
        let event = BivvyEvent::RecoveryActionTaken {
            step: "build".to_string(),
            action: "fix".to_string(),
            command: Some("cargo clean".to_string()),
        };
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "recovery_action_taken");
        assert_eq!(value["action"], "fix");
        assert_eq!(value["command"], "cargo clean");
    }

    #[test]
    fn recovery_action_taken_omits_none_command() {
        let event = BivvyEvent::RecoveryActionTaken {
            step: "build".to_string(),
            action: "retry".to_string(),
            command: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(value.get("command").is_none());
    }

    #[test]
    fn workflow_started_serializes() {
        let event = BivvyEvent::WorkflowStarted {
            name: "default".to_string(),
            step_count: 5,
        };
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "workflow_started");
        assert_eq!(value["step_count"], 5);
    }

    #[test]
    fn workflow_completed_serializes() {
        let event = BivvyEvent::WorkflowCompleted {
            name: "default".to_string(),
            success: true,
            aborted: false,
            steps_run: 4,
            steps_skipped: 1,
            duration_ms: 12345,
        };
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "workflow_completed");
        assert_eq!(value["success"], true);
        assert_eq!(value["aborted"], false);
        assert_eq!(value["steps_run"], 4);
        assert_eq!(value["steps_skipped"], 1);
        assert_eq!(value["duration_ms"], 12345);
    }

    #[test]
    fn type_name_matches_serde_tag() {
        // Verify type_name() matches the serde tag for a representative sample
        let events: Vec<BivvyEvent> = vec![
            BivvyEvent::SessionStarted {
                command: "run".to_string(),
                args: vec![],
                version: "1.0.0".to_string(),
                os: None,
                working_directory: None,
            },
            BivvyEvent::SessionEnded {
                exit_code: 0,
                duration_ms: 0,
            },
            BivvyEvent::CheckEvaluated {
                step: "s".to_string(),
                check_name: None,
                check_type: "presence".to_string(),
                outcome: "passed".to_string(),
                description: "d".to_string(),
                details: None,
                duration_ms: None,
            },
            BivvyEvent::StepCompleted {
                name: "s".to_string(),
                success: true,
                exit_code: None,
                duration_ms: 0,
                error: None,
            },
            BivvyEvent::WorkflowStarted {
                name: "default".to_string(),
                step_count: 0,
            },
        ];

        for event in &events {
            let json = serde_json::to_string(event).unwrap();
            let value: serde_json::Value = serde_json::from_str(&json).unwrap();
            assert_eq!(
                value["type"].as_str().unwrap(),
                event.type_name(),
                "type_name() mismatch for {:?}",
                event.type_name()
            );
        }
    }

    #[test]
    fn input_method_keypress_serializes() {
        let method = InputMethod::Keypress('y');
        let json = serde_json::to_string(&method).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "keypress");
        assert_eq!(value["value"], "y");
    }

    #[test]
    fn input_method_arrow_select_serializes() {
        let method = InputMethod::ArrowSelect;
        let json = serde_json::to_string(&method).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "arrow_select");
    }

    #[test]
    fn input_method_typed_input_serializes() {
        let method = InputMethod::TypedInput("hello".to_string());
        let json = serde_json::to_string(&method).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "typed_input");
    }

    #[test]
    fn step_outcome_kind_as_str_round_trips() {
        use std::str::FromStr;
        let variants = [
            StepOutcomeKind::Completed,
            StepOutcomeKind::Failed,
            StepOutcomeKind::Satisfied,
            StepOutcomeKind::Declined,
            StepOutcomeKind::FilteredOut,
            StepOutcomeKind::Blocked,
        ];
        for v in variants {
            assert_eq!(StepOutcomeKind::from_str(v.as_str()), Ok(v));
        }
    }

    #[test]
    fn step_outcome_kind_as_str_matches_serde_tag() {
        // The string returned by as_str() must equal the serde rename for
        // each variant — that's the contract the JSONL parser relies on.
        for v in [
            StepOutcomeKind::Completed,
            StepOutcomeKind::Failed,
            StepOutcomeKind::Satisfied,
            StepOutcomeKind::Declined,
            StepOutcomeKind::FilteredOut,
            StepOutcomeKind::Blocked,
        ] {
            let serialized = serde_json::to_string(&v).unwrap();
            // Strip surrounding quotes from the JSON string literal.
            let trimmed = serialized.trim_matches('"');
            assert_eq!(trimmed, v.as_str());
        }
    }

    #[test]
    fn step_outcome_kind_from_str_rejects_unknown() {
        use std::str::FromStr;
        assert!(StepOutcomeKind::from_str("nope").is_err());
        assert!(StepOutcomeKind::from_str("").is_err());
    }

    fn assert_outcome_round_trip(outcome: StepOutcomeKind, detail: Option<&str>, dur: Option<u64>) {
        let event = BivvyEvent::StepOutcome {
            name: "step".to_string(),
            outcome,
            detail: detail.map(|s| s.to_string()),
            duration_ms: dur,
        };
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "step_outcome");
        assert_eq!(value["name"], "step");
        assert_eq!(value["outcome"], outcome.as_str());
        match detail {
            Some(d) => assert_eq!(value["detail"], d),
            None => assert!(value.get("detail").is_none()),
        }
        match dur {
            Some(ms) => assert_eq!(value["duration_ms"], ms),
            None => assert!(value.get("duration_ms").is_none()),
        }
    }

    #[test]
    fn step_outcome_completed_round_trip() {
        assert_outcome_round_trip(StepOutcomeKind::Completed, Some("ran ok"), Some(420));
    }

    #[test]
    fn step_outcome_failed_round_trip() {
        assert_outcome_round_trip(StepOutcomeKind::Failed, Some("exit code 1"), Some(101));
    }

    #[test]
    fn step_outcome_satisfied_round_trip() {
        assert_outcome_round_trip(
            StepOutcomeKind::Satisfied,
            Some("✓ rustc --version succeeded"),
            None,
        );
    }

    #[test]
    fn step_outcome_declined_round_trip() {
        assert_outcome_round_trip(StepOutcomeKind::Declined, Some("user_declined"), None);
    }

    #[test]
    fn step_outcome_filtered_out_round_trip() {
        assert_outcome_round_trip(StepOutcomeKind::FilteredOut, Some("skip_flag"), None);
    }

    #[test]
    fn step_outcome_blocked_round_trip() {
        assert_outcome_round_trip(
            StepOutcomeKind::Blocked,
            Some("dependency 'build' failed"),
            None,
        );
    }

    #[test]
    fn step_outcome_omits_none_optional_fields() {
        let event = BivvyEvent::StepOutcome {
            name: "x".to_string(),
            outcome: StepOutcomeKind::Completed,
            detail: None,
            duration_ms: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(value.get("detail").is_none());
        assert!(value.get("duration_ms").is_none());
    }

    #[test]
    fn step_outcome_type_name_matches_serde_tag() {
        let event = BivvyEvent::StepOutcome {
            name: "x".to_string(),
            outcome: StepOutcomeKind::Completed,
            detail: None,
            duration_ms: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"].as_str().unwrap(), event.type_name());
    }

    /// Verify that a mock EventConsumer can receive events.
    #[test]
    fn event_consumer_trait_works() {
        struct Counter {
            count: usize,
        }
        impl EventConsumer for Counter {
            fn on_event(&mut self, _event: &BivvyEvent) {
                self.count += 1;
            }
        }

        let mut consumer = Counter { count: 0 };
        let event = BivvyEvent::SessionStarted {
            command: "run".to_string(),
            args: vec![],
            version: "1.0.0".to_string(),
            os: None,
            working_directory: None,
        };
        consumer.on_event(&event);
        consumer.on_event(&event);
        assert_eq!(consumer.count, 2);
    }
}
