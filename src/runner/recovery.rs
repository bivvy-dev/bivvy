//! Interactive recovery for step failures.
//!
//! When a step fails during `bivvy run`, this module provides a recovery menu
//! allowing the user to retry, fix, skip, open a debug shell, enter a custom
//! fix command, or abort.

use std::collections::HashMap;
use std::path::Path;

use crate::error::Result;
use crate::shell::{execute, CommandOptions};
use crate::ui::{Prompt, PromptOption, PromptType, UserInterface};

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
/// their own command.
pub fn prompt_recovery(
    ui: &mut dyn UserInterface,
    step_name: &str,
    fix: Option<&FixSuggestion>,
    has_hint: bool,
) -> Result<RecoveryAction> {
    let mut options = vec![PromptOption {
        label: "Retry".to_string(),
        value: "retry".to_string(),
    }];

    if let Some(f) = fix {
        options.push(PromptOption {
            label: format!("Fix — {}", f.label),
            value: "fix".to_string(),
        });
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

/// Ask user to confirm running a fix command before executing it.
pub fn confirm_fix(ui: &mut dyn UserInterface, step_name: &str, command: &str) -> Result<bool> {
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
pub fn run_fix(command: &str, project_root: &Path, env: &HashMap<String, String>) -> Result<bool> {
    let options = CommandOptions {
        cwd: Some(project_root.to_path_buf()),
        env: env.clone(),
        capture_stdout: false,
        capture_stderr: false,
        ..Default::default()
    };

    let result = execute(command, &options)?;
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
        let action = prompt_recovery(&mut ui, "bundler", None, false).unwrap();
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

        let action = prompt_recovery(&mut ui, "bundler", Some(&fix), false).unwrap();
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

        let action = prompt_recovery(&mut ui, "bundler", None, false).unwrap();
        assert_eq!(action, RecoveryAction::Skip);
    }

    #[test]
    fn prompt_recovery_abort() {
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        ui.set_prompt_response("recovery_test", "abort");

        let action = prompt_recovery(&mut ui, "test", None, false).unwrap();
        assert_eq!(action, RecoveryAction::Abort);
    }

    #[test]
    fn prompt_recovery_shell() {
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        ui.set_prompt_response("recovery_test", "shell");

        let action = prompt_recovery(&mut ui, "test", None, false).unwrap();
        assert_eq!(action, RecoveryAction::Shell);
    }

    #[test]
    fn prompt_recovery_custom_fix_when_hint_present() {
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        ui.queue_prompt_responses("recovery_test", vec!["custom_fix"]);
        ui.set_prompt_response("custom_fix_test", "chmod +x script.sh");

        let action = prompt_recovery(&mut ui, "test", None, true).unwrap();
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

        let action = prompt_recovery(&mut ui, "test", None, true).unwrap();
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
}
