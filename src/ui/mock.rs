//! Mock UI implementation for testing.
//!
//! `MockUI` implements the `UserInterface` trait and captures all
//! interactions for later assertion. It can be configured with
//! pre-determined prompt responses.
//!
//! # Example
//!
//! ```
//! use bivvy::ui::{MockUI, OutputMode, Prompt, PromptType, UserInterface};
//!
//! let mut ui = MockUI::new();
//! ui.set_prompt_response("db_name", "myapp_dev");
//!
//! // Use ui in code under test...
//! ui.message("Starting setup");
//! ui.success("Done!");
//!
//! // Assert on captured interactions
//! assert!(ui.messages().contains(&"Starting setup".to_string()));
//! assert!(ui.successes().contains(&"Done!".to_string()));
//! ```

use std::collections::{HashMap, VecDeque};

use crate::error::Result;

use super::{
    OutputMode, Prompt, PromptResult, PromptType, RunSummary, SpinnerHandle, UserInterface,
};

/// Mock UI implementation for testing.
///
/// Captures all UI interactions and allows pre-configured prompt responses.
/// Supports both single responses (via `set_prompt_response`) and queued
/// responses (via `queue_prompt_responses`) for keys called multiple times.
#[derive(Debug, Default)]
pub struct MockUI {
    mode: OutputMode,
    interactive: bool,
    messages: Vec<String>,
    successes: Vec<String>,
    warnings: Vec<String>,
    errors: Vec<String>,
    headers: Vec<String>,
    hints: Vec<String>,
    progress: Vec<(usize, usize)>,
    spinners: Vec<String>,
    run_headers: Vec<(String, String, usize, String)>,
    error_blocks: Vec<(String, String, Option<String>)>,
    summaries: Vec<RunSummary>,
    prompt_responses: HashMap<String, String>,
    prompt_queues: HashMap<String, VecDeque<String>>,
    prompts_shown: Vec<String>,
    /// Fallback response for any prompt key not in `prompt_responses` or `prompt_queues`.
    default_prompt_response: Option<String>,
}

impl MockUI {
    /// Create a new MockUI with Normal output mode.
    pub fn new() -> Self {
        Self {
            mode: OutputMode::Normal,
            ..Default::default()
        }
    }

    /// Create a new MockUI with a specific output mode.
    pub fn with_mode(mode: OutputMode) -> Self {
        Self {
            mode,
            ..Default::default()
        }
    }

    /// Set a response for a prompt key.
    ///
    /// When `prompt()` is called with this key, it returns the configured response.
    pub fn set_prompt_response(&mut self, key: &str, response: &str) {
        self.prompt_responses
            .insert(key.to_string(), response.to_string());
    }

    /// Set multiple prompt responses at once.
    pub fn with_prompt_responses(mut self, responses: HashMap<String, String>) -> Self {
        self.prompt_responses = responses;
        self
    }

    /// Queue multiple responses for the same prompt key.
    ///
    /// Responses are returned in order. After the queue is exhausted,
    /// falls back to `set_prompt_response` or defaults.
    pub fn queue_prompt_responses(&mut self, key: &str, responses: Vec<&str>) {
        let queue = responses.into_iter().map(|s| s.to_string()).collect();
        self.prompt_queues.insert(key.to_string(), queue);
    }

    /// Set a default response for any prompt key not explicitly configured.
    ///
    /// Useful when a workflow may prompt with unpredictable keys (e.g., recovery
    /// prompts whose key includes the step name, which depends on template detection).
    pub fn set_default_prompt_response(&mut self, response: &str) {
        self.default_prompt_response = Some(response.to_string());
    }

    /// Set whether this mock behaves as interactive.
    pub fn set_interactive(&mut self, interactive: bool) {
        self.interactive = interactive;
    }

    /// Get all captured messages.
    pub fn messages(&self) -> &[String] {
        &self.messages
    }

    /// Get all captured success messages.
    pub fn successes(&self) -> &[String] {
        &self.successes
    }

    /// Get all captured warning messages.
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// Get all captured error messages.
    pub fn errors(&self) -> &[String] {
        &self.errors
    }

    /// Get all captured headers.
    pub fn headers(&self) -> &[String] {
        &self.headers
    }

    /// Get all captured hints.
    pub fn hints(&self) -> &[String] {
        &self.hints
    }

    /// Get all captured progress updates.
    pub fn progress(&self) -> &[(usize, usize)] {
        &self.progress
    }

    /// Get all spinner messages that were started.
    pub fn spinners(&self) -> &[String] {
        &self.spinners
    }

    /// Get all prompts that were shown (by key).
    pub fn prompts_shown(&self) -> &[String] {
        &self.prompts_shown
    }

    /// Check if a specific message was shown.
    pub fn has_message(&self, msg: &str) -> bool {
        self.messages.iter().any(|m| m.contains(msg))
    }

    /// Check if a specific success was shown.
    pub fn has_success(&self, msg: &str) -> bool {
        self.successes.iter().any(|m| m.contains(msg))
    }

    /// Check if a specific error was shown.
    pub fn has_error(&self, msg: &str) -> bool {
        self.errors.iter().any(|m| m.contains(msg))
    }

    /// Check if a specific hint was shown.
    pub fn has_hint(&self, msg: &str) -> bool {
        self.hints.iter().any(|m| m.contains(msg))
    }

    /// Check if a specific warning was shown.
    pub fn has_warning(&self, msg: &str) -> bool {
        self.warnings.iter().any(|m| m.contains(msg))
    }

    /// Get all captured run headers as (app_name, workflow, step_count, version).
    pub fn run_headers(&self) -> &[(String, String, usize, String)] {
        &self.run_headers
    }

    /// Get all captured error blocks as (command, output, hint).
    pub fn error_blocks(&self) -> &[(String, String, Option<String>)] {
        &self.error_blocks
    }

    /// Get all captured run summaries.
    pub fn summaries(&self) -> &[RunSummary] {
        &self.summaries
    }

    /// Check if any summary was a success.
    pub fn has_successful_summary(&self) -> bool {
        self.summaries.iter().any(|s| s.success)
    }

    /// Clear all captured interactions.
    pub fn clear(&mut self) {
        self.messages.clear();
        self.successes.clear();
        self.warnings.clear();
        self.errors.clear();
        self.headers.clear();
        self.hints.clear();
        self.progress.clear();
        self.spinners.clear();
        self.run_headers.clear();
        self.error_blocks.clear();
        self.summaries.clear();
        self.prompts_shown.clear();
    }
}

impl UserInterface for MockUI {
    fn output_mode(&self) -> OutputMode {
        self.mode
    }

    fn message(&mut self, msg: &str) {
        self.messages.push(msg.to_string());
    }

    fn success(&mut self, msg: &str) {
        self.successes.push(msg.to_string());
    }

    fn warning(&mut self, msg: &str) {
        self.warnings.push(msg.to_string());
    }

    fn error(&mut self, msg: &str) {
        self.errors.push(msg.to_string());
    }

    fn prompt(&mut self, prompt: &Prompt) -> Result<PromptResult> {
        self.prompts_shown.push(prompt.key.clone());

        let is_confirm = matches!(prompt.prompt_type, PromptType::Confirm);
        let is_multiselect = matches!(prompt.prompt_type, PromptType::MultiSelect { .. });

        // Check queued responses first (for keys called multiple times)
        if let Some(queue) = self.prompt_queues.get_mut(&prompt.key) {
            if let Some(response) = queue.pop_front() {
                if is_confirm {
                    let val = matches!(response.as_str(), "true" | "yes" | "y" | "1");
                    return Ok(PromptResult::Bool(val));
                }
                if is_multiselect {
                    let values: Vec<String> =
                        response.split(',').map(|s| s.trim().to_string()).collect();
                    return Ok(PromptResult::Strings(values));
                }
                return Ok(PromptResult::String(response));
            }
        }

        // Return pre-configured response if available
        if let Some(response) = self.prompt_responses.get(&prompt.key) {
            if is_confirm {
                let val = matches!(response.as_str(), "true" | "yes" | "y" | "1");
                return Ok(PromptResult::Bool(val));
            }
            if is_multiselect {
                let values: Vec<String> =
                    response.split(',').map(|s| s.trim().to_string()).collect();
                return Ok(PromptResult::Strings(values));
            }
            return Ok(PromptResult::String(response.clone()));
        }

        // Fall back to default_prompt_response if set (before prompt.default)
        if let Some(ref response) = self.default_prompt_response {
            if is_confirm {
                let val = matches!(response.as_str(), "true" | "yes" | "y" | "1");
                return Ok(PromptResult::Bool(val));
            }
            if is_multiselect {
                let values: Vec<String> =
                    response.split(',').map(|s| s.trim().to_string()).collect();
                return Ok(PromptResult::Strings(values));
            }
            return Ok(PromptResult::String(response.clone()));
        }

        // Fall back to default if available
        if let Some(default) = &prompt.default {
            if is_confirm {
                let val = matches!(default.as_str(), "true" | "yes" | "y" | "1");
                return Ok(PromptResult::Bool(val));
            }
            if is_multiselect {
                let values: Vec<String> =
                    default.split(',').map(|s| s.trim().to_string()).collect();
                return Ok(PromptResult::Strings(values));
            }
            return Ok(PromptResult::String(default.clone()));
        }

        // Return type-appropriate empty for last resort (for testing)
        if is_confirm {
            return Ok(PromptResult::Bool(false));
        }
        if is_multiselect {
            return Ok(PromptResult::Strings(Vec::new()));
        }
        Ok(PromptResult::String(String::new()))
    }

    fn start_spinner(&mut self, message: &str) -> Box<dyn SpinnerHandle> {
        self.spinners.push(message.to_string());
        Box::new(MockSpinner::new())
    }

    fn show_header(&mut self, title: &str) {
        self.headers.push(title.to_string());
    }

    fn show_hint(&mut self, hint: &str) {
        self.hints.push(hint.to_string());
    }

    fn show_progress(&mut self, current: usize, total: usize) {
        self.progress.push((current, total));
    }

    fn show_run_header(
        &mut self,
        app_name: &str,
        workflow: &str,
        step_count: usize,
        version: &str,
    ) {
        self.run_headers.push((
            app_name.to_string(),
            workflow.to_string(),
            step_count,
            version.to_string(),
        ));
        self.headers.push(app_name.to_string());
    }

    fn show_error_block(&mut self, command: &str, output: &str, hint: Option<&str>) {
        self.error_blocks.push((
            command.to_string(),
            output.to_string(),
            hint.map(|h| h.to_string()),
        ));
        self.errors.push(command.to_string());
        if !output.is_empty() {
            self.messages.push(output.to_string());
        }
        if let Some(h) = hint {
            self.hints.push(h.to_string());
        }
    }

    fn show_run_summary(&mut self, summary: &RunSummary) {
        self.summaries.push(summary.clone());
        // Also delegate to the default behavior for backward compatibility
        if summary.success {
            self.successes.push(format!(
                "Setup complete! ({} run, {} skipped)",
                summary.steps_run, summary.steps_skipped
            ));
        } else {
            self.errors.push(format!(
                "Setup failed ({} run, {} failed)",
                summary.steps_run,
                summary.failed_steps.len()
            ));
        }
    }

    fn is_interactive(&self) -> bool {
        self.interactive
    }

    fn set_output_mode(&mut self, mode: OutputMode) {
        self.mode = mode;
    }
}

/// Mock spinner that captures finish messages.
#[derive(Debug, Default)]
pub struct MockSpinner {
    messages: Vec<String>,
    finish_message: Option<String>,
    status: Option<SpinnerStatus>,
}

/// Status of a mock spinner when finished.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpinnerStatus {
    /// Finished successfully.
    Success,
    /// Finished with error.
    Error,
    /// Finished as skipped.
    Skipped,
}

impl MockSpinner {
    /// Create a new mock spinner.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get all messages set during spinning.
    pub fn messages(&self) -> &[String] {
        &self.messages
    }

    /// Get the final finish message.
    pub fn finish_message(&self) -> Option<&str> {
        self.finish_message.as_deref()
    }

    /// Get the final status.
    pub fn status(&self) -> Option<SpinnerStatus> {
        self.status
    }
}

impl SpinnerHandle for MockSpinner {
    fn set_message(&mut self, msg: &str) {
        self.messages.push(msg.to_string());
    }

    fn finish_success(&mut self, msg: &str) {
        self.finish_message = Some(msg.to_string());
        self.status = Some(SpinnerStatus::Success);
    }

    fn finish_error(&mut self, msg: &str) {
        self.finish_message = Some(msg.to_string());
        self.status = Some(SpinnerStatus::Error);
    }

    fn finish_skipped(&mut self, msg: &str) {
        self.finish_message = Some(msg.to_string());
        self.status = Some(SpinnerStatus::Skipped);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::PromptOption;

    #[test]
    fn mock_ui_captures_messages() {
        let mut ui = MockUI::new();

        ui.message("Hello");
        ui.success("Done");
        ui.warning("Be careful");
        ui.error("Oops");

        assert_eq!(ui.messages(), &["Hello"]);
        assert_eq!(ui.successes(), &["Done"]);
        assert_eq!(ui.warnings(), &["Be careful"]);
        assert_eq!(ui.errors(), &["Oops"]);
    }

    #[test]
    fn mock_ui_prompt_with_response() {
        let mut ui = MockUI::new();
        ui.set_prompt_response("db_name", "myapp_dev");

        let prompt = Prompt {
            key: "db_name".to_string(),
            question: "Database name?".to_string(),
            prompt_type: PromptType::Input,
            default: None,
        };

        let result = ui.prompt(&prompt).unwrap();
        assert_eq!(result.as_string(), "myapp_dev");
        assert_eq!(ui.prompts_shown(), &["db_name"]);
    }

    #[test]
    fn mock_ui_prompt_falls_back_to_default() {
        let mut ui = MockUI::new();

        let prompt = Prompt {
            key: "env".to_string(),
            question: "Environment?".to_string(),
            prompt_type: PromptType::Input,
            default: Some("development".to_string()),
        };

        let result = ui.prompt(&prompt).unwrap();
        assert_eq!(result.as_string(), "development");
    }

    #[test]
    fn mock_ui_captures_spinners() {
        let mut ui = MockUI::new();

        let _spinner = ui.start_spinner("Installing dependencies");

        assert_eq!(ui.spinners(), &["Installing dependencies"]);
    }

    #[test]
    fn mock_ui_captures_progress() {
        let mut ui = MockUI::new();

        ui.show_progress(1, 5);
        ui.show_progress(2, 5);

        assert_eq!(ui.progress(), &[(1, 5), (2, 5)]);
    }

    #[test]
    fn mock_ui_captures_headers() {
        let mut ui = MockUI::new();

        ui.show_header("Setup Wizard");

        assert_eq!(ui.headers(), &["Setup Wizard"]);
    }

    #[test]
    fn mock_ui_clear_resets() {
        let mut ui = MockUI::new();

        ui.message("test");
        ui.success("done");
        ui.clear();

        assert!(ui.messages().is_empty());
        assert!(ui.successes().is_empty());
    }

    #[test]
    fn mock_ui_has_helpers() {
        let mut ui = MockUI::new();

        ui.message("Setting up project");
        ui.success("Complete!");
        ui.error("Failed to connect");

        assert!(ui.has_message("Setting up"));
        assert!(ui.has_success("Complete"));
        assert!(ui.has_error("Failed"));
        assert!(!ui.has_message("not there"));
    }

    #[test]
    fn mock_ui_output_mode() {
        let ui = MockUI::with_mode(OutputMode::Quiet);
        assert_eq!(ui.output_mode(), OutputMode::Quiet);
    }

    #[test]
    fn mock_ui_is_not_interactive() {
        let ui = MockUI::new();
        assert!(!ui.is_interactive());
    }

    #[test]
    fn mock_spinner_captures_finish() {
        let mut spinner = MockSpinner::new();

        spinner.set_message("Working...");
        spinner.finish_success("Done!");

        assert_eq!(spinner.messages(), &["Working..."]);
        assert_eq!(spinner.finish_message(), Some("Done!"));
        assert_eq!(spinner.status(), Some(SpinnerStatus::Success));
    }

    #[test]
    fn mock_spinner_error_status() {
        let mut spinner = MockSpinner::new();
        spinner.finish_error("Failed!");

        assert_eq!(spinner.status(), Some(SpinnerStatus::Error));
    }

    #[test]
    fn mock_spinner_skipped_status() {
        let mut spinner = MockSpinner::new();
        spinner.finish_skipped("Skipped!");

        assert_eq!(spinner.status(), Some(SpinnerStatus::Skipped));
    }

    #[test]
    fn mock_ui_with_prompt_responses() {
        let mut responses = HashMap::new();
        responses.insert("key1".to_string(), "value1".to_string());
        responses.insert("key2".to_string(), "value2".to_string());

        let mut ui = MockUI::new().with_prompt_responses(responses);

        let prompt1 = Prompt {
            key: "key1".to_string(),
            question: "?".to_string(),
            prompt_type: PromptType::Input,
            default: None,
        };
        let prompt2 = Prompt {
            key: "key2".to_string(),
            question: "?".to_string(),
            prompt_type: PromptType::Input,
            default: None,
        };

        assert_eq!(ui.prompt(&prompt1).unwrap().as_string(), "value1");
        assert_eq!(ui.prompt(&prompt2).unwrap().as_string(), "value2");
    }

    #[test]
    fn mock_ui_multiselect_returns_strings_from_response() {
        let mut ui = MockUI::new();
        ui.set_prompt_response("steps", "bundler,cargo");

        let prompt = Prompt {
            key: "steps".to_string(),
            question: "Select steps".to_string(),
            prompt_type: PromptType::MultiSelect {
                options: vec![
                    PromptOption {
                        label: "Bundler".to_string(),
                        value: "bundler".to_string(),
                    },
                    PromptOption {
                        label: "Cargo".to_string(),
                        value: "cargo".to_string(),
                    },
                ],
            },
            default: None,
        };

        let result = ui.prompt(&prompt).unwrap();
        assert!(matches!(result, PromptResult::Strings(_)));
        assert_eq!(result.as_string(), "bundler,cargo");
    }

    #[test]
    fn mock_ui_multiselect_returns_strings_from_default() {
        let mut ui = MockUI::new();

        let prompt = Prompt {
            key: "steps".to_string(),
            question: "Select steps".to_string(),
            prompt_type: PromptType::MultiSelect {
                options: vec![PromptOption {
                    label: "Bundler".to_string(),
                    value: "bundler".to_string(),
                }],
            },
            default: Some("bundler".to_string()),
        };

        let result = ui.prompt(&prompt).unwrap();
        assert!(matches!(result, PromptResult::Strings(_)));
        assert_eq!(result.as_string(), "bundler");
    }

    #[test]
    fn mock_ui_multiselect_empty_without_response_or_default() {
        let mut ui = MockUI::new();

        let prompt = Prompt {
            key: "steps".to_string(),
            question: "Select steps".to_string(),
            prompt_type: PromptType::MultiSelect { options: vec![] },
            default: None,
        };

        let result = ui.prompt(&prompt).unwrap();
        assert!(matches!(result, PromptResult::Strings(ref v) if v.is_empty()));
    }

    #[test]
    fn mock_ui_set_interactive() {
        let mut ui = MockUI::new();
        assert!(!ui.is_interactive());

        ui.set_interactive(true);
        assert!(ui.is_interactive());
    }

    #[test]
    fn mock_ui_confirm_returns_bool_from_response() {
        let mut ui = MockUI::new();
        ui.set_prompt_response("confirm_key", "true");

        let prompt = Prompt {
            key: "confirm_key".to_string(),
            question: "Continue?".to_string(),
            prompt_type: PromptType::Confirm,
            default: None,
        };

        let result = ui.prompt(&prompt).unwrap();
        assert_eq!(result.as_bool(), Some(true));
    }

    #[test]
    fn mock_ui_confirm_returns_false_for_no() {
        let mut ui = MockUI::new();
        ui.set_prompt_response("confirm_key", "false");

        let prompt = Prompt {
            key: "confirm_key".to_string(),
            question: "Continue?".to_string(),
            prompt_type: PromptType::Confirm,
            default: None,
        };

        let result = ui.prompt(&prompt).unwrap();
        assert_eq!(result.as_bool(), Some(false));
    }

    #[test]
    fn mock_ui_confirm_uses_default() {
        let mut ui = MockUI::new();

        let prompt = Prompt {
            key: "confirm_key".to_string(),
            question: "Continue?".to_string(),
            prompt_type: PromptType::Confirm,
            default: Some("yes".to_string()),
        };

        let result = ui.prompt(&prompt).unwrap();
        assert_eq!(result.as_bool(), Some(true));
    }

    #[test]
    fn mock_ui_confirm_without_response_or_default_returns_false() {
        let mut ui = MockUI::new();

        let prompt = Prompt {
            key: "confirm_key".to_string(),
            question: "Continue?".to_string(),
            prompt_type: PromptType::Confirm,
            default: None,
        };

        let result = ui.prompt(&prompt).unwrap();
        assert_eq!(result.as_bool(), Some(false));
    }

    #[test]
    fn mock_ui_set_output_mode() {
        let mut ui = MockUI::new();
        assert_eq!(ui.output_mode(), OutputMode::Normal);

        ui.set_output_mode(OutputMode::Quiet);
        assert_eq!(ui.output_mode(), OutputMode::Quiet);

        ui.set_output_mode(OutputMode::Silent);
        assert_eq!(ui.output_mode(), OutputMode::Silent);
    }

    #[test]
    fn mock_ui_captures_run_headers() {
        let mut ui = MockUI::new();

        ui.show_run_header("MyApp", "default", 7, "1.0.0");

        assert_eq!(ui.run_headers().len(), 1);
        assert_eq!(
            ui.run_headers()[0],
            (
                "MyApp".to_string(),
                "default".to_string(),
                7,
                "1.0.0".to_string()
            )
        );
        // Also delegates to headers for backward compat
        assert!(ui.headers().contains(&"MyApp".to_string()));
    }

    #[test]
    fn mock_ui_captures_error_blocks() {
        let mut ui = MockUI::new();

        ui.show_error_block(
            "npm run build",
            "Cannot find module 'webpack'",
            Some("Run `npm install` first"),
        );

        assert_eq!(ui.error_blocks().len(), 1);
        let (cmd, output, hint) = &ui.error_blocks()[0];
        assert_eq!(cmd, "npm run build");
        assert_eq!(output, "Cannot find module 'webpack'");
        assert_eq!(hint.as_deref(), Some("Run `npm install` first"));
        // Delegates to errors, messages, hints
        assert!(ui.has_error("npm run build"));
        assert!(ui.has_message("Cannot find module"));
        assert!(ui.has_hint("npm install"));
    }

    #[test]
    fn mock_ui_error_block_without_hint() {
        let mut ui = MockUI::new();

        ui.show_error_block("cargo build", "compilation error", None);

        assert_eq!(ui.error_blocks().len(), 1);
        assert!(ui.error_blocks()[0].2.is_none());
        assert!(ui.hints().is_empty());
    }

    #[test]
    fn mock_ui_error_block_empty_output_not_captured_as_message() {
        let mut ui = MockUI::new();

        ui.show_error_block("failing_cmd", "", None);

        assert!(ui.messages().is_empty());
        assert!(ui.has_error("failing_cmd"));
    }

    #[test]
    fn mock_ui_captures_run_summaries() {
        use crate::ui::{StatusKind, StepSummary};
        use std::time::Duration;

        let mut ui = MockUI::new();

        let summary = RunSummary {
            step_results: vec![StepSummary {
                name: "install".to_string(),
                status: StatusKind::Success,
                duration: Some(Duration::from_secs(1)),
                detail: None,
            }],
            total_duration: Duration::from_secs(1),
            steps_run: 1,
            steps_skipped: 0,
            success: true,
            failed_steps: vec![],
        };

        ui.show_run_summary(&summary);

        assert_eq!(ui.summaries().len(), 1);
        assert!(ui.has_successful_summary());
        // Delegates to successes for backward compat
        assert!(ui.has_success("Setup complete!"));
    }

    #[test]
    fn mock_ui_failed_summary() {
        use std::time::Duration;

        let mut ui = MockUI::new();

        let summary = RunSummary {
            step_results: vec![],
            total_duration: Duration::from_secs(5),
            steps_run: 2,
            steps_skipped: 0,
            success: false,
            failed_steps: vec!["build".to_string()],
        };

        ui.show_run_summary(&summary);

        assert!(!ui.has_successful_summary());
        assert!(ui.has_error("Setup failed"));
    }

    #[test]
    fn mock_ui_has_warning_helper() {
        let mut ui = MockUI::new();

        ui.warning("Step may be outdated");

        assert!(ui.has_warning("outdated"));
        assert!(!ui.has_warning("missing"));
    }

    #[test]
    fn mock_ui_clear_resets_new_fields() {
        use std::time::Duration;

        let mut ui = MockUI::new();

        ui.show_run_header("App", "default", 3, "1.0.0");
        ui.show_error_block("cmd", "err", Some("hint"));
        let summary = RunSummary {
            step_results: vec![],
            total_duration: Duration::from_secs(1),
            steps_run: 1,
            steps_skipped: 0,
            success: true,
            failed_steps: vec![],
        };
        ui.show_run_summary(&summary);

        assert!(!ui.run_headers().is_empty());
        assert!(!ui.error_blocks().is_empty());
        assert!(!ui.summaries().is_empty());

        ui.clear();

        assert!(ui.run_headers().is_empty());
        assert!(ui.error_blocks().is_empty());
        assert!(ui.summaries().is_empty());
    }

    #[test]
    fn mock_ui_queued_responses_returned_in_order() {
        let mut ui = MockUI::new();
        ui.queue_prompt_responses("recovery_bundler", vec!["shell", "retry"]);

        let prompt = Prompt {
            key: "recovery_bundler".to_string(),
            question: "How to proceed?".to_string(),
            prompt_type: PromptType::Input,
            default: None,
        };

        assert_eq!(ui.prompt(&prompt).unwrap().as_string(), "shell");
        assert_eq!(ui.prompt(&prompt).unwrap().as_string(), "retry");
        // Queue exhausted, falls back to empty string (no set_prompt_response or default)
        assert_eq!(ui.prompt(&prompt).unwrap().as_string(), "");
    }

    #[test]
    fn mock_ui_queued_responses_fallback_to_set_response() {
        let mut ui = MockUI::new();
        ui.set_prompt_response("key", "fallback");
        ui.queue_prompt_responses("key", vec!["first"]);

        let prompt = Prompt {
            key: "key".to_string(),
            question: "?".to_string(),
            prompt_type: PromptType::Input,
            default: None,
        };

        assert_eq!(ui.prompt(&prompt).unwrap().as_string(), "first");
        // Queue exhausted, falls back to set_prompt_response
        assert_eq!(ui.prompt(&prompt).unwrap().as_string(), "fallback");
    }

    #[test]
    fn mock_ui_queued_confirm_responses() {
        let mut ui = MockUI::new();
        ui.queue_prompt_responses("confirm", vec!["yes", "no"]);

        let prompt = Prompt {
            key: "confirm".to_string(),
            question: "Continue?".to_string(),
            prompt_type: PromptType::Confirm,
            default: None,
        };

        assert_eq!(ui.prompt(&prompt).unwrap().as_bool(), Some(true));
        assert_eq!(ui.prompt(&prompt).unwrap().as_bool(), Some(false));
    }
}
