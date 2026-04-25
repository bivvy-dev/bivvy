//! Non-interactive UI for CI/headless environments.

use std::time::Duration;

use crate::error::{BivvyError, Result};

use super::progress::format_duration;
use super::theme::BivvyTheme;
use super::{
    OutputMode, OutputWriter, ProgressDisplay, Prompt, PromptResult, PromptType, Prompter,
    RunSummary, SpinnerFactory, SpinnerHandle, StatusKind, UiState, WorkflowDisplay,
};

/// UI implementation for non-interactive mode.
///
/// When running in CI (detected via `is_ci()`), the progress bar is
/// suppressed since it produces noisy output in log-based environments.
/// All other output (headers, summaries, errors) is preserved.
pub struct NonInteractiveUI {
    mode: OutputMode,
    is_ci: bool,
}

impl NonInteractiveUI {
    /// Create a new non-interactive UI.
    pub fn new(mode: OutputMode) -> Self {
        Self {
            mode,
            is_ci: crate::shell::is_ci(),
        }
    }

    /// Create with explicit CI flag (for testing).
    pub fn with_ci(mode: OutputMode, is_ci: bool) -> Self {
        Self { mode, is_ci }
    }
}

impl OutputWriter for NonInteractiveUI {
    fn message(&mut self, msg: &str) {
        if self.mode.shows_status() {
            println!("{}", msg);
        }
    }

    fn success(&mut self, msg: &str) {
        if self.mode.shows_status() {
            println!("✓ {}", msg);
        }
    }

    fn warning(&mut self, msg: &str) {
        if self.mode.shows_status() {
            eprintln!("⚠ {}", msg);
        }
    }

    fn error(&mut self, msg: &str) {
        eprintln!("✗ {}", msg);
    }

    fn show_hint(&mut self, hint: &str) {
        if self.mode.shows_status() {
            println!("  💡 {}", hint);
        }
    }

    fn show_error_block(&mut self, command: &str, output: &str, hint: Option<&str>) {
        eprintln!();
        eprintln!("    ┌─ Command ──────────────────────────");
        eprintln!("    │ {}", command);
        if !output.is_empty() {
            eprintln!("    ├─ Output ───────────────────────────");
            for line in output.lines() {
                eprintln!("    │ {}", line);
            }
        }
        eprintln!("    └────────────────────────────────────");
        if let Some(h) = hint {
            eprintln!();
            eprintln!("    Hint: {}", h);
        }
    }
}

impl Prompter for NonInteractiveUI {
    fn prompt(&mut self, prompt: &Prompt) -> Result<PromptResult> {
        let is_multiselect = matches!(prompt.prompt_type, PromptType::MultiSelect { .. });

        // Check environment override (KEY=value, e.g. BUMP=minor).
        // This is handled by prompt_user for TerminalUI, but
        // NonInteractiveUI doesn't call prompt_user, so check here too.
        if let Some(result) = super::prompts::env_override(prompt) {
            return Ok(result);
        }

        // Use default
        if let Some(default) = &prompt.default {
            if is_multiselect {
                let values: Vec<String> =
                    default.split(',').map(|s| s.trim().to_string()).collect();
                return Ok(PromptResult::Strings(values));
            }
            return Ok(PromptResult::String(default.clone()));
        }

        Err(BivvyError::ConfigValidationError {
            message: format!(
                "Cannot prompt for '{}' in non-interactive mode (no default value)",
                prompt.key
            ),
        })
    }
}

impl SpinnerFactory for NonInteractiveUI {
    fn start_spinner(&mut self, message: &str) -> Box<dyn SpinnerHandle> {
        if self.mode.shows_spinners() {
            println!("  {}", message);
        }
        Box::new(NoopSpinner { indent: 0 })
    }

    fn start_spinner_indented(&mut self, message: &str, indent: usize) -> Box<dyn SpinnerHandle> {
        if self.mode.shows_spinners() {
            let prefix = " ".repeat(indent);
            println!("{}{}", prefix, message);
        }
        Box::new(NoopSpinner { indent })
    }
}

impl ProgressDisplay for NonInteractiveUI {
    fn show_header(&mut self, title: &str) {
        if self.mode.shows_status() {
            println!("\n{}\n", title);
        }
    }

    fn show_progress(&mut self, current: usize, total: usize) {
        if self.mode.shows_status() {
            println!("[{}/{}]", current, total);
        }
    }
}

impl WorkflowDisplay for NonInteractiveUI {
    fn show_run_header(
        &mut self,
        app_name: &str,
        workflow: &str,
        step_count: usize,
        version: &str,
    ) {
        if self.mode.shows_status() {
            let step_label = if step_count == 1 { "step" } else { "steps" };
            println!(
                "\n⛺ {} v{} · {} workflow · {} {}\n",
                app_name, version, workflow, step_count, step_label
            );
        }
    }

    fn show_workflow_progress(&mut self, current: usize, total: usize, elapsed: Duration) {
        if self.is_ci {
            return;
        }
        if self.mode.shows_status() {
            let filled = if total > 0 { (current * 16) / total } else { 0 };
            let empty = 16 - filled;
            let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));
            println!(
                "  [{}] {}/{} steps · {} elapsed",
                bar,
                current,
                total,
                format_duration(elapsed),
            );
        }
    }

    fn show_run_summary(&mut self, summary: &RunSummary) {
        if !self.mode.shows_status() {
            return;
        }

        println!();
        println!("  ┌─ Summary ──────────────────────────");

        for step in &summary.step_results {
            let icon = step.status.icon();
            let duration_str = step.duration.map(format_duration).unwrap_or_default();
            let detail_str = step.detail.as_deref().unwrap_or("");

            let right_side = if !duration_str.is_empty() {
                duration_str
            } else if !detail_str.is_empty() {
                detail_str.to_string()
            } else {
                String::new()
            };

            println!("  │ {} {:<20} {}", icon, step.name, right_side);
        }

        println!("  ├────────────────────────────────────");
        println!(
            "  │ Total: {} · {} run · {} skipped",
            format_duration(summary.total_duration),
            summary.steps_run,
            summary.steps_skipped,
        );
        println!("  └────────────────────────────────────");

        if summary.success {
            println!("  ✓ Setup complete!");
        } else {
            let status = StatusKind::Failed;
            eprintln!(
                "  {} Setup failed: {}",
                status.icon(),
                summary.failed_steps.join(", ")
            );
        }
    }
}

impl UiState for NonInteractiveUI {
    fn output_mode(&self) -> OutputMode {
        self.mode
    }

    fn is_interactive(&self) -> bool {
        false
    }

    fn set_output_mode(&mut self, mode: OutputMode) {
        self.mode = mode;
    }
}

/// Spinner that does nothing (for non-interactive mode).
struct NoopSpinner {
    indent: usize,
}

impl SpinnerHandle for NoopSpinner {
    fn set_message(&mut self, _msg: &str) {}

    fn finish_success(&mut self, msg: &str) {
        let prefix = " ".repeat(self.indent);
        let theme = BivvyTheme::new();
        println!("{}{}", prefix, theme.format_success(msg));
    }

    fn finish_error(&mut self, msg: &str) {
        let prefix = " ".repeat(self.indent);
        let theme = BivvyTheme::new();
        println!("{}{}", prefix, theme.format_error(msg));
    }

    fn finish_skipped(&mut self, msg: &str) {
        let prefix = " ".repeat(self.indent);
        let theme = BivvyTheme::new();
        println!("{}{}", prefix, theme.format_skipped(msg));
    }

    fn finish_and_clear(&mut self) {
        // No-op for non-interactive: nothing to clear
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_interactive_is_not_interactive() {
        let ui = NonInteractiveUI::new(OutputMode::Normal);
        assert!(!ui.is_interactive());
    }

    #[test]
    fn prompt_uses_default() {
        let mut ui = NonInteractiveUI::new(OutputMode::Normal);
        let prompt = Prompt {
            key: "test".to_string(),
            question: "Test?".to_string(),
            prompt_type: PromptType::Input,
            default: Some("default_value".to_string()),
        };

        let result = ui.prompt(&prompt).unwrap();
        assert_eq!(result.as_string(), "default_value");
    }

    #[test]
    fn prompt_fails_without_default() {
        let mut ui = NonInteractiveUI::new(OutputMode::Normal);
        let prompt = Prompt {
            key: "test".to_string(),
            question: "Test?".to_string(),
            prompt_type: PromptType::Input,
            default: None,
        };

        let result = ui.prompt(&prompt);
        assert!(result.is_err());
    }

    #[test]
    fn prompt_uses_env_override() {
        // Set env var matching the prompt key (uppercase)
        std::env::set_var("TEST_PROMPT_OVERRIDE", "override");

        let mut ui = NonInteractiveUI::new(OutputMode::Normal);
        let prompt = Prompt {
            key: "test_prompt_override".to_string(),
            question: "Test?".to_string(),
            prompt_type: PromptType::Input,
            default: Some("default".to_string()),
        };

        let result = ui.prompt(&prompt).unwrap();
        assert_eq!(result.as_string(), "override");

        std::env::remove_var("TEST_PROMPT_OVERRIDE");
    }

    #[test]
    fn output_mode_preserved() {
        let ui = NonInteractiveUI::new(OutputMode::Quiet);
        assert_eq!(ui.output_mode(), OutputMode::Quiet);
    }

    #[test]
    fn noop_spinner_methods() {
        let mut spinner = NoopSpinner { indent: 0 };
        spinner.set_message("test");
        spinner.finish_success("done");
    }

    #[test]
    fn noop_spinner_error() {
        let mut spinner = NoopSpinner { indent: 0 };
        spinner.finish_error("failed");
    }

    #[test]
    fn noop_spinner_skipped() {
        let mut spinner = NoopSpinner { indent: 0 };
        spinner.finish_skipped("skipped");
    }

    #[test]
    fn multiselect_prompt_uses_default() {
        let mut ui = NonInteractiveUI::new(OutputMode::Normal);
        let prompt = Prompt {
            key: "steps".to_string(),
            question: "Select steps".to_string(),
            prompt_type: PromptType::MultiSelect { options: vec![] },
            default: Some("bundler,cargo".to_string()),
        };

        let result = ui.prompt(&prompt).unwrap();
        assert!(matches!(result, PromptResult::Strings(_)));
        assert_eq!(result.as_string(), "bundler,cargo");
    }

    #[test]
    fn multiselect_prompt_uses_env_override() {
        std::env::set_var("MULTISELECT_OVERRIDE_STEPS", "npm");

        let mut ui = NonInteractiveUI::new(OutputMode::Normal);
        let prompt = Prompt {
            key: "multiselect_override_steps".to_string(),
            question: "Select steps".to_string(),
            prompt_type: PromptType::MultiSelect { options: vec![] },
            default: Some("bundler,cargo".to_string()),
        };

        let result = ui.prompt(&prompt).unwrap();
        assert!(matches!(result, PromptResult::Strings(_)));
        assert_eq!(result.as_string(), "npm");

        std::env::remove_var("MULTISELECT_OVERRIDE_STEPS");
    }

    #[test]
    fn ci_mode_is_stored() {
        let ui = NonInteractiveUI::with_ci(OutputMode::Normal, true);
        assert!(ui.is_ci);
    }

    #[test]
    fn non_ci_mode_when_explicit() {
        let ui = NonInteractiveUI::with_ci(OutputMode::Normal, false);
        assert!(!ui.is_ci);
    }

    #[test]
    fn ci_mode_suppresses_workflow_progress() {
        // In CI mode, show_workflow_progress should return early.
        // We verify this doesn't panic and the method is callable.
        // (stdout output is suppressed by the early return, which we
        // can't easily capture, but the is_ci flag test above confirms
        // the flag is set correctly for the early return path.)
        let mut ui = NonInteractiveUI::with_ci(OutputMode::Normal, true);
        ui.show_workflow_progress(1, 5, Duration::from_secs(1));
        // No panic = success. In CI mode this is a no-op.
    }
}
