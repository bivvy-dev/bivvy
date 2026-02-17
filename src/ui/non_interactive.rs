//! Non-interactive UI for CI/headless environments.

use std::collections::HashMap;
use std::time::Duration;

use crate::error::{BivvyError, Result};

use super::progress::format_duration;
use super::theme::BivvyTheme;
use super::{
    OutputMode, Prompt, PromptResult, PromptType, RunSummary, SpinnerHandle, StatusKind,
    UserInterface,
};

/// UI implementation for non-interactive mode.
///
/// When running in CI (detected via `is_ci()`), the progress bar is
/// suppressed since it produces noisy output in log-based environments.
/// All other output (headers, summaries, errors) is preserved.
pub struct NonInteractiveUI {
    mode: OutputMode,
    env_overrides: HashMap<String, String>,
    is_ci: bool,
}

impl NonInteractiveUI {
    /// Create a new non-interactive UI.
    pub fn new(mode: OutputMode) -> Self {
        // Collect BIVVY_PROMPT_* env vars
        let env_overrides: HashMap<String, String> = std::env::vars()
            .filter(|(k, _)| k.starts_with("BIVVY_PROMPT_"))
            .collect();

        Self {
            mode,
            env_overrides,
            is_ci: crate::shell::is_ci(),
        }
    }

    /// Create with explicit overrides (for testing).
    pub fn with_overrides(mode: OutputMode, overrides: HashMap<String, String>) -> Self {
        Self {
            mode,
            env_overrides: overrides,
            is_ci: false,
        }
    }

    /// Create with explicit CI flag (for testing).
    pub fn with_ci(mode: OutputMode, is_ci: bool) -> Self {
        let env_overrides: HashMap<String, String> = std::env::vars()
            .filter(|(k, _)| k.starts_with("BIVVY_PROMPT_"))
            .collect();

        Self {
            mode,
            env_overrides,
            is_ci,
        }
    }
}

impl UserInterface for NonInteractiveUI {
    fn output_mode(&self) -> OutputMode {
        self.mode
    }

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

    fn prompt(&mut self, prompt: &Prompt) -> Result<PromptResult> {
        let is_multiselect = matches!(prompt.prompt_type, PromptType::MultiSelect { .. });

        // Check environment override
        let env_key = format!("BIVVY_PROMPT_{}", prompt.key.to_uppercase());
        if let Some(value) = self.env_overrides.get(&env_key) {
            if is_multiselect {
                let values: Vec<String> = value.split(',').map(|s| s.trim().to_string()).collect();
                return Ok(PromptResult::Strings(values));
            }
            return Ok(PromptResult::String(value.clone()));
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

    fn show_run_header(&mut self, app_name: &str, workflow: &str, step_count: usize) {
        if self.mode.shows_status() {
            let step_label = if step_count == 1 { "step" } else { "steps" };
            println!(
                "\n⛺ {} · {} workflow · {} {}\n",
                app_name, workflow, step_count, step_label
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
        let mut overrides = HashMap::new();
        overrides.insert("BIVVY_PROMPT_TEST".to_string(), "override".to_string());

        let mut ui = NonInteractiveUI::with_overrides(OutputMode::Normal, overrides);
        let prompt = Prompt {
            key: "test".to_string(),
            question: "Test?".to_string(),
            prompt_type: PromptType::Input,
            default: Some("default".to_string()),
        };

        let result = ui.prompt(&prompt).unwrap();
        assert_eq!(result.as_string(), "override");
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
        let mut overrides = HashMap::new();
        overrides.insert("BIVVY_PROMPT_STEPS".to_string(), "npm".to_string());

        let mut ui = NonInteractiveUI::with_overrides(OutputMode::Normal, overrides);
        let prompt = Prompt {
            key: "steps".to_string(),
            question: "Select steps".to_string(),
            prompt_type: PromptType::MultiSelect { options: vec![] },
            default: Some("bundler,cargo".to_string()),
        };

        let result = ui.prompt(&prompt).unwrap();
        assert!(matches!(result, PromptResult::Strings(_)));
        assert_eq!(result.as_string(), "npm");
    }

    #[test]
    fn ci_mode_is_stored() {
        let ui = NonInteractiveUI::with_ci(OutputMode::Normal, true);
        assert!(ui.is_ci);
    }

    #[test]
    fn non_ci_mode_default_for_with_overrides() {
        let ui = NonInteractiveUI::with_overrides(OutputMode::Normal, HashMap::new());
        assert!(!ui.is_ci);
    }
}
