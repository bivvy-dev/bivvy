//! Interactive workflow orchestration.
//!
//! This module contains the interactive execution loop (`run_with_ui`) — the
//! workflow-level coordination layer. Step-level concerns (prompts, execution,
//! recovery, error display) are delegated to [`super::step_manager::StepManager`].
//!
//! Step execution with recovery is in [`super::execution`]. Prompt conversion
//! is in [`super::execution::config_prompt_to_ui_prompt`].

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::checks::CheckResult;
use crate::config::schema::StepOverride;
use crate::error::{BivvyError, Result};
use crate::logging::{BivvyEvent, StepOutcomeKind};
use crate::state::satisfaction::SatisfactionRecord;
use crate::ui::theme::BivvyTheme;

use super::plan::build_execution_plan;
use super::step_manager::{
    SkipCategory, StepAction, StepExecState, StepExecutionOptions, StepManager, StepRunChannels,
    WorkflowSnapshot,
};
use super::workflow::{RunChannels, RunContext, RunInputs, WorkflowResult, WorkflowRunner};

impl<'a> WorkflowRunner<'a> {
    /// Run a workflow with full interactive UI support.
    ///
    /// This is the primary execution entry point for interactive use. It manages
    /// the workflow lifecycle: building the execution plan, iterating over steps,
    /// updating the progress bar, tracking workflow state, and recording results.
    ///
    /// Step-level concerns (check evaluation, prompts, execution, recovery) are
    /// delegated to [`StepManager`].
    pub fn run_with_ui(
        &mut self,
        ctx: &RunContext<'_>,
        inputs: RunInputs<'_>,
        channels: RunChannels<'_>,
        workflow_non_interactive: bool,
        step_overrides: &HashMap<String, StepOverride>,
    ) -> Result<WorkflowResult> {
        let RunContext {
            options,
            interpolation,
            project_root,
            base_env,
            process_env,
        } = *ctx;
        let RunInputs {
            mut gap_checker,
            state,
            satisfaction_cache,
        } = inputs;
        let RunChannels {
            ui,
            workflow_display,
            event_bus,
        } = channels;

        let start = Instant::now();
        let workflow_name = options.workflow.as_deref().unwrap_or("default");
        let mut context = interpolation.clone();

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
            event_bus.emit(&BivvyEvent::StepOutcome {
                name: skip_name.clone(),
                outcome: StepOutcomeKind::FilteredOut,
                detail: Some("skip_flag".to_string()),
                duration_ms: None,
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
            let env_label = options.active_environment.as_deref().unwrap_or("unknown");
            event_bus.emit(&BivvyEvent::StepOutcome {
                name: skip_name.clone(),
                outcome: StepOutcomeKind::FilteredOut,
                detail: Some(format!("not in {} environment", env_label)),
                duration_ms: None,
            });
            ui.message(&format!(
                "    {}",
                theme.format_skipped(&format!(
                    "{} skipped (not in {} environment)",
                    skip_name, env_label
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
        workflow_display.start_progress(total);

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
            workflow_display.update_progress(index + 1, total, start.elapsed());

            // Hand off to the step display for this iteration.
            let mut step_display = workflow_display.begin_step(index, total);

            // Create StepManager and delegate step-level execution
            let step_mgr = StepManager::new(step, step_name, index, total, &theme);

            let exec_opts = StepExecutionOptions {
                dry_run: options.dry_run,
                interactive,
                diagnostic_funnel: options.diagnostic_funnel,
                project_root,
                base_env,
                process_env,
                force_steps: &options.force,
                force_all: options.force_all,
                provided_requirements: &options.provided_requirements,
            };

            let exec_state = StepExecState {
                context: &mut context,
                step_overrides,
                gap_checker: &mut gap_checker,
                snapshot_store: &mut self.snapshot_store,
                state: state.as_deref(),
                satisfaction_cache,
                named_check_results: &mut named_check_results,
            };
            let snapshot = WorkflowSnapshot {
                steps: &self.steps,
                results: &results,
                failed_steps: &failed_steps,
                user_skipped_steps: &user_skipped_steps,
                satisfied_steps: &satisfied_steps,
            };
            let channels = StepRunChannels {
                ui,
                step_display: step_display.as_mut(),
                event_bus,
            };
            let action = step_mgr.execute(&exec_opts, exec_state, &snapshot, channels)?;

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
            workflow_display.update_progress(index + 1, total, start.elapsed());
        }

        // Flush satisfaction cache to disk
        if !options.dry_run {
            if let Err(e) = satisfaction_cache.flush() {
                tracing::warn!("Failed to flush satisfaction cache: {}", e);
            }
        }

        // Finish progress bar (clear before summary)
        workflow_display.finish_progress();

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
