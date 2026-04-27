//! Satisfaction evaluation for steps.
//!
//! Evaluates `satisfied_when` conditions to determine if a step's purpose
//! is already fulfilled. Satisfaction is distinct from check evaluation:
//! checks report facts about the world, while satisfaction declares when
//! a step's work is already done.
//!
//! # Ref Resolution
//!
//! `satisfied_when` conditions can reference named checks:
//! - `ref: check_name` — references a named check on the same step
//! - `ref: step_name.check_name` — references a named check on another step

use std::collections::HashMap;

use crate::checks::evaluator::CheckEvaluator;
use crate::checks::{Check, CheckResult, SatisfactionCondition};
use crate::steps::ResolvedStep;

/// Result of evaluating a step's satisfaction conditions.
#[derive(Debug, Clone)]
pub struct SatisfactionResult {
    /// Whether all conditions passed (step is satisfied).
    pub satisfied: bool,
    /// Individual condition results.
    pub condition_results: Vec<CheckResult>,
    /// Number of conditions evaluated.
    pub condition_count: usize,
    /// Number of conditions that passed.
    pub passed_count: usize,
}

/// Evaluate a step's `satisfied_when` conditions.
///
/// Returns `None` if the step has no satisfaction conditions.
/// Returns `Some(SatisfactionResult)` with the evaluation results otherwise.
///
/// # Arguments
///
/// * `step` - The step whose satisfaction conditions to evaluate
/// * `evaluator` - The check evaluator for inline checks
/// * `named_check_results` - Pre-evaluated named check results, keyed by
///   `"step_name.check_name"` or `"check_name"` (for same-step refs)
/// * `step_name` - Name of the current step (for resolving unqualified refs)
pub fn evaluate_satisfaction(
    step: &ResolvedStep,
    evaluator: &mut CheckEvaluator<'_>,
    named_check_results: &HashMap<String, CheckResult>,
    step_name: &str,
) -> Option<SatisfactionResult> {
    if step.satisfied_when.is_empty() {
        return None;
    }

    let mut results = Vec::with_capacity(step.satisfied_when.len());

    for condition in &step.satisfied_when {
        let result = evaluate_condition(condition, evaluator, named_check_results, step_name);
        results.push(result);
    }

    let passed_count = results.iter().filter(|r| r.passed_check()).count();
    let condition_count = results.len();

    Some(SatisfactionResult {
        satisfied: passed_count == condition_count,
        condition_results: results,
        condition_count,
        passed_count,
    })
}

/// Evaluate a single satisfaction condition.
fn evaluate_condition(
    condition: &SatisfactionCondition,
    evaluator: &mut CheckEvaluator<'_>,
    named_check_results: &HashMap<String, CheckResult>,
    step_name: &str,
) -> CheckResult {
    match condition {
        SatisfactionCondition::Check(check) => evaluator.evaluate(check),
        SatisfactionCondition::Ref { check_ref } => {
            resolve_ref(check_ref, named_check_results, step_name)
        }
    }
}

/// Resolve a `ref: <name>` satisfaction condition.
///
/// Tries two lookup strategies:
/// 1. Qualified: `step_name.check_name` — look up directly
/// 2. Unqualified: `check_name` — look up as `step_name.check_name` (same step)
fn resolve_ref(
    check_ref: &str,
    named_check_results: &HashMap<String, CheckResult>,
    step_name: &str,
) -> CheckResult {
    // First try the ref as-is (handles both qualified "step.check" and
    // direct keys that might be in the map)
    if let Some(result) = named_check_results.get(check_ref) {
        return result.clone();
    }

    // If unqualified, try qualifying with the current step name
    if !check_ref.contains('.') {
        let qualified = format!("{}.{}", step_name, check_ref);
        if let Some(result) = named_check_results.get(&qualified) {
            return result.clone();
        }
    }

    CheckResult::failed(
        format!("ref: {}", check_ref),
        format!(
            "Named check '{}' not found. Ensure the referenced check has a 'name' field.",
            check_ref
        ),
    )
}

/// Collect named check results from evaluating a step's checks.
///
/// Walks the check tree and evaluates each named check, storing results
/// keyed by `"step_name.check_name"`.
pub fn collect_named_check_results(
    step_name: &str,
    check: &Check,
    evaluator: &mut CheckEvaluator<'_>,
) -> HashMap<String, CheckResult> {
    let mut results = HashMap::new();
    collect_named_recursive(step_name, check, evaluator, &mut results);
    results
}

fn collect_named_recursive(
    step_name: &str,
    check: &Check,
    evaluator: &mut CheckEvaluator<'_>,
    results: &mut HashMap<String, CheckResult>,
) {
    // If this check has a name, evaluate and store it
    if let Some(name) = check.name() {
        let result = evaluator.evaluate(check);
        results.insert(format!("{}.{}", step_name, name), result);
    }

    // Recurse into combinators
    match check {
        Check::All { checks, .. } | Check::Any { checks, .. } => {
            for sub_check in checks {
                collect_named_recursive(step_name, sub_check, evaluator, results);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::{CheckOutcome, SatisfactionCondition};
    use crate::steps::{
        ResolvedBehavior, ResolvedEnvironmentVars, ResolvedExecution, ResolvedHooks,
        ResolvedOutput, ResolvedScoping, ResolvedStep,
    };

    fn make_step_with_satisfaction(conditions: Vec<SatisfactionCondition>) -> ResolvedStep {
        ResolvedStep {
            name: "test_step".to_string(),
            title: "test_step".to_string(),
            description: None,
            depends_on: vec![],
            requires: vec![],
            inputs: HashMap::new(),
            satisfied_when: conditions,
            execution: ResolvedExecution {
                command: "echo test".to_string(),
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
    fn no_conditions_returns_none() {
        let step = make_step_with_satisfaction(vec![]);
        let mut snapshots = crate::snapshots::SnapshotStore::new(std::env::temp_dir());
        let context = crate::config::interpolation::InterpolationContext::new();
        let mut evaluator =
            CheckEvaluator::new(std::path::Path::new("/tmp"), &context, &mut snapshots);
        let named = HashMap::new();

        let result = evaluate_satisfaction(&step, &mut evaluator, &named, "test_step");
        assert!(result.is_none());
    }

    #[test]
    fn inline_presence_check_satisfied_when_file_exists() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("node_modules"), "").unwrap();

        let step =
            make_step_with_satisfaction(vec![SatisfactionCondition::Check(Check::Presence {
                name: None,
                target: Some("node_modules".to_string()),
                kind: Some(crate::checks::PresenceKind::File),
                command: None,
            })]);

        let mut snapshots = crate::snapshots::SnapshotStore::new(std::env::temp_dir());
        let context = crate::config::interpolation::InterpolationContext::new();
        let mut evaluator = CheckEvaluator::new(dir.path(), &context, &mut snapshots);
        let named = HashMap::new();

        let result = evaluate_satisfaction(&step, &mut evaluator, &named, "test_step").unwrap();
        assert!(result.satisfied);
        assert_eq!(result.condition_count, 1);
        assert_eq!(result.passed_count, 1);
    }

    #[test]
    fn inline_presence_check_not_satisfied_when_file_missing() {
        let dir = tempfile::TempDir::new().unwrap();

        let step =
            make_step_with_satisfaction(vec![SatisfactionCondition::Check(Check::Presence {
                name: None,
                target: Some("node_modules".to_string()),
                kind: Some(crate::checks::PresenceKind::File),
                command: None,
            })]);

        let mut snapshots = crate::snapshots::SnapshotStore::new(std::env::temp_dir());
        let context = crate::config::interpolation::InterpolationContext::new();
        let mut evaluator = CheckEvaluator::new(dir.path(), &context, &mut snapshots);
        let named = HashMap::new();

        let result = evaluate_satisfaction(&step, &mut evaluator, &named, "test_step").unwrap();
        assert!(!result.satisfied);
        assert_eq!(result.passed_count, 0);
    }

    #[test]
    fn ref_resolves_same_step_named_check() {
        let step = make_step_with_satisfaction(vec![SatisfactionCondition::Ref {
            check_ref: "deps_installed".to_string(),
        }]);

        let mut snapshots = crate::snapshots::SnapshotStore::new(std::env::temp_dir());
        let context = crate::config::interpolation::InterpolationContext::new();
        let mut evaluator =
            CheckEvaluator::new(std::path::Path::new("/tmp"), &context, &mut snapshots);

        let mut named = HashMap::new();
        named.insert(
            "test_step.deps_installed".to_string(),
            CheckResult::passed("node_modules exists"),
        );

        let result = evaluate_satisfaction(&step, &mut evaluator, &named, "test_step").unwrap();
        assert!(result.satisfied);
    }

    #[test]
    fn ref_resolves_cross_step_named_check() {
        let step = make_step_with_satisfaction(vec![SatisfactionCondition::Ref {
            check_ref: "install_deps.deps_present".to_string(),
        }]);

        let mut snapshots = crate::snapshots::SnapshotStore::new(std::env::temp_dir());
        let context = crate::config::interpolation::InterpolationContext::new();
        let mut evaluator =
            CheckEvaluator::new(std::path::Path::new("/tmp"), &context, &mut snapshots);

        let mut named = HashMap::new();
        named.insert(
            "install_deps.deps_present".to_string(),
            CheckResult::passed("vendor/bundle exists"),
        );

        let result = evaluate_satisfaction(&step, &mut evaluator, &named, "test_step").unwrap();
        assert!(result.satisfied);
    }

    #[test]
    fn ref_not_found_fails() {
        let step = make_step_with_satisfaction(vec![SatisfactionCondition::Ref {
            check_ref: "nonexistent".to_string(),
        }]);

        let mut snapshots = crate::snapshots::SnapshotStore::new(std::env::temp_dir());
        let context = crate::config::interpolation::InterpolationContext::new();
        let mut evaluator =
            CheckEvaluator::new(std::path::Path::new("/tmp"), &context, &mut snapshots);
        let named = HashMap::new();

        let result = evaluate_satisfaction(&step, &mut evaluator, &named, "test_step").unwrap();
        assert!(!result.satisfied);
        assert_eq!(result.condition_results[0].outcome, CheckOutcome::Failed);
    }

    #[test]
    fn multiple_conditions_all_must_pass() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("node_modules"), "").unwrap();

        let step = make_step_with_satisfaction(vec![
            SatisfactionCondition::Check(Check::Presence {
                name: None,
                target: Some("node_modules".to_string()),
                kind: Some(crate::checks::PresenceKind::File),
                command: None,
            }),
            SatisfactionCondition::Check(Check::Presence {
                name: None,
                target: Some("missing_file".to_string()),
                kind: Some(crate::checks::PresenceKind::File),
                command: None,
            }),
        ]);

        let mut snapshots = crate::snapshots::SnapshotStore::new(std::env::temp_dir());
        let context = crate::config::interpolation::InterpolationContext::new();
        let mut evaluator = CheckEvaluator::new(dir.path(), &context, &mut snapshots);
        let named = HashMap::new();

        let result = evaluate_satisfaction(&step, &mut evaluator, &named, "test_step").unwrap();
        assert!(!result.satisfied);
        assert_eq!(result.condition_count, 2);
        assert_eq!(result.passed_count, 1);
    }

    #[test]
    fn multiple_conditions_all_pass() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("file_a"), "").unwrap();
        std::fs::write(dir.path().join("file_b"), "").unwrap();

        let step = make_step_with_satisfaction(vec![
            SatisfactionCondition::Check(Check::Presence {
                name: None,
                target: Some("file_a".to_string()),
                kind: Some(crate::checks::PresenceKind::File),
                command: None,
            }),
            SatisfactionCondition::Check(Check::Presence {
                name: None,
                target: Some("file_b".to_string()),
                kind: Some(crate::checks::PresenceKind::File),
                command: None,
            }),
        ]);

        let mut snapshots = crate::snapshots::SnapshotStore::new(std::env::temp_dir());
        let context = crate::config::interpolation::InterpolationContext::new();
        let mut evaluator = CheckEvaluator::new(dir.path(), &context, &mut snapshots);
        let named = HashMap::new();

        let result = evaluate_satisfaction(&step, &mut evaluator, &named, "test_step").unwrap();
        assert!(result.satisfied);
        assert_eq!(result.condition_count, 2);
        assert_eq!(result.passed_count, 2);
    }

    #[test]
    fn collect_named_check_results_from_single_named_check() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("node_modules"), "").unwrap();

        let check = Check::Presence {
            name: Some("deps_installed".to_string()),
            target: Some("node_modules".to_string()),
            kind: Some(crate::checks::PresenceKind::File),
            command: None,
        };

        let mut snapshots = crate::snapshots::SnapshotStore::new(std::env::temp_dir());
        let context = crate::config::interpolation::InterpolationContext::new();
        let mut evaluator = CheckEvaluator::new(dir.path(), &context, &mut snapshots);

        let results = collect_named_check_results("install_deps", &check, &mut evaluator);
        assert!(results.contains_key("install_deps.deps_installed"));
        assert!(results["install_deps.deps_installed"].passed_check());
    }

    #[test]
    fn unnamed_checks_not_collected() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("file_a"), "").unwrap();

        let check = Check::Presence {
            name: None,
            target: Some("file_a".to_string()),
            kind: Some(crate::checks::PresenceKind::File),
            command: None,
        };

        let mut snapshots = crate::snapshots::SnapshotStore::new(std::env::temp_dir());
        let context = crate::config::interpolation::InterpolationContext::new();
        let mut evaluator = CheckEvaluator::new(dir.path(), &context, &mut snapshots);

        let results = collect_named_check_results("my_step", &check, &mut evaluator);
        assert!(results.is_empty());
    }

    #[test]
    fn mixed_ref_and_inline_conditions() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("vendor"), "").unwrap();

        let step = make_step_with_satisfaction(vec![
            // Inline check that passes
            SatisfactionCondition::Check(Check::Presence {
                name: None,
                target: Some("vendor".to_string()),
                kind: Some(crate::checks::PresenceKind::File),
                command: None,
            }),
            // Ref to a passing named check
            SatisfactionCondition::Ref {
                check_ref: "dep_step.deps_ok".to_string(),
            },
        ]);

        let mut snapshots = crate::snapshots::SnapshotStore::new(std::env::temp_dir());
        let context = crate::config::interpolation::InterpolationContext::new();
        let mut evaluator = CheckEvaluator::new(dir.path(), &context, &mut snapshots);

        let mut named = HashMap::new();
        named.insert(
            "dep_step.deps_ok".to_string(),
            CheckResult::passed("deps OK"),
        );

        let result = evaluate_satisfaction(&step, &mut evaluator, &named, "test_step").unwrap();
        assert!(result.satisfied);
        assert_eq!(result.condition_count, 2);
        assert_eq!(result.passed_count, 2);
    }

    #[test]
    fn mixed_ref_and_inline_one_fails() {
        let dir = tempfile::TempDir::new().unwrap();
        // vendor does NOT exist

        let step = make_step_with_satisfaction(vec![
            SatisfactionCondition::Check(Check::Presence {
                name: None,
                target: Some("vendor".to_string()),
                kind: Some(crate::checks::PresenceKind::File),
                command: None,
            }),
            SatisfactionCondition::Ref {
                check_ref: "dep_step.deps_ok".to_string(),
            },
        ]);

        let mut snapshots = crate::snapshots::SnapshotStore::new(std::env::temp_dir());
        let context = crate::config::interpolation::InterpolationContext::new();
        let mut evaluator = CheckEvaluator::new(dir.path(), &context, &mut snapshots);

        let mut named = HashMap::new();
        named.insert(
            "dep_step.deps_ok".to_string(),
            CheckResult::passed("deps OK"),
        );

        let result = evaluate_satisfaction(&step, &mut evaluator, &named, "test_step").unwrap();
        assert!(!result.satisfied);
        assert_eq!(result.passed_count, 1);
    }

    #[test]
    fn collect_named_check_results_from_combinator() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("file_a"), "").unwrap();

        let check = Check::All {
            name: None,
            checks: vec![
                Check::Presence {
                    name: Some("a_exists".to_string()),
                    target: Some("file_a".to_string()),
                    kind: Some(crate::checks::PresenceKind::File),
                    command: None,
                },
                Check::Presence {
                    name: Some("b_exists".to_string()),
                    target: Some("file_b".to_string()),
                    kind: Some(crate::checks::PresenceKind::File),
                    command: None,
                },
            ],
        };

        let mut snapshots = crate::snapshots::SnapshotStore::new(std::env::temp_dir());
        let context = crate::config::interpolation::InterpolationContext::new();
        let mut evaluator = CheckEvaluator::new(dir.path(), &context, &mut snapshots);

        let results = collect_named_check_results("my_step", &check, &mut evaluator);
        assert_eq!(results.len(), 2);
        assert!(results["my_step.a_exists"].passed_check());
        assert!(!results["my_step.b_exists"].passed_check());
    }
}
