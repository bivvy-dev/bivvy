//! Workflow execution orchestration.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::{Duration, Instant};

use tracing::warn;

use crate::checks::evaluator::CheckEvaluator;
use crate::config::interpolation::InterpolationContext;
use crate::config::BivvyConfig;
use crate::error::{BivvyError, Result};
use crate::logging::EventBus;
use crate::requirements::checker::GapChecker;
use crate::snapshots::SnapshotStore;
use crate::state::StateStore;
use crate::steps::{execute_step, ExecutionOptions, ResolvedStep, StepResult, StepStatus};

use super::decision;
use super::dependency::{DependencyGraph, SkipBehavior};
use super::plan::build_execution_plan;

/// Progress events emitted during workflow execution.
#[derive(Debug)]
pub enum RunProgress<'a> {
    /// A step is about to start.
    StepStarting {
        name: &'a str,
        index: usize,
        total: usize,
    },
    /// A step finished.
    StepFinished {
        name: &'a str,
        result: &'a StepResult,
    },
    /// A step was skipped.
    StepSkipped { name: &'a str },
}

/// Orchestrates the execution of a workflow.
pub struct WorkflowRunner<'a> {
    pub(super) config: &'a BivvyConfig,
    pub(super) steps: HashMap<String, ResolvedStep>,
    pub(super) snapshot_store: SnapshotStore,
}

/// Result of running a workflow.
#[derive(Debug)]
pub struct WorkflowResult {
    /// Workflow name.
    pub workflow: String,
    /// Results from each executed step.
    pub steps: Vec<StepResult>,
    /// Names of skipped steps.
    pub skipped: Vec<String>,
    /// Total duration.
    pub duration: Duration,
    /// Whether all steps succeeded.
    pub success: bool,
    /// Whether the workflow was aborted by the user during recovery.
    pub aborted: bool,
}

/// Options for running a workflow.
#[derive(Debug, Default)]
pub struct RunOptions {
    /// Workflow name (defaults to "default").
    pub workflow: Option<String>,
    /// Only run these steps.
    pub only: HashSet<String>,
    /// Skip these steps.
    pub skip: HashSet<String>,
    /// How to handle skipped step dependencies.
    pub skip_behavior: SkipBehavior,
    /// Force re-run these steps even if complete.
    pub force: HashSet<String>,
    /// Force re-run every step in the workflow, bypassing all checks.
    pub force_all: bool,
    /// Dry run mode.
    pub dry_run: bool,
    /// Requirements that are provided by the environment and should skip gap checks.
    pub provided_requirements: HashSet<String>,
    /// Active environment name for only_environments filtering.
    pub active_environment: Option<String>,
    /// Use the diagnostic funnel pipeline for error recovery.
    pub diagnostic_funnel: bool,
    /// Discard all persisted satisfaction records and evaluate everything fresh.
    pub fresh: bool,
}

impl RunOptions {
    /// Whether the named step should be forced to run, bypassing checks.
    ///
    /// A step is forced if `force_all` is set, or if its name appears in the
    /// `force` set. Step- and workflow-level force directives are merged into
    /// these fields by the caller before the runner is invoked.
    pub fn should_force(&self, step_name: &str) -> bool {
        self.force_all || self.force.contains(step_name)
    }
}

impl<'a> WorkflowRunner<'a> {
    /// Create a new workflow runner with an in-memory snapshot store.
    pub fn new(config: &'a BivvyConfig, steps: HashMap<String, ResolvedStep>) -> Self {
        Self {
            config,
            steps,
            snapshot_store: SnapshotStore::empty(),
        }
    }

    /// Create a new workflow runner with a specific snapshot store.
    pub fn with_snapshot_store(
        config: &'a BivvyConfig,
        steps: HashMap<String, ResolvedStep>,
        snapshot_store: SnapshotStore,
    ) -> Self {
        Self {
            config,
            steps,
            snapshot_store,
        }
    }

    /// Get a mutable reference to the snapshot store (for saving after run).
    pub fn snapshot_store_mut(&mut self) -> &mut SnapshotStore {
        &mut self.snapshot_store
    }

    /// Run the specified workflow.
    ///
    /// `base_env` is the YAML-derived env stack (settings + workflow), already
    /// merged in priority order. `process_env` is the parent process
    /// environment, which wins over both `base_env` and any step-level env.
    #[allow(clippy::too_many_arguments)]
    pub fn run(
        &mut self,
        options: &RunOptions,
        context: &InterpolationContext,
        base_env: &HashMap<String, String>,
        process_env: &HashMap<String, String>,
        project_root: &Path,
    ) -> Result<WorkflowResult> {
        let mut event_bus = EventBus::new();
        self.run_with_progress(
            options,
            context,
            base_env,
            process_env,
            project_root,
            None,
            None,
            |_| {},
            &mut event_bus,
        )
    }

    /// Run the specified workflow with a progress callback.
    #[allow(clippy::too_many_arguments)]
    pub fn run_with_progress(
        &mut self,
        options: &RunOptions,
        context: &InterpolationContext,
        base_env: &HashMap<String, String>,
        process_env: &HashMap<String, String>,
        project_root: &Path,
        mut gap_checker: Option<&mut GapChecker<'_>>,
        mut state: Option<&mut StateStore>,
        mut on_progress: impl FnMut(RunProgress<'_>),
        _event_bus: &mut EventBus,
    ) -> Result<WorkflowResult> {
        let start = Instant::now();
        let workflow_name = options.workflow.as_deref().unwrap_or("default");

        // Build dependency graph and execution plan
        let graph = self.build_graph(workflow_name)?;
        let workflow_steps = &self.config.workflows[workflow_name].steps;
        let plan = build_execution_plan(&graph, workflow_steps, options, &self.steps)?;

        let total = plan.steps_to_run.len();

        // Report skipped steps
        for skip_name in &plan.flag_skipped {
            on_progress(RunProgress::StepSkipped { name: skip_name });
        }
        for skip_name in &plan.env_skipped {
            on_progress(RunProgress::StepSkipped { name: skip_name });
        }

        let mut results = Vec::new();
        let mut all_success = true;
        let mut failed_steps: HashSet<String> = HashSet::new();

        for (index, step_name) in plan.steps_to_run.iter().enumerate() {
            let step =
                self.steps
                    .get(step_name)
                    .ok_or_else(|| BivvyError::ConfigValidationError {
                        message: format!("Step '{}' not found in resolved steps", step_name),
                    })?;

            // Check if any dependency failed
            if decision::blocked_by_failure(step, &failed_steps) {
                on_progress(RunProgress::StepSkipped { name: step_name });
                all_success = false;
                failed_steps.insert(step_name.clone());
                continue;
            }

            // Check requirement gaps (non-UI: any blocking gap is an error)
            if let Some(ref mut checker) = gap_checker {
                let provided = if options.provided_requirements.is_empty() {
                    None
                } else {
                    Some(&options.provided_requirements)
                };
                let gaps = checker.check_step(step, provided);
                let blocking: Vec<_> = gaps
                    .iter()
                    .filter(|g| !g.status.is_satisfied() && !g.status.can_proceed())
                    .collect();
                if !blocking.is_empty() {
                    let names: Vec<_> = blocking.iter().map(|g| g.requirement.as_str()).collect();
                    return Err(BivvyError::RequirementMissing {
                        requirement: names.join(", "),
                        message: format!(
                            "Step '{}' requires: {}. Run 'bivvy lint' for details.",
                            step_name,
                            names.join(", ")
                        ),
                    });
                }
            }

            // Evaluate precondition using the new CheckEvaluator (never bypassed by --force)
            if let Some(precondition) = step.execution.effective_precondition() {
                let mut evaluator =
                    CheckEvaluator::new(project_root, context, &mut self.snapshot_store);
                let precond_result = evaluator.evaluate(&precondition);
                if !precond_result.passed_check() {
                    return Err(BivvyError::StepExecutionError {
                        step: step_name.clone(),
                        message: format!("Precondition failed: {}", precond_result.description),
                    });
                }
            }

            // Evaluate completed check using the new CheckEvaluator
            if !options.should_force(step_name) && !step.behavior.force {
                if let Some(check) = step.execution.effective_check() {
                    let config_hash = check.config_hash();
                    let mut evaluator =
                        CheckEvaluator::new(project_root, context, &mut self.snapshot_store)
                            .with_step(step_name, &config_hash)
                            .with_workflow(workflow_name);
                    let check_result = evaluator.evaluate(&check);
                    if check_result.passed_check() {
                        let skip_result = StepResult::skipped(step_name, check_result);
                        on_progress(RunProgress::StepFinished {
                            name: step_name,
                            result: &skip_result,
                        });
                        results.push(skip_result);
                        continue;
                    }
                }
            }

            on_progress(RunProgress::StepStarting {
                name: step_name,
                index,
                total,
            });

            let exec_options = ExecutionOptions {
                force: options.should_force(step_name) || step.behavior.force,
                dry_run: options.dry_run,
                capture_output: true,
                ..Default::default()
            };

            let step_start = Instant::now();
            let result = match execute_step(
                step,
                project_root,
                context,
                base_env,
                process_env,
                &exec_options,
                None,
            ) {
                Ok(result) => result,
                Err(e) => {
                    warn!("Step '{}' errored: {}", step_name, e);
                    StepResult::failure(step_name, step_start.elapsed(), e.to_string(), None)
                }
            };

            on_progress(RunProgress::StepFinished {
                name: step_name,
                result: &result,
            });

            // Record step state if state tracking is available
            if let Some(ref mut state_store) = state {
                record_step_state(step, step_name, &result, state_store, project_root);
            }

            let status = result.status();

            results.push(result);

            if status == StepStatus::Failed {
                all_success = false;
                if !step.behavior.allow_failure {
                    failed_steps.insert(step_name.clone());
                }
            }
        }

        let mut all_skipped: Vec<String> = plan.flag_skipped.into_iter().collect();
        all_skipped.extend(plan.env_skipped);

        Ok(WorkflowResult {
            workflow: workflow_name.to_string(),
            steps: results,
            skipped: all_skipped,
            duration: start.elapsed(),
            success: all_success,
            aborted: false,
        })
    }

    /// Build the dependency graph for the given workflow.
    pub fn build_graph(&self, workflow: &str) -> Result<DependencyGraph> {
        let workflow_config = self.config.workflows.get(workflow).ok_or_else(|| {
            BivvyError::ConfigValidationError {
                message: format!("Unknown workflow: {}", workflow),
            }
        })?;

        let mut builder = DependencyGraph::builder();

        for step_name in &workflow_config.steps {
            let step =
                self.steps
                    .get(step_name)
                    .ok_or_else(|| BivvyError::ConfigValidationError {
                        message: format!(
                            "Workflow '{}' references unknown step '{}'",
                            workflow, step_name
                        ),
                    })?;

            builder = builder.add_step(step_name.clone(), step.depends_on.clone());
        }

        builder.build()
    }
}

/// Record a step's result in the state store.
pub(super) fn record_step_state(
    _step: &ResolvedStep,
    step_name: &str,
    result: &StepResult,
    state: &mut StateStore,
    _project_root: &Path,
) {
    let state_status = match result.status() {
        StepStatus::Completed => crate::state::StepStatus::Success,
        StepStatus::Failed => crate::state::StepStatus::Failed,
        StepStatus::Skipped => crate::state::StepStatus::Skipped,
        _ => crate::state::StepStatus::NeverRun,
    };
    state.record_step_result(step_name, state_status, result.duration);
}

#[cfg(test)]
#[path = "workflow_tests.rs"]
mod tests;
