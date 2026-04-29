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
use crate::steps::{execute_step, ExecutionOptions, ResolvedStep, StepResult, StepStatus};
use crate::ui::spinner::live_output_callback;
use crate::ui::theme::BivvyTheme;
use crate::ui::{format_duration, OutputMode, Prompt, PromptOption, PromptType, UserInterface};

use super::diagnostic;
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

/// Execute a step with retry and interactive recovery.
///
/// This handles the full execution lifecycle: spinner display, output capture,
/// auto-retries, and the interactive recovery menu (retry/fix/shell/skip/abort).
#[allow(clippy::too_many_arguments)]
pub(super) fn execute_step_with_recovery(
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
    diagnostic_funnel: bool,
    workflow_state: &diagnostic::WorkflowState<'_>,
    ui: &mut dyn UserInterface,
    event_bus: &mut EventBus,
) -> Result<StepExecutionResult> {
    let theme = BivvyTheme::new();

    let mut retry_count: u32 = 0;
    let mut fix_history: HashSet<String> = HashSet::new();
    let mut skipped_by_user = false;
    let mut aborted = false;
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
                // Non-interactive verbose: stream output through VerboseStreamSink
                if output_mode == OutputMode::Verbose {
                    let cb: crate::shell::OutputCallback =
                        Box::new(crate::ui::VerboseStreamSink::new(6));
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
                    !ui.is_interactive() && output_mode == OutputMode::Verbose;
                if !output_was_streamed {
                    ui.show_error_block(
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
                    ui.message(&format!(
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
                handle_recovery_menu(
                    step,
                    step_name,
                    step_indent,
                    result,
                    &combined_output,
                    fix,
                    hint,
                    &resolutions,
                    diagnostic_funnel,
                    &mut fix_history,
                    &mut retry_count,
                    &mut skipped_by_user,
                    &mut aborted,
                    &mut final_result,
                    project_root,
                    global_env,
                    ui,
                    event_bus,
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

/// Handle the interactive recovery menu after a step failure.
///
/// Sets `final_result` if the user chose to skip or abort. Returns `Ok(())`
/// to let the caller decide whether to continue or break the execution loop.
#[allow(clippy::too_many_arguments)]
fn handle_recovery_menu(
    step: &ResolvedStep,
    step_name: &str,
    step_indent: usize,
    result: StepResult,
    combined_output: &str,
    fix: Option<FixSuggestion>,
    hint: Option<String>,
    resolutions: &[diagnostic::ResolutionCandidate],
    diagnostic_funnel: bool,
    fix_history: &mut HashSet<String>,
    retry_count: &mut u32,
    skipped_by_user: &mut bool,
    aborted: &mut bool,
    final_result: &mut Option<StepResult>,
    project_root: &Path,
    global_env: &HashMap<String, String>,
    ui: &mut dyn UserInterface,
    event_bus: &mut EventBus,
) -> Result<()> {
    let pad = " ".repeat(step_indent);
    event_bus.emit(&BivvyEvent::RecoveryStarted {
        step: step_name.to_string(),
        error: combined_output.to_string(),
    });
    let has_hint = hint.is_some();
    loop {
        let action = if diagnostic_funnel {
            recovery::prompt_recovery_multi(ui, step_name, resolutions, fix_history, step_indent)?
        } else {
            recovery::prompt_recovery(
                ui,
                step_name,
                fix.as_ref(),
                has_hint,
                fix_history,
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
                *retry_count += 1;
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
                    fix_history.insert(cmd.clone());
                    if fix_ok {
                        ui.message(&format!("{}Fix command succeeded.", pad));
                    } else {
                        ui.message(&format!("{}Fix command failed.", pad));
                    }
                    *retry_count += 1;
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
                ui.message(&format!(
                    "{}Dropping to debug shell (exit to return)...",
                    pad
                ));
                crate::shell::debug::spawn_debug_shell(
                    step_name,
                    project_root,
                    &step.env_vars.env,
                    global_env,
                )?;
                // After shell exit, re-show recovery menu
            }
            RecoveryAction::Skip => {
                event_bus.emit(&BivvyEvent::RecoveryActionTaken {
                    step: step_name.to_string(),
                    action: "skip".to_string(),
                    command: None,
                });
                *skipped_by_user = true;
                let mut r = result;
                r.recovery_detail = Some("skipped by user after failure".to_string());
                *final_result = Some(r);
                return Ok(());
            }
            RecoveryAction::Abort => {
                event_bus.emit(&BivvyEvent::RecoveryActionTaken {
                    step: step_name.to_string(),
                    action: "abort".to_string(),
                    command: None,
                });
                *aborted = true;
                let mut r = result;
                r.recovery_detail = Some("aborted by user".to_string());
                *final_result = Some(r);
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
