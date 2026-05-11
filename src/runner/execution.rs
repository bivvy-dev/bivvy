//! Step execution with retry and interactive recovery.
//!
//! Extracted from `orchestrate.rs` to reduce its size. Contains the
//! execution lifecycle: spinner display, output capture, auto-retries,
//! and the interactive recovery menu.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Instant;

use tracing::warn;

use crate::config::interpolation::InterpolationContext;
use crate::error::Result;
use crate::logging::{BivvyEvent, EventBus};
use crate::shell::OutputCallback;
use crate::steps::{execute_step, ExecutionOptions, ResolvedStep, StepResult, StepStatus};
use crate::ui::{format_duration, OutputMode, Prompt, PromptOption, PromptType, UserInterface};

use super::diagnostic;
use super::display::StepDisplay;
use super::patterns::{self, FixSuggestion, StepContext};
use super::recovery::{self, RecoveryAction};

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

/// Identity and display layout for the step being executed.
pub(super) struct StepIdentity<'a> {
    pub step: &'a ResolvedStep,
    pub name: &'a str,
    pub number: &'a str,
    pub indent: usize,
}

/// Environment and interpolation context for step execution.
pub(super) struct StepRunEnv<'a> {
    pub project_root: &'a Path,
    pub context: &'a InterpolationContext,
    pub base_env: &'a HashMap<String, String>,
    pub process_env: &'a HashMap<String, String>,
}

/// Flags controlling step execution behavior.
pub(super) struct StepRunFlags {
    pub needs_force: bool,
    pub dry_run: bool,
    pub interactive: bool,
    pub diagnostic_funnel: bool,
}

/// Mutable UI channels available to the step execution lifecycle.
pub(super) struct StepRunUi<'a> {
    pub ui: &'a mut dyn UserInterface,
    pub step_display: &'a mut dyn StepDisplay,
    pub event_bus: &'a mut EventBus,
}

/// Diagnostic helpers passed through to error analysis and the recovery menu.
pub(super) struct StepRunDiagnostics<'a> {
    pub step_ctx: &'a StepContext<'a>,
    pub workflow_state: &'a diagnostic::WorkflowState<'a>,
}

/// Execute a step with retry and interactive recovery.
///
/// This handles the full execution lifecycle: spinner display, output capture,
/// auto-retries, and the interactive recovery menu (retry/fix/shell/skip/abort).
pub(super) fn execute_step_with_recovery(
    identity: &StepIdentity<'_>,
    env: &StepRunEnv<'_>,
    flags: &StepRunFlags,
    diagnostics: &StepRunDiagnostics<'_>,
    run_ui: StepRunUi<'_>,
) -> Result<StepExecutionResult> {
    let step = identity.step;
    let step_name = identity.name;
    let step_number = identity.number;
    let step_indent = identity.indent;
    let project_root = env.project_root;
    let context = env.context;
    let base_env = env.base_env;
    let process_env = env.process_env;
    let needs_force = flags.needs_force;
    let dry_run = flags.dry_run;
    let interactive = flags.interactive;
    let diagnostic_funnel = flags.diagnostic_funnel;
    let step_ctx = diagnostics.step_ctx;
    let workflow_state = diagnostics.workflow_state;
    let StepRunUi {
        ui,
        step_display,
        event_bus,
    } = run_ui;
    let mut retry_count: u32 = 0;
    let mut fix_history: HashSet<String> = HashSet::new();
    let mut skipped_by_user = false;
    let mut aborted = false;
    // Every break out of the 'step_execution loop assigns final_result
    // first. The four arms that break are: Completed (sets `final_result`
    // then breaks), Skipped (sets then breaks), Failed (sets and breaks
    // when `allow_failure`, `!interactive`, or `>= MAX_STEP_ATTEMPTS`;
    // otherwise calls handle_recovery_menu, which mutates `final_result`
    // through a &mut and the loop only breaks when `final_result.is_some()`),
    // and the wildcard `_` (sets then breaks). The trailing `.expect`
    // below is therefore unreachable in practice and exists only as a
    // panic-on-bug guard. No `#[allow(unused_assignments)]` is needed.
    let mut final_result: Option<StepResult> = None;

    // Outer loop: step execution (retry/fix re-enter here)
    'step_execution: loop {
        // Fresh spinner per attempt — hide command text for sensitive steps
        let display_command = if step.behavior.sensitive {
            "[SENSITIVE]".to_string()
        } else {
            step.execution.command.clone()
        };
        // Mount the transient region with the spinner. The step display
        // owns the live-output ring buffer.
        step_display.start_running(&display_command);
        let output_mode = step_display.output_mode();
        let output_callback: Option<OutputCallback> = step_display.live_output_callback();

        let exec_options = ExecutionOptions {
            force: needs_force,
            dry_run,
            capture_output: output_callback.is_none(),
            ..Default::default()
        };
        let _ = step_number;

        let step_start = Instant::now();
        let result = match execute_step(
            step,
            project_root,
            context,
            base_env,
            process_env,
            &exec_options,
            output_callback,
        ) {
            Ok(result) => result,
            Err(e) => {
                warn!("Step '{}' errored: {}", step_name, e);
                StepResult::failure(step_name, step_start.elapsed(), e.to_string(), None)
            }
        };

        // Emit StepOutput events for captured output
        if let Some(ref output) = result.output {
            for line in output.lines() {
                event_bus.emit(&BivvyEvent::StepOutput {
                    name: step_name.to_string(),
                    stream: "stdout".to_string(),
                    line: line.to_string(),
                });
            }
        }

        let duration_str = format_duration(result.duration);

        match result.status() {
            StepStatus::Completed => {
                let detail = if retry_count > 0 {
                    Some(format!("succeeded on retry (attempt {})", retry_count + 1))
                } else {
                    None
                };
                // Clear the transient region and write the final result
                // line into scrollback. The label is derived from the
                // status enum inside `finish`.
                step_display.finish(
                    StepStatus::Completed,
                    Some(result.duration),
                    detail.as_deref(),
                );
                let mut r = result;
                r.recovery_detail = detail;
                final_result = Some(r);
                break 'step_execution;
            }
            StepStatus::Skipped => {
                step_display.finish(StepStatus::Skipped, None, None);
                final_result = Some(result);
                break 'step_execution;
            }
            StepStatus::Failed => {
                step_display.finish(StepStatus::Failed, Some(result.duration), None);
                let _ = duration_str;

                // Build combined error output for pattern matching and display
                let combined_output = result
                    .output
                    .as_deref()
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| result.error.as_deref().unwrap_or("Command failed"))
                    .to_string();

                // Match against error recovery system
                let (fix, hint, resolutions) = if diagnostic_funnel {
                    let diag_ctx = diagnostic::StepContext {
                        name: step_ctx.name,
                        command: step_ctx.command,
                        requires: step_ctx.requires,
                        template: step_ctx.template,
                    };
                    let diag = diagnostic::diagnose(&combined_output, &diag_ctx, workflow_state);
                    // Collect all resolutions with confidence 0.1–0.29 as hint text
                    // (shown below error block, not in menu)
                    let hints: Vec<String> = diag
                        .resolutions
                        .iter()
                        .filter(|r| r.confidence >= 0.1 && r.confidence < 0.3)
                        .map(|r| r.label.clone())
                        .collect();
                    let hint = if hints.is_empty() {
                        None
                    } else {
                        Some(format!("You might try: {}", hints.join(", or ")))
                    };
                    (None, hint, diag.resolutions)
                } else {
                    let fix = patterns::find_fix(&combined_output, step_ctx);
                    let hint = patterns::find_hint(&combined_output, step_ctx);
                    (fix, hint, Vec::new())
                };

                // Show error block — skip in non-interactive verbose
                // where output was already streamed to stdout
                let output_was_streamed =
                    !step_display.is_interactive() && output_mode == OutputMode::Verbose;
                if !output_was_streamed {
                    step_display.show_error_block(
                        &step.execution.command,
                        &combined_output,
                        hint.as_deref(),
                        step_indent,
                    );
                }

                // allow_failure: record and move on, no recovery menu
                if step.behavior.allow_failure {
                    final_result = Some(result);
                    break 'step_execution;
                }

                // Auto-retry before showing recovery menu
                if retry_count < step.execution.retry {
                    retry_count += 1;
                    step_display.message(&format!(
                        "{}Retrying... (attempt {}/{})",
                        " ".repeat(step_indent),
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
                let failure = FailureContext {
                    result,
                    combined_output: &combined_output,
                    fix,
                    hint,
                    resolutions: &resolutions,
                    diagnostic_funnel,
                };
                let mut outcome = RecoveryOutcome {
                    fix_history: &mut fix_history,
                    retry_count: &mut retry_count,
                    skipped_by_user: &mut skipped_by_user,
                    aborted: &mut aborted,
                    final_result: &mut final_result,
                };
                handle_recovery_menu(
                    RecoveryStep {
                        step,
                        name: step_name,
                        indent: step_indent,
                    },
                    failure,
                    &mut outcome,
                    RecoveryEnv {
                        project_root,
                        base_env,
                        process_env,
                    },
                    StepRunUi {
                        ui,
                        step_display,
                        event_bus,
                    },
                )?;
                if final_result.is_some() {
                    break 'step_execution;
                }
                // If final_result is still None, recovery chose retry/fix → continue
                continue 'step_execution;
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
        aborted,
    })
}

/// Immutable description of a step failure, fed to the recovery menu.
struct FailureContext<'a> {
    /// The failing step result; consumed if the user skips/aborts.
    result: StepResult,
    combined_output: &'a str,
    fix: Option<FixSuggestion>,
    hint: Option<String>,
    resolutions: &'a [diagnostic::ResolutionCandidate],
    diagnostic_funnel: bool,
}

/// Mutable recovery state that the menu updates on the way out.
struct RecoveryOutcome<'a> {
    fix_history: &'a mut HashSet<String>,
    retry_count: &'a mut u32,
    skipped_by_user: &'a mut bool,
    aborted: &'a mut bool,
    final_result: &'a mut Option<StepResult>,
}

/// Minimal step identity needed by the recovery menu (no `step_number` here).
struct RecoveryStep<'a> {
    step: &'a ResolvedStep,
    name: &'a str,
    indent: usize,
}

/// Environment paths/env-vars required to run fix commands and debug shells.
struct RecoveryEnv<'a> {
    project_root: &'a Path,
    base_env: &'a HashMap<String, String>,
    process_env: &'a HashMap<String, String>,
}

/// Handle the interactive recovery menu after a step failure.
///
/// Sets `final_result` (via `outcome`) if the user chose to skip or abort.
/// Returns `Ok(())` to let the caller decide whether to continue or break
/// the execution loop.
fn handle_recovery_menu(
    target: RecoveryStep<'_>,
    failure: FailureContext<'_>,
    outcome: &mut RecoveryOutcome<'_>,
    env: RecoveryEnv<'_>,
    run_ui: StepRunUi<'_>,
) -> Result<()> {
    let RecoveryStep {
        step,
        name: step_name,
        indent: step_indent,
    } = target;
    let RecoveryEnv {
        project_root,
        base_env,
        process_env,
    } = env;
    let FailureContext {
        result,
        combined_output,
        fix,
        hint,
        resolutions,
        diagnostic_funnel,
    } = failure;
    let StepRunUi {
        ui,
        step_display,
        event_bus,
    } = run_ui;

    let pad = " ".repeat(step_indent);
    event_bus.emit(&BivvyEvent::RecoveryStarted {
        step: step_name.to_string(),
        error: combined_output.to_string(),
    });
    let has_hint = hint.is_some();
    loop {
        let action = if diagnostic_funnel {
            recovery::prompt_recovery_multi(
                ui,
                step_name,
                resolutions,
                outcome.fix_history,
                step_indent,
            )?
        } else {
            recovery::prompt_recovery(
                ui,
                step_name,
                fix.as_ref(),
                has_hint,
                outcome.fix_history,
                step_indent,
            )?
        };

        match action {
            RecoveryAction::Retry => {
                event_bus.emit(&BivvyEvent::RecoveryActionTaken {
                    step: step_name.to_string(),
                    action: "retry".to_string(),
                    command: None,
                });
                *outcome.retry_count += 1;
                return Ok(());
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
                    let fix_ok = recovery::run_fix(&cmd, project_root, &step.env_vars.env)?;
                    outcome.fix_history.insert(cmd.clone());
                    if fix_ok {
                        step_display.message(&format!("{}Fix command succeeded.", pad));
                    } else {
                        step_display.message(&format!("{}Fix command failed.", pad));
                    }
                    *outcome.retry_count += 1;
                    return Ok(());
                }
                // User declined the fix — re-show recovery menu
            }
            RecoveryAction::Shell => {
                event_bus.emit(&BivvyEvent::RecoveryActionTaken {
                    step: step_name.to_string(),
                    action: "shell".to_string(),
                    command: None,
                });
                step_display.message(&format!(
                    "{}Dropping to debug shell (exit to return)...",
                    pad
                ));
                let debug_env =
                    crate::steps::build_step_env(step, project_root, base_env, process_env)?;
                crate::shell::debug::spawn_debug_shell(step_name, project_root, &debug_env)?;
                // After shell exit, re-show recovery menu
            }
            RecoveryAction::Skip => {
                event_bus.emit(&BivvyEvent::RecoveryActionTaken {
                    step: step_name.to_string(),
                    action: "skip".to_string(),
                    command: None,
                });
                *outcome.skipped_by_user = true;
                let mut r = result;
                r.recovery_detail = Some("skipped by user after failure".to_string());
                *outcome.final_result = Some(r);
                return Ok(());
            }
            RecoveryAction::Abort => {
                event_bus.emit(&BivvyEvent::RecoveryActionTaken {
                    step: step_name.to_string(),
                    action: "abort".to_string(),
                    command: None,
                });
                *outcome.aborted = true;
                let mut r = result;
                r.recovery_detail = Some("aborted by user".to_string());
                *outcome.final_result = Some(r);
                return Ok(());
            }
        }
    }
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
