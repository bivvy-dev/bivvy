//! Step decision logic for workflow execution.
//!
//! This module contains types and helpers for deciding what to do with a step
//! before execution: run it, skip it, block it, or prompt the user.

use std::collections::HashSet;

use crate::config::schema::StepOverride;
use crate::steps::ResolvedStep;

/// What to do with a step before execution.
///
/// Produced by the pre-step evaluation phase. The orchestrator acts on this
/// decision — running, skipping, prompting, or blocking the step accordingly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepDecision {
    /// Execute the step.
    Run,
    /// Auto-skip the step without prompting.
    Skip { reason: SkipReason },
    /// Ask the user whether to run/skip/re-run.
    Prompt { prompt_key: String },
    /// The step cannot proceed — a hard stop.
    Block { reason: BlockReason },
}

/// Why a step was blocked and cannot proceed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockReason {
    /// A dependency of this step failed.
    DependencyFailed,
    /// A dependency of this step was skipped by the user.
    DependencySkipped,
    /// A precondition check failed.
    PreconditionFailed,
}

impl BlockReason {
    /// User-facing message for the block reason.
    pub fn message(&self) -> &'static str {
        match self {
            BlockReason::DependencyFailed => "Blocked (dependency failed)",
            BlockReason::DependencySkipped => "Skipped (dependency skipped)",
            BlockReason::PreconditionFailed => "Blocked (precondition failed)",
        }
    }
}

/// Why a step was skipped without execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// The step's completed check passed (work already done).
    CheckPassed,
    /// The user declined to run the step.
    UserDeclined,
    /// The user declined a sensitive step.
    SensitiveDeclined,
    /// A requirement gap could not be resolved.
    RequirementNotMet,
}

impl SkipReason {
    /// User-facing message for the skip reason.
    pub fn message(&self) -> &'static str {
        match self {
            SkipReason::CheckPassed => "Already complete",
            SkipReason::UserDeclined => "Skipped",
            SkipReason::SensitiveDeclined => "Skipped (declined sensitive step)",
            SkipReason::RequirementNotMet => "Skipped (requirement not met)",
        }
    }
}

/// Check if a step is blocked by a failed dependency.
pub fn blocked_by_failure(step: &ResolvedStep, failed_steps: &HashSet<String>) -> bool {
    step.depends_on.iter().any(|dep| failed_steps.contains(dep))
}

/// Check if a step should be auto-skipped due to a user-skipped dependency.
pub fn blocked_by_user_skip(step: &ResolvedStep, user_skipped: &HashSet<String>) -> bool {
    step.depends_on.iter().any(|dep| user_skipped.contains(dep))
}

/// Resolve the effective `prompt_if_complete` value for a step, considering
/// step-level overrides from workflow environments.
pub fn effective_prompt_if_complete(
    step: &ResolvedStep,
    step_name: &str,
    step_overrides: &std::collections::HashMap<String, StepOverride>,
) -> bool {
    step_overrides
        .get(step_name)
        .and_then(|o| o.prompt_if_complete)
        .unwrap_or(step.behavior.prompt_if_complete)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::steps::{
        ResolvedBehavior, ResolvedEnvironmentVars, ResolvedExecution, ResolvedHooks,
        ResolvedOutput, ResolvedScoping, ResolvedStep,
    };
    use std::collections::HashMap;

    fn make_step(name: &str, depends_on: Vec<String>) -> ResolvedStep {
        ResolvedStep {
            name: name.to_string(),
            title: name.to_string(),
            description: None,
            depends_on,
            requires: vec![],
            inputs: HashMap::new(),
            execution: ResolvedExecution {
                command: format!("echo {}", name),
                ..Default::default()
            },
            env_vars: ResolvedEnvironmentVars::default(),
            behavior: ResolvedBehavior::default(),
            hooks: ResolvedHooks::default(),
            output: ResolvedOutput::default(),
            scoping: ResolvedScoping::default(),
        }
    }

    #[test]
    fn blocked_by_failure_when_dep_failed() {
        let step = make_step("b", vec!["a".to_string()]);
        let mut failed = HashSet::new();
        failed.insert("a".to_string());
        assert!(blocked_by_failure(&step, &failed));
    }

    #[test]
    fn not_blocked_when_dep_succeeded() {
        let step = make_step("b", vec!["a".to_string()]);
        let failed = HashSet::new();
        assert!(!blocked_by_failure(&step, &failed));
    }

    #[test]
    fn blocked_by_user_skip_when_dep_skipped() {
        let step = make_step("b", vec!["a".to_string()]);
        let mut skipped = HashSet::new();
        skipped.insert("a".to_string());
        assert!(blocked_by_user_skip(&step, &skipped));
    }

    #[test]
    fn not_blocked_when_dep_not_skipped() {
        let step = make_step("b", vec!["a".to_string()]);
        let skipped = HashSet::new();
        assert!(!blocked_by_user_skip(&step, &skipped));
    }

    #[test]
    fn effective_prompt_uses_step_default() {
        let step = make_step("a", vec![]);
        let overrides = HashMap::new();
        assert!(effective_prompt_if_complete(&step, "a", &overrides));
    }

    #[test]
    fn effective_prompt_override_takes_precedence() {
        let step = make_step("a", vec![]);
        let mut overrides = HashMap::new();
        overrides.insert(
            "a".to_string(),
            StepOverride {
                prompt_if_complete: Some(false),
                ..Default::default()
            },
        );
        assert!(!effective_prompt_if_complete(&step, "a", &overrides));
    }

    #[test]
    fn skip_reason_messages() {
        assert_eq!(SkipReason::CheckPassed.message(), "Already complete");
        assert_eq!(SkipReason::UserDeclined.message(), "Skipped");
        assert_eq!(
            SkipReason::SensitiveDeclined.message(),
            "Skipped (declined sensitive step)"
        );
        assert_eq!(
            SkipReason::RequirementNotMet.message(),
            "Skipped (requirement not met)"
        );
    }

    #[test]
    fn block_reason_messages() {
        assert_eq!(
            BlockReason::DependencyFailed.message(),
            "Blocked (dependency failed)"
        );
        assert_eq!(
            BlockReason::DependencySkipped.message(),
            "Skipped (dependency skipped)"
        );
        assert_eq!(
            BlockReason::PreconditionFailed.message(),
            "Blocked (precondition failed)"
        );
    }

    #[test]
    fn step_decision_variants() {
        let run = StepDecision::Run;
        assert_eq!(run, StepDecision::Run);

        let skip = StepDecision::Skip {
            reason: SkipReason::CheckPassed,
        };
        assert!(matches!(skip, StepDecision::Skip { .. }));

        let prompt = StepDecision::Prompt {
            prompt_key: "rerun_install".to_string(),
        };
        assert!(matches!(prompt, StepDecision::Prompt { .. }));

        let block = StepDecision::Block {
            reason: BlockReason::DependencyFailed,
        };
        assert!(matches!(block, StepDecision::Block { .. }));
    }
}
