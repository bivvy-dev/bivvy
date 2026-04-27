//! Interactive workflow orchestration.
//!
//! This module contains the interactive execution loop (`run_with_ui`) and its
//! supporting functions: step execution with recovery, state recording, and
//! config-to-UI prompt conversion.
//!
//! The orchestrator coordinates the check evaluator, step executor, state recorder,
//! and presenter — but does not contain their logic. It is the only place where
//! multiple concerns converge.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Instant;

use tracing::warn;

use crate::checks::evaluator::CheckEvaluator;
use crate::checks::CheckResult;
use crate::config::interpolation::InterpolationContext;
use crate::config::schema::StepOverride;
use crate::error::{BivvyError, Result};
use crate::logging::{BivvyEvent, EventBus};
use crate::requirements::checker::GapChecker;
use crate::requirements::installer;
use crate::shell::OutputLine;
use crate::state::StateStore;
use crate::steps::{execute_step, ExecutionOptions, ResolvedStep, StepResult, StepStatus};
use crate::ui::spinner::live_output_callback;
use crate::ui::theme::BivvyTheme;
use crate::ui::{
    format_duration, OutputMode, Prompt, PromptOption, PromptType, StatusKind, UserInterface,
};

use super::decision;
use super::patterns::{self, StepContext};
use super::plan::build_execution_plan;
use super::recovery::{self, RecoveryAction};
use super::satisfaction;
use super::workflow::{record_step_state, RunOptions, WorkflowResult, WorkflowRunner};

/// Maximum total execution attempts per step (auto-retries + manual retries).
/// Prevents infinite loops when the recovery prompt always returns "retry"
/// (e.g., in test environments with MockUI).
const MAX_STEP_ATTEMPTS: u32 = 100;

/// Result of executing a step with the retry/recovery loop.
pub(super) struct StepExecutionResult {
    /// The final step result.
    pub result: StepResult,
    /// Whether the user chose to skip in the recovery menu.
    pub skipped_by_user: bool,
    /// Whether the user chose to abort in the recovery menu.
    pub aborted: bool,
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
        mut state: Option<&mut StateStore>,
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

            // Resolve effective prompt_if_complete (step-level, possibly overridden)
            let effective_prompt_if_complete =
                decision::effective_prompt_if_complete(step, step_name, step_overrides);

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
                        });
                        event_bus.emit(&BivvyEvent::StepSkipped {
                            name: step_name.clone(),
                            reason: format!("satisfied: {}", satisfied_desc),
                        });
                        ui.message(&step_display);
                        let skip_label = format!("Satisfied ({})", satisfied_desc);
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
                        check_name,
                        check_type: check_type_name,
                        outcome: check_result.outcome.as_str().to_string(),
                        description: check_result.description.clone(),
                        details: check_result.details.clone(),
                        duration_ms: None,
                    });
                    if check_result.passed_check() {
                        if interactive && effective_prompt_if_complete {
                            if step.behavior.skippable {
                                // Show step header, then ask if they want to re-run
                                ui.message(&step_header);
                                let prompt_label = format!(
                                    "Check passed ({}). Run anyway?",
                                    &check_result.description
                                );
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
                                        format!("Check passed ({})", &check_result.description);
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
                            // Not interactive or prompt_if_complete is false: check passed
                            event_bus.emit(&BivvyEvent::StepDecided {
                                name: step_name.clone(),
                                decision: "skip".to_string(),
                                reason: Some("check_passed".to_string()),
                            });
                            ui.message(&step_display);
                            let skip_label =
                                format!("Check passed ({})", &check_result.description);
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
            });
            event_bus.emit(&BivvyEvent::StepStarting {
                name: step_name.clone(),
            });

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

            // Record step state if state tracking is available
            if let Some(ref mut state_store) = state {
                record_step_state(
                    step,
                    step_name,
                    &exec_result.result,
                    state_store,
                    project_root,
                );
            }

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

/// Execute a step with retry and interactive recovery.
///
/// This handles the full execution lifecycle: spinner display, output capture,
/// auto-retries, and the interactive recovery menu (retry/fix/shell/skip/abort).
#[allow(clippy::too_many_arguments)]
fn execute_step_with_recovery(
    step: &ResolvedStep,
    step_name: &str,
    step_number: &str,
    step_indent: usize,
    project_root: &Path,
    context: &InterpolationContext,
    global_env: &HashMap<String, String>,
    needs_force: bool,
    dry_run: bool,
    interactive: bool,
    step_ctx: &StepContext<'_>,
    ui: &mut dyn UserInterface,
    event_bus: &mut EventBus,
) -> Result<StepExecutionResult> {
    let theme = BivvyTheme::new();

    let mut retry_count: u32 = 0;
    let mut fix_history: HashSet<String> = HashSet::new();
    let mut skipped_by_user = false;
    #[allow(unused_assignments)]
    let mut final_result: Option<StepResult> = None;

    // Outer loop: step execution (retry/fix re-enter here)
    'step_execution: loop {
        // Fresh spinner per attempt — hide command text for sensitive steps
        let display_command = if step.behavior.sensitive {
            "[SENSITIVE]".to_string()
        } else {
            step.execution.command.clone()
        };
        let attempt_label = if retry_count > 0 {
            format!(
                "Running `{}`... (attempt {}/{})",
                display_command,
                retry_count + 1,
                step.execution.retry + 1
            )
        } else {
            format!("Running `{}`...", display_command)
        };
        let mut spinner = ui.start_spinner_indented(&attempt_label, step_indent);

        // Create live output callback:
        // - Interactive mode: spinner-based ring buffer (3 lines verbose, 2 normal)
        // - Non-interactive verbose: print all output directly to stdout
        let output_mode = ui.output_mode();
        let output_callback = spinner
            .progress_bar()
            .and_then(|bar| {
                let max_lines = match output_mode {
                    OutputMode::Verbose => 3,
                    OutputMode::Normal => 2,
                    _ => return None,
                };
                Some(live_output_callback(
                    bar,
                    attempt_label.clone(),
                    6,
                    max_lines,
                ))
            })
            .or_else(|| {
                // Non-interactive verbose: stream output directly
                if output_mode == OutputMode::Verbose {
                    let cb: crate::shell::OutputCallback = Box::new(|line: OutputLine| {
                        let text = match &line {
                            OutputLine::Stdout(s) => s.trim_end(),
                            OutputLine::Stderr(s) => s.trim_end(),
                        };
                        if !text.is_empty() {
                            println!("      {text}");
                        }
                    });
                    Some(cb)
                } else {
                    None
                }
            });

        let exec_options = ExecutionOptions {
            force: needs_force,
            dry_run,
            capture_output: output_callback.is_none(),
            ..Default::default()
        };

        let step_start = Instant::now();
        let result = match execute_step(
            step,
            project_root,
            context,
            global_env,
            &exec_options,
            output_callback,
        ) {
            Ok(result) => result,
            Err(e) => {
                warn!("Step '{}' errored: {}", step_name, e);
                StepResult::failure(step_name, step_start.elapsed(), e.to_string(), None)
            }
        };

        let duration_str = format_duration(result.duration);

        match result.status() {
            StepStatus::Completed => {
                let detail = if retry_count > 0 {
                    Some(format!("succeeded on retry (attempt {})", retry_count + 1))
                } else {
                    None
                };
                // Clear spinner, then collapse step header → single result line
                spinner.finish_and_clear();
                ui.clear_lines(1);
                ui.message(&format!(
                    "{} {}",
                    theme.step_number.apply_to(step_number),
                    theme.format_success(&format!("{} ({})", step_name, duration_str))
                ));
                let mut r = result;
                r.recovery_detail = detail;
                final_result = Some(r);
                break 'step_execution;
            }
            StepStatus::Skipped => {
                spinner.finish_skipped("Skipped");
                final_result = Some(result);
                break 'step_execution;
            }
            StepStatus::Failed => {
                spinner.finish_error(&format!("Failed ({})", duration_str));

                // Build combined error output for pattern matching and display
                let mut output_parts = Vec::new();
                if let Some(ref err) = result.error {
                    output_parts.push(err.as_str());
                }
                if let Some(ref output) = result.output {
                    let trimmed = output.trim();
                    if !trimmed.is_empty() {
                        output_parts.push(trimmed);
                    }
                }
                let combined_output = output_parts.join("\n");

                // Match against pattern registry
                let fix = patterns::find_fix(&combined_output, step_ctx);
                let hint = patterns::find_hint(&combined_output, step_ctx);

                // Show error block — skip in non-interactive verbose
                // where output was already streamed to stdout
                let output_was_streamed =
                    !ui.is_interactive() && output_mode == OutputMode::Verbose;
                if !output_was_streamed {
                    ui.show_error_block(&step.execution.command, &combined_output, hint.as_deref());
                }

                // allow_failure: record and move on, no recovery menu
                if step.behavior.allow_failure {
                    final_result = Some(result);
                    break 'step_execution;
                }

                // Auto-retry before showing recovery menu
                if retry_count < step.execution.retry {
                    retry_count += 1;
                    ui.message(&format!(
                        "    Retrying... (attempt {}/{})",
                        retry_count + 1,
                        step.execution.retry + 1
                    ));
                    continue 'step_execution;
                }

                // Non-interactive: no recovery menu
                if !interactive {
                    final_result = Some(result);
                    break 'step_execution;
                }

                // Safety: cap total attempts to prevent infinite loops
                // (e.g., in tests where MockUI defaults to "retry")
                if retry_count >= MAX_STEP_ATTEMPTS {
                    warn!(
                        "Step '{}' exceeded max recovery attempts ({})",
                        step_name, MAX_STEP_ATTEMPTS
                    );
                    final_result = Some(result);
                    break 'step_execution;
                }

                // Interactive recovery menu
                event_bus.emit(&BivvyEvent::RecoveryStarted {
                    step: step_name.to_string(),
                    error: combined_output.clone(),
                });
                let has_hint = hint.is_some();
                'recovery_menu: loop {
                    let action = recovery::prompt_recovery(
                        ui,
                        step_name,
                        fix.as_ref(),
                        has_hint,
                        &fix_history,
                    )?;

                    match action {
                        RecoveryAction::Retry => {
                            event_bus.emit(&BivvyEvent::RecoveryActionTaken {
                                step: step_name.to_string(),
                                action: "retry".to_string(),
                                command: None,
                            });
                            retry_count += 1;
                            continue 'step_execution;
                        }
                        RecoveryAction::Fix(ref cmd) | RecoveryAction::CustomFix(ref cmd) => {
                            let is_custom = matches!(action, RecoveryAction::CustomFix(_));
                            let cmd = cmd.clone();
                            if recovery::confirm_fix(ui, step_name, &cmd)? {
                                event_bus.emit(&BivvyEvent::RecoveryActionTaken {
                                    step: step_name.to_string(),
                                    action: if is_custom {
                                        "custom_fix".to_string()
                                    } else {
                                        "fix".to_string()
                                    },
                                    command: Some(cmd.clone()),
                                });
                                let fix_ok =
                                    recovery::run_fix(&cmd, project_root, &step.env_vars.env)?;
                                fix_history.insert(cmd.clone());
                                if fix_ok {
                                    ui.message("    Fix command succeeded.");
                                } else {
                                    ui.message("    Fix command failed.");
                                }
                                retry_count += 1;
                                continue 'step_execution;
                            } else {
                                // User declined the fix — re-show recovery menu
                                continue 'recovery_menu;
                            }
                        }
                        RecoveryAction::Shell => {
                            event_bus.emit(&BivvyEvent::RecoveryActionTaken {
                                step: step_name.to_string(),
                                action: "shell".to_string(),
                                command: None,
                            });
                            ui.message("    Dropping to debug shell (exit to return)...");
                            crate::shell::debug::spawn_debug_shell(
                                step_name,
                                project_root,
                                &step.env_vars.env,
                                global_env,
                            )?;
                            // After shell exit, re-show recovery menu
                            continue 'recovery_menu;
                        }
                        RecoveryAction::Skip => {
                            event_bus.emit(&BivvyEvent::RecoveryActionTaken {
                                step: step_name.to_string(),
                                action: "skip".to_string(),
                                command: None,
                            });
                            skipped_by_user = true;
                            let mut r = result;
                            r.recovery_detail = Some("skipped by user after failure".to_string());
                            final_result = Some(r);
                            break 'step_execution;
                        }
                        RecoveryAction::Abort => {
                            event_bus.emit(&BivvyEvent::RecoveryActionTaken {
                                step: step_name.to_string(),
                                action: "abort".to_string(),
                                command: None,
                            });
                            let mut r = result;
                            r.recovery_detail = Some("aborted by user".to_string());
                            return Ok(StepExecutionResult {
                                result: r,
                                skipped_by_user: false,
                                aborted: true,
                            });
                        }
                    }
                }
            }
            _ => {
                final_result = Some(result);
                break 'step_execution;
            }
        }
    }

    Ok(StepExecutionResult {
        result: final_result.expect("step execution loop must produce a result"),
        skipped_by_user,
        aborted: false,
    })
}

/// Convert a config-level PromptConfig into a UI Prompt.
pub(super) fn config_prompt_to_ui_prompt(config: &crate::config::schema::PromptConfig) -> Prompt {
    use crate::config::schema::PromptType as ConfigPromptType;

    let default = config.default.as_ref().and_then(|v| match v {
        serde_yaml::Value::String(s) => Some(s.clone()),
        serde_yaml::Value::Bool(b) => Some(b.to_string()),
        serde_yaml::Value::Number(n) => Some(n.to_string()),
        _ => None,
    });

    let prompt_type = match &config.prompt_type {
        ConfigPromptType::Select => PromptType::Select {
            options: config
                .options
                .iter()
                .map(|o| PromptOption {
                    label: o.label.clone(),
                    value: o.value.clone(),
                })
                .collect(),
        },
        ConfigPromptType::Multiselect => PromptType::MultiSelect {
            options: config
                .options
                .iter()
                .map(|o| PromptOption {
                    label: o.label.clone(),
                    value: o.value.clone(),
                })
                .collect(),
        },
        ConfigPromptType::Confirm => PromptType::Confirm,
        ConfigPromptType::Input => PromptType::Input,
    };

    Prompt {
        key: config.key.clone(),
        question: config.question.clone(),
        prompt_type,
        default,
    }
}
