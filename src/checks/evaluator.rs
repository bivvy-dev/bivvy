//! Unified check evaluation pipeline.
//!
//! All checks go through [`CheckEvaluator::evaluate`], which dispatches
//! to the appropriate check-type handler and produces a [`CheckResult`].
//!
//! The evaluator has access to:
//! - `project_root` — for resolving relative paths
//! - `interpolation` — for variable resolution in commands
//! - `snapshots` — for change check baseline reads during evaluation and baseline writes after execution
//!
//! It does NOT access the state store, UI, or gap checker.

use super::change::{compute_target_hash, evaluate_change_result, HashResult};
use super::execution::evaluate_execution;
use super::presence::evaluate_presence;
use super::{BaselineConfig, Check, CheckOutcome, CheckResult};
use crate::config::interpolation::{interpolate_if_needed, InterpolationContext};
use crate::snapshots::{SnapshotKey, SnapshotStore};
use std::path::Path;

/// Unified check evaluator.
///
/// Evaluates any [`Check`] variant against the external world. This is the
/// single entry point for all check evaluation — presence, execution, change,
/// and combinators all go through [`evaluate`](Self::evaluate).
///
/// ```no_run
/// use bivvy::checks::evaluator::CheckEvaluator;
/// use bivvy::checks::Check;
/// use bivvy::config::interpolation::InterpolationContext;
/// use bivvy::snapshots::SnapshotStore;
/// use std::path::Path;
///
/// let interpolation = InterpolationContext::new();
/// let mut snapshots = SnapshotStore::new("/tmp/snapshots");
/// let mut evaluator = CheckEvaluator::new(
///     Path::new("/project"),
///     &interpolation,
///     &mut snapshots,
/// );
///
/// let check: Check = serde_yaml::from_str(r#"
///     type: presence
///     target: node_modules
/// "#).unwrap();
///
/// let result = evaluator.evaluate(&check);
/// println!("{}: {:?}", result.description, result.outcome);
/// ```
pub struct CheckEvaluator<'a> {
    project_root: &'a Path,
    interpolation: &'a InterpolationContext,
    snapshots: &'a mut SnapshotStore,
    /// Step name for snapshot key construction. Set via [`with_step`](Self::with_step).
    step_name: Option<String>,
    /// Workflow name for workflow-scoped snapshot keys. Set via [`with_workflow`](Self::with_workflow).
    workflow_name: Option<String>,
}

impl<'a> CheckEvaluator<'a> {
    /// Create a new evaluator.
    pub fn new(
        project_root: &'a Path,
        interpolation: &'a InterpolationContext,
        snapshots: &'a mut SnapshotStore,
    ) -> Self {
        Self {
            project_root,
            interpolation,
            snapshots,
            step_name: None,
            workflow_name: None,
        }
    }

    /// Set the step context for change check baseline lookups.
    ///
    /// Must be called before evaluating change checks that need snapshot
    /// store access. Without it, change checks are indeterminate because no
    /// baseline key can be constructed.
    ///
    /// The rest of the key comes from each change leaf's own
    /// [`Check::config_hash`], so the step name is the only context required.
    pub fn with_step(mut self, step_name: impl Into<String>) -> Self {
        self.step_name = Some(step_name.into());
        self
    }

    /// Set the workflow name for workflow-scoped snapshot keys.
    ///
    /// When a change check uses `scope: workflow`, the baseline is isolated
    /// per-workflow. This method sets the workflow name used to construct
    /// that scoped key.
    pub fn with_workflow(mut self, workflow_name: impl Into<String>) -> Self {
        self.workflow_name = Some(workflow_name.into());
        self
    }

    /// Evaluate a check and produce a result.
    ///
    /// Dispatches to the appropriate handler based on check type.
    /// Commands in execution and custom presence checks are interpolated
    /// if they contain `${variable}` references.
    pub fn evaluate(&mut self, check: &Check) -> CheckResult {
        match check {
            Check::Presence {
                target,
                kind,
                command,
                ..
            } => {
                let resolved_command = command.as_deref().map(|c| self.interpolate(c));
                let cmd_ref = resolved_command.as_deref().or(command.as_deref());
                evaluate_presence(target.as_deref(), *kind, cmd_ref, self.project_root)
            }
            Check::Execution {
                command,
                validation,
                ..
            } => {
                let resolved = self.interpolate(command);
                evaluate_execution(&resolved, *validation, self.project_root)
            }
            change @ Check::Change { .. } => self.evaluate_change(change),
            Check::All { checks, .. } => self.evaluate_all(checks),
            Check::Any { checks, .. } => self.evaluate_any(checks),
        }
    }

    /// Evaluate a list of checks as an implicit `All` combinator.
    ///
    /// Useful when a step has multiple `checks` (plural) — the spec says
    /// a top-level list is implicitly `all`.
    pub fn evaluate_all_checks(&mut self, checks: &[Check]) -> CheckResult {
        self.evaluate_all(checks)
    }

    /// Record change-check baselines for a step after successful execution.
    ///
    /// Walks the check tree and records the current target hash for all
    /// `Change` leaves that use automatic baselines (`_last_run` or
    /// `_first_run`). Skips named snapshots and git baselines.
    ///
    /// For `baseline: first_run`, the baseline is recorded only once — when no
    /// prior baseline exists for that key.
    ///
    /// Call this after a step has run and its result is [`StepStatus::Completed`].
    pub fn record_run_baselines(&mut self, check: &Check) {
        self.record_baselines_recursive(check);
    }

    fn record_baselines_recursive(&mut self, check: &Check) {
        match check {
            Check::Change {
                target,
                kind,
                baseline,
                baseline_snapshot,
                baseline_git,
                size_limit,
                scope,
                ..
            } => {
                if baseline_snapshot.is_some() || baseline_git.is_some() {
                    return;
                }

                let resolved_target = if *kind == super::ChangeKind::Command {
                    self.interpolate(target)
                } else {
                    target.to_string()
                };

                let hash = match super::change::compute_target_hash(
                    &resolved_target,
                    *kind,
                    self.project_root,
                    size_limit,
                ) {
                    super::change::HashResult::Ok(h) => h,
                    _ => return,
                };

                let config_hash = check.config_hash();
                let baseline_name = self.resolve_baseline_name(baseline, None);

                let Some(key) = self.make_snapshot_key(scope, &config_hash) else {
                    return;
                };

                if *baseline == super::BaselineConfig::FirstRun
                    && self.snapshots.get_baseline(&key, &baseline_name).is_some()
                {
                    return;
                }

                self.snapshots
                    .record_baseline(&key, &baseline_name, hash, resolved_target);
            }
            Check::All { checks, .. } | Check::Any { checks, .. } => {
                for c in checks {
                    self.record_baselines_recursive(c);
                }
            }
            _ => {}
        }
    }

    /// Interpolate a string if it contains variable references.
    fn interpolate(&self, input: &str) -> String {
        interpolate_if_needed(input, self.interpolation)
    }

    /// Evaluate a change check. Expects `check` to be `Check::Change`.
    ///
    /// When the target cannot be hashed - it does not exist, hashing errored,
    /// or it exceeds `size_limit` - the check has no evidence either way and
    /// returns [`CheckOutcome::Indeterminate`] rather than a failure. A missing
    /// target is not drift.
    ///
    /// The baseline is keyed by this leaf's own [`Check::config_hash`], so
    /// sibling change checks in the same step keep independent baselines and
    /// agree with the keys written by `bivvy snapshot capture`.
    fn evaluate_change(&mut self, check: &Check) -> CheckResult {
        let Check::Change {
            target,
            kind,
            on_change,
            require_step,
            baseline,
            baseline_snapshot,
            baseline_git,
            size_limit,
            scope,
            ..
        } = check
        else {
            unreachable!("evaluate_change called with non-Change check");
        };

        // Interpolate the target if it's a command
        let resolved_target = if *kind == super::ChangeKind::Command {
            self.interpolate(target)
        } else {
            target.to_string()
        };

        // Compute current hash
        let current_hash =
            match compute_target_hash(&resolved_target, *kind, self.project_root, size_limit) {
                HashResult::Ok(hash) => hash,
                HashResult::SizeLimitExceeded { actual, limit } => {
                    let actual_mb = actual as f64 / (1024.0 * 1024.0);
                    let limit_mb = limit as f64 / (1024.0 * 1024.0);
                    return CheckResult::indeterminate_with_details(
                        format!(
                            "Change target '{}' exceeds size limit ({:.1} MB > {:.1} MB)",
                            target, actual_mb, limit_mb
                        ),
                        format!(
                            "{} could not be hashed, so no change verdict is available",
                            target
                        ),
                        "Narrow the target or increase size_limit",
                    );
                }
                HashResult::NotFound(msg) => {
                    return CheckResult::indeterminate_with_details(
                        format!("{} not found", target),
                        format!("{} does not exist, so there is nothing to compare", target),
                        msg,
                    );
                }
                HashResult::Error(msg) => {
                    return CheckResult::indeterminate_with_details(
                        format!("Error hashing {}", target),
                        format!(
                            "{} could not be hashed, so no change verdict is available",
                            target
                        ),
                        msg,
                    );
                }
            };

        // Determine baseline name and get stored hash
        let baseline_name = self.resolve_baseline_name(baseline, baseline_snapshot.as_deref());
        let baseline_hash = if let Some(git_ref) = baseline_git {
            self.get_git_baseline(target, git_ref)
        } else {
            self.get_snapshot_baseline(&baseline_name, scope, &check.config_hash())
        };

        evaluate_change_result(
            target,
            &current_hash,
            baseline_hash.as_deref(),
            on_change,
            require_step.as_deref(),
        )
    }

    /// Determine the baseline name from config.
    fn resolve_baseline_name(
        &self,
        baseline: &BaselineConfig,
        baseline_snapshot: Option<&str>,
    ) -> String {
        if let Some(slug) = baseline_snapshot {
            slug.to_string()
        } else {
            match baseline {
                BaselineConfig::EachRun => "_last_run".to_string(),
                BaselineConfig::FirstRun => "_first_run".to_string(),
            }
        }
    }

    /// Get a baseline hash from the snapshot store.
    fn get_snapshot_baseline(
        &mut self,
        baseline_name: &str,
        scope: &super::SnapshotScope,
        config_hash: &str,
    ) -> Option<String> {
        let key = self.make_snapshot_key(scope, config_hash)?;
        self.snapshots.get_baseline(&key, baseline_name)
    }

    /// Get a baseline hash from a git ref.
    ///
    /// Uses `git show <ref>:<path>` to get file content at that ref,
    /// then hashes it for comparison.
    fn get_git_baseline(&self, target: &str, git_ref: &str) -> Option<String> {
        let output = std::process::Command::new("git")
            .args(["show", &format!("{}:{}", git_ref, target)])
            .current_dir(self.project_root)
            .output()
            .ok()?;

        if output.status.success() {
            use sha2::{Digest, Sha256};
            let hash = Sha256::digest(&output.stdout);
            Some(format!("sha256:{}", hex::encode(hash)))
        } else {
            None
        }
    }

    /// Construct a snapshot key from the current step context.
    ///
    /// `config_hash` is the change leaf's own [`Check::config_hash`].
    fn make_snapshot_key(
        &self,
        scope: &super::SnapshotScope,
        config_hash: &str,
    ) -> Option<SnapshotKey> {
        let step_name = self.step_name.as_ref()?;

        Some(match scope {
            super::SnapshotScope::Project => SnapshotKey::project(step_name.clone(), config_hash),
            super::SnapshotScope::Workflow => {
                if let Some(wf_name) = &self.workflow_name {
                    SnapshotKey::workflow(step_name.clone(), wf_name.clone(), config_hash)
                } else {
                    // Fall back to project scope when no workflow name is set.
                    // This can happen when evaluating checks outside of a
                    // workflow context (e.g., `bivvy lint`).
                    SnapshotKey::project(step_name.clone(), config_hash)
                }
            }
        })
    }

    /// Evaluate an `All` combinator.
    ///
    /// All checks must pass. Short-circuits on first failure.
    /// If any check is indeterminate and none failed, the result is indeterminate.
    fn evaluate_all(&mut self, checks: &[Check]) -> CheckResult {
        if checks.is_empty() {
            return CheckResult::passed("No checks to evaluate (all vacuously true)");
        }

        let mut descriptions = Vec::new();
        let mut has_indeterminate = false;
        let mut indeterminate_reason = String::new();

        for check in checks {
            let result = self.evaluate(check);
            descriptions.push(result.description.clone());

            match &result.outcome {
                CheckOutcome::Failed => {
                    return CheckResult {
                        outcome: CheckOutcome::Failed,
                        description: format!("All: {} failed", result.description),
                        details: result.details,
                    };
                }
                CheckOutcome::Indeterminate(reason) => {
                    has_indeterminate = true;
                    indeterminate_reason = reason.clone();
                }
                CheckOutcome::Passed => {}
            }
        }

        if has_indeterminate {
            CheckResult::indeterminate(
                format!("All: indeterminate ({})", descriptions.join(", ")),
                indeterminate_reason,
            )
        } else {
            CheckResult::passed(format!("All passed: {}", descriptions.join(", ")))
        }
    }

    /// Evaluate an `Any` combinator.
    ///
    /// At least one check must pass. Short-circuits on first pass.
    ///
    /// Disjunction precedence is `Passed > Indeterminate > Failed`: the result
    /// is only `Failed` when every branch actually failed. If nothing passed but
    /// at least one branch was indeterminate, the combinator is indeterminate,
    /// because a branch that gave no answer could have been the one that passed.
    fn evaluate_any(&mut self, checks: &[Check]) -> CheckResult {
        if checks.is_empty() {
            return CheckResult::failed(
                "No checks to evaluate (any vacuously false)",
                "At least one check required",
            );
        }

        let mut descriptions = Vec::new();
        let mut last_details = None;
        let mut indeterminate_reason: Option<String> = None;

        for check in checks {
            let result = self.evaluate(check);
            descriptions.push(result.description.clone());

            match &result.outcome {
                CheckOutcome::Passed => {
                    return CheckResult::passed(format!("Any: {} passed", result.description));
                }
                CheckOutcome::Indeterminate(reason) => {
                    indeterminate_reason = Some(reason.clone());
                }
                CheckOutcome::Failed => {}
            }
            last_details = result.details;
        }

        match indeterminate_reason {
            Some(reason) => CheckResult::indeterminate(
                format!("Any: indeterminate ({})", descriptions.join(", ")),
                reason,
            ),
            None => CheckResult {
                outcome: CheckOutcome::Failed,
                description: format!("Any: none passed ({})", descriptions.join(", ")),
                details: last_details,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_context() -> InterpolationContext {
        InterpolationContext::new()
    }

    fn make_store(dir: &Path) -> SnapshotStore {
        SnapshotStore::new(dir)
    }

    // --- Presence checks through evaluator ---

    #[test]
    fn evaluator_presence_file_exists() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join("node_modules")).unwrap();
        fs::write(temp.path().join("node_modules/.keep"), "").unwrap();

        let ctx = make_context();
        let mut store = make_store(snap_dir.path());
        let mut eval = CheckEvaluator::new(temp.path(), &ctx, &mut store);

        let check = Check::Presence {
            name: None,
            target: Some("node_modules".to_string()),
            kind: Some(PresenceKind::File),
            command: None,
        };

        let result = eval.evaluate(&check);
        assert!(result.passed_check());
        assert_eq!(result.description, "\u{2713} node_modules exists");
    }

    #[test]
    fn evaluator_presence_file_missing() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();
        let ctx = make_context();
        let mut store = make_store(snap_dir.path());
        let mut eval = CheckEvaluator::new(temp.path(), &ctx, &mut store);

        let check = Check::Presence {
            name: None,
            target: Some("missing_dir".to_string()),
            kind: Some(PresenceKind::File),
            command: None,
        };

        let result = eval.evaluate(&check);
        assert!(!result.passed_check());
        assert!(result.description.contains("not found"));
    }

    #[test]
    fn evaluator_presence_binary_found() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();
        let ctx = make_context();
        let mut store = make_store(snap_dir.path());
        let mut eval = CheckEvaluator::new(temp.path(), &ctx, &mut store);

        let check = Check::Presence {
            name: None,
            target: Some("sh".to_string()),
            kind: Some(PresenceKind::Binary),
            command: None,
        };

        let result = eval.evaluate(&check);
        assert!(result.passed_check());
    }

    #[test]
    fn evaluator_presence_binary_missing() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();
        let ctx = make_context();
        let mut store = make_store(snap_dir.path());
        let mut eval = CheckEvaluator::new(temp.path(), &ctx, &mut store);

        let check = Check::Presence {
            name: None,
            target: Some("nonexistent_binary_xyz_99999".to_string()),
            kind: Some(PresenceKind::Binary),
            command: None,
        };

        let result = eval.evaluate(&check);
        assert!(!result.passed_check());
    }

    #[test]
    fn evaluator_presence_custom_success() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();
        let ctx = make_context();
        let mut store = make_store(snap_dir.path());
        let mut eval = CheckEvaluator::new(temp.path(), &ctx, &mut store);

        let check = Check::Presence {
            name: None,
            target: None,
            kind: Some(PresenceKind::Custom),
            command: Some("exit 0".to_string()),
        };

        let result = eval.evaluate(&check);
        assert!(result.passed_check());
    }

    // --- Execution checks through evaluator ---

    #[test]
    fn evaluator_execution_success() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();
        let ctx = make_context();
        let mut store = make_store(snap_dir.path());
        let mut eval = CheckEvaluator::new(temp.path(), &ctx, &mut store);

        let check = Check::Execution {
            name: None,
            command: "exit 0".to_string(),
            validation: ValidationMode::Success,
        };

        let result = eval.evaluate(&check);
        assert!(result.passed_check());
    }

    #[test]
    fn evaluator_execution_failure() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();
        let ctx = make_context();
        let mut store = make_store(snap_dir.path());
        let mut eval = CheckEvaluator::new(temp.path(), &ctx, &mut store);

        let check = Check::Execution {
            name: None,
            command: "exit 1".to_string(),
            validation: ValidationMode::Success,
        };

        let result = eval.evaluate(&check);
        assert!(!result.passed_check());
    }

    #[test]
    fn evaluator_execution_truthy() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();
        let ctx = make_context();
        let mut store = make_store(snap_dir.path());
        let mut eval = CheckEvaluator::new(temp.path(), &ctx, &mut store);

        let check = Check::Execution {
            name: None,
            command: "echo hello".to_string(),
            validation: ValidationMode::Truthy,
        };

        let result = eval.evaluate(&check);
        assert!(result.passed_check());
    }

    #[test]
    fn evaluator_execution_falsy() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();
        let ctx = make_context();
        let mut store = make_store(snap_dir.path());
        let mut eval = CheckEvaluator::new(temp.path(), &ctx, &mut store);

        let check = Check::Execution {
            name: None,
            command: "true".to_string(),
            validation: ValidationMode::Falsy,
        };

        let result = eval.evaluate(&check);
        assert!(result.passed_check());
    }

    // --- Execution with interpolation ---

    #[test]
    fn evaluator_execution_with_interpolation() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();
        fs::write(temp.path().join("target.txt"), "found").unwrap();

        let mut ctx = make_context();
        ctx.vars
            .insert("filename".to_string(), "target.txt".to_string());
        let mut store = make_store(snap_dir.path());
        let mut eval = CheckEvaluator::new(temp.path(), &ctx, &mut store);

        let check = Check::Execution {
            name: None,
            command: "test -f ${filename}".to_string(),
            validation: ValidationMode::Success,
        };

        let result = eval.evaluate(&check);
        assert!(result.passed_check());
    }

    #[test]
    fn evaluator_presence_custom_with_interpolation() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();
        fs::write(temp.path().join("check_me.txt"), "here").unwrap();

        let mut ctx = make_context();
        ctx.vars
            .insert("check_file".to_string(), "check_me.txt".to_string());
        let mut store = make_store(snap_dir.path());
        let mut eval = CheckEvaluator::new(temp.path(), &ctx, &mut store);

        let check = Check::Presence {
            name: None,
            target: None,
            kind: Some(PresenceKind::Custom),
            command: Some("test -f ${check_file}".to_string()),
        };

        let result = eval.evaluate(&check);
        assert!(result.passed_check());
    }

    // --- Change checks through evaluator ---

    #[test]
    fn evaluator_change_no_baseline_returns_indeterminate() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();
        fs::write(temp.path().join("Gemfile.lock"), "gem contents").unwrap();

        let ctx = make_context();
        let mut store = make_store(snap_dir.path());
        let mut eval =
            CheckEvaluator::new(temp.path(), &ctx, &mut store).with_step("bundle_install");

        let check = Check::Change {
            name: None,
            target: "Gemfile.lock".to_string(),
            kind: ChangeKind::File,
            on_change: OnChange::Proceed,
            require_step: None,
            baseline: BaselineConfig::EachRun,
            baseline_snapshot: None,
            baseline_git: None,
            size_limit: SizeLimit::default(),
            scope: SnapshotScope::Project,
        };

        let result = eval.evaluate(&check);
        assert!(matches!(result.outcome, CheckOutcome::Indeterminate(_)));
        assert!(result.description.contains("No baseline"));
    }

    #[test]
    fn evaluator_change_with_matching_baseline_proceed_passes() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();
        fs::write(temp.path().join("Gemfile.lock"), "gem contents").unwrap();

        let ctx = make_context();
        let mut store = make_store(snap_dir.path());

        let check = Check::Change {
            name: None,
            target: "Gemfile.lock".to_string(),
            kind: ChangeKind::File,
            on_change: OnChange::Proceed,
            require_step: None,
            baseline: BaselineConfig::EachRun,
            baseline_snapshot: None,
            baseline_git: None,
            size_limit: SizeLimit::default(),
            scope: SnapshotScope::Project,
        };

        let current_hash = match compute_target_hash(
            "Gemfile.lock",
            ChangeKind::File,
            temp.path(),
            &SizeLimit::default(),
        ) {
            HashResult::Ok(h) => h,
            _ => panic!("Expected hash"),
        };

        let key = SnapshotKey::project("bundle_install", check.config_hash());
        store.record_baseline(&key, "_last_run", current_hash, "Gemfile.lock".to_string());

        let mut eval =
            CheckEvaluator::new(temp.path(), &ctx, &mut store).with_step("bundle_install");

        // File unchanged since baseline - proceed means "skip when nothing to do"
        let result = eval.evaluate(&check);
        assert!(result.passed_check());
        assert!(result.description.contains("unchanged"));
    }

    #[test]
    fn evaluator_change_with_different_baseline_proceed_fails() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();
        fs::write(temp.path().join("Gemfile.lock"), "new contents").unwrap();

        let ctx = make_context();
        let mut store = make_store(snap_dir.path());

        let check = Check::Change {
            name: None,
            target: "Gemfile.lock".to_string(),
            kind: ChangeKind::File,
            on_change: OnChange::Proceed,
            require_step: None,
            baseline: BaselineConfig::EachRun,
            baseline_snapshot: None,
            baseline_git: None,
            size_limit: SizeLimit::default(),
            scope: SnapshotScope::Project,
        };

        let key = SnapshotKey::project("bundle_install", check.config_hash());
        store.record_baseline(
            &key,
            "_last_run",
            "sha256:old_hash_value".to_string(),
            "Gemfile.lock".to_string(),
        );

        let mut eval =
            CheckEvaluator::new(temp.path(), &ctx, &mut store).with_step("bundle_install");

        // File changed since baseline - proceed means "run when something changed"
        let result = eval.evaluate(&check);
        assert!(!result.passed_check());
        assert!(result.description.contains("changed"));
    }

    #[test]
    fn evaluator_change_on_change_fail_unchanged_passes() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();
        fs::write(temp.path().join(".env.example"), "DB_HOST=localhost").unwrap();

        let ctx = make_context();
        let mut store = make_store(snap_dir.path());

        let check = Check::Change {
            name: None,
            target: ".env.example".to_string(),
            kind: ChangeKind::File,
            on_change: OnChange::Fail,
            require_step: None,
            baseline: BaselineConfig::EachRun,
            baseline_snapshot: None,
            baseline_git: None,
            size_limit: SizeLimit::default(),
            scope: SnapshotScope::Project,
        };

        let current_hash = match compute_target_hash(
            ".env.example",
            ChangeKind::File,
            temp.path(),
            &SizeLimit::default(),
        ) {
            HashResult::Ok(h) => h,
            _ => panic!("Expected hash"),
        };

        let key = SnapshotKey::project("validate_env", check.config_hash());
        store.record_baseline(&key, "_last_run", current_hash, ".env.example".to_string());

        let mut eval = CheckEvaluator::new(temp.path(), &ctx, &mut store).with_step("validate_env");

        let result = eval.evaluate(&check);
        assert!(result.passed_check());
    }

    #[test]
    fn evaluator_change_on_change_fail_changed_fails() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();
        fs::write(temp.path().join(".env.example"), "DB_HOST=localhost").unwrap();

        let ctx = make_context();
        let mut store = make_store(snap_dir.path());

        let check = Check::Change {
            name: None,
            target: ".env.example".to_string(),
            kind: ChangeKind::File,
            on_change: OnChange::Fail,
            require_step: None,
            baseline: BaselineConfig::EachRun,
            baseline_snapshot: None,
            baseline_git: None,
            size_limit: SizeLimit::default(),
            scope: SnapshotScope::Project,
        };

        let key = SnapshotKey::project("validate_env", check.config_hash());
        store.record_baseline(
            &key,
            "_last_run",
            "sha256:old_hash".to_string(),
            ".env.example".to_string(),
        );

        let mut eval = CheckEvaluator::new(temp.path(), &ctx, &mut store).with_step("validate_env");

        let result = eval.evaluate(&check);
        assert!(!result.passed_check());
        assert!(result.description.contains("changed unexpectedly"));
    }

    #[test]
    fn evaluator_change_on_change_require_is_indeterminate() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();
        fs::write(temp.path().join("Gemfile.lock"), "new gem contents").unwrap();

        let ctx = make_context();
        let mut store = make_store(snap_dir.path());

        let check = Check::Change {
            name: None,
            target: "Gemfile.lock".to_string(),
            kind: ChangeKind::File,
            on_change: OnChange::Require,
            require_step: Some("bundle_install".to_string()),
            baseline: BaselineConfig::EachRun,
            baseline_snapshot: None,
            baseline_git: None,
            size_limit: SizeLimit::default(),
            scope: SnapshotScope::Project,
        };

        let key = SnapshotKey::project("check_gemfile", check.config_hash());
        store.record_baseline(
            &key,
            "_last_run",
            "sha256:old_hash".to_string(),
            "Gemfile.lock".to_string(),
        );

        let mut eval =
            CheckEvaluator::new(temp.path(), &ctx, &mut store).with_step("check_gemfile");

        let result = eval.evaluate(&check);
        assert!(matches!(result.outcome, CheckOutcome::Indeterminate(_)));
        assert!(result.description.contains("bundle_install"));
    }

    #[test]
    fn evaluator_change_file_not_found() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();

        let ctx = make_context();
        let mut store = make_store(snap_dir.path());
        let mut eval = CheckEvaluator::new(temp.path(), &ctx, &mut store).with_step("step");

        let check = Check::Change {
            name: None,
            target: "nonexistent.lock".to_string(),
            kind: ChangeKind::File,
            on_change: OnChange::Proceed,
            require_step: None,
            baseline: BaselineConfig::EachRun,
            baseline_snapshot: None,
            baseline_git: None,
            size_limit: SizeLimit::default(),
            scope: SnapshotScope::Project,
        };

        let result = eval.evaluate(&check);
        assert!(!result.passed_check());
        assert!(result.description.contains("not found"));
        assert!(
            matches!(result.outcome, CheckOutcome::Indeterminate(_)),
            "missing target must be Indeterminate, not Failed"
        );
    }

    #[test]
    fn evaluator_change_size_limit_exceeded() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();
        fs::write(temp.path().join("big.dat"), "x".repeat(200)).unwrap();

        let ctx = make_context();
        let mut store = make_store(snap_dir.path());
        let mut eval = CheckEvaluator::new(temp.path(), &ctx, &mut store).with_step("step");

        let check = Check::Change {
            name: None,
            target: "big.dat".to_string(),
            kind: ChangeKind::File,
            on_change: OnChange::Proceed,
            require_step: None,
            baseline: BaselineConfig::EachRun,
            baseline_snapshot: None,
            baseline_git: None,
            size_limit: SizeLimit::bytes(100),
            scope: SnapshotScope::Project,
        };

        let result = eval.evaluate(&check);
        assert!(!result.passed_check());
        assert!(result.description.contains("exceeds size limit"));
        assert!(
            matches!(result.outcome, CheckOutcome::Indeterminate(_)),
            "size limit exceeded must be Indeterminate, not Failed"
        );
    }

    #[test]
    fn evaluator_change_named_snapshot_baseline() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();
        fs::write(temp.path().join("Gemfile.lock"), "gem contents v2").unwrap();

        let ctx = make_context();
        let mut store = make_store(snap_dir.path());

        let check = Check::Change {
            name: None,
            target: "Gemfile.lock".to_string(),
            kind: ChangeKind::File,
            on_change: OnChange::Proceed,
            require_step: None,
            baseline: BaselineConfig::EachRun,
            baseline_snapshot: Some("v1.0".to_string()),
            baseline_git: None,
            size_limit: SizeLimit::default(),
            scope: SnapshotScope::Project,
        };

        let key = SnapshotKey::project("bundle_install", check.config_hash());
        store.capture_named(
            &key,
            "v1.0",
            "sha256:old_release_hash".to_string(),
            "Gemfile.lock".to_string(),
        );

        let mut eval =
            CheckEvaluator::new(temp.path(), &ctx, &mut store).with_step("bundle_install");

        // File differs from named snapshot - proceed means not satisfied (step must run)
        let result = eval.evaluate(&check);
        assert!(!result.passed_check());
    }

    #[test]
    fn evaluator_change_without_step_context_returns_indeterminate() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();
        fs::write(temp.path().join("file.txt"), "content").unwrap();

        let ctx = make_context();
        let mut store = make_store(snap_dir.path());
        // No with_step call — no step context
        let mut eval = CheckEvaluator::new(temp.path(), &ctx, &mut store);

        let check = Check::Change {
            name: None,
            target: "file.txt".to_string(),
            kind: ChangeKind::File,
            on_change: OnChange::Proceed,
            require_step: None,
            baseline: BaselineConfig::EachRun,
            baseline_snapshot: None,
            baseline_git: None,
            size_limit: SizeLimit::default(),
            scope: SnapshotScope::Project,
        };

        // No step context means no snapshot key → no baseline → indeterminate
        let result = eval.evaluate(&check);
        assert!(matches!(result.outcome, CheckOutcome::Indeterminate(_)));
    }

    // --- Combinator tests ---

    #[test]
    fn evaluator_all_passes_when_all_pass() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();
        fs::write(temp.path().join("file.txt"), "content").unwrap();

        let ctx = make_context();
        let mut store = make_store(snap_dir.path());
        let mut eval = CheckEvaluator::new(temp.path(), &ctx, &mut store);

        let check = Check::All {
            name: None,
            checks: vec![
                Check::Presence {
                    name: None,
                    target: Some("file.txt".to_string()),
                    kind: Some(PresenceKind::File),
                    command: None,
                },
                Check::Execution {
                    name: None,
                    command: "exit 0".to_string(),
                    validation: ValidationMode::Success,
                },
            ],
        };

        let result = eval.evaluate(&check);
        assert!(result.passed_check());
        assert!(result.description.contains("All passed"));
    }

    #[test]
    fn evaluator_all_fails_when_one_fails() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();

        let ctx = make_context();
        let mut store = make_store(snap_dir.path());
        let mut eval = CheckEvaluator::new(temp.path(), &ctx, &mut store);

        let check = Check::All {
            name: None,
            checks: vec![
                Check::Presence {
                    name: None,
                    target: Some("missing.txt".to_string()),
                    kind: Some(PresenceKind::File),
                    command: None,
                },
                Check::Execution {
                    name: None,
                    command: "exit 0".to_string(),
                    validation: ValidationMode::Success,
                },
            ],
        };

        let result = eval.evaluate(&check);
        assert!(!result.passed_check());
        assert!(result.description.contains("All:"));
        assert!(result.description.contains("failed"));
    }

    #[test]
    fn evaluator_all_short_circuits_on_failure() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();

        let ctx = make_context();
        let mut store = make_store(snap_dir.path());
        let mut eval = CheckEvaluator::new(temp.path(), &ctx, &mut store);

        // First check fails, second would also fail but shouldn't be evaluated
        let check = Check::All {
            name: None,
            checks: vec![
                Check::Execution {
                    name: None,
                    command: "exit 1".to_string(),
                    validation: ValidationMode::Success,
                },
                // This would take time if evaluated
                Check::Execution {
                    name: None,
                    command: "sleep 10".to_string(),
                    validation: ValidationMode::Success,
                },
            ],
        };

        let result = eval.evaluate(&check);
        assert!(!result.passed_check());
    }

    #[test]
    fn evaluator_any_passes_when_one_passes() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();
        fs::write(temp.path().join("file.txt"), "content").unwrap();

        let ctx = make_context();
        let mut store = make_store(snap_dir.path());
        let mut eval = CheckEvaluator::new(temp.path(), &ctx, &mut store);

        let check = Check::Any {
            name: None,
            checks: vec![
                Check::Presence {
                    name: None,
                    target: Some("missing.txt".to_string()),
                    kind: Some(PresenceKind::File),
                    command: None,
                },
                Check::Presence {
                    name: None,
                    target: Some("file.txt".to_string()),
                    kind: Some(PresenceKind::File),
                    command: None,
                },
            ],
        };

        let result = eval.evaluate(&check);
        assert!(result.passed_check());
        assert!(result.description.contains("Any:"));
    }

    #[test]
    fn evaluator_any_fails_when_all_fail() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();

        let ctx = make_context();
        let mut store = make_store(snap_dir.path());
        let mut eval = CheckEvaluator::new(temp.path(), &ctx, &mut store);

        let check = Check::Any {
            name: None,
            checks: vec![
                Check::Presence {
                    name: None,
                    target: Some("missing1.txt".to_string()),
                    kind: Some(PresenceKind::File),
                    command: None,
                },
                Check::Presence {
                    name: None,
                    target: Some("missing2.txt".to_string()),
                    kind: Some(PresenceKind::File),
                    command: None,
                },
            ],
        };

        let result = eval.evaluate(&check);
        assert!(!result.passed_check());
        assert!(result.description.contains("none passed"));
    }

    #[test]
    fn evaluator_any_short_circuits_on_pass() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();

        let ctx = make_context();
        let mut store = make_store(snap_dir.path());
        let mut eval = CheckEvaluator::new(temp.path(), &ctx, &mut store);

        let check = Check::Any {
            name: None,
            checks: vec![
                Check::Execution {
                    name: None,
                    command: "exit 0".to_string(),
                    validation: ValidationMode::Success,
                },
                // Would take time if evaluated
                Check::Execution {
                    name: None,
                    command: "sleep 10".to_string(),
                    validation: ValidationMode::Success,
                },
            ],
        };

        let result = eval.evaluate(&check);
        assert!(result.passed_check());
    }

    // --- Nested combinators ---

    #[test]
    fn evaluator_nested_all_in_any() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();
        fs::write(temp.path().join("a.txt"), "a").unwrap();
        fs::write(temp.path().join("b.txt"), "b").unwrap();

        let ctx = make_context();
        let mut store = make_store(snap_dir.path());
        let mut eval = CheckEvaluator::new(temp.path(), &ctx, &mut store);

        // any(all(missing1, missing2), all(a.txt, b.txt)) → passes because second all passes
        let check = Check::Any {
            name: None,
            checks: vec![
                Check::All {
                    name: None,
                    checks: vec![
                        Check::Presence {
                            name: None,
                            target: Some("missing1.txt".to_string()),
                            kind: Some(PresenceKind::File),
                            command: None,
                        },
                        Check::Presence {
                            name: None,
                            target: Some("missing2.txt".to_string()),
                            kind: Some(PresenceKind::File),
                            command: None,
                        },
                    ],
                },
                Check::All {
                    name: None,
                    checks: vec![
                        Check::Presence {
                            name: None,
                            target: Some("a.txt".to_string()),
                            kind: Some(PresenceKind::File),
                            command: None,
                        },
                        Check::Presence {
                            name: None,
                            target: Some("b.txt".to_string()),
                            kind: Some(PresenceKind::File),
                            command: None,
                        },
                    ],
                },
            ],
        };

        let result = eval.evaluate(&check);
        assert!(result.passed_check());
    }

    // --- Empty combinator edge cases ---

    #[test]
    fn evaluator_all_empty_passes() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();
        let ctx = make_context();
        let mut store = make_store(snap_dir.path());
        let mut eval = CheckEvaluator::new(temp.path(), &ctx, &mut store);

        let check = Check::All {
            name: None,
            checks: vec![],
        };

        let result = eval.evaluate(&check);
        assert!(result.passed_check());
    }

    #[test]
    fn evaluator_any_empty_fails() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();
        let ctx = make_context();
        let mut store = make_store(snap_dir.path());
        let mut eval = CheckEvaluator::new(temp.path(), &ctx, &mut store);

        let check = Check::Any {
            name: None,
            checks: vec![],
        };

        let result = eval.evaluate(&check);
        assert!(!result.passed_check());
    }

    #[test]
    fn evaluator_any_indeterminate_when_all_indeterminate() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();
        let ctx = make_context();
        let mut store = make_store(snap_dir.path());
        let mut eval = CheckEvaluator::new(temp.path(), &ctx, &mut store).with_step("step");

        let check = Check::Any {
            name: None,
            checks: vec![
                Check::Change {
                    name: None,
                    target: "nonexistent1.lock".to_string(),
                    kind: ChangeKind::File,
                    on_change: OnChange::Proceed,
                    require_step: None,
                    baseline: BaselineConfig::EachRun,
                    baseline_snapshot: None,
                    baseline_git: None,
                    size_limit: SizeLimit::default(),
                    scope: SnapshotScope::Project,
                },
                Check::Change {
                    name: None,
                    target: "nonexistent2.lock".to_string(),
                    kind: ChangeKind::File,
                    on_change: OnChange::Proceed,
                    require_step: None,
                    baseline: BaselineConfig::EachRun,
                    baseline_snapshot: None,
                    baseline_git: None,
                    size_limit: SizeLimit::default(),
                    scope: SnapshotScope::Project,
                },
            ],
        };

        let result = eval.evaluate(&check);
        assert!(!result.passed_check());
        assert!(
            matches!(result.outcome, CheckOutcome::Indeterminate(_)),
            "all-Indeterminate Any must be Indeterminate, not Failed"
        );
    }

    #[test]
    fn evaluator_any_indeterminate_beats_failed() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();
        let ctx = make_context();
        let mut store = make_store(snap_dir.path());
        let mut eval = CheckEvaluator::new(temp.path(), &ctx, &mut store).with_step("step");

        let check = Check::Any {
            name: None,
            checks: vec![
                Check::Presence {
                    name: None,
                    target: Some("missing.txt".to_string()),
                    kind: Some(PresenceKind::File),
                    command: None,
                },
                Check::Change {
                    name: None,
                    target: "nonexistent.lock".to_string(),
                    kind: ChangeKind::File,
                    on_change: OnChange::Proceed,
                    require_step: None,
                    baseline: BaselineConfig::EachRun,
                    baseline_snapshot: None,
                    baseline_git: None,
                    size_limit: SizeLimit::default(),
                    scope: SnapshotScope::Project,
                },
            ],
        };

        let result = eval.evaluate(&check);
        assert!(!result.passed_check());
        assert!(
            matches!(result.outcome, CheckOutcome::Indeterminate(_)),
            "[Failed, Indeterminate] Any must be Indeterminate, not Failed"
        );
    }

    #[test]
    fn evaluator_any_indeterminate_beats_failed_reversed_order() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();
        let ctx = make_context();
        let mut store = make_store(snap_dir.path());
        let mut eval = CheckEvaluator::new(temp.path(), &ctx, &mut store).with_step("step");

        let check = Check::Any {
            name: None,
            checks: vec![
                Check::Change {
                    name: None,
                    target: "nonexistent.lock".to_string(),
                    kind: ChangeKind::File,
                    on_change: OnChange::Proceed,
                    require_step: None,
                    baseline: BaselineConfig::EachRun,
                    baseline_snapshot: None,
                    baseline_git: None,
                    size_limit: SizeLimit::default(),
                    scope: SnapshotScope::Project,
                },
                Check::Presence {
                    name: None,
                    target: Some("missing.txt".to_string()),
                    kind: Some(PresenceKind::File),
                    command: None,
                },
            ],
        };

        let result = eval.evaluate(&check);
        assert!(!result.passed_check());
        assert!(
            matches!(result.outcome, CheckOutcome::Indeterminate(_)),
            "[Indeterminate, Failed] Any must be Indeterminate, not Failed"
        );
    }

    // --- evaluate_all_checks helper ---

    #[test]
    fn evaluate_all_checks_works_for_check_list() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();
        fs::write(temp.path().join("file.txt"), "content").unwrap();

        let ctx = make_context();
        let mut store = make_store(snap_dir.path());
        let mut eval = CheckEvaluator::new(temp.path(), &ctx, &mut store);

        let checks = vec![
            Check::Presence {
                name: None,
                target: Some("file.txt".to_string()),
                kind: Some(PresenceKind::File),
                command: None,
            },
            Check::Execution {
                name: None,
                command: "exit 0".to_string(),
                validation: ValidationMode::Success,
            },
        ];

        let result = eval.evaluate_all_checks(&checks);
        assert!(result.passed_check());
    }

    // --- Change check with glob through evaluator ---

    #[test]
    fn evaluator_change_glob_with_baseline() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();
        fs::write(temp.path().join("001_create.rb"), "migration 1").unwrap();

        let ctx = make_context();
        let mut store = make_store(snap_dir.path());

        let check = Check::Change {
            name: None,
            target: "*.rb".to_string(),
            kind: ChangeKind::Glob,
            on_change: OnChange::Proceed,
            require_step: None,
            baseline: BaselineConfig::EachRun,
            baseline_snapshot: None,
            baseline_git: None,
            size_limit: SizeLimit::default(),
            scope: SnapshotScope::Project,
        };

        let key = SnapshotKey::project("db_migrate", check.config_hash());
        store.record_baseline(
            &key,
            "_last_run",
            "sha256:old_glob_hash".to_string(),
            "*.rb".to_string(),
        );

        let mut eval = CheckEvaluator::new(temp.path(), &ctx, &mut store).with_step("db_migrate");

        // Current glob hash differs from stored - proceed means not satisfied
        let result = eval.evaluate(&check);
        assert!(!result.passed_check());
    }

    // --- Change check with command through evaluator ---

    #[test]
    fn evaluator_change_command_with_baseline() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();

        let ctx = make_context();
        let mut store = make_store(snap_dir.path());

        let check = Check::Change {
            name: None,
            target: "echo hello".to_string(),
            kind: ChangeKind::Command,
            on_change: OnChange::Fail,
            require_step: None,
            baseline: BaselineConfig::FirstRun,
            baseline_snapshot: None,
            baseline_git: None,
            size_limit: SizeLimit::default(),
            scope: SnapshotScope::Project,
        };

        // Store under _last_run but baseline: first_run looks for _first_run - no match
        let key = SnapshotKey::project("check_version", check.config_hash());
        store.record_baseline(
            &key,
            "_last_run",
            "sha256:old_output_hash".to_string(),
            "echo hello".to_string(),
        );

        let mut eval =
            CheckEvaluator::new(temp.path(), &ctx, &mut store).with_step("check_version");

        // FirstRun looks for "_first_run" baseline, only "_last_run" exists - indeterminate
        let result = eval.evaluate(&check);
        assert!(matches!(result.outcome, CheckOutcome::Indeterminate(_)));
    }

    #[test]
    fn evaluator_change_command_first_run_with_correct_baseline() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();

        let ctx = make_context();
        let mut store = make_store(snap_dir.path());

        // Compute the actual hash for "echo stable"
        let actual_hash = {
            use sha2::{Digest, Sha256};
            let output = std::process::Command::new("sh")
                .arg("-c")
                .arg("echo stable")
                .output()
                .unwrap();
            format!("sha256:{}", hex::encode(Sha256::digest(&output.stdout)))
        };

        let check = Check::Change {
            name: None,
            target: "echo stable".to_string(),
            kind: ChangeKind::Command,
            on_change: OnChange::Fail,
            require_step: None,
            baseline: BaselineConfig::FirstRun,
            baseline_snapshot: None,
            baseline_git: None,
            size_limit: SizeLimit::default(),
            scope: SnapshotScope::Project,
        };

        let key = SnapshotKey::project("check_version", check.config_hash());
        store.record_baseline(&key, "_first_run", actual_hash, "echo stable".to_string());

        let mut eval =
            CheckEvaluator::new(temp.path(), &ctx, &mut store).with_step("check_version");

        // Output hasn't changed from first run → on_change: fail → passes
        let result = eval.evaluate(&check);
        assert!(result.passed_check());
        assert!(result.description.contains("unchanged"));
    }

    // --- Deserialized check through evaluator (integration-style) ---

    #[test]
    fn evaluator_with_deserialized_yaml_check() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join("node_modules")).unwrap();
        fs::write(temp.path().join("node_modules/.keep"), "").unwrap();

        let yaml = r#"
            type: all
            checks:
              - type: presence
                target: node_modules
                kind: file
              - type: execution
                command: "exit 0"
                validation: success
        "#;

        let check: Check = serde_yaml::from_str(yaml).unwrap();

        let ctx = make_context();
        let mut store = make_store(snap_dir.path());
        let mut eval = CheckEvaluator::new(temp.path(), &ctx, &mut store);

        let result = eval.evaluate(&check);
        assert!(result.passed_check());
    }

    #[test]
    fn evaluator_with_deserialized_any_check() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();
        fs::write(temp.path().join(".ruby-version"), "3.2.0").unwrap();

        let yaml = r#"
            type: any
            checks:
              - type: presence
                target: ".ruby-version"
              - type: presence
                target: ".tool-versions"
        "#;

        let check: Check = serde_yaml::from_str(yaml).unwrap();

        let ctx = make_context();
        let mut store = make_store(snap_dir.path());
        let mut eval = CheckEvaluator::new(temp.path(), &ctx, &mut store);

        let result = eval.evaluate(&check);
        assert!(result.passed_check());
    }

    // --- Workflow scope tests ---

    #[test]
    fn evaluator_change_workflow_scope_uses_workflow_key() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();
        fs::write(temp.path().join("Gemfile.lock"), "gem contents").unwrap();

        let ctx = make_context();
        let mut store = make_store(snap_dir.path());

        let check = Check::Change {
            name: None,
            target: "Gemfile.lock".to_string(),
            kind: ChangeKind::File,
            on_change: OnChange::Proceed,
            require_step: None,
            baseline: BaselineConfig::EachRun,
            baseline_snapshot: None,
            baseline_git: None,
            size_limit: SizeLimit::default(),
            scope: SnapshotScope::Workflow,
        };

        let current_hash = match compute_target_hash(
            "Gemfile.lock",
            ChangeKind::File,
            temp.path(),
            &SizeLimit::default(),
        ) {
            HashResult::Ok(h) => h,
            _ => panic!("Expected hash"),
        };

        let wf_key = SnapshotKey::workflow("bundle_install", "ci", check.config_hash());
        store.record_baseline(
            &wf_key,
            "_last_run",
            current_hash,
            "Gemfile.lock".to_string(),
        );

        let mut eval = CheckEvaluator::new(temp.path(), &ctx, &mut store)
            .with_step("bundle_install")
            .with_workflow("ci");

        // File unchanged since baseline - proceed means satisfied (skip)
        let result = eval.evaluate(&check);
        assert!(result.passed_check());
        assert!(result.description.contains("unchanged"));
    }

    #[test]
    fn evaluator_change_workflow_scope_isolates_from_project() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();
        fs::write(temp.path().join("Gemfile.lock"), "gem contents").unwrap();

        let ctx = make_context();
        let mut store = make_store(snap_dir.path());

        let check = Check::Change {
            name: None,
            target: "Gemfile.lock".to_string(),
            kind: ChangeKind::File,
            on_change: OnChange::Proceed,
            require_step: None,
            baseline: BaselineConfig::EachRun,
            baseline_snapshot: None,
            baseline_git: None,
            size_limit: SizeLimit::default(),
            scope: SnapshotScope::Workflow,
        };

        // Store baseline under project-scoped key only (not workflow-scoped)
        let proj_key = SnapshotKey::project("bundle_install", check.config_hash());
        store.record_baseline(
            &proj_key,
            "_last_run",
            "sha256:some_hash".to_string(),
            "Gemfile.lock".to_string(),
        );

        // Evaluate with workflow scope — no workflow baseline exists
        let mut eval = CheckEvaluator::new(temp.path(), &ctx, &mut store)
            .with_step("bundle_install")
            .with_workflow("ci");

        // Workflow key has no baseline - indeterminate
        let result = eval.evaluate(&check);
        assert!(matches!(result.outcome, CheckOutcome::Indeterminate(_)));
    }

    #[test]
    fn evaluator_change_workflow_scope_without_workflow_name_falls_back() {
        let temp = TempDir::new().unwrap();
        let snap_dir = TempDir::new().unwrap();
        fs::write(temp.path().join("file.txt"), "content").unwrap();

        let ctx = make_context();
        let mut store = make_store(snap_dir.path());

        let check = Check::Change {
            name: None,
            target: "file.txt".to_string(),
            kind: ChangeKind::File,
            on_change: OnChange::Proceed,
            require_step: None,
            baseline: BaselineConfig::EachRun,
            baseline_snapshot: None,
            baseline_git: None,
            size_limit: SizeLimit::default(),
            scope: SnapshotScope::Workflow,
        };

        let current_hash = match compute_target_hash(
            "file.txt",
            ChangeKind::File,
            temp.path(),
            &SizeLimit::default(),
        ) {
            HashResult::Ok(h) => h,
            _ => panic!("Expected hash"),
        };

        // Store baseline under project key
        let key = SnapshotKey::project("step", check.config_hash());
        store.record_baseline(&key, "_last_run", current_hash, "file.txt".to_string());

        // No with_workflow call - falls back to project scope
        let mut eval = CheckEvaluator::new(temp.path(), &ctx, &mut store).with_step("step");

        // Falls back to project scope, finds baseline, file unchanged - satisfied (skip)
        let result = eval.evaluate(&check);
        assert!(result.passed_check());
    }
}
