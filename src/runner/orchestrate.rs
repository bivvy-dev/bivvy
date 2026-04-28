//! Interactive workflow orchestration.
//!
//! This module contains the interactive execution loop (`run_with_ui`) — the
//! coordination layer between check evaluation, step execution, state recording,
//! and the presenter.
//!
//! Step execution with recovery is in [`super::execution`]. Prompt conversion
//! is in [`super::execution::config_prompt_to_ui_prompt`].

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Instant;

use crate::checks::evaluator::CheckEvaluator;
use crate::checks::CheckResult;
use crate::config::interpolation::InterpolationContext;
use crate::config::schema::StepOverride;
use crate::error::{BivvyError, Result};
use crate::logging::{BehaviorFlags, BivvyEvent, DecisionTrace, EventBus, NamedCheckResult};
use crate::requirements::checker::GapChecker;
use crate::requirements::installer;
use crate::state::StateStore;
use crate::steps::{ResolvedStep, StepResult, StepStatus};
use crate::ui::theme::BivvyTheme;
use crate::ui::{Prompt, PromptOption, PromptType, StatusKind, UserInterface};

use super::decision;
use super::diagnostic;
use super::execution::{config_prompt_to_ui_prompt, execute_step_with_recovery};
use super::patterns::StepContext;
use super::plan::build_execution_plan;
use super::satisfaction;
use super::workflow::{RunOptions, WorkflowResult, WorkflowRunner};

/// Format a `chrono::Duration` as a human-readable "time since" string.
fn format_time_since(elapsed: chrono::Duration) -> String {
    let secs = elapsed.num_seconds();
    if secs < 60 {
        "seconds ago".to_string()
    } else if secs < 3600 {
        let mins = secs / 60;
        if mins == 1 {
            "1 minute ago".to_string()
        } else {
            format!("{} minutes ago", mins)
        }
    } else if secs < 86400 {
        let hours = secs / 3600;
        if hours == 1 {
            "1 hour ago".to_string()
        } else {
            format!("{} hours ago", hours)
        }
    } else {
        let days = secs / 86400;
        if days == 1 {
            "1 day ago".to_string()
        } else {
            format!("{} days ago", days)
        }
    }
}

/// Drain pending baseline events from the snapshot store and emit them on the event bus.
fn emit_baseline_events(
    snapshot_store: &mut crate::snapshots::SnapshotStore,
    event_bus: &mut EventBus,
) {
    for evt in snapshot_store.drain_events() {
        match evt.change {
            crate::snapshots::BaselineChange::Established => {
                event_bus.emit(&BivvyEvent::BaselineEstablished {
                    step: evt.step,
                    target: evt.target,
                    hash: evt.hash,
                    scope: evt.scope,
                });
            }
            crate::snapshots::BaselineChange::Updated { old_hash } => {
                event_bus.emit(&BivvyEvent::BaselineUpdated {
                    step: evt.step,
                    target: evt.target,
                    old_hash,
                    new_hash: evt.hash,
                });
            }
            crate::snapshots::BaselineChange::Unchanged => {}
        }
    }
}

/// Build behavior flags from a resolved step and force status.
fn make_behavior_flags(step: &ResolvedStep, forced: bool) -> BehaviorFlags {
    BehaviorFlags {
        skippable: step.behavior.skippable,
        required: step.behavior.required,
        forced,
        prompt_on_rerun: step.behavior.prompt_on_rerun,
    }
}

impl<'a> WorkflowRunner<'a> {
    /// Run a workflow with full interactive UI support.
    ///
    /// This is the primary execution entry point for interactive use. It evaluates
    /// checks, prompts the user, manages recovery on failure, records state,
    /// and handles interactive prompts for completed steps and sensitive steps.
    /// An optional `GapChecker` enables requirement gap detection before step execution.
    /// An optional `StateStore` enables state-aware marker checks and step recording.
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
        ui: &mut dyn UserInterface,
        event_bus: &mut EventBus,
    ) -> Result<WorkflowResult> {
        let start = Instant::now();
        let workflow_name = options.workflow.as_deref().unwrap_or("default");
        let mut context = context.clone();

        // Build dependency graph and execution plan
        let graph = self.build_graph(workflow_name)?;
        let workflow_steps = &self.config.workflows[workflow_name].steps;
        let plan = build_execution_plan(&graph, workflow_steps, options, &self.steps)?;

        let total = plan.steps_to_run.len();
        let theme = BivvyTheme::new();

        // Emit workflow started
        event_bus.emit(&BivvyEvent::WorkflowStarted {
            name: workflow_name.to_string(),
            step_count: total,
        });

        // Report skipped steps (from --skip flag)
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
        let installer_ctx = installer::default_context();

        let mut results = Vec::new();
        let mut all_success = true;
        let mut failed_steps: HashSet<String> = HashSet::new();
        let mut user_skipped_steps: HashSet<String> = HashSet::new();
        let mut satisfied_steps: HashSet<String> = HashSet::new();
        let mut named_check_results: HashMap<String, CheckResult> = HashMap::new();
        let mut workflow_aborted = false;

        for (index, step_name) in plan.steps_to_run.iter().enumerate() {
            let step =
                &self
                    .steps
                    .get(step_name)
                    .ok_or_else(|| BivvyError::ConfigValidationError {
                        message: format!("Step '{}' not found in resolved steps", step_name),
                    })?;

            event_bus.emit(&BivvyEvent::StepPlanned {
                name: step_name.clone(),
                index,
                total,
            });

            // Blank line between steps
            if index > 0 {
                ui.message("");
            }

            // Format step display with numbering: "[1/7] name" as header line
            let step_number = format!("[{}/{}]", index + 1, total);
            let step_indent = step_number.len() + 1; // +1 for the space after
            let step_pad = " ".repeat(step_indent);
            let step_header = format!(
                "{} {}",
                theme.step_number.apply_to(&step_number),
                theme.step_title.apply_to(step_name)
            );
            // Full display includes title if different from name (used for non-prompt contexts)
            let step_display = if *step_name == step.title {
                step_header.clone()
            } else {
                format!(
                    "{} {} {}",
                    step_header,
                    theme.dim.apply_to("—"),
                    theme.dim.apply_to(&step.title)
                )
            };

            // Check if any dependency failed
            if decision::blocked_by_failure(step, &failed_steps) {
                let blocked_by = step
                    .depends_on
                    .iter()
                    .find(|d| failed_steps.contains(*d))
                    .cloned()
                    .unwrap_or_default();
                event_bus.emit(&BivvyEvent::DependencyBlocked {
                    name: step_name.clone(),
                    blocked_by: blocked_by.clone(),
                    reason: "dependency_failed".to_string(),
                });
                event_bus.emit(&BivvyEvent::StepDecided {
                    name: step_name.clone(),
                    decision: "block".to_string(),
                    reason: Some("dependency_failed".to_string()),
                    trace: None,
                });
                ui.message(&step_display);
                ui.message(&format!(
                    "{}{}",
                    step_pad,
                    StatusKind::Blocked
                        .format(&theme, decision::BlockReason::DependencyFailed.message())
                ));
                ui.show_workflow_progress(index + 1, total, start.elapsed());
                all_success = false;
                failed_steps.insert(step_name.clone());
                continue;
            }

            // Auto-skip if any dependency was user-skipped and not satisfied
            if decision::blocked_by_user_skip(step, &user_skipped_steps, &satisfied_steps) {
                event_bus.emit(&BivvyEvent::StepDecided {
                    name: step_name.clone(),
                    decision: "skip".to_string(),
                    reason: Some("dependency_skipped".to_string()),
                    trace: None,
                });
                event_bus.emit(&BivvyEvent::StepSkipped {
                    name: step_name.clone(),
                    reason: "dependency_skipped".to_string(),
                });
                ui.message(&step_display);
                ui.message(&format!(
                    "{}{}",
                    step_pad,
                    StatusKind::Skipped.format(&theme, "Skipped (dependency skipped)")
                ));
                ui.show_workflow_progress(index + 1, total, start.elapsed());
                user_skipped_steps.insert(step_name.clone());
                results.push(StepResult::skipped(
                    &step.name,
                    CheckResult::passed("Dependency skipped"),
                ));
                continue;
            }

            // Check requirement gaps before proceeding
            if let Some(ref mut checker) = gap_checker {
                let provided = if options.provided_requirements.is_empty() {
                    None
                } else {
                    Some(&options.provided_requirements)
                };
                let gaps = checker.check_step(step, provided);
                if !gaps.is_empty() {
                    let can_proceed =
                        installer::handle_gaps(&gaps, checker, ui, interactive, &installer_ctx)?;
                    if !can_proceed {
                        for gap in &gaps {
                            event_bus.emit(&BivvyEvent::RequirementGap {
                                name: step_name.clone(),
                                requirement: gap.requirement.clone(),
                                status: format!("{:?}", gap.status),
                            });
                        }
                        event_bus.emit(&BivvyEvent::StepDecided {
                            name: step_name.clone(),
                            decision: "skip".to_string(),
                            reason: Some("requirement_not_met".to_string()),
                            trace: None,
                        });
                        event_bus.emit(&BivvyEvent::StepSkipped {
                            name: step_name.clone(),
                            reason: "requirement_not_met".to_string(),
                        });
                        ui.message(&step_display);
                        ui.message(&format!(
                            "{}{}",
                            step_pad,
                            StatusKind::Skipped.format(&theme, "Skipped (requirement not met)")
                        ));
                        ui.show_workflow_progress(index + 1, total, start.elapsed());
                        continue;
                    }
                }
            }

            // Resolve effective prompt_on_rerun (step-level, possibly overridden)
            let effective_prompt_on_rerun =
                decision::effective_prompt_on_rerun(step, step_name, step_overrides);

            let mut needs_force = options.force.contains(step_name);
            let mut already_prompted = false;
            let mut had_prompt = false;

            // Evaluate precondition using the new CheckEvaluator (never bypassed by --force)
            if !options.dry_run {
                if let Some(precondition) = step.execution.effective_precondition() {
                    let mut evaluator =
                        CheckEvaluator::new(project_root, &context, &mut self.snapshot_store);
                    let precond_result = evaluator.evaluate(&precondition);
                    event_bus.emit(&BivvyEvent::PreconditionEvaluated {
                        step: step_name.clone(),
                        check_type: precondition.type_name().to_string(),
                        outcome: precond_result.outcome.as_str().to_string(),
                        description: precond_result.description.clone(),
                    });
                    if !precond_result.passed_check() {
                        event_bus.emit(&BivvyEvent::StepDecided {
                            name: step_name.clone(),
                            decision: "block".to_string(),
                            reason: Some("precondition_failed".to_string()),
                            trace: Some(DecisionTrace {
                                precondition_result: Some(crate::logging::TraceCheckResult {
                                    check_type: precondition.type_name().to_string(),
                                    outcome: precond_result.outcome.as_str().to_string(),
                                    description: precond_result.description.clone(),
                                }),
                                behavior_flags: make_behavior_flags(step, needs_force),
                                ..Default::default()
                            }),
                        });
                        ui.message(&step_display);
                        ui.message(&format!(
                            "{}{}",
                            step_pad,
                            StatusKind::Blocked.format(
                                &theme,
                                &format!("Precondition failed: {}", precond_result.description)
                            )
                        ));
                        ui.show_workflow_progress(index + 1, total, start.elapsed());
                        all_success = false;
                        failed_steps.insert(step_name.clone());
                        continue;
                    }
                }
            }

            // Collect named check results from this step's checks for cross-step
            // ref resolution. Must happen for ALL steps with named checks, not just
            // those with satisfied_when, because downstream steps may reference them
            // via `ref: step_name.check_name` in their own satisfied_when conditions.
            if !options.dry_run {
                if let Some(check) = step.execution.effective_check() {
                    if check.has_named_checks() {
                        let mut evaluator =
                            CheckEvaluator::new(project_root, &context, &mut self.snapshot_store);
                        let step_named = satisfaction::collect_named_check_results(
                            step_name,
                            &check,
                            &mut evaluator,
                        );
                        named_check_results.extend(step_named);
                    }
                }
            }

            // Evaluate satisfied_when (unless forced). If satisfied, auto-skip.
            // --force bypasses satisfaction-based auto-skip.
            if !needs_force && !options.dry_run && !step.satisfied_when.is_empty() {
                let mut evaluator =
                    CheckEvaluator::new(project_root, &context, &mut self.snapshot_store);

                let sat_result = satisfaction::evaluate_satisfaction(
                    step,
                    &mut evaluator,
                    &named_check_results,
                    step_name,
                );

                if let Some(ref result) = sat_result {
                    event_bus.emit(&BivvyEvent::SatisfactionEvaluated {
                        step: step_name.clone(),
                        satisfied: result.satisfied,
                        condition_count: result.condition_count,
                        passed_count: result.passed_count,
                    });
                    if result.satisfied {
                        satisfied_steps.insert(step_name.clone());

                        // Build a description of why it's satisfied
                        let descriptions: Vec<&str> = result
                            .condition_results
                            .iter()
                            .map(|r| r.description.as_str())
                            .collect();
                        let satisfied_desc = descriptions.join(", ");

                        event_bus.emit(&BivvyEvent::StepDecided {
                            name: step_name.clone(),
                            decision: "skip".to_string(),
                            reason: Some("satisfied".to_string()),
                            trace: Some(DecisionTrace {
                                satisfaction: Some(crate::logging::SatisfactionResult {
                                    satisfied: result.satisfied,
                                    condition_count: result.condition_count,
                                    passed_count: result.passed_count,
                                }),
                                behavior_flags: make_behavior_flags(step, needs_force),
                                ..Default::default()
                            }),
                        });
                        event_bus.emit(&BivvyEvent::StepSkipped {
                            name: step_name.clone(),
                            reason: format!("satisfied: {}", satisfied_desc),
                        });
                        ui.message(&step_display);
                        let skip_label = crate::ui::satisfaction_label(&satisfied_desc);
                        ui.message(&format!(
                            "{}{}",
                            step_pad,
                            StatusKind::Success.format(&theme, &skip_label)
                        ));
                        ui.show_workflow_progress(index + 1, total, start.elapsed());
                        results.push(StepResult::check_passed(
                            &step.name,
                            CheckResult::passed(format!("satisfied: {}", satisfied_desc)),
                        ));
                        continue;
                    }
                }
            }

            // Detect rerun from execution history (independent of check evaluation)
            let rerun_info = if !needs_force && effective_prompt_on_rerun {
                state
                    .as_ref()
                    .and_then(|s| s.step_last_run(step_name))
                    .map(|last_run| {
                        let elapsed = chrono::Utc::now().signed_duration_since(last_run);
                        let time_since = format_time_since(elapsed);
                        event_bus.emit(&BivvyEvent::RerunDetected {
                            name: step_name.clone(),
                            last_run: last_run.to_rfc3339(),
                            time_since: time_since.clone(),
                        });
                        (last_run.to_rfc3339(), time_since)
                    })
            } else {
                None
            };

            // Check if already complete (unless forced) using the new CheckEvaluator
            if !needs_force && !options.dry_run {
                if let Some(check) = step.execution.effective_check() {
                    let config_hash = check.config_hash();
                    let check_type_name = check.type_name().to_string();
                    let check_name = check.name().map(|s| s.to_string());
                    let mut evaluator =
                        CheckEvaluator::new(project_root, &context, &mut self.snapshot_store)
                            .with_step(step_name, &config_hash)
                            .with_workflow(workflow_name);
                    let check_result = evaluator.evaluate(&check);
                    event_bus.emit(&BivvyEvent::CheckEvaluated {
                        step: step_name.clone(),
                        check_name: check_name.clone(),
                        check_type: check_type_name.clone(),
                        outcome: check_result.outcome.as_str().to_string(),
                        description: check_result.description.clone(),
                        details: check_result.details.clone(),
                        duration_ms: None,
                    });
                    if check_result.passed_check() {
                        if interactive && effective_prompt_on_rerun {
                            if step.behavior.skippable {
                                // Show step header, then ask if they want to re-run
                                ui.message(&step_header);
                                let prompt_label = if let Some((_, ref time_since)) = rerun_info {
                                    format!("This step ran {}. Run again?", time_since)
                                } else {
                                    crate::ui::rerun_prompt_label(&check_result.description)
                                };
                                let prompt = Prompt {
                                    key: format!("rerun_{}", step_name),
                                    question: format!("{}{}", step_pad, prompt_label),
                                    prompt_type: PromptType::Select {
                                        options: vec![
                                            PromptOption {
                                                label: "No  (n)".to_string(),
                                                value: "no".to_string(),
                                            },
                                            PromptOption {
                                                label: "Yes (y)".to_string(),
                                                value: "yes".to_string(),
                                            },
                                        ],
                                    },
                                    default: Some("no".to_string()),
                                };

                                event_bus.emit(&BivvyEvent::UserPrompted {
                                    step: Some(step_name.clone()),
                                    prompt: prompt_label.clone(),
                                    options: vec!["No  (n)".to_string(), "Yes (y)".to_string()],
                                });
                                let answer = ui.prompt(&prompt)?;
                                let answer_str = answer.as_string();
                                event_bus.emit(&BivvyEvent::UserResponded {
                                    step: Some(step_name.clone()),
                                    input: answer_str.clone(),
                                    method: crate::logging::InputMethod::ArrowSelect,
                                });
                                if answer_str != "yes" {
                                    // Clear prompt output (question + answer = 2 lines)
                                    ui.clear_lines(2);
                                    let skip_label =
                                        crate::ui::check_passed_label(&check_result.description);
                                    ui.message(&format!(
                                        "{}{}",
                                        step_pad,
                                        StatusKind::Success.format(&theme, &skip_label)
                                    ));
                                    // Record as check-passed (not skipped) — dependents proceed.
                                    event_bus.emit(&BivvyEvent::StepDecided {
                                        name: step_name.clone(),
                                        decision: "skip".to_string(),
                                        reason: Some("check_passed".to_string()),
                                        trace: Some(DecisionTrace {
                                            check_results: vec![NamedCheckResult {
                                                name: check_name.clone(),
                                                check_type: check_type_name.clone(),
                                                outcome: check_result.outcome.as_str().to_string(),
                                                description: check_result.description.clone(),
                                            }],
                                            behavior_flags: make_behavior_flags(step, needs_force),
                                            ..Default::default()
                                        }),
                                    });
                                    satisfied_steps.insert(step_name.clone());
                                    results
                                        .push(StepResult::check_passed(&step.name, check_result));
                                    continue;
                                }
                                // Clear prompt output so spinner starts below step header
                                ui.clear_lines(2);
                                needs_force = true;
                                already_prompted = true;
                                had_prompt = true;
                            } else {
                                // Not skippable, inform and re-run
                                ui.message(&step_display);
                                ui.message(&format!("{}Re-running (not skippable)", step_pad));
                                needs_force = true;
                            }
                        } else {
                            // Not interactive or prompt_on_rerun is false: check passed
                            event_bus.emit(&BivvyEvent::StepDecided {
                                name: step_name.clone(),
                                decision: "skip".to_string(),
                                reason: Some("check_passed".to_string()),
                                trace: Some(DecisionTrace {
                                    check_results: vec![NamedCheckResult {
                                        name: check_name.clone(),
                                        check_type: check_type_name.clone(),
                                        outcome: check_result.outcome.as_str().to_string(),
                                        description: check_result.description.clone(),
                                    }],
                                    behavior_flags: make_behavior_flags(step, needs_force),
                                    ..Default::default()
                                }),
                            });
                            ui.message(&step_display);
                            let skip_label =
                                crate::ui::check_passed_label(&check_result.description);
                            ui.message(&format!(
                                "{}{}",
                                step_pad,
                                StatusKind::Success.format(&theme, &skip_label)
                            ));
                            satisfied_steps.insert(step_name.clone());
                            results.push(StepResult::check_passed(&step.name, check_result));
                            continue;
                        }
                    }
                }
            }

            // Emit any baseline events from check evaluation
            emit_baseline_events(&mut self.snapshot_store, event_bus);

            // In interactive mode, prompt before running skippable steps
            // (skip if already prompted by completed check)
            if interactive && step.behavior.skippable && !already_prompted {
                // Show step header, then prompt with indented title
                ui.message(&step_header);
                let prompt_text = format!("{}?", step.title);
                let prompt = Prompt {
                    key: format!("run_{}", step_name),
                    question: format!("{}{}", step_pad, prompt_text),
                    prompt_type: PromptType::Select {
                        options: vec![
                            PromptOption {
                                label: "No  (n)".to_string(),
                                value: "no".to_string(),
                            },
                            PromptOption {
                                label: "Yes (y)".to_string(),
                                value: "yes".to_string(),
                            },
                        ],
                    },
                    default: Some("no".to_string()),
                };
                event_bus.emit(&BivvyEvent::UserPrompted {
                    step: Some(step_name.clone()),
                    prompt: prompt_text,
                    options: vec!["No  (n)".to_string(), "Yes (y)".to_string()],
                });
                let answer = ui.prompt(&prompt)?;
                let answer_str = answer.as_string();
                event_bus.emit(&BivvyEvent::UserResponded {
                    step: Some(step_name.clone()),
                    input: answer_str.clone(),
                    method: crate::logging::InputMethod::ArrowSelect,
                });
                if answer_str != "yes" {
                    // Clear prompt output (question + answer = 2 lines)
                    ui.clear_lines(2);
                    event_bus.emit(&BivvyEvent::StepDecided {
                        name: step_name.clone(),
                        decision: "skip".to_string(),
                        reason: Some("user_declined".to_string()),
                        trace: None,
                    });
                    event_bus.emit(&BivvyEvent::StepSkipped {
                        name: step_name.clone(),
                        reason: "user_declined".to_string(),
                    });
                    ui.message(&format!("{}{}", step_pad, theme.format_skipped("Skipped")));
                    user_skipped_steps.insert(step_name.clone());
                    results.push(StepResult::skipped(
                        &step.name,
                        CheckResult::passed("User declined"),
                    ));
                    continue;
                }
                // Clear prompt output (question + answer = 2 lines)
                // so spinner starts directly below step header
                ui.clear_lines(2);
                had_prompt = true;
            }

            // Show step name if no prompt was shown (non-interactive or non-skippable)
            if !had_prompt && !already_prompted {
                ui.message(&step_display);
            }

            // Sensitive confirmation (skip in dry-run — nothing will actually execute)
            if step.behavior.sensitive && interactive && !options.dry_run {
                let prompt_text = "Handles sensitive data. Continue?";
                let prompt = Prompt {
                    key: format!("sensitive_{}", step_name),
                    question: prompt_text.to_string(),
                    prompt_type: PromptType::Select {
                        options: vec![
                            PromptOption {
                                label: "Yes (y)".to_string(),
                                value: "yes".to_string(),
                            },
                            PromptOption {
                                label: "No (n)".to_string(),
                                value: "no".to_string(),
                            },
                        ],
                    },
                    default: Some("yes".to_string()),
                };

                event_bus.emit(&BivvyEvent::UserPrompted {
                    step: Some(step_name.clone()),
                    prompt: prompt_text.to_string(),
                    options: vec!["Yes (y)".to_string(), "No (n)".to_string()],
                });
                let answer = ui.prompt(&prompt)?;
                let answer_str = answer.as_string();
                event_bus.emit(&BivvyEvent::UserResponded {
                    step: Some(step_name.clone()),
                    input: answer_str.clone(),
                    method: crate::logging::InputMethod::ArrowSelect,
                });
                if answer_str != "yes" {
                    if step.behavior.skippable {
                        event_bus.emit(&BivvyEvent::StepSkipped {
                            name: step_name.clone(),
                            reason: "user_declined_sensitive".to_string(),
                        });
                        ui.message(&format!(
                            "{}{}",
                            step_pad,
                            theme.format_skipped("Skipped (declined sensitive step)")
                        ));
                        results.push(StepResult::skipped(
                            &step.name,
                            CheckResult::passed("User declined sensitive step"),
                        ));
                        continue;
                    } else {
                        return Err(BivvyError::StepExecutionError {
                            step: step_name.clone(),
                            message: format!(
                                "Step '{}' is sensitive and not skippable, but user declined",
                                step.title
                            ),
                        });
                    }
                }
            }

            // No extra blank line needed — spinner renders indented below step header

            // Execute step-level prompts (template inputs / interactive params)
            if !step.output.prompts.is_empty() {
                for prompt_config in &step.output.prompts {
                    // Skip if the value is already available in the context
                    if context.resolve(&prompt_config.key).is_some() {
                        continue;
                    }

                    if !interactive {
                        // Non-interactive: check for default, otherwise error
                        if let Some(default) = &prompt_config.default {
                            let default_str = match default {
                                serde_yaml::Value::String(s) => s.clone(),
                                serde_yaml::Value::Bool(b) => b.to_string(),
                                serde_yaml::Value::Number(n) => n.to_string(),
                                _ => format!("{:?}", default),
                            };
                            context
                                .prompts
                                .insert(prompt_config.key.clone(), default_str);
                        } else {
                            return Err(BivvyError::StepExecutionError {
                                step: step_name.to_string(),
                                message: format!(
                                    "Prompt '{}' requires a value in non-interactive mode. \
                                     Set via env var, template input, or provide a default.",
                                    prompt_config.key
                                ),
                            });
                        }
                        continue;
                    }

                    // Build UI prompt from config prompt
                    let ui_prompt = config_prompt_to_ui_prompt(prompt_config);
                    let result = ui.prompt(&ui_prompt)?;
                    context
                        .prompts
                        .insert(prompt_config.key.clone(), result.as_string());
                }
            }

            // Build step context for pattern matching
            let step_ctx = StepContext {
                name: step_name,
                command: &step.execution.command,
                requires: &step.requires,
                template: None,
            };

            // Emit decision to run
            event_bus.emit(&BivvyEvent::StepDecided {
                name: step_name.clone(),
                decision: "run".to_string(),
                reason: None,
                trace: Some(DecisionTrace {
                    behavior_flags: make_behavior_flags(step, needs_force),
                    ..Default::default()
                }),
            });
            event_bus.emit(&BivvyEvent::StepStarting {
                name: step_name.clone(),
            });

            // Build workflow state for diagnostic funnel
            let workflow_state = {
                let step_refs: Vec<(&str, &ResolvedStep)> = self
                    .steps
                    .iter()
                    .map(|(name, step)| (name.as_str(), step))
                    .collect();
                let mut outcomes = HashMap::new();
                for r in &results {
                    outcomes.insert(r.name.clone(), r.status());
                }
                for s in &satisfied_steps {
                    outcomes.entry(s.clone()).or_insert(StepStatus::Completed);
                }
                for s in &failed_steps {
                    outcomes.entry(s.clone()).or_insert(StepStatus::Failed);
                }
                for s in &user_skipped_steps {
                    outcomes.entry(s.clone()).or_insert(StepStatus::Skipped);
                }
                (step_refs, outcomes)
            };
            let ws = diagnostic::WorkflowState {
                steps: &workflow_state.0,
                outcomes: &workflow_state.1,
            };

            // Execute step with retry and recovery
            let exec_result = execute_step_with_recovery(
                step,
                step_name,
                &step_number,
                step_indent,
                project_root,
                &context,
                global_env,
                needs_force,
                options.dry_run,
                interactive,
                &step_ctx,
                options.diagnostic_funnel,
                &ws,
                ui,
                event_bus,
            )?;

            // Blank line before progress bar
            ui.message("");
            // Update progress bar
            ui.show_workflow_progress(index + 1, total, start.elapsed());

            // Emit step completion event
            match exec_result.result.status() {
                StepStatus::Completed => {
                    event_bus.emit(&BivvyEvent::StepCompleted {
                        name: step_name.clone(),
                        success: true,
                        exit_code: exec_result.result.exit_code,
                        duration_ms: exec_result.result.duration.as_millis() as u64,
                        error: None,
                    });
                }
                StepStatus::Failed => {
                    event_bus.emit(&BivvyEvent::StepCompleted {
                        name: step_name.clone(),
                        success: false,
                        exit_code: exec_result.result.exit_code,
                        duration_ms: exec_result.result.duration.as_millis() as u64,
                        error: exec_result.result.error.clone(),
                    });
                }
                StepStatus::Skipped => {
                    event_bus.emit(&BivvyEvent::StepSkipped {
                        name: step_name.clone(),
                        reason: exec_result
                            .result
                            .recovery_detail
                            .clone()
                            .unwrap_or_else(|| "skipped".to_string()),
                    });
                }
                _ => {}
            }

            // State recording is now handled by the StateRecorder EventConsumer
            // which consumes StepCompleted/StepSkipped events emitted above.

            let status = exec_result.result.status();

            // Successfully completed steps are satisfied (conservative default:
            // if satisfied_when is omitted, satisfaction = "step ran successfully")
            if status == StepStatus::Completed {
                satisfied_steps.insert(step_name.clone());
            }

            results.push(exec_result.result);

            if status == StepStatus::Failed {
                all_success = false;
                // Skip does NOT add to failed_steps (user made active choice)
                if !step.behavior.allow_failure && !exec_result.skipped_by_user {
                    failed_steps.insert(step_name.clone());
                }
            }

            // Abort: stop processing further steps
            if exec_result.aborted {
                workflow_aborted = true;
                all_success = false;
                break;
            }
        }

        // Finish progress bar
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_time_since_seconds() {
        let d = chrono::Duration::seconds(30);
        assert_eq!(format_time_since(d), "seconds ago");
    }

    #[test]
    fn format_time_since_one_minute() {
        let d = chrono::Duration::seconds(90);
        assert_eq!(format_time_since(d), "1 minute ago");
    }

    #[test]
    fn format_time_since_minutes() {
        let d = chrono::Duration::minutes(15);
        assert_eq!(format_time_since(d), "15 minutes ago");
    }

    #[test]
    fn format_time_since_one_hour() {
        let d = chrono::Duration::hours(1);
        assert_eq!(format_time_since(d), "1 hour ago");
    }

    #[test]
    fn format_time_since_hours() {
        let d = chrono::Duration::hours(5);
        assert_eq!(format_time_since(d), "5 hours ago");
    }

    #[test]
    fn format_time_since_one_day() {
        let d = chrono::Duration::days(1);
        assert_eq!(format_time_since(d), "1 day ago");
    }

    #[test]
    fn format_time_since_days() {
        let d = chrono::Duration::days(3);
        assert_eq!(format_time_since(d), "3 days ago");
    }
}
