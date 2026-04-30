//! Step-level execution manager.
//!
//! Owns step state and handles step execution, prompts, error blocks,
//! and step-level UI output. Extracted from `orchestrate.rs` to separate
//! step-level concerns from workflow-level orchestration.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::checks::evaluator::CheckEvaluator;
use crate::checks::CheckResult;
use crate::config::interpolation::InterpolationContext;
use crate::config::schema::StepOverride;
use crate::error::{BivvyError, Result};
use crate::logging::{BehaviorFlags, BivvyEvent, DecisionTrace, EventBus};
use crate::requirements::checker::GapChecker;
use crate::requirements::installer;
use crate::snapshots::SnapshotStore;
use crate::state::satisfaction::SatisfactionCache;
use crate::state::StateStore;
use crate::steps::{ResolvedStep, StepResult, StepStatus};
use crate::ui::theme::BivvyTheme;
use crate::ui::{Prompt, PromptOption, PromptType, StatusKind, UserInterface};

use super::decision::{BlockReason, SkipReason, StepDecision};
use super::diagnostic;
use super::display::StepDisplay;
use super::engine::{self, EngineContext, EvaluationResult};
use super::execution::{config_prompt_to_ui_prompt, execute_step_with_recovery};
use super::patterns::StepContext;
use super::satisfaction;

/// Options passed from the workflow layer to control step execution.
pub(super) struct StepExecutionOptions<'a> {
    pub dry_run: bool,
    pub interactive: bool,
    pub diagnostic_funnel: bool,
    pub project_root: &'a Path,
    /// YAML-defined env (settings + workflow), pre-merged in priority order.
    /// Step-level env layers and `process_env` are applied on top.
    pub base_env: &'a HashMap<String, String>,
    /// Parent process environment. Wins over `base_env` and step env.
    pub process_env: &'a HashMap<String, String>,
    pub force_steps: &'a HashSet<String>,
    pub force_all: bool,
    pub provided_requirements: &'a HashSet<String>,
}

impl StepExecutionOptions<'_> {
    /// Whether the named step should be forced (workflow-wide or by name).
    pub(super) fn should_force(&self, step_name: &str) -> bool {
        self.force_all || self.force_steps.contains(step_name)
    }
}

/// Why a step was skipped at runtime.
///
/// This lets the workflow layer categorize skips without inspecting
/// `StepResult` internals — per-step classification stays in StepManager.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SkipCategory {
    /// Check passed — step is already satisfied.
    Satisfied,
    /// Dependency was skipped by user, so this step was transitively skipped.
    DependencySkipped,
    /// User actively declined a prompt (autorun, confirm, sensitive).
    UserDeclined,
    /// User chose to skip during recovery after a failure.
    /// Unlike `UserDeclined`, this does NOT block dependents — the user
    /// explicitly chose to continue past the failure.
    RecoverySkipped,
    /// Other skip reason (generic fallback from engine).
    Other,
}

/// The action taken by a step during execution, returned to the workflow layer.
pub(super) enum StepAction {
    /// Step executed successfully.
    Completed(StepResult),
    /// Step was skipped at runtime, with the reason categorized.
    Skipped(StepResult, SkipCategory),
    /// Step failed.
    Failed(StepResult),
    /// Step was blocked (dependency failed, precondition failed, etc.).
    Blocked,
    /// User chose to abort the workflow.
    Aborted(StepResult),
}

/// Manages the execution of a single step within a workflow.
///
/// Handles step-level concerns: printing step headers, evaluating checks,
/// showing prompts, executing the step command, and handling recovery.
/// Does NOT know about workflow state, progress bars, or summaries.
pub(super) struct StepManager<'a> {
    step: &'a ResolvedStep,
    step_name: &'a str,
    index: usize,
    total: usize,
    theme: &'a BivvyTheme,
}

impl<'a> StepManager<'a> {
    /// Create a new StepManager for a specific step.
    pub fn new(
        step: &'a ResolvedStep,
        step_name: &'a str,
        index: usize,
        total: usize,
        theme: &'a BivvyTheme,
    ) -> Self {
        Self {
            step,
            step_name,
            index,
            total,
            theme,
        }
    }

    /// Format the step number string, e.g., `[1/7]`.
    fn step_number(&self) -> String {
        format!("[{}/{}]", self.index + 1, self.total)
    }

    /// Compute the indentation width that aligns content under the step name.
    fn step_indent(&self) -> usize {
        self.step_number().len() + 1 // +1 for the space after
    }

    /// Compute the indentation padding string.
    fn step_pad(&self) -> String {
        " ".repeat(self.step_indent())
    }

    /// Format the step display line with optional title.
    fn step_header_text(&self) -> String {
        let step_number = self.step_number();
        let step_header = format!(
            "{} {}",
            self.theme.step_number.apply_to(&step_number),
            self.theme.step_title.apply_to(self.step_name)
        );
        if self.step_name == self.step.title {
            step_header
        } else {
            format!(
                "{} {} {}",
                step_header,
                self.theme.dim.apply_to("—"),
                self.theme.dim.apply_to(&self.step.title)
            )
        }
    }

    /// Execute the step through the full decision/execution lifecycle.
    ///
    /// This method handles:
    /// - Requirement gap resolution
    /// - Decision engine evaluation
    /// - Prompts (rerun, autorun, confirm, sensitive)
    /// - Step-level template prompts
    /// - Step execution with retry and recovery
    /// - Event emission for all step-level events
    ///
    /// Returns a `StepAction` describing what happened.
    #[allow(clippy::too_many_arguments)]
    pub fn execute(
        &self,
        opts: &StepExecutionOptions<'_>,
        context: &mut InterpolationContext,
        step_overrides: &HashMap<String, StepOverride>,
        gap_checker: &mut Option<&mut GapChecker<'_>>,
        snapshot_store: &mut SnapshotStore,
        steps: &HashMap<String, ResolvedStep>,
        state: Option<&StateStore>,
        satisfaction_cache: &mut SatisfactionCache,
        failed_steps: &HashSet<String>,
        user_skipped_steps: &HashSet<String>,
        satisfied_steps: &HashSet<String>,
        named_check_results: &mut HashMap<String, CheckResult>,
        results: &[StepResult],
        ui: &mut dyn UserInterface,
        step_display: &mut dyn StepDisplay,
        event_bus: &mut EventBus,
    ) -> Result<StepAction> {
        let step_pad = self.step_pad();
        let step_header = self.step_header_text();

        // Emit step planned
        event_bus.emit(&BivvyEvent::StepPlanned {
            name: self.step_name.to_string(),
            index: self.index,
            total: self.total,
        });

        // Blank line between steps
        if self.index > 0 {
            ui.message("");
        }

        // ── Pre-engine: requirement gap resolution ──
        let mut unresolved_gaps: HashSet<String> = HashSet::new();
        if let Some(ref mut checker) = gap_checker {
            let provided = if opts.provided_requirements.is_empty() {
                None
            } else {
                Some(opts.provided_requirements)
            };
            let gaps = checker.check_step(self.step, provided);
            if !gaps.is_empty() {
                let installer_ctx = installer::default_context();
                let resolved =
                    installer::handle_gaps(&gaps, checker, ui, opts.interactive, &installer_ctx);
                match resolved {
                    Ok(true) => {}
                    Ok(false) | Err(_) => {
                        for gap in &gaps {
                            event_bus.emit(&BivvyEvent::RequirementGap {
                                name: self.step_name.to_string(),
                                requirement: gap.requirement.clone(),
                                status: format!("{:?}", gap.status),
                            });
                        }
                        unresolved_gaps.insert(self.step_name.to_string());
                    }
                }
            }
        }

        let needs_force = opts.should_force(self.step_name) || self.step.behavior.force;

        // ── Pre-engine: collect named check results for cross-step refs ──
        if !opts.dry_run {
            if let Some(check) = self.step.execution.effective_check() {
                if check.has_named_checks() {
                    let mut evaluator =
                        CheckEvaluator::new(opts.project_root, context, snapshot_store);
                    let step_named = satisfaction::collect_named_check_results(
                        self.step_name,
                        &check,
                        &mut evaluator,
                    );
                    named_check_results.extend(step_named);
                }
            }
        }

        // ── Decision Engine ──
        let eval_result = if opts.dry_run {
            EvaluationResult {
                decision: StepDecision::AutoRun,
                reason: "dry run".to_string(),
                satisfaction: None,
            }
        } else {
            let mut engine_ctx = EngineContext {
                steps,
                project_root: opts.project_root,
                interpolation: context,
                snapshot_store,
                state,
                step_overrides,
                force: opts.force_steps,
                force_all: opts.force_all,
                named_check_results,
                satisfaction_cache,
                evaluated: HashMap::new(),
                failed_steps,
                user_skipped_steps,
                satisfied_steps,
                unresolved_gaps: &unresolved_gaps,
            };
            engine::evaluate_step(self.step_name, &mut engine_ctx)
        };

        // Emit any baseline events from check evaluation
        emit_baseline_events(snapshot_store, event_bus);

        // ── Act on the engine's decision ──
        match &eval_result.decision {
            StepDecision::Block { reason } => {
                return self.handle_block(
                    reason,
                    &step_header,
                    &step_pad,
                    failed_steps,
                    event_bus,
                    step_display,
                );
            }

            StepDecision::Skip {
                reason: SkipReason::AutoSatisfied,
            } => {
                return self.handle_auto_satisfied(
                    &eval_result,
                    &step_header,
                    &step_pad,
                    needs_force,
                    event_bus,
                    step_display,
                );
            }

            StepDecision::Prompt { prompt_key } => {
                step_display.message(&step_header);

                if opts.interactive {
                    let outcome = self.handle_prompt(
                        prompt_key,
                        &eval_result,
                        &step_pad,
                        needs_force,
                        event_bus,
                        ui,
                        step_display,
                    )?;
                    if let Some(outcome) = outcome {
                        return Ok(outcome);
                    }
                    // None means fall through to execution
                }
                // Non-interactive: fall through to execution
            }

            StepDecision::AutoRun | StepDecision::Run => {
                step_display.message(&step_header);
            }

            StepDecision::Skip { reason } => {
                step_display.message(&step_header);
                step_display.message(&format!(
                    "{}{}",
                    step_pad,
                    StatusKind::Skipped.format(self.theme, reason.message())
                ));
                event_bus.emit(&BivvyEvent::StepSkipped {
                    name: self.step_name.to_string(),
                    reason: reason.message().to_string(),
                });
                return Ok(StepAction::Skipped(
                    StepResult::skipped(&self.step.name, CheckResult::passed(reason.message())),
                    SkipCategory::Other,
                ));
            }
        }

        // ── Step-level prompts (template inputs) ──
        if !self.step.output.prompts.is_empty() {
            for prompt_config in &self.step.output.prompts {
                if context.resolve(&prompt_config.key).is_some() {
                    continue;
                }

                if !opts.interactive {
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
                            step: self.step_name.to_string(),
                            message: format!(
                                "Prompt '{}' requires a value in non-interactive mode. \
                                 Set via env var, template input, or provide a default.",
                                prompt_config.key
                            ),
                        });
                    }
                    continue;
                }

                let ui_prompt = config_prompt_to_ui_prompt(prompt_config);
                let result = ui.prompt(&ui_prompt)?;
                context
                    .prompts
                    .insert(prompt_config.key.clone(), result.as_string());
            }
        }

        // ── Build step context and emit decision to run ──
        let step_ctx = StepContext {
            name: self.step_name,
            command: &self.step.execution.command,
            requires: &self.step.requires,
            template: None,
        };

        event_bus.emit(&BivvyEvent::StepDecided {
            name: self.step_name.to_string(),
            decision: "run".to_string(),
            reason: None,
            trace: Some(DecisionTrace {
                behavior_flags: make_behavior_flags(self.step, needs_force),
                ..Default::default()
            }),
        });
        event_bus.emit(&BivvyEvent::StepStarting {
            name: self.step_name.to_string(),
        });

        // ── Build workflow state for diagnostic funnel ──
        let workflow_state = {
            let step_refs: Vec<(&str, &ResolvedStep)> = steps
                .iter()
                .map(|(name, step)| (name.as_str(), step))
                .collect();
            let mut outcomes = HashMap::new();
            for r in results {
                outcomes.insert(r.name.clone(), r.status());
            }
            for s in satisfied_steps {
                outcomes.entry(s.clone()).or_insert(StepStatus::Completed);
            }
            for s in failed_steps {
                outcomes.entry(s.clone()).or_insert(StepStatus::Failed);
            }
            for s in user_skipped_steps {
                outcomes.entry(s.clone()).or_insert(StepStatus::Skipped);
            }
            (step_refs, outcomes)
        };
        let ws = diagnostic::WorkflowState {
            steps: &workflow_state.0,
            outcomes: &workflow_state.1,
        };

        // ── Execute step with retry and recovery ──
        let step_number = self.step_number();
        let step_indent = self.step_indent();
        let exec_result = execute_step_with_recovery(
            self.step,
            self.step_name,
            &step_number,
            step_indent,
            opts.project_root,
            context,
            opts.base_env,
            opts.process_env,
            needs_force,
            opts.dry_run,
            opts.interactive,
            &step_ctx,
            opts.diagnostic_funnel,
            &ws,
            ui,
            step_display,
            event_bus,
        )?;

        // ── Emit step completion event ──
        match exec_result.result.status() {
            StepStatus::Completed => {
                event_bus.emit(&BivvyEvent::StepCompleted {
                    name: self.step_name.to_string(),
                    success: true,
                    exit_code: exec_result.result.exit_code,
                    duration_ms: exec_result.result.duration.as_millis() as u64,
                    error: None,
                });
            }
            StepStatus::Failed => {
                event_bus.emit(&BivvyEvent::StepCompleted {
                    name: self.step_name.to_string(),
                    success: false,
                    exit_code: exec_result.result.exit_code,
                    duration_ms: exec_result.result.duration.as_millis() as u64,
                    error: exec_result.result.error.clone(),
                });
            }
            StepStatus::Skipped => {
                event_bus.emit(&BivvyEvent::StepSkipped {
                    name: self.step_name.to_string(),
                    reason: exec_result
                        .result
                        .recovery_detail
                        .clone()
                        .unwrap_or_else(|| "skipped".to_string()),
                });
            }
            _ => {}
        }

        // ── Determine action ──
        if exec_result.aborted {
            return Ok(StepAction::Aborted(exec_result.result));
        }

        let status = exec_result.result.status();
        Ok(match status {
            StepStatus::Completed => StepAction::Completed(exec_result.result),
            StepStatus::Failed => {
                if exec_result.skipped_by_user {
                    StepAction::Skipped(exec_result.result, SkipCategory::RecoverySkipped)
                } else {
                    StepAction::Failed(exec_result.result)
                }
            }
            StepStatus::Skipped => {
                StepAction::Skipped(exec_result.result, SkipCategory::RecoverySkipped)
            }
            _ => StepAction::Completed(exec_result.result),
        })
    }

    /// Handle a blocked step (dependency failed, precondition failed, etc.).
    fn handle_block(
        &self,
        reason: &BlockReason,
        step_header_text: &str,
        step_pad: &str,
        _failed_steps: &HashSet<String>,
        event_bus: &mut EventBus,
        step_display: &mut dyn StepDisplay,
    ) -> Result<StepAction> {
        let reason_str = match reason {
            BlockReason::DependencyFailed { dependency } => {
                event_bus.emit(&BivvyEvent::DependencyBlocked {
                    name: self.step_name.to_string(),
                    blocked_by: dependency.clone(),
                    reason: "dependency_failed".to_string(),
                });
                "dependency_failed"
            }
            BlockReason::DependencySkipped { .. } => "dependency_skipped",
            BlockReason::PreconditionFailed { .. } => "precondition_failed",
            BlockReason::DependencyUnsatisfied { .. } => "dependency_unsatisfied",
        };
        event_bus.emit(&BivvyEvent::StepDecided {
            name: self.step_name.to_string(),
            decision: "block".to_string(),
            reason: Some(reason_str.to_string()),
            trace: None,
        });
        step_display.message(step_header_text);
        step_display.message(&format!(
            "{}{}",
            step_pad,
            StatusKind::Blocked.format(self.theme, &reason.message())
        ));

        // For dependency_skipped, return a skip result, not a block
        if matches!(reason, BlockReason::DependencySkipped { .. }) {
            Ok(StepAction::Skipped(
                StepResult::skipped(&self.step.name, CheckResult::passed("Dependency skipped")),
                SkipCategory::DependencySkipped,
            ))
        } else {
            Ok(StepAction::Blocked)
        }
    }

    /// Handle an auto-satisfied step (check passed, no prompt needed).
    fn handle_auto_satisfied(
        &self,
        eval_result: &EvaluationResult,
        step_header_text: &str,
        step_pad: &str,
        needs_force: bool,
        event_bus: &mut EventBus,
        step_display: &mut dyn StepDisplay,
    ) -> Result<StepAction> {
        event_bus.emit(&BivvyEvent::StepDecided {
            name: self.step_name.to_string(),
            decision: "skip".to_string(),
            reason: Some("auto_satisfied".to_string()),
            trace: Some(DecisionTrace {
                behavior_flags: make_behavior_flags(self.step, needs_force),
                ..Default::default()
            }),
        });
        event_bus.emit(&BivvyEvent::StepSkipped {
            name: self.step_name.to_string(),
            reason: eval_result.reason.clone(),
        });
        step_display.message(step_header_text);
        let skip_label = crate::ui::satisfaction_label(&eval_result.reason);
        step_display.message(&format!(
            "{}{}",
            step_pad,
            StatusKind::Success.format(self.theme, &skip_label)
        ));
        Ok(StepAction::Skipped(
            StepResult::check_passed(
                &self.step.name,
                CheckResult::passed(eval_result.reason.clone()),
            ),
            SkipCategory::Satisfied,
        ))
    }

    /// Handle a prompt decision. Returns `Some(StepAction)` if the step should
    /// not proceed to execution (user declined), or `None` to fall through.
    #[allow(clippy::too_many_arguments)]
    fn handle_prompt(
        &self,
        prompt_key: &str,
        eval_result: &EvaluationResult,
        step_pad: &str,
        _needs_force: bool,
        event_bus: &mut EventBus,
        ui: &mut dyn UserInterface,
        step_display: &mut dyn StepDisplay,
    ) -> Result<Option<StepAction>> {
        if prompt_key.starts_with("rerun_") {
            let prompt_text = format!("{}. Run again?", eval_result.reason);
            let prompt = Prompt {
                key: prompt_key.to_string(),
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
            // clear_lines removed — surface tracks regions
            if answer.as_string() != "yes" {
                event_bus.emit(&BivvyEvent::StepSkipped {
                    name: self.step_name.to_string(),
                    reason: eval_result.reason.clone(),
                });
                let skip_label = crate::ui::satisfaction_label(&eval_result.reason);
                step_display.message(&format!(
                    "{}{}",
                    step_pad,
                    StatusKind::Success.format(self.theme, &skip_label)
                ));
                return Ok(Some(StepAction::Skipped(
                    StepResult::check_passed(
                        &self.step.name,
                        CheckResult::passed(eval_result.reason.clone()),
                    ),
                    SkipCategory::Satisfied,
                )));
            }
            // User chose yes — fall through to execution
        } else if prompt_key.starts_with("autorun_") {
            let prompt_text = format!("Run {}?", self.step.title);
            let prompt = Prompt {
                key: prompt_key.to_string(),
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
                step: Some(self.step_name.to_string()),
                prompt: prompt_text,
                options: vec!["Yes (y)".to_string(), "No  (n)".to_string()],
            });
            let answer = ui.prompt(&prompt)?;
            let answer_str = answer.as_string();
            event_bus.emit(&BivvyEvent::UserResponded {
                step: Some(self.step_name.to_string()),
                input: answer_str.clone(),
                method: crate::logging::InputMethod::ArrowSelect,
            });
            if answer_str != "yes" {
                // clear_lines removed — surface tracks regions
                event_bus.emit(&BivvyEvent::StepDecided {
                    name: self.step_name.to_string(),
                    decision: "skip".to_string(),
                    reason: Some("user_declined_auto_run".to_string()),
                    trace: None,
                });
                event_bus.emit(&BivvyEvent::StepSkipped {
                    name: self.step_name.to_string(),
                    reason: "user_declined_auto_run".to_string(),
                });
                step_display.message(&format!(
                    "{}{}",
                    step_pad,
                    self.theme.format_skipped("Skipped")
                ));
                return Ok(Some(StepAction::Skipped(
                    StepResult::skipped(
                        &self.step.name,
                        CheckResult::passed("User declined to run"),
                    ),
                    SkipCategory::UserDeclined,
                )));
            }
            // clear_lines removed — surface tracks regions
        } else if prompt_key.starts_with("confirm_") {
            let prompt_text = format!("{}?", self.step.title);
            let prompt = Prompt {
                key: prompt_key.to_string(),
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
                step: Some(self.step_name.to_string()),
                prompt: prompt_text,
                options: vec!["Yes (y)".to_string(), "No  (n)".to_string()],
            });
            let answer = ui.prompt(&prompt)?;
            let answer_str = answer.as_string();
            event_bus.emit(&BivvyEvent::UserResponded {
                step: Some(self.step_name.to_string()),
                input: answer_str.clone(),
                method: crate::logging::InputMethod::ArrowSelect,
            });
            if answer_str != "yes" {
                // clear_lines removed — surface tracks regions
                event_bus.emit(&BivvyEvent::StepDecided {
                    name: self.step_name.to_string(),
                    decision: "skip".to_string(),
                    reason: Some("user_declined".to_string()),
                    trace: None,
                });
                event_bus.emit(&BivvyEvent::StepSkipped {
                    name: self.step_name.to_string(),
                    reason: "user_declined".to_string(),
                });
                step_display.message(&format!(
                    "{}{}",
                    step_pad,
                    self.theme.format_skipped("Skipped")
                ));
                return Ok(Some(StepAction::Skipped(
                    StepResult::skipped(&self.step.name, CheckResult::passed("User declined")),
                    SkipCategory::UserDeclined,
                )));
            }
            // clear_lines removed — surface tracks regions
        } else if prompt_key.starts_with("sensitive_") {
            let prompt_text = "Handles sensitive data. Continue?";
            let prompt = Prompt {
                key: prompt_key.to_string(),
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
                step: Some(self.step_name.to_string()),
                prompt: prompt_text.to_string(),
                options: vec!["Yes (y)".to_string(), "No (n)".to_string()],
            });
            let answer = ui.prompt(&prompt)?;
            let answer_str = answer.as_string();
            event_bus.emit(&BivvyEvent::UserResponded {
                step: Some(self.step_name.to_string()),
                input: answer_str.clone(),
                method: crate::logging::InputMethod::ArrowSelect,
            });
            if answer_str != "yes" {
                if self.step.behavior.skippable {
                    event_bus.emit(&BivvyEvent::StepSkipped {
                        name: self.step_name.to_string(),
                        reason: "user_declined_sensitive".to_string(),
                    });
                    step_display.message(&format!(
                        "{}{}",
                        step_pad,
                        self.theme
                            .format_skipped("Skipped (declined sensitive step)")
                    ));
                    return Ok(Some(StepAction::Skipped(
                        StepResult::skipped(
                            &self.step.name,
                            CheckResult::passed("User declined sensitive step"),
                        ),
                        SkipCategory::UserDeclined,
                    )));
                } else {
                    return Err(BivvyError::StepExecutionError {
                        step: self.step_name.to_string(),
                        message: format!(
                            "Step '{}' is sensitive and not skippable, but user declined",
                            self.step.title
                        ),
                    });
                }
            }
            // User chose yes — fall through to execution
        }

        Ok(None) // Fall through to execution
    }
}

/// Drain pending baseline events from the snapshot store and emit them on the event bus.
fn emit_baseline_events(snapshot_store: &mut SnapshotStore, event_bus: &mut EventBus) {
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
        auto_run: step.behavior.auto_run,
        prompt_on_rerun: step.behavior.prompt_on_rerun,
        confirm: step.behavior.confirm,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::steps::{
        ResolvedBehavior, ResolvedEnvironmentVars, ResolvedExecution, ResolvedHooks,
        ResolvedOutput, ResolvedScoping,
    };

    fn dummy_step(name: &str) -> ResolvedStep {
        ResolvedStep {
            name: name.to_string(),
            title: name.to_string(),
            description: None,
            depends_on: vec![],
            requires: vec![],
            inputs: HashMap::new(),
            satisfied_when: vec![],
            execution: ResolvedExecution::default(),
            env_vars: ResolvedEnvironmentVars::default(),
            behavior: ResolvedBehavior::default(),
            hooks: ResolvedHooks::default(),
            output: ResolvedOutput::default(),
            scoping: ResolvedScoping::default(),
        }
    }

    /// `step_indent` must match the rendered step number width plus a single
    /// space — so error blocks, status lines, and recovery prompts align under
    /// the step name.
    #[test]
    fn step_indent_matches_step_number_width() {
        let step = dummy_step("setup");
        let theme = BivvyTheme::new();

        // [1/3] = 5 chars + space = 6
        let mgr = StepManager::new(&step, "setup", 0, 3, &theme);
        assert_eq!(mgr.step_number(), "[1/3]");
        assert_eq!(mgr.step_indent(), 6);
        assert_eq!(mgr.step_pad(), "      ");

        // [10/15] = 7 chars + space = 8
        let mgr = StepManager::new(&step, "setup", 9, 15, &theme);
        assert_eq!(mgr.step_number(), "[10/15]");
        assert_eq!(mgr.step_indent(), 8);
        assert_eq!(mgr.step_pad(), "        ");

        // [1/1] = 5 chars + space = 6
        let mgr = StepManager::new(&step, "setup", 0, 1, &theme);
        assert_eq!(mgr.step_number(), "[1/1]");
        assert_eq!(mgr.step_indent(), 6);

        // [100/100] = 9 chars + space = 10
        let mgr = StepManager::new(&step, "setup", 99, 100, &theme);
        assert_eq!(mgr.step_number(), "[100/100]");
        assert_eq!(mgr.step_indent(), 10);
    }
}
