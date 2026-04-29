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
    /// Execute the step (user explicitly requested, e.g. via prompt response).
    Run,
    /// Auto-run the step without prompting. The decision engine determined the
    /// step is not satisfied and should execute.
    AutoRun,
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
    /// A dependency of this step was skipped by the user and is not satisfied.
    DependencySkipped,
    /// A precondition check failed.
    PreconditionFailed,
    /// A dependency was skipped and its `satisfied_when` conditions failed.
    DependencyUnsatisfied,
}

impl BlockReason {
    /// User-facing message for the block reason.
    pub fn message(&self) -> &'static str {
        match self {
            BlockReason::DependencyFailed => "Blocked (dependency failed)",
            BlockReason::DependencySkipped => "Skipped (dependency skipped)",
            BlockReason::PreconditionFailed => "Blocked (precondition failed)",
            BlockReason::DependencyUnsatisfied => "Blocked (dependency not satisfied)",
        }
    }
}

/// Why a step was skipped without execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// The step's completed check passed (work already done).
    CheckPassed,
    /// The step's `satisfied_when` conditions all passed (purpose already fulfilled).
    Satisfied,
    /// The decision engine auto-determined that the step is satisfied
    /// (via computed satisfaction: checks, rerun window, or explicit conditions).
    AutoSatisfied,
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
            SkipReason::Satisfied => "Already satisfied",
            SkipReason::AutoSatisfied => "Already satisfied",
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
///
/// A user-skipped dependency only blocks dependents if it is NOT satisfied.
/// If the skipped dependency's `satisfied_when` conditions passed, dependents
/// can proceed — the step's purpose is already fulfilled.
pub fn blocked_by_user_skip(
    step: &ResolvedStep,
    user_skipped: &HashSet<String>,
    satisfied_steps: &HashSet<String>,
) -> bool {
    step.depends_on
        .iter()
        .any(|dep| user_skipped.contains(dep) && !satisfied_steps.contains(dep))
}

/// Resolve the effective `auto_run` value for a step, considering
/// step-level overrides from workflow environments.
pub fn effective_auto_run(
    step: &ResolvedStep,
    step_name: &str,
    step_overrides: &std::collections::HashMap<String, StepOverride>,
) -> bool {
    step_overrides
        .get(step_name)
        .and_then(|o| o.auto_run)
        .unwrap_or(step.behavior.auto_run)
}

/// Resolve the effective `prompt_on_rerun` value for a step, considering
/// step-level overrides from workflow environments.
pub fn effective_prompt_on_rerun(
    step: &ResolvedStep,
    step_name: &str,
    step_overrides: &std::collections::HashMap<String, StepOverride>,
) -> bool {
    step_overrides
        .get(step_name)
        .and_then(|o| o.prompt_on_rerun)
        .unwrap_or(step.behavior.prompt_on_rerun)
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
            satisfied_when: vec![],
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
    fn blocked_by_user_skip_when_dep_skipped_and_unsatisfied() {
        let step = make_step("b", vec!["a".to_string()]);
        let mut skipped = HashSet::new();
        skipped.insert("a".to_string());
        let satisfied = HashSet::new();
        assert!(blocked_by_user_skip(&step, &skipped, &satisfied));
    }

    #[test]
    fn not_blocked_when_dep_skipped_but_satisfied() {
        let step = make_step("b", vec!["a".to_string()]);
        let mut skipped = HashSet::new();
        skipped.insert("a".to_string());
        let mut satisfied = HashSet::new();
        satisfied.insert("a".to_string());
        assert!(!blocked_by_user_skip(&step, &skipped, &satisfied));
    }

    #[test]
    fn not_blocked_when_dep_not_skipped() {
        let step = make_step("b", vec!["a".to_string()]);
        let skipped = HashSet::new();
        let satisfied = HashSet::new();
        assert!(!blocked_by_user_skip(&step, &skipped, &satisfied));
    }

    #[test]
    fn effective_auto_run_uses_step_default() {
        let step = make_step("a", vec![]);
        let overrides = HashMap::new();
        // Default ResolvedBehavior has auto_run: true
        assert!(effective_auto_run(&step, "a", &overrides));
    }

    #[test]
    fn effective_auto_run_override_takes_precedence() {
        let step = make_step("a", vec![]);
        let mut overrides = HashMap::new();
        overrides.insert(
            "a".to_string(),
            StepOverride {
                auto_run: Some(false),
                ..Default::default()
            },
        );
        assert!(!effective_auto_run(&step, "a", &overrides));
    }

    #[test]
    fn effective_auto_run_step_false_no_override() {
        let mut step = make_step("a", vec![]);
        step.behavior.auto_run = false;
        let overrides = HashMap::new();
        assert!(!effective_auto_run(&step, "a", &overrides));
    }

    #[test]
    fn effective_auto_run_step_false_override_true() {
        let mut step = make_step("a", vec![]);
        step.behavior.auto_run = false;
        let mut overrides = HashMap::new();
        overrides.insert(
            "a".to_string(),
            StepOverride {
                auto_run: Some(true),
                ..Default::default()
            },
        );
        assert!(effective_auto_run(&step, "a", &overrides));
    }

    #[test]
    fn effective_prompt_uses_step_default() {
        let step = make_step("a", vec![]);
        let overrides = HashMap::new();
        assert!(!effective_prompt_on_rerun(&step, "a", &overrides));
    }

    #[test]
    fn effective_prompt_override_takes_precedence() {
        let step = make_step("a", vec![]);
        let mut overrides = HashMap::new();
        overrides.insert(
            "a".to_string(),
            StepOverride {
                prompt_on_rerun: Some(false),
                ..Default::default()
            },
        );
        assert!(!effective_prompt_on_rerun(&step, "a", &overrides));
    }

    #[test]
    fn skip_reason_messages() {
        assert_eq!(SkipReason::CheckPassed.message(), "Already complete");
        assert_eq!(SkipReason::Satisfied.message(), "Already satisfied");
        assert_eq!(SkipReason::AutoSatisfied.message(), "Already satisfied");
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
        assert_eq!(
            BlockReason::DependencyUnsatisfied.message(),
            "Blocked (dependency not satisfied)"
        );
    }

    #[test]
    fn blocked_by_user_skip_multiple_deps_one_satisfied() {
        let step = make_step("c", vec!["a".to_string(), "b".to_string()]);
        let mut skipped = HashSet::new();
        skipped.insert("a".to_string());
        skipped.insert("b".to_string());
        let mut satisfied = HashSet::new();
        satisfied.insert("a".to_string());
        // "a" is satisfied but "b" is not — still blocked
        assert!(blocked_by_user_skip(&step, &skipped, &satisfied));
    }

    #[test]
    fn not_blocked_when_all_skipped_deps_satisfied() {
        let step = make_step("c", vec!["a".to_string(), "b".to_string()]);
        let mut skipped = HashSet::new();
        skipped.insert("a".to_string());
        skipped.insert("b".to_string());
        let mut satisfied = HashSet::new();
        satisfied.insert("a".to_string());
        satisfied.insert("b".to_string());
        assert!(!blocked_by_user_skip(&step, &skipped, &satisfied));
    }

    #[test]
    fn step_decision_variants() {
        let run = StepDecision::Run;
        assert_eq!(run, StepDecision::Run);

        let auto_run = StepDecision::AutoRun;
        assert_eq!(auto_run, StepDecision::AutoRun);

        let skip = StepDecision::Skip {
            reason: SkipReason::CheckPassed,
        };
        assert!(matches!(skip, StepDecision::Skip { .. }));

        let auto_satisfied = StepDecision::Skip {
            reason: SkipReason::AutoSatisfied,
        };
        assert!(matches!(
            auto_satisfied,
            StepDecision::Skip {
                reason: SkipReason::AutoSatisfied
            }
        ));

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
