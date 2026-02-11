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

use std::collections::HashMap;

use crate::error::Result;

use super::{OutputMode, Prompt, PromptResult, PromptType, SpinnerHandle, UserInterface};

/// Mock UI implementation for testing.
///
/// Captures all UI interactions and allows pre-configured prompt responses.
#[derive(Debug, Default)]
pub struct MockUI {
    mode: OutputMode,
    interactive: bool,
    messages: Vec<String>,
    successes: Vec<String>,
    warnings: Vec<String>,
    errors: Vec<String>,
    headers: Vec<String>,
    progress: Vec<(usize, usize)>,
    spinners: Vec<String>,
    prompt_responses: HashMap<String, String>,
    prompts_shown: Vec<String>,
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

    /// Clear all captured interactions.
    pub fn clear(&mut self) {
        self.messages.clear();
        self.successes.clear();
        self.warnings.clear();
        self.errors.clear();
        self.headers.clear();
        self.progress.clear();
        self.spinners.clear();
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

        let is_multiselect = matches!(prompt.prompt_type, PromptType::MultiSelect { .. });

        // Return pre-configured response if available
        if let Some(response) = self.prompt_responses.get(&prompt.key) {
            if is_multiselect {
                let values: Vec<String> =
                    response.split(',').map(|s| s.trim().to_string()).collect();
                return Ok(PromptResult::Strings(values));
            }
            return Ok(PromptResult::String(response.clone()));
        }

        // Fall back to default if available
        if let Some(default) = &prompt.default {
            if is_multiselect {
                let values: Vec<String> =
                    default.split(',').map(|s| s.trim().to_string()).collect();
                return Ok(PromptResult::Strings(values));
            }
            return Ok(PromptResult::String(default.clone()));
        }

        // Return empty for last resort (for testing)
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

    fn show_progress(&mut self, current: usize, total: usize) {
        self.progress.push((current, total));
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
    fn mock_ui_set_output_mode() {
        let mut ui = MockUI::new();
        assert_eq!(ui.output_mode(), OutputMode::Normal);

        ui.set_output_mode(OutputMode::Quiet);
        assert_eq!(ui.output_mode(), OutputMode::Quiet);

        ui.set_output_mode(OutputMode::Silent);
        assert_eq!(ui.output_mode(), OutputMode::Silent);
    }
}
