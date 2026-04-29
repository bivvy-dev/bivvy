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
use crate::logging::{BehaviorFlags, BivvyEvent, DecisionTrace, EventBus};
use crate::requirements::checker::GapChecker;
use crate::requirements::installer;
use crate::state::satisfaction::{SatisfactionCache, SatisfactionRecord};
use crate::state::StateStore;
use crate::steps::{ResolvedStep, StepResult, StepStatus};
use crate::ui::theme::BivvyTheme;
use crate::ui::{Prompt, PromptOption, PromptType, StatusKind, UserInterface};

use super::decision::{BlockReason, SkipReason, StepDecision};
use super::diagnostic;
use super::engine::{self, EngineContext, EvaluationResult};
use super::execution::{config_prompt_to_ui_prompt, execute_step_with_recovery};
use super::patterns::StepContext;
use super::plan::build_execution_plan;
use super::satisfaction;
use super::workflow::{RunOptions, WorkflowResult, WorkflowRunner};

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
        satisfaction_cache: &mut SatisfactionCache,
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

            // ── Pre-engine: requirement gap resolution (side effect — may install) ──
            // Gap detection and install attempts are side-effectful (probe environment,
            // prompt user, run install commands). The engine checks `unresolved_gaps`
            // at step 3 (soft blocks) to make the actual skip/proceed decision.
            let mut unresolved_gaps: HashSet<String> = HashSet::new();
            if let Some(ref mut checker) = gap_checker {
                let provided = if options.provided_requirements.is_empty() {
                    None
                } else {
                    Some(&options.provided_requirements)
                };
                let gaps = checker.check_step(step, provided);
                if !gaps.is_empty() {
                    let resolved =
                        installer::handle_gaps(&gaps, checker, ui, interactive, &installer_ctx);
                    match resolved {
                        Ok(true) => {} // all gaps resolved, proceed
                        Ok(false) | Err(_) => {
                            // Gaps remain — let the engine decide what to do
                            for gap in &gaps {
                                event_bus.emit(&BivvyEvent::RequirementGap {
                                    name: step_name.clone(),
                                    requirement: gap.requirement.clone(),
                                    status: format!("{:?}", gap.status),
                                });
                            }
                            unresolved_gaps.insert(step_name.clone());
                        }
                    }
                }
            }

            let needs_force = options.force.contains(step_name);

            // ── Pre-engine: collect named check results for cross-step refs ──
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

            // ── Decision Engine: evaluate step through the full matrix ──
            // The engine handles: dependency blocks, force, satisfaction,
            // prompt_on_rerun, sensitive, confirm, and auto-run decisions.
            let eval_result = if options.dry_run {
                // In dry-run mode, skip the engine and auto-run everything
                EvaluationResult {
                    decision: StepDecision::AutoRun,
                    reason: "dry run".to_string(),
                    satisfaction: None,
                }
            } else {
                // Scope the engine context so snapshot_store borrow is released after
                let result = {
                    let mut engine_ctx = EngineContext {
                        steps: &self.steps,
                        project_root,
                        interpolation: &context,
                        snapshot_store: &mut self.snapshot_store,
                        state: state.as_deref(),
                        step_overrides,
                        force: &options.force,
                        named_check_results: &named_check_results,
                        satisfaction_cache,
                        evaluated: HashMap::new(),
                        failed_steps: &failed_steps,
                        user_skipped_steps: &user_skipped_steps,
                        satisfied_steps: &satisfied_steps,
                        unresolved_gaps: &unresolved_gaps,
                    };
                    engine::evaluate_step(step_name, &mut engine_ctx)
                };
                result
            };

            // Emit any baseline events from check evaluation
            emit_baseline_events(&mut self.snapshot_store, event_bus);

            // ── Act on the engine's decision ──
            match &eval_result.decision {
                StepDecision::Block { reason } => {
                    let reason_str = match reason {
                        BlockReason::DependencyFailed => {
                            let blocked_by = step
                                .depends_on
                                .iter()
                                .find(|d| failed_steps.contains(*d))
                                .cloned()
                                .unwrap_or_default();
                            event_bus.emit(&BivvyEvent::DependencyBlocked {
                                name: step_name.clone(),
                                blocked_by,
                                reason: "dependency_failed".to_string(),
                            });
                            "dependency_failed"
                        }
                        BlockReason::DependencySkipped => "dependency_skipped",
                        BlockReason::PreconditionFailed => "precondition_failed",
                        BlockReason::DependencyUnsatisfied => "dependency_unsatisfied",
                    };
                    event_bus.emit(&BivvyEvent::StepDecided {
                        name: step_name.clone(),
                        decision: "block".to_string(),
                        reason: Some(reason_str.to_string()),
                        trace: None,
                    });
                    ui.message(&step_display);
                    ui.message(&format!(
                        "{}{}",
                        step_pad,
                        StatusKind::Blocked.format(&theme, reason.message())
                    ));
                    ui.show_workflow_progress(index + 1, total, start.elapsed());
                    all_success = false;
                    if matches!(reason, BlockReason::DependencySkipped) {
                        user_skipped_steps.insert(step_name.clone());
                        results.push(StepResult::skipped(
                            &step.name,
                            CheckResult::passed("Dependency skipped"),
                        ));
                    } else {
                        failed_steps.insert(step_name.clone());
                    }
                    continue;
                }

                StepDecision::Skip {
                    reason: SkipReason::AutoSatisfied,
                } => {
                    satisfied_steps.insert(step_name.clone());
                    event_bus.emit(&BivvyEvent::StepDecided {
                        name: step_name.clone(),
                        decision: "skip".to_string(),
                        reason: Some("auto_satisfied".to_string()),
                        trace: Some(DecisionTrace {
                            behavior_flags: make_behavior_flags(step, needs_force),
                            ..Default::default()
                        }),
                    });
                    event_bus.emit(&BivvyEvent::StepSkipped {
                        name: step_name.clone(),
                        reason: format!("satisfied: {}", eval_result.reason),
                    });
                    ui.message(&step_display);
                    let skip_label = crate::ui::satisfaction_label(&eval_result.reason);
                    ui.message(&format!(
                        "{}{}",
                        step_pad,
                        StatusKind::Success.format(&theme, &skip_label)
                    ));
                    ui.show_workflow_progress(index + 1, total, start.elapsed());
                    results.push(StepResult::check_passed(
                        &step.name,
                        CheckResult::passed(format!("satisfied: {}", eval_result.reason)),
                    ));
                    continue;
                }

                StepDecision::Prompt { prompt_key } => {
                    ui.message(&step_display);

                    if !interactive {
                        // Non-interactive: auto-run (can't prompt)
                        // Fall through to execution
                    } else if prompt_key.starts_with("rerun_") {
                        // Rerun prompt: "Ran X ago. Run again?" (default No)
                        let prompt_text = format!("{}. Run again?", eval_result.reason);
                        let prompt = Prompt {
                            key: prompt_key.clone(),
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
                        let answer = ui.prompt(&prompt)?;
                        ui.clear_lines(2);
                        if answer.as_string() != "yes" {
                            satisfied_steps.insert(step_name.clone());
                            event_bus.emit(&BivvyEvent::StepSkipped {
                                name: step_name.clone(),
                                reason: format!("satisfied: {}", eval_result.reason),
                            });
                            let skip_label = crate::ui::satisfaction_label(&eval_result.reason);
                            ui.message(&format!(
                                "{}{}",
                                step_pad,
                                StatusKind::Success.format(&theme, &skip_label)
                            ));
                            ui.show_workflow_progress(index + 1, total, start.elapsed());
                            results.push(StepResult::check_passed(
                                &step.name,
                                CheckResult::passed(format!("satisfied: {}", eval_result.reason)),
                            ));
                            continue;
                        }
                        // User chose yes — fall through to execution
                    } else if prompt_key.starts_with("autorun_") {
                        // Auto-run prompt: "Run step_title?" (default Yes)
                        let prompt_text = format!("Run {}?", step.title);
                        let prompt = Prompt {
                            key: prompt_key.clone(),
                            question: format!("{}{}", step_pad, prompt_text),
                            prompt_type: PromptType::Select {
                                options: vec![
                                    PromptOption {
                                        label: "Yes (y)".to_string(),
                                        value: "yes".to_string(),
                                    },
                                    PromptOption {
                                        label: "No  (n)".to_string(),
                                        value: "no".to_string(),
                                    },
                                ],
                            },
                            default: Some("yes".to_string()),
                        };
                        event_bus.emit(&BivvyEvent::UserPrompted {
                            step: Some(step_name.clone()),
                            prompt: prompt_text,
                            options: vec!["Yes (y)".to_string(), "No  (n)".to_string()],
                        });
                        let answer = ui.prompt(&prompt)?;
                        let answer_str = answer.as_string();
                        event_bus.emit(&BivvyEvent::UserResponded {
                            step: Some(step_name.clone()),
                            input: answer_str.clone(),
                            method: crate::logging::InputMethod::ArrowSelect,
                        });
                        if answer_str != "yes" {
                            ui.clear_lines(2);
                            event_bus.emit(&BivvyEvent::StepDecided {
                                name: step_name.clone(),
                                decision: "skip".to_string(),
                                reason: Some("user_declined_auto_run".to_string()),
                                trace: None,
                            });
                            event_bus.emit(&BivvyEvent::StepSkipped {
                                name: step_name.clone(),
                                reason: "user_declined_auto_run".to_string(),
                            });
                            ui.message(&format!("{}{}", step_pad, theme.format_skipped("Skipped")));
                            user_skipped_steps.insert(step_name.clone());
                            results.push(StepResult::skipped(
                                &step.name,
                                CheckResult::passed("User declined to run"),
                            ));
                            continue;
                        }
                        ui.clear_lines(2);
                        // User chose yes — fall through to execution
                    } else if prompt_key.starts_with("confirm_") {
                        // Confirm prompt: "Step title?" (default Yes)
                        let prompt_text = format!("{}?", step.title);
                        let prompt = Prompt {
                            key: prompt_key.clone(),
                            question: format!("{}{}", step_pad, prompt_text),
                            prompt_type: PromptType::Select {
                                options: vec![
                                    PromptOption {
                                        label: "Yes (y)".to_string(),
                                        value: "yes".to_string(),
                                    },
                                    PromptOption {
                                        label: "No  (n)".to_string(),
                                        value: "no".to_string(),
                                    },
                                ],
                            },
                            default: Some("yes".to_string()),
                        };
                        event_bus.emit(&BivvyEvent::UserPrompted {
                            step: Some(step_name.clone()),
                            prompt: prompt_text,
                            options: vec!["Yes (y)".to_string(), "No  (n)".to_string()],
                        });
                        let answer = ui.prompt(&prompt)?;
                        let answer_str = answer.as_string();
                        event_bus.emit(&BivvyEvent::UserResponded {
                            step: Some(step_name.clone()),
                            input: answer_str.clone(),
                            method: crate::logging::InputMethod::ArrowSelect,
                        });
                        if answer_str != "yes" {
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
                        ui.clear_lines(2);
                        // User chose yes — fall through to execution
                    } else if prompt_key.starts_with("sensitive_") {
                        // Sensitive prompt: "Handles sensitive data. Continue?"
                        let prompt_text = "Handles sensitive data. Continue?";
                        let prompt = Prompt {
                            key: prompt_key.clone(),
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
                        // User chose yes — fall through to execution
                    }
                }

                StepDecision::AutoRun | StepDecision::Run => {
                    // Show step header, then fall through to execution
                    ui.message(&step_display);
                }

                StepDecision::Skip { reason } => {
                    // Other skip reasons (shouldn't normally happen from engine)
                    ui.message(&step_display);
                    ui.message(&format!(
                        "{}{}",
                        step_pad,
                        StatusKind::Skipped.format(&theme, reason.message())
                    ));
                    ui.show_workflow_progress(index + 1, total, start.elapsed());
                    results.push(StepResult::skipped(
                        &step.name,
                        CheckResult::passed(reason.message()),
                    ));
                    continue;
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

            // Update the pinned progress bar
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

                // Record successful execution in satisfaction cache
                let record = SatisfactionRecord {
                    satisfied: true,
                    source: crate::state::satisfaction::SatisfactionSource::ExecutionHistory,
                    recorded_at: chrono::Utc::now(),
                    evidence: crate::state::satisfaction::SatisfactionEvidence::HistoricalRun {
                        ran_at: chrono::Utc::now(),
                        exit_code: exec_result.result.exit_code.unwrap_or(0),
                    },
                    config_hash: None,
                    step_hash: None,
                };
                satisfaction_cache.store(step_name, record);
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

        // Flush satisfaction cache to disk
        if !options.dry_run {
            if let Err(e) = satisfaction_cache.flush() {
                tracing::warn!("Failed to flush satisfaction cache: {}", e);
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
