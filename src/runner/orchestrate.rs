//! Interactive workflow orchestration.
//!
//! This module contains the interactive execution loop (`run_with_ui`) — the
//! workflow-level coordination layer. Step-level concerns (prompts, execution,
//! recovery, error display) are delegated to [`super::step_manager::StepManager`].
//!
//! Step execution with recovery is in [`super::execution`]. Prompt conversion
//! is in [`super::execution::config_prompt_to_ui_prompt`].

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Instant;

use crate::checks::CheckResult;
use crate::config::interpolation::InterpolationContext;
use crate::config::schema::StepOverride;
use crate::error::{BivvyError, Result};
use crate::logging::{BivvyEvent, EventBus};
use crate::requirements::checker::GapChecker;
use crate::state::satisfaction::{SatisfactionCache, SatisfactionRecord};
use crate::state::StateStore;
use crate::ui::theme::BivvyTheme;
use crate::ui::UserInterface;

use super::plan::build_execution_plan;
use super::step_manager::{SkipCategory, StepAction, StepExecutionOptions, StepManager};
use super::workflow::{RunOptions, WorkflowResult, WorkflowRunner};

impl<'a> WorkflowRunner<'a> {
    /// Run a workflow with full interactive UI support.
    ///
    /// This is the primary execution entry point for interactive use. It manages
    /// the workflow lifecycle: building the execution plan, iterating over steps,
    /// updating the progress bar, tracking workflow state, and recording results.
    ///
    /// Step-level concerns (check evaluation, prompts, execution, recovery) are
    /// delegated to [`StepManager`].
    #[allow(clippy::too_many_arguments)]
    pub fn run_with_ui(
        &mut self,
        options: &RunOptions,
        context: &InterpolationContext,
        global_env: &HashMap<String, String>,
        project_root: &Path,
        workflow_non_interactive: bool,
        step_overrides: &HashMap<String, StepOverride>,
        mut gap_checker: Option<&mut GapChecker<'_>>,
        state: Option<&mut StateStore>,
        satisfaction_cache: &mut SatisfactionCache,
        ui: &mut dyn UserInterface,
        event_bus: &mut EventBus,
    ) -> Result<WorkflowResult> {
        let start = Instant::now();
        let workflow_name = options.workflow.as_deref().unwrap_or("default");
        let mut context = context.clone();

        // Topological sort: compute execution order from the dependency graph.
        let graph = self.build_graph(workflow_name)?;
        let workflow_steps = &self.config.workflows[workflow_name].steps;
        // Pre-filter: remove steps excluded by --skip flags or only_environments mismatch.
        let plan = build_execution_plan(&graph, workflow_steps, options, &self.steps)?;

        let total = plan.steps_to_run.len();
        let theme = BivvyTheme::new();

        // Emit workflow started
        event_bus.emit(&BivvyEvent::WorkflowStarted {
            name: workflow_name.to_string(),
            step_count: total,
        });

        // Report pre-filtered steps before the loop begins.
        for skip_name in &plan.flag_skipped {
            event_bus.emit(&BivvyEvent::StepFilteredOut {
                name: skip_name.clone(),
                reason: "skip_flag".to_string(),
            });
            ui.message(&format!(
                "    {}",
                theme.format_skipped(&format!("{} skipped", skip_name))
            ));
        }
        for skip_name in &plan.env_skipped {
            event_bus.emit(&BivvyEvent::StepFilteredOut {
                name: skip_name.clone(),
                reason: "environment".to_string(),
            });
            ui.message(&format!(
                "    {}",
                theme.format_skipped(&format!(
                    "{} skipped (not in {} environment)",
                    skip_name,
                    options.active_environment.as_deref().unwrap_or("unknown")
                ))
            ));
        }

        let interactive = ui.is_interactive() && !workflow_non_interactive;

        let mut results = Vec::new();
        let mut all_success = true;
        let mut failed_steps: HashSet<String> = HashSet::new();
        let mut user_skipped_steps: HashSet<String> = HashSet::new();
        let mut satisfied_steps: HashSet<String> = HashSet::new();
        let mut named_check_results: HashMap<String, CheckResult> = HashMap::new();
        let mut workflow_aborted = false;

        // Initialize the persistent progress bar (pinned at terminal bottom).
        ui.init_workflow_progress(total);

        for (index, step_name) in plan.steps_to_run.iter().enumerate() {
            let step =
                &self
                    .steps
                    .get(step_name)
                    .ok_or_else(|| BivvyError::ConfigValidationError {
                        message: format!("Step '{}' not found in resolved steps", step_name),
                    })?;

            // Update progress bar immediately — before execution, so the user
            // sees "Step N/M" the moment iteration reaches this step.
            ui.show_workflow_progress(index + 1, total, start.elapsed());

            // Create StepManager and delegate step-level execution
            let step_mgr = StepManager::new(step, step_name, index, total, &theme);

            let exec_opts = StepExecutionOptions {
                dry_run: options.dry_run,
                interactive,
                diagnostic_funnel: options.diagnostic_funnel,
                project_root,
                global_env,
                force_steps: &options.force,
                provided_requirements: &options.provided_requirements,
            };

            let action = step_mgr.execute(
                &exec_opts,
                &mut context,
                step_overrides,
                &mut gap_checker,
                &mut self.snapshot_store,
                &self.steps,
                state.as_deref(),
                satisfaction_cache,
                &failed_steps,
                &user_skipped_steps,
                &satisfied_steps,
                &mut named_check_results,
                &results,
                ui,
                event_bus,
            )?;

            // Update workflow state based on step action.
            // The workflow only tracks aggregate state (satisfied/failed/skipped sets)
            // — all per-step decisions are made by StepManager via the decision engine.
            match action {
                StepAction::Completed(result) => {
                    satisfied_steps.insert(step_name.clone());

                    // Record successful execution in satisfaction cache
                    let record = SatisfactionRecord {
                        satisfied: true,
                        source: crate::state::satisfaction::SatisfactionSource::ExecutionHistory,
                        recorded_at: chrono::Utc::now(),
                        evidence: crate::state::satisfaction::SatisfactionEvidence::HistoricalRun {
                            ran_at: chrono::Utc::now(),
                            exit_code: result.exit_code.unwrap_or(0),
                        },
                        config_hash: None,
                        step_hash: None,
                    };
                    satisfaction_cache.store(step_name, record);

                    results.push(result);
                }

                StepAction::Skipped(result, category) => {
                    // StepManager classifies the skip reason so the workflow
                    // doesn't need to inspect StepResult internals.
                    match category {
                        SkipCategory::Satisfied => {
                            satisfied_steps.insert(step_name.clone());
                        }
                        SkipCategory::UserDeclined | SkipCategory::DependencySkipped => {
                            user_skipped_steps.insert(step_name.clone());
                        }
                        SkipCategory::RecoverySkipped | SkipCategory::Other => {
                            // Recovery-skipped steps do NOT block dependents —
                            // the user chose to move past the failure.
                        }
                    }
                    results.push(result);
                }

                StepAction::Failed(result) => {
                    all_success = false;
                    if !step.behavior.allow_failure {
                        failed_steps.insert(step_name.clone());
                    }
                    results.push(result);
                }

                StepAction::Blocked => {
                    all_success = false;
                    failed_steps.insert(step_name.clone());
                }

                StepAction::Aborted(result) => {
                    results.push(result);
                    workflow_aborted = true;
                    all_success = false;
                    break;
                }
            }

            // Update progress bar after step completes (reflects final position)
            ui.show_workflow_progress(index + 1, total, start.elapsed());
        }

        // Flush satisfaction cache to disk
        if !options.dry_run {
            if let Err(e) = satisfaction_cache.flush() {
                tracing::warn!("Failed to flush satisfaction cache: {}", e);
            }
        }

        // Finish progress bar (clear before summary)
        ui.finish_workflow_progress();

        let mut all_skipped: Vec<String> = plan.flag_skipped.into_iter().collect();
        all_skipped.extend(plan.env_skipped);

        let steps_run = results.len();
        let steps_skipped_count = all_skipped.len();
        let duration = start.elapsed();

        event_bus.emit(&BivvyEvent::WorkflowCompleted {
            name: workflow_name.to_string(),
            success: all_success,
            aborted: workflow_aborted,
            steps_run,
            steps_skipped: steps_skipped_count,
            duration_ms: duration.as_millis() as u64,
        });

        Ok(WorkflowResult {
            workflow: workflow_name.to_string(),
            steps: results,
            skipped: all_skipped,
            duration,
            success: all_success,
            aborted: workflow_aborted,
        })
    }
}
