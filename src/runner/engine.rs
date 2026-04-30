//! Step decision engine.
//!
//! The `evaluate_step()` function is the single entry point for the decision matrix.
//! It evaluates a step through the full hierarchy: resolve dependencies (recursive),
//! hard blocks, soft blocks, force, satisfaction, and prompt decisions.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::checks::evaluator::CheckEvaluator;
use crate::checks::CheckResult;
use crate::config::interpolation::InterpolationContext;
use crate::config::schema::StepOverride;
use crate::runner::decision::{BlockReason, SkipReason, StepDecision};
use crate::runner::satisfaction::{self, ComputedSatisfaction};
use crate::runner::RerunWindow;
use crate::snapshots::SnapshotStore;
use crate::state::satisfaction::{SatisfactionCache, SatisfactionRecord};
use crate::state::StateStore;
use crate::steps::ResolvedStep;

/// Result of evaluating a step through the decision engine.
#[derive(Debug, Clone)]
pub struct EvaluationResult {
    /// The decision for this step.
    pub decision: StepDecision,
    /// Why the decision was made (human-readable).
    pub reason: String,
    /// Computed satisfaction details (if satisfaction was evaluated).
    pub satisfaction: Option<ComputedSatisfaction>,
}

/// Context needed by the decision engine to evaluate steps.
pub struct EngineContext<'a> {
    /// All resolved steps in the workflow.
    pub steps: &'a HashMap<String, ResolvedStep>,
    /// Project root directory.
    pub project_root: &'a Path,
    /// Interpolation context for check evaluation.
    pub interpolation: &'a InterpolationContext,
    /// Snapshot store for change checks.
    pub snapshot_store: &'a mut SnapshotStore,
    /// State store for execution history.
    pub state: Option<&'a StateStore>,
    /// Per-step workflow overrides.
    pub step_overrides: &'a HashMap<String, StepOverride>,
    /// Steps forced to re-run via --force flag.
    pub force: &'a HashSet<String>,
    /// Whether every step in this workflow should be forced (--force-all).
    pub force_all: bool,
    /// Pre-evaluated named check results for cross-step refs.
    pub named_check_results: &'a HashMap<String, CheckResult>,
    /// Satisfaction cache (two-layer).
    pub satisfaction_cache: &'a mut SatisfactionCache,
    /// Steps that have already been evaluated this session (decision cache).
    /// Maps step name -> evaluation result.
    pub evaluated: HashMap<String, EvaluationResult>,
    /// Steps that failed execution (set by the orchestrator after execution).
    pub failed_steps: &'a HashSet<String>,
    /// Steps the user explicitly skipped.
    pub user_skipped_steps: &'a HashSet<String>,
    /// Steps confirmed as satisfied.
    pub satisfied_steps: &'a HashSet<String>,
    /// Steps with unresolved requirement gaps (after install attempts).
    /// The orchestrator populates this after attempting installation.
    pub unresolved_gaps: &'a HashSet<String>,
}

/// Evaluate a step through the full decision matrix.
///
/// This is the single entry point for step decisions. It evaluates the step
/// through the hierarchy defined in the plan:
///
/// 1. Check decision cache (return early if already evaluated this session)
/// 2. Hard blocks (dependency failed/blocked, precondition failed)
/// 3. Force override → Run
/// 4. Compute satisfaction (satisfied_when > checks > rerun window)
/// 5. Satisfied → AutoSkip
/// 6. Not satisfied → sensitive? Prompt : confirm? Prompt : AutoRun
pub fn evaluate_step(step_name: &str, ctx: &mut EngineContext<'_>) -> EvaluationResult {
    // 1. Check decision cache
    if let Some(cached) = ctx.evaluated.get(step_name) {
        return cached.clone();
    }

    let step = match ctx.steps.get(step_name) {
        Some(s) => s,
        None => {
            return EvaluationResult {
                decision: StepDecision::Block {
                    reason: BlockReason::DependencyFailed {
                        dependency: step_name.to_string(),
                    },
                },
                reason: format!("Step '{}' not found", step_name),
                satisfaction: None,
            };
        }
    };

    // 2. Hard blocks — dependency failed
    if let Some(failed_dep) = step
        .depends_on
        .iter()
        .find(|dep| ctx.failed_steps.contains(*dep))
    {
        let result = EvaluationResult {
            decision: StepDecision::Block {
                reason: BlockReason::DependencyFailed {
                    dependency: failed_dep.clone(),
                },
            },
            reason: format!("dependency '{}' failed", failed_dep),
            satisfaction: None,
        };
        ctx.evaluated.insert(step_name.to_string(), result.clone());
        return result;
    }

    // Hard block — dependency skipped and not satisfied
    if let Some(skipped_dep) = step
        .depends_on
        .iter()
        .find(|dep| ctx.user_skipped_steps.contains(*dep) && !ctx.satisfied_steps.contains(*dep))
    {
        let result = EvaluationResult {
            decision: StepDecision::Block {
                reason: BlockReason::DependencySkipped {
                    dependency: skipped_dep.clone(),
                },
            },
            reason: format!("dependency '{}' skipped and not satisfied", skipped_dep),
            satisfaction: None,
        };
        ctx.evaluated.insert(step_name.to_string(), result.clone());
        return result;
    }

    // Hard block — precondition fails
    if let Some(precondition) = step.execution.effective_precondition() {
        let mut evaluator =
            CheckEvaluator::new(ctx.project_root, ctx.interpolation, ctx.snapshot_store);
        let precond_result = evaluator.evaluate(&precondition);
        if !precond_result.passed_check() {
            let result = EvaluationResult {
                decision: StepDecision::Block {
                    reason: BlockReason::PreconditionFailed {
                        description: precond_result.description.clone(),
                    },
                },
                reason: format!("precondition failed: {}", precond_result.description),
                satisfaction: None,
            };
            ctx.evaluated.insert(step_name.to_string(), result.clone());
            return result;
        }
    }

    // 3. Soft blocks — unresolved requirement gaps (install already attempted)
    if ctx.unresolved_gaps.contains(step_name) {
        let result = EvaluationResult {
            decision: StepDecision::Skip {
                reason: SkipReason::RequirementNotMet,
            },
            reason: "requirement not met".to_string(),
            satisfaction: None,
        };
        ctx.evaluated.insert(step_name.to_string(), result.clone());
        return result;
    }

    // 4. Force override
    if ctx.force_all || ctx.force.contains(step_name) || step.behavior.force {
        let result = EvaluationResult {
            decision: StepDecision::Run,
            reason: "forced".to_string(),
            satisfaction: None,
        };
        ctx.evaluated.insert(step_name.to_string(), result.clone());
        return result;
    }

    // 4. Compute satisfaction
    let rerun_window = resolve_effective_rerun_window(step, step_name, ctx.step_overrides);
    let last_success = ctx.state.and_then(|s| {
        let step_state = s.get_step(step_name)?;
        if step_state.status == crate::state::StepStatus::Success {
            step_state.last_run
        } else {
            None
        }
    });

    let mut evaluator =
        CheckEvaluator::new(ctx.project_root, ctx.interpolation, ctx.snapshot_store);

    let computed = satisfaction::compute_satisfaction(
        step,
        &mut evaluator,
        ctx.named_check_results,
        step_name,
        &rerun_window,
        last_success,
    );

    // 5. Satisfied → AutoSkip (or Prompt if rerun with prompt_on_rerun)
    if computed.satisfied {
        // Store in satisfaction cache
        let record = SatisfactionRecord {
            satisfied: true,
            source: computed.source.clone(),
            recorded_at: chrono::Utc::now(),
            evidence: computed.evidence.clone(),
            config_hash: None,
            step_hash: None,
        };
        ctx.satisfaction_cache.store(step_name, record);

        // Check prompt_on_rerun: if satisfied via execution history and
        // prompt_on_rerun is true, prompt the user instead of auto-skipping.
        let is_rerun =
            computed.source == crate::state::satisfaction::SatisfactionSource::ExecutionHistory;
        let prompt_rerun =
            super::decision::effective_prompt_on_rerun(step, step_name, ctx.step_overrides);

        if is_rerun && prompt_rerun {
            let result = EvaluationResult {
                decision: StepDecision::Prompt {
                    prompt_key: format!("rerun_{}", step_name),
                },
                reason: computed.description.clone(),
                satisfaction: Some(computed),
            };
            ctx.evaluated.insert(step_name.to_string(), result.clone());
            return result;
        }

        let result = EvaluationResult {
            decision: StepDecision::Skip {
                reason: SkipReason::AutoSatisfied,
            },
            reason: computed.description.clone(),
            satisfaction: Some(computed),
        };
        ctx.evaluated.insert(step_name.to_string(), result.clone());
        return result;
    }

    // 6. Not satisfied — determine how to proceed
    // Sensitive steps always prompt
    if step.behavior.sensitive {
        let result = EvaluationResult {
            decision: StepDecision::Prompt {
                prompt_key: format!("sensitive_{}", step_name),
            },
            reason: "sensitive step requires confirmation".to_string(),
            satisfaction: Some(computed),
        };
        ctx.evaluated.insert(step_name.to_string(), result.clone());
        return result;
    }

    // Confirm steps always prompt
    if step.behavior.confirm {
        let result = EvaluationResult {
            decision: StepDecision::Prompt {
                prompt_key: format!("confirm_{}", step_name),
            },
            reason: "step requires confirmation".to_string(),
            satisfaction: Some(computed),
        };
        ctx.evaluated.insert(step_name.to_string(), result.clone());
        return result;
    }

    // auto_run: false → prompt before running
    let effective_auto_run =
        super::decision::effective_auto_run(step, step_name, ctx.step_overrides);
    if !effective_auto_run {
        let result = EvaluationResult {
            decision: StepDecision::Prompt {
                prompt_key: format!("autorun_{}", step_name),
            },
            reason: "auto_run disabled — awaiting user confirmation".to_string(),
            satisfaction: Some(computed),
        };
        ctx.evaluated.insert(step_name.to_string(), result.clone());
        return result;
    }

    // Default: auto-run
    let result = EvaluationResult {
        decision: StepDecision::AutoRun,
        reason: computed.description.clone(),
        satisfaction: Some(computed),
    };
    ctx.evaluated.insert(step_name.to_string(), result.clone());
    result
}

/// Resolve the effective rerun window for a step, considering overrides.
fn resolve_effective_rerun_window(
    step: &ResolvedStep,
    step_name: &str,
    step_overrides: &HashMap<String, StepOverride>,
) -> RerunWindow {
    // Check for workflow-level override
    if let Some(override_cfg) = step_overrides.get(step_name) {
        if let Some(ref window_str) = override_cfg.rerun_window {
            if let Ok(w) = window_str.parse() {
                return w;
            }
        }
    }
    // Use the step's resolved rerun window
    step.behavior.rerun_window.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::steps::{
        ResolvedBehavior, ResolvedEnvironmentVars, ResolvedExecution, ResolvedHooks,
        ResolvedOutput, ResolvedScoping, ResolvedStep,
    };

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

    // Shared empty collections for tests
    static EMPTY_OVERRIDES: std::sync::LazyLock<HashMap<String, StepOverride>> =
        std::sync::LazyLock::new(HashMap::new);
    static EMPTY_FORCE: std::sync::LazyLock<HashSet<String>> =
        std::sync::LazyLock::new(HashSet::new);
    static EMPTY_NAMED_CHECKS: std::sync::LazyLock<HashMap<String, CheckResult>> =
        std::sync::LazyLock::new(HashMap::new);
    static EMPTY_GAPS: std::sync::LazyLock<HashSet<String>> =
        std::sync::LazyLock::new(HashSet::new);

    fn make_context<'a>(
        steps: &'a HashMap<String, ResolvedStep>,
        snapshot_store: &'a mut SnapshotStore,
        interpolation: &'a InterpolationContext,
        satisfaction_cache: &'a mut SatisfactionCache,
        failed_steps: &'a HashSet<String>,
        user_skipped_steps: &'a HashSet<String>,
        satisfied_steps: &'a HashSet<String>,
    ) -> EngineContext<'a> {
        EngineContext {
            steps,
            project_root: Path::new("/tmp"),
            interpolation,
            snapshot_store,
            state: None,
            step_overrides: &EMPTY_OVERRIDES,
            force: &EMPTY_FORCE,
            force_all: false,
            named_check_results: &EMPTY_NAMED_CHECKS,
            satisfaction_cache,
            evaluated: HashMap::new(),
            failed_steps,
            user_skipped_steps,
            satisfied_steps,
            unresolved_gaps: &EMPTY_GAPS,
        }
    }

    #[test]
    fn fresh_step_no_checks_auto_runs() {
        let mut steps = HashMap::new();
        steps.insert("build".to_string(), make_step("build", vec![]));

        let temp = tempfile::TempDir::new().unwrap();
        let mut snapshots = SnapshotStore::new(temp.path().to_path_buf());
        let context = InterpolationContext::new();
        let mut cache = SatisfactionCache::empty(temp.path().join("satisfaction.json"));
        let failed = HashSet::new();
        let skipped = HashSet::new();
        let satisfied = HashSet::new();

        let mut ctx = make_context(
            &steps,
            &mut snapshots,
            &context,
            &mut cache,
            &failed,
            &skipped,
            &satisfied,
        );

        let result = evaluate_step("build", &mut ctx);
        assert_eq!(result.decision, StepDecision::AutoRun);
    }

    #[test]
    fn failed_dependency_blocks() {
        let mut steps = HashMap::new();
        steps.insert("a".to_string(), make_step("a", vec![]));
        steps.insert("b".to_string(), make_step("b", vec!["a".to_string()]));

        let temp = tempfile::TempDir::new().unwrap();
        let mut snapshots = SnapshotStore::new(temp.path().to_path_buf());
        let context = InterpolationContext::new();
        let mut cache = SatisfactionCache::empty(temp.path().join("satisfaction.json"));
        let mut failed = HashSet::new();
        failed.insert("a".to_string());
        let skipped = HashSet::new();
        let satisfied = HashSet::new();

        let mut ctx = make_context(
            &steps,
            &mut snapshots,
            &context,
            &mut cache,
            &failed,
            &skipped,
            &satisfied,
        );

        let result = evaluate_step("b", &mut ctx);
        assert!(matches!(
            result.decision,
            StepDecision::Block {
                reason: BlockReason::DependencyFailed { .. }
            }
        ));
        if let StepDecision::Block {
            reason: BlockReason::DependencyFailed { dependency },
        } = &result.decision
        {
            assert_eq!(dependency, "a");
        } else {
            panic!("expected DependencyFailed with dependency name");
        }
    }

    #[test]
    fn force_bypasses_satisfaction() {
        let mut steps = HashMap::new();
        steps.insert("build".to_string(), make_step("build", vec![]));

        let temp = tempfile::TempDir::new().unwrap();
        let mut snapshots = SnapshotStore::new(temp.path().to_path_buf());
        let context = InterpolationContext::new();
        let mut cache = SatisfactionCache::empty(temp.path().join("satisfaction.json"));
        let failed = HashSet::new();
        let skipped = HashSet::new();
        let satisfied = HashSet::new();
        let mut force = HashSet::new();
        force.insert("build".to_string());

        let mut ctx = make_context(
            &steps,
            &mut snapshots,
            &context,
            &mut cache,
            &failed,
            &skipped,
            &satisfied,
        );
        ctx.force = &force;

        let result = evaluate_step("build", &mut ctx);
        assert_eq!(result.decision, StepDecision::Run);
    }

    #[test]
    fn step_level_force_bypasses_satisfaction() {
        let mut steps = HashMap::new();
        let mut step = make_step("build", vec![]);
        step.behavior.force = true;
        steps.insert("build".to_string(), step);

        let temp = tempfile::TempDir::new().unwrap();
        let mut snapshots = SnapshotStore::new(temp.path().to_path_buf());
        let context = InterpolationContext::new();
        let mut cache = SatisfactionCache::empty(temp.path().join("satisfaction.json"));
        let failed = HashSet::new();
        let skipped = HashSet::new();
        let satisfied = HashSet::new();

        let mut ctx = make_context(
            &steps,
            &mut snapshots,
            &context,
            &mut cache,
            &failed,
            &skipped,
            &satisfied,
        );

        let result = evaluate_step("build", &mut ctx);
        assert_eq!(
            result.decision,
            StepDecision::Run,
            "step.behavior.force should force the step to run"
        );
    }

    #[test]
    fn force_all_bypasses_satisfaction_for_unnamed_step() {
        let mut steps = HashMap::new();
        steps.insert("build".to_string(), make_step("build", vec![]));

        let temp = tempfile::TempDir::new().unwrap();
        let mut snapshots = SnapshotStore::new(temp.path().to_path_buf());
        let context = InterpolationContext::new();
        let mut cache = SatisfactionCache::empty(temp.path().join("satisfaction.json"));
        let failed = HashSet::new();
        let skipped = HashSet::new();
        let satisfied = HashSet::new();

        let mut ctx = make_context(
            &steps,
            &mut snapshots,
            &context,
            &mut cache,
            &failed,
            &skipped,
            &satisfied,
        );
        // Empty force set — force_all alone should run the step.
        ctx.force_all = true;

        let result = evaluate_step("build", &mut ctx);
        assert_eq!(result.decision, StepDecision::Run);
    }

    #[test]
    fn sensitive_step_prompts() {
        let mut step = make_step("deploy", vec![]);
        step.behavior.sensitive = true;
        let mut steps = HashMap::new();
        steps.insert("deploy".to_string(), step);

        let temp = tempfile::TempDir::new().unwrap();
        let mut snapshots = SnapshotStore::new(temp.path().to_path_buf());
        let context = InterpolationContext::new();
        let mut cache = SatisfactionCache::empty(temp.path().join("satisfaction.json"));
        let failed = HashSet::new();
        let skipped = HashSet::new();
        let satisfied = HashSet::new();

        let mut ctx = make_context(
            &steps,
            &mut snapshots,
            &context,
            &mut cache,
            &failed,
            &skipped,
            &satisfied,
        );

        let result = evaluate_step("deploy", &mut ctx);
        assert!(matches!(result.decision, StepDecision::Prompt { .. }));
        assert!(result.reason.contains("sensitive"));
    }

    #[test]
    fn confirm_step_prompts() {
        let mut step = make_step("deploy", vec![]);
        step.behavior.confirm = true;
        let mut steps = HashMap::new();
        steps.insert("deploy".to_string(), step);

        let temp = tempfile::TempDir::new().unwrap();
        let mut snapshots = SnapshotStore::new(temp.path().to_path_buf());
        let context = InterpolationContext::new();
        let mut cache = SatisfactionCache::empty(temp.path().join("satisfaction.json"));
        let failed = HashSet::new();
        let skipped = HashSet::new();
        let satisfied = HashSet::new();

        let mut ctx = make_context(
            &steps,
            &mut snapshots,
            &context,
            &mut cache,
            &failed,
            &skipped,
            &satisfied,
        );

        let result = evaluate_step("deploy", &mut ctx);
        assert!(matches!(result.decision, StepDecision::Prompt { .. }));
        assert!(result.reason.contains("confirmation"));
    }

    #[test]
    fn satisfied_step_auto_skips() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("build_output"), "").unwrap();

        let mut step = make_step("build", vec![]);
        step.execution.check = Some(crate::checks::Check::Presence {
            name: None,
            target: Some("build_output".to_string()),
            kind: Some(crate::checks::PresenceKind::File),
            command: None,
        });
        let mut steps = HashMap::new();
        steps.insert("build".to_string(), step);

        let mut snapshots = SnapshotStore::new(dir.path().to_path_buf());
        let context = InterpolationContext::new();
        let mut cache = SatisfactionCache::empty(dir.path().join("satisfaction.json"));
        let failed = HashSet::new();
        let skipped = HashSet::new();
        let satisfied = HashSet::new();

        let mut ctx = EngineContext {
            steps: &steps,
            project_root: dir.path(),
            interpolation: &context,
            snapshot_store: &mut snapshots,
            state: None,
            step_overrides: &HashMap::new(),
            force: &HashSet::new(),
            force_all: false,
            named_check_results: &HashMap::new(),
            satisfaction_cache: &mut cache,
            evaluated: HashMap::new(),
            failed_steps: &failed,
            user_skipped_steps: &skipped,
            satisfied_steps: &satisfied,
            unresolved_gaps: &HashSet::new(),
        };

        let result = evaluate_step("build", &mut ctx);
        assert!(matches!(
            result.decision,
            StepDecision::Skip {
                reason: SkipReason::AutoSatisfied
            }
        ));
    }

    #[test]
    fn cached_result_returned() {
        let mut steps = HashMap::new();
        steps.insert("build".to_string(), make_step("build", vec![]));

        let temp = tempfile::TempDir::new().unwrap();
        let mut snapshots = SnapshotStore::new(temp.path().to_path_buf());
        let context = InterpolationContext::new();
        let mut cache = SatisfactionCache::empty(temp.path().join("satisfaction.json"));
        let failed = HashSet::new();
        let skipped = HashSet::new();
        let satisfied = HashSet::new();

        let mut ctx = make_context(
            &steps,
            &mut snapshots,
            &context,
            &mut cache,
            &failed,
            &skipped,
            &satisfied,
        );

        // First call
        let result1 = evaluate_step("build", &mut ctx);
        assert_eq!(result1.decision, StepDecision::AutoRun);

        // Second call returns cached result
        let result2 = evaluate_step("build", &mut ctx);
        assert_eq!(result2.decision, StepDecision::AutoRun);
    }

    #[test]
    fn skipped_unsatisfied_dependency_blocks() {
        let mut steps = HashMap::new();
        steps.insert("a".to_string(), make_step("a", vec![]));
        steps.insert("b".to_string(), make_step("b", vec!["a".to_string()]));

        let temp = tempfile::TempDir::new().unwrap();
        let mut snapshots = SnapshotStore::new(temp.path().to_path_buf());
        let context = InterpolationContext::new();
        let mut cache = SatisfactionCache::empty(temp.path().join("satisfaction.json"));
        let failed = HashSet::new();
        let mut skipped = HashSet::new();
        skipped.insert("a".to_string());
        let satisfied = HashSet::new(); // "a" is NOT satisfied

        let mut ctx = make_context(
            &steps,
            &mut snapshots,
            &context,
            &mut cache,
            &failed,
            &skipped,
            &satisfied,
        );

        let result = evaluate_step("b", &mut ctx);
        assert!(matches!(
            result.decision,
            StepDecision::Block {
                reason: BlockReason::DependencySkipped { .. }
            }
        ));
        if let StepDecision::Block {
            reason: BlockReason::DependencySkipped { dependency },
        } = &result.decision
        {
            assert_eq!(dependency, "a");
        } else {
            panic!("expected DependencySkipped with dependency name");
        }
    }

    #[test]
    fn skipped_satisfied_dependency_allows_proceed() {
        let mut steps = HashMap::new();
        steps.insert("a".to_string(), make_step("a", vec![]));
        steps.insert("b".to_string(), make_step("b", vec!["a".to_string()]));

        let temp = tempfile::TempDir::new().unwrap();
        let mut snapshots = SnapshotStore::new(temp.path().to_path_buf());
        let context = InterpolationContext::new();
        let mut cache = SatisfactionCache::empty(temp.path().join("satisfaction.json"));
        let failed = HashSet::new();
        let mut skipped = HashSet::new();
        skipped.insert("a".to_string());
        let mut satisfied = HashSet::new();
        satisfied.insert("a".to_string()); // "a" IS satisfied

        let mut ctx = make_context(
            &steps,
            &mut snapshots,
            &context,
            &mut cache,
            &failed,
            &skipped,
            &satisfied,
        );

        let result = evaluate_step("b", &mut ctx);
        // Should auto-run (not blocked) since dependency is satisfied
        assert_eq!(result.decision, StepDecision::AutoRun);
    }

    #[test]
    fn unknown_step_blocks() {
        let steps = HashMap::new();

        let temp = tempfile::TempDir::new().unwrap();
        let mut snapshots = SnapshotStore::new(temp.path().to_path_buf());
        let context = InterpolationContext::new();
        let mut cache = SatisfactionCache::empty(temp.path().join("satisfaction.json"));
        let failed = HashSet::new();
        let skipped = HashSet::new();
        let satisfied = HashSet::new();

        let mut ctx = make_context(
            &steps,
            &mut snapshots,
            &context,
            &mut cache,
            &failed,
            &skipped,
            &satisfied,
        );

        let result = evaluate_step("nonexistent", &mut ctx);
        assert!(matches!(result.decision, StepDecision::Block { .. }));
    }

    #[test]
    fn sensitive_takes_priority_over_confirm() {
        let mut step = make_step("deploy", vec![]);
        step.behavior.sensitive = true;
        step.behavior.confirm = true;
        let mut steps = HashMap::new();
        steps.insert("deploy".to_string(), step);

        let temp = tempfile::TempDir::new().unwrap();
        let mut snapshots = SnapshotStore::new(temp.path().to_path_buf());
        let context = InterpolationContext::new();
        let mut cache = SatisfactionCache::empty(temp.path().join("satisfaction.json"));
        let failed = HashSet::new();
        let skipped = HashSet::new();
        let satisfied = HashSet::new();

        let mut ctx = make_context(
            &steps,
            &mut snapshots,
            &context,
            &mut cache,
            &failed,
            &skipped,
            &satisfied,
        );

        let result = evaluate_step("deploy", &mut ctx);
        assert!(result.reason.contains("sensitive"));
    }

    #[test]
    fn satisfied_via_history_with_prompt_on_rerun_prompts() {
        let mut step = make_step("build", vec![]);
        step.behavior.prompt_on_rerun = true;
        let mut steps = HashMap::new();
        steps.insert("build".to_string(), step);

        let temp = tempfile::TempDir::new().unwrap();
        let mut snapshots = SnapshotStore::new(temp.path().to_path_buf());
        let context = InterpolationContext::new();
        let mut cache = SatisfactionCache::empty(temp.path().join("satisfaction.json"));
        let failed = HashSet::new();
        let skipped = HashSet::new();
        let satisfied = HashSet::new();

        // Create a state store with a recent successful run
        let mut state = crate::state::StateStore::new(
            &crate::state::ProjectId::from_path(temp.path()).unwrap(),
        );
        state.record_step_result(
            "build",
            crate::state::StepStatus::Success,
            std::time::Duration::from_secs(1),
        );

        let mut ctx = make_context(
            &steps,
            &mut snapshots,
            &context,
            &mut cache,
            &failed,
            &skipped,
            &satisfied,
        );
        ctx.state = Some(&state);

        let result = evaluate_step("build", &mut ctx);
        // Should prompt for rerun since satisfied via history + prompt_on_rerun
        assert!(
            matches!(result.decision, StepDecision::Prompt { ref prompt_key } if prompt_key.starts_with("rerun_")),
            "expected Prompt(rerun_*), got {:?}",
            result.decision
        );
    }

    #[test]
    fn satisfied_via_history_without_prompt_on_rerun_auto_skips() {
        let mut step = make_step("build", vec![]);
        step.behavior.prompt_on_rerun = false;
        let mut steps = HashMap::new();
        steps.insert("build".to_string(), step);

        let temp = tempfile::TempDir::new().unwrap();
        let mut snapshots = SnapshotStore::new(temp.path().to_path_buf());
        let context = InterpolationContext::new();
        let mut cache = SatisfactionCache::empty(temp.path().join("satisfaction.json"));
        let failed = HashSet::new();
        let skipped = HashSet::new();
        let satisfied = HashSet::new();

        let mut state = crate::state::StateStore::new(
            &crate::state::ProjectId::from_path(temp.path()).unwrap(),
        );
        state.record_step_result(
            "build",
            crate::state::StepStatus::Success,
            std::time::Duration::from_secs(1),
        );

        let mut ctx = make_context(
            &steps,
            &mut snapshots,
            &context,
            &mut cache,
            &failed,
            &skipped,
            &satisfied,
        );
        ctx.state = Some(&state);

        let result = evaluate_step("build", &mut ctx);
        assert!(matches!(
            result.decision,
            StepDecision::Skip {
                reason: SkipReason::AutoSatisfied
            }
        ));
    }

    #[test]
    fn auto_run_false_prompts_when_not_satisfied() {
        let mut step = make_step("install", vec![]);
        step.behavior.auto_run = false;
        let mut steps = HashMap::new();
        steps.insert("install".to_string(), step);

        let temp = tempfile::TempDir::new().unwrap();
        let mut snapshots = SnapshotStore::new(temp.path().to_path_buf());
        let context = InterpolationContext::new();
        let mut cache = SatisfactionCache::empty(temp.path().join("satisfaction.json"));
        let failed = HashSet::new();
        let skipped = HashSet::new();
        let satisfied = HashSet::new();

        let mut ctx = make_context(
            &steps,
            &mut snapshots,
            &context,
            &mut cache,
            &failed,
            &skipped,
            &satisfied,
        );

        let result = evaluate_step("install", &mut ctx);
        assert!(
            matches!(result.decision, StepDecision::Prompt { ref prompt_key } if prompt_key.starts_with("autorun_")),
            "expected Prompt(autorun_*), got {:?}",
            result.decision
        );
    }

    #[test]
    fn auto_run_true_auto_runs_when_not_satisfied() {
        let mut step = make_step("install", vec![]);
        step.behavior.auto_run = true;
        let mut steps = HashMap::new();
        steps.insert("install".to_string(), step);

        let temp = tempfile::TempDir::new().unwrap();
        let mut snapshots = SnapshotStore::new(temp.path().to_path_buf());
        let context = InterpolationContext::new();
        let mut cache = SatisfactionCache::empty(temp.path().join("satisfaction.json"));
        let failed = HashSet::new();
        let skipped = HashSet::new();
        let satisfied = HashSet::new();

        let mut ctx = make_context(
            &steps,
            &mut snapshots,
            &context,
            &mut cache,
            &failed,
            &skipped,
            &satisfied,
        );

        let result = evaluate_step("install", &mut ctx);
        assert!(
            matches!(result.decision, StepDecision::AutoRun),
            "expected AutoRun, got {:?}",
            result.decision
        );
    }

    #[test]
    fn sensitive_takes_priority_over_auto_run() {
        let mut step = make_step("deploy", vec![]);
        step.behavior.sensitive = true;
        step.behavior.auto_run = false;
        let mut steps = HashMap::new();
        steps.insert("deploy".to_string(), step);

        let temp = tempfile::TempDir::new().unwrap();
        let mut snapshots = SnapshotStore::new(temp.path().to_path_buf());
        let context = InterpolationContext::new();
        let mut cache = SatisfactionCache::empty(temp.path().join("satisfaction.json"));
        let failed = HashSet::new();
        let skipped = HashSet::new();
        let satisfied = HashSet::new();

        let mut ctx = make_context(
            &steps,
            &mut snapshots,
            &context,
            &mut cache,
            &failed,
            &skipped,
            &satisfied,
        );

        let result = evaluate_step("deploy", &mut ctx);
        // sensitive prompt takes priority over auto_run prompt
        assert!(
            matches!(result.decision, StepDecision::Prompt { ref prompt_key } if prompt_key.starts_with("sensitive_")),
            "expected Prompt(sensitive_*), got {:?}",
            result.decision
        );
    }

    #[test]
    fn confirm_takes_priority_over_auto_run() {
        let mut step = make_step("deploy", vec![]);
        step.behavior.confirm = true;
        step.behavior.auto_run = false;
        let mut steps = HashMap::new();
        steps.insert("deploy".to_string(), step);

        let temp = tempfile::TempDir::new().unwrap();
        let mut snapshots = SnapshotStore::new(temp.path().to_path_buf());
        let context = InterpolationContext::new();
        let mut cache = SatisfactionCache::empty(temp.path().join("satisfaction.json"));
        let failed = HashSet::new();
        let skipped = HashSet::new();
        let satisfied = HashSet::new();

        let mut ctx = make_context(
            &steps,
            &mut snapshots,
            &context,
            &mut cache,
            &failed,
            &skipped,
            &satisfied,
        );

        let result = evaluate_step("deploy", &mut ctx);
        // confirm prompt takes priority over auto_run prompt
        assert!(
            matches!(result.decision, StepDecision::Prompt { ref prompt_key } if prompt_key.starts_with("confirm_")),
            "expected Prompt(confirm_*), got {:?}",
            result.decision
        );
    }

    #[test]
    fn auto_run_false_does_not_affect_satisfied_steps() {
        // When a step is satisfied, auto_run doesn't matter — prompt_on_rerun controls it
        let mut step = make_step("build", vec![]);
        step.behavior.auto_run = false;
        step.behavior.prompt_on_rerun = false;
        let mut steps = HashMap::new();
        steps.insert("build".to_string(), step);

        let temp = tempfile::TempDir::new().unwrap();
        let mut snapshots = SnapshotStore::new(temp.path().to_path_buf());
        let context = InterpolationContext::new();
        let mut cache = SatisfactionCache::empty(temp.path().join("satisfaction.json"));
        let failed = HashSet::new();
        let skipped = HashSet::new();
        let satisfied = HashSet::new();

        let mut state = crate::state::StateStore::new(
            &crate::state::ProjectId::from_path(temp.path()).unwrap(),
        );
        state.record_step_result(
            "build",
            crate::state::StepStatus::Success,
            std::time::Duration::from_secs(1),
        );

        let mut ctx = make_context(
            &steps,
            &mut snapshots,
            &context,
            &mut cache,
            &failed,
            &skipped,
            &satisfied,
        );
        ctx.state = Some(&state);

        let result = evaluate_step("build", &mut ctx);
        // Should auto-skip because satisfied, regardless of auto_run: false
        assert!(
            matches!(
                result.decision,
                StepDecision::Skip {
                    reason: SkipReason::AutoSatisfied
                }
            ),
            "expected AutoSatisfied skip, got {:?}",
            result.decision
        );
    }
}
