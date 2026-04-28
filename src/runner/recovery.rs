//! Interactive recovery for step failures.
//!
//! When a step fails during `bivvy run`, this module provides a recovery menu
//! allowing the user to retry, fix, skip, open a debug shell, enter a custom
//! fix command, or abort.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::error::Result;
use crate::shell::{execute_streaming, CommandOptions, OutputCallback};
use crate::ui::{FixOutputSink, OutputWriter, Prompt, PromptOption, PromptType, Prompter};

use super::diagnostic::ResolutionCandidate;
use super::patterns::FixSuggestion;

/// Recovery action chosen by the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryAction {
    /// Re-execute the step.
    Retry,
    /// Run the suggested fix command, then re-execute.
    Fix(String),
    /// Run a user-entered custom fix command, then re-execute.
    CustomFix(String),
    /// Skip this step and continue.
    Skip,
    /// Open a debug shell, then return to the recovery menu.
    Shell,
    /// Stop the workflow.
    Abort,
}

/// Prompt the user for a recovery action after a step failure.
///
/// Returns the chosen action. The `fix` parameter controls whether a Fix
/// option appears in the menu. The `has_hint` flag indicates a low-confidence
/// match exists, which adds a "Fix (custom)" option for the user to enter
/// their own command. The `fix_history` set tracks which fix commands have
/// already been attempted — if the suggested fix was already tried, it is
/// not offered again.
pub fn prompt_recovery(
    ui: &mut (impl Prompter + OutputWriter + ?Sized),
    step_name: &str,
    fix: Option<&FixSuggestion>,
    has_hint: bool,
    fix_history: &HashSet<String>,
) -> Result<RecoveryAction> {
    let mut options = vec![PromptOption {
        label: "Retry".to_string(),
        value: "retry".to_string(),
    }];

    if let Some(f) = fix {
        if fix_history.contains(&f.command) {
            ui.warning(&format!(
                "    Previous fix `{}` did not resolve this error.",
                f.command
            ));
        } else {
            options.push(PromptOption {
                label: format!("Fix — {}", f.label),
                value: "fix".to_string(),
            });
        }
    }

    if has_hint {
        options.push(PromptOption {
            label: "Fix (custom) — enter your own command".to_string(),
            value: "custom_fix".to_string(),
        });
    }

    options.push(PromptOption {
        label: "Skip".to_string(),
        value: "skip".to_string(),
    });

    options.push(PromptOption {
        label: "Shell".to_string(),
        value: "shell".to_string(),
    });

    options.push(PromptOption {
        label: "Abort".to_string(),
        value: "abort".to_string(),
    });

    let prompt = Prompt {
        key: format!("recovery_{}", step_name),
        question: "How do you want to proceed?".to_string(),
        prompt_type: PromptType::Select { options },
        default: Some("retry".to_string()),
    };

    let answer = ui.prompt(&prompt)?;
    let value = answer.as_string();

    match value.as_str() {
        "retry" => Ok(RecoveryAction::Retry),
        "fix" => {
            if let Some(f) = fix {
                Ok(RecoveryAction::Fix(f.command.clone()))
            } else {
                Ok(RecoveryAction::Retry)
            }
        }
        "custom_fix" => {
            let input_prompt = Prompt {
                key: format!("custom_fix_{}", step_name),
                question: "Enter fix command:".to_string(),
                prompt_type: PromptType::Input,
                default: None,
            };
            let input = ui.prompt(&input_prompt)?;
            let cmd = input.as_string();
            if cmd.trim().is_empty() {
                // User entered nothing, treat as retry
                Ok(RecoveryAction::Retry)
            } else {
                Ok(RecoveryAction::CustomFix(cmd))
            }
        }
        "skip" => Ok(RecoveryAction::Skip),
        "shell" => Ok(RecoveryAction::Shell),
        "abort" => Ok(RecoveryAction::Abort),
        _ => Ok(RecoveryAction::Retry),
    }
}

/// Prompt the user for a recovery action using ranked diagnostic resolutions.
///
/// Shows up to 3 high-confidence fixes (>= 0.6) and up to 2 suggestions
/// (0.3–0.59). Resolutions already in `fix_history` are demoted with a
/// warning instead of being offered again.
pub fn prompt_recovery_multi(
    ui: &mut (impl Prompter + OutputWriter + ?Sized),
    step_name: &str,
    resolutions: &[ResolutionCandidate],
    fix_history: &HashSet<String>,
) -> Result<RecoveryAction> {
    let mut options = vec![PromptOption {
        label: "Retry".to_string(),
        value: "retry".to_string(),
    }];

    // Collect actionable resolutions (with runnable commands)
    let mut fix_count = 0;
    let mut suggestion_count = 0;
    let mut fix_commands: Vec<String> = Vec::new();

    for resolution in resolutions {
        let cmd = match &resolution.command {
            Some(c) => c,
            None => continue,
        };

        if fix_history.contains(cmd) {
            ui.warning(&format!(
                "    Previous fix `{}` did not resolve this error.",
                cmd
            ));
            continue;
        }

        if resolution.confidence >= 0.6 && fix_count < 3 {
            options.push(PromptOption {
                label: format!("Fix — {}", resolution.label),
                value: format!("fix_{}", fix_count),
            });
            fix_commands.push(cmd.clone());
            fix_count += 1;
        } else if resolution.confidence >= 0.3 && suggestion_count < 2 {
            options.push(PromptOption {
                label: format!("Suggestion — {}", resolution.label),
                value: format!("fix_{}", fix_commands.len()),
            });
            fix_commands.push(cmd.clone());
            suggestion_count += 1;
        }
    }

    // Custom fix is available when any resolution exists
    let has_any = !resolutions.is_empty();
    if has_any {
        options.push(PromptOption {
            label: "Fix (custom) — enter your own command".to_string(),
            value: "custom_fix".to_string(),
        });
    }

    options.push(PromptOption {
        label: "Skip".to_string(),
        value: "skip".to_string(),
    });
    options.push(PromptOption {
        label: "Shell".to_string(),
        value: "shell".to_string(),
    });
    options.push(PromptOption {
        label: "Abort".to_string(),
        value: "abort".to_string(),
    });

    let prompt = Prompt {
        key: format!("recovery_{}", step_name),
        question: "How do you want to proceed?".to_string(),
        prompt_type: PromptType::Select { options },
        default: Some("retry".to_string()),
    };

    let answer = ui.prompt(&prompt)?;
    let value = answer.as_string();

    match value.as_str() {
        "retry" => Ok(RecoveryAction::Retry),
        "custom_fix" => {
            let input_prompt = Prompt {
                key: format!("custom_fix_{}", step_name),
                question: "Enter fix command:".to_string(),
                prompt_type: PromptType::Input,
                default: None,
            };
            let input = ui.prompt(&input_prompt)?;
            let cmd = input.as_string();
            if cmd.trim().is_empty() {
                Ok(RecoveryAction::Retry)
            } else {
                Ok(RecoveryAction::CustomFix(cmd))
            }
        }
        "skip" => Ok(RecoveryAction::Skip),
        "shell" => Ok(RecoveryAction::Shell),
        "abort" => Ok(RecoveryAction::Abort),
        other => {
            // Parse "fix_N" to get the command
            if let Some(idx_str) = other.strip_prefix("fix_") {
                if let Ok(idx) = idx_str.parse::<usize>() {
                    if let Some(cmd) = fix_commands.get(idx) {
                        return Ok(RecoveryAction::Fix(cmd.clone()));
                    }
                }
            }
            Ok(RecoveryAction::Retry)
        }
    }
}

/// Ask user to confirm running a fix command before executing it.
pub fn confirm_fix(ui: &mut dyn Prompter, step_name: &str, command: &str) -> Result<bool> {
    let prompt = Prompt {
        key: format!("confirm_fix_{}", step_name),
        question: format!("Run `{}`?", command),
        prompt_type: PromptType::Confirm,
        default: Some("yes".to_string()),
    };

    let answer = ui.prompt(&prompt)?;
    Ok(answer.as_bool().unwrap_or(true))
}

/// Execute a fix command with the step's environment and working directory.
///
/// Output is streamed through `execute_streaming` with each line prefixed
/// by `"    fix: "` for consistent TUI formatting. After the command
/// completes, any buffered terminal input is drained to prevent queued
/// keypresses from triggering unintended recovery menu actions.
pub fn run_fix(command: &str, project_root: &Path, env: &HashMap<String, String>) -> Result<bool> {
    let options = CommandOptions {
        cwd: Some(project_root.to_path_buf()),
        env: env.clone(),
        ..Default::default()
    };

    let callback: OutputCallback = Box::new(FixOutputSink);

    let result = execute_streaming(command, &options, callback)?;

    // Drain buffered terminal input to prevent queued keypresses
    // from triggering unintended recovery menu actions.
    #[cfg(unix)]
    crate::shell::command::drain_input();

    Ok(result.success)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::patterns::{Confidence, FixSuggestion};
    use crate::ui::MockUI;

    #[test]
    fn prompt_recovery_default_is_retry() {
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        // No response set — falls back to default "retry"
        let action = prompt_recovery(&mut ui, "bundler", None, false, &HashSet::new()).unwrap();
        assert_eq!(action, RecoveryAction::Retry);
    }

    #[test]
    fn prompt_recovery_includes_fix_when_high_confidence() {
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        ui.set_prompt_response("recovery_bundler", "fix");

        let fix = FixSuggestion {
            label: "bundle update nokogiri".to_string(),
            command: "bundle update nokogiri".to_string(),
            explanation: "Native ext build failed".to_string(),
            confidence: Confidence::High,
        };

        let action =
            prompt_recovery(&mut ui, "bundler", Some(&fix), false, &HashSet::new()).unwrap();
        assert_eq!(
            action,
            RecoveryAction::Fix("bundle update nokogiri".to_string())
        );
    }

    #[test]
    fn prompt_recovery_excludes_fix_when_no_match() {
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        ui.set_prompt_response("recovery_bundler", "skip");

        let action = prompt_recovery(&mut ui, "bundler", None, false, &HashSet::new()).unwrap();
        assert_eq!(action, RecoveryAction::Skip);
    }

    #[test]
    fn prompt_recovery_abort() {
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        ui.set_prompt_response("recovery_test", "abort");

        let action = prompt_recovery(&mut ui, "test", None, false, &HashSet::new()).unwrap();
        assert_eq!(action, RecoveryAction::Abort);
    }

    #[test]
    fn prompt_recovery_shell() {
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        ui.set_prompt_response("recovery_test", "shell");

        let action = prompt_recovery(&mut ui, "test", None, false, &HashSet::new()).unwrap();
        assert_eq!(action, RecoveryAction::Shell);
    }

    #[test]
    fn prompt_recovery_custom_fix_when_hint_present() {
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        ui.queue_prompt_responses("recovery_test", vec!["custom_fix"]);
        ui.set_prompt_response("custom_fix_test", "chmod +x script.sh");

        let action = prompt_recovery(&mut ui, "test", None, true, &HashSet::new()).unwrap();
        assert_eq!(
            action,
            RecoveryAction::CustomFix("chmod +x script.sh".to_string())
        );
    }

    #[test]
    fn prompt_recovery_custom_fix_empty_falls_back_to_retry() {
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        ui.queue_prompt_responses("recovery_test", vec!["custom_fix"]);
        ui.set_prompt_response("custom_fix_test", "");

        let action = prompt_recovery(&mut ui, "test", None, true, &HashSet::new()).unwrap();
        assert_eq!(action, RecoveryAction::Retry);
    }

    #[test]
    fn confirm_fix_true_on_yes() {
        let mut ui = MockUI::new();
        ui.set_prompt_response("confirm_fix_bundler", "yes");

        let result = confirm_fix(&mut ui, "bundler", "bundle update nokogiri").unwrap();
        assert!(result);
    }

    #[test]
    fn confirm_fix_false_on_no() {
        let mut ui = MockUI::new();
        ui.set_prompt_response("confirm_fix_bundler", "no");

        let result = confirm_fix(&mut ui, "bundler", "bundle update nokogiri").unwrap();
        assert!(!result);
    }

    #[test]
    fn run_fix_returns_success() {
        let temp = tempfile::TempDir::new().unwrap();
        let result = run_fix("true", temp.path(), &HashMap::new()).unwrap();
        assert!(result);
    }

    #[test]
    fn run_fix_returns_failure() {
        let temp = tempfile::TempDir::new().unwrap();
        let result = run_fix("false", temp.path(), &HashMap::new()).unwrap();
        assert!(!result);
    }

    #[test]
    fn prompt_recovery_skips_fix_already_in_history() {
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        // When "fix" is not an option (already in history), the default "retry" is returned
        ui.set_prompt_response("recovery_bundler", "retry");

        let fix = FixSuggestion {
            label: "bundle update nokogiri".to_string(),
            command: "bundle update nokogiri".to_string(),
            explanation: "Native ext build failed".to_string(),
            confidence: Confidence::High,
        };

        let mut history = HashSet::new();
        history.insert("bundle update nokogiri".to_string());

        let action = prompt_recovery(&mut ui, "bundler", Some(&fix), false, &history).unwrap();
        // Fix was in history, so "fix" option was not offered and default "retry" is returned
        assert_eq!(action, RecoveryAction::Retry);
    }

    // --- prompt_recovery_multi tests ---

    fn make_resolution(label: &str, command: &str, confidence: f32) -> ResolutionCandidate {
        ResolutionCandidate {
            label: label.to_string(),
            command: Some(command.to_string()),
            explanation: format!("Explanation for {}", label),
            confidence,
            source: crate::runner::diagnostic::ResolutionSource::Deduced,
            platform: None,
        }
    }

    #[test]
    fn prompt_recovery_multi_default_is_retry() {
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        let action = prompt_recovery_multi(&mut ui, "test_step", &[], &HashSet::new()).unwrap();
        assert_eq!(action, RecoveryAction::Retry);
    }

    #[test]
    fn prompt_recovery_multi_selects_first_fix() {
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        ui.set_prompt_response("recovery_db_prepare", "fix_0");

        let resolutions = vec![
            make_resolution("update pg_dump", "brew install postgresql@16", 0.7),
            make_resolution("fix PATH", "export PATH=...", 0.65),
        ];

        let action =
            prompt_recovery_multi(&mut ui, "db_prepare", &resolutions, &HashSet::new()).unwrap();
        assert_eq!(
            action,
            RecoveryAction::Fix("brew install postgresql@16".to_string())
        );
    }

    #[test]
    fn prompt_recovery_multi_selects_second_fix() {
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        ui.set_prompt_response("recovery_db_prepare", "fix_1");

        let resolutions = vec![
            make_resolution("update pg_dump", "brew install postgresql@16", 0.7),
            make_resolution(
                "fix PATH",
                "export PATH=/opt/homebrew/opt/postgresql@16/bin:$PATH",
                0.65,
            ),
        ];

        let action =
            prompt_recovery_multi(&mut ui, "db_prepare", &resolutions, &HashSet::new()).unwrap();
        assert_eq!(
            action,
            RecoveryAction::Fix(
                "export PATH=/opt/homebrew/opt/postgresql@16/bin:$PATH".to_string()
            )
        );
    }

    #[test]
    fn prompt_recovery_multi_limits_fixes_to_three() {
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        // Try to select the 4th fix: only 3 are offered (fix_0..fix_2),
        // so the 4th high-confidence resolution overflows to suggestion (fix_3).
        // We verify the first 3 are fixes and the 4th is available as suggestion.
        ui.set_prompt_response("recovery_test", "fix_3");

        let resolutions = vec![
            make_resolution("fix1", "cmd1", 0.9),
            make_resolution("fix2", "cmd2", 0.8),
            make_resolution("fix3", "cmd3", 0.7),
            make_resolution("fix4", "cmd4", 0.65), // 4th high-confidence → becomes suggestion
        ];

        let action = prompt_recovery_multi(&mut ui, "test", &resolutions, &HashSet::new()).unwrap();
        // fix_3 maps to the overflowed suggestion
        assert_eq!(action, RecoveryAction::Fix("cmd4".to_string()));
    }

    #[test]
    fn prompt_recovery_multi_caps_total_options() {
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        // 3 fixes + 2 suggestions = 5 total. 6th should not be offered.
        ui.set_prompt_response("recovery_test", "fix_5");

        let resolutions = vec![
            make_resolution("fix1", "cmd1", 0.9),
            make_resolution("fix2", "cmd2", 0.8),
            make_resolution("fix3", "cmd3", 0.7),
            make_resolution("sug1", "cmd4", 0.5),
            make_resolution("sug2", "cmd5", 0.4),
            make_resolution("sug3", "cmd6", 0.35), // Should be capped
        ];

        let action = prompt_recovery_multi(&mut ui, "test", &resolutions, &HashSet::new()).unwrap();
        // fix_5 doesn't exist, falls through to Retry
        assert_eq!(action, RecoveryAction::Retry);
    }

    #[test]
    fn prompt_recovery_multi_shows_suggestions_for_medium_confidence() {
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        // fix_0 is the suggestion (medium confidence, no high-confidence fixes)
        ui.set_prompt_response("recovery_test", "fix_0");

        let resolutions = vec![make_resolution(
            "check requirements.txt",
            "pip install -r requirements.txt",
            0.45,
        )];

        let action = prompt_recovery_multi(&mut ui, "test", &resolutions, &HashSet::new()).unwrap();
        assert_eq!(
            action,
            RecoveryAction::Fix("pip install -r requirements.txt".to_string())
        );
    }

    #[test]
    fn prompt_recovery_multi_skips_fix_in_history() {
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        // Only retry is available since the fix is in history
        ui.set_prompt_response("recovery_test", "retry");

        let resolutions = vec![make_resolution("install pkg", "apt install pkg", 0.8)];

        let mut history = HashSet::new();
        history.insert("apt install pkg".to_string());

        let action = prompt_recovery_multi(&mut ui, "test", &resolutions, &history).unwrap();
        assert_eq!(action, RecoveryAction::Retry);
    }

    #[test]
    fn prompt_recovery_multi_custom_fix() {
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        ui.queue_prompt_responses("recovery_test", vec!["custom_fix"]);
        ui.set_prompt_response("custom_fix_test", "my custom command");

        let resolutions = vec![make_resolution("some fix", "some cmd", 0.7)];

        let action = prompt_recovery_multi(&mut ui, "test", &resolutions, &HashSet::new()).unwrap();
        assert_eq!(
            action,
            RecoveryAction::CustomFix("my custom command".to_string())
        );
    }

    #[test]
    fn prompt_recovery_multi_skip() {
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        ui.set_prompt_response("recovery_test", "skip");

        let resolutions = vec![make_resolution("fix", "cmd", 0.7)];

        let action = prompt_recovery_multi(&mut ui, "test", &resolutions, &HashSet::new()).unwrap();
        assert_eq!(action, RecoveryAction::Skip);
    }

    #[test]
    fn prompt_recovery_multi_abort() {
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        ui.set_prompt_response("recovery_test", "abort");

        let resolutions = vec![make_resolution("fix", "cmd", 0.7)];

        let action = prompt_recovery_multi(&mut ui, "test", &resolutions, &HashSet::new()).unwrap();
        assert_eq!(action, RecoveryAction::Abort);
    }

    #[test]
    fn prompt_recovery_multi_no_custom_fix_without_resolutions() {
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        // With no resolutions, "custom_fix" shouldn't be offered
        ui.set_prompt_response("recovery_test", "skip");

        let action = prompt_recovery_multi(&mut ui, "test", &[], &HashSet::new()).unwrap();
        assert_eq!(action, RecoveryAction::Skip);
    }

    #[test]
    fn prompt_recovery_multi_advisory_only_resolutions_show_custom_fix() {
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        ui.queue_prompt_responses("recovery_test", vec!["custom_fix"]);
        ui.set_prompt_response("custom_fix_test", "manual fix");

        // Advisory-only resolution (no command) — still enables custom fix option
        let resolutions = vec![ResolutionCandidate {
            label: "check service logs".to_string(),
            command: None,
            explanation: "The service may have crashed".to_string(),
            confidence: 0.5,
            source: crate::runner::diagnostic::ResolutionSource::Deduced,
            platform: None,
        }];

        let action = prompt_recovery_multi(&mut ui, "test", &resolutions, &HashSet::new()).unwrap();
        assert_eq!(action, RecoveryAction::CustomFix("manual fix".to_string()));
    }
}
