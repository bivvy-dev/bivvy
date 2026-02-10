//! Interactive user interface components.
//!
//! This module provides:
//! - [`UserInterface`] trait for UI abstraction
//! - [`TerminalUI`] for interactive terminal usage
//! - [`NonInteractiveUI`] for CI/headless environments
//! - Prompts, spinners, progress indicators, and tables
//!
//! # Example
//!
//! ```
//! use bivvy::ui::{create_ui, OutputMode};
//!
//! // Use non-interactive mode for testability
//! let mut ui = create_ui(false, OutputMode::Quiet);
//! ui.show_header("My App");
//! ui.success("Setup complete!");
//! ```

pub mod mock;
pub mod non_interactive;
pub mod output;
pub mod preflight;
pub mod progress;
pub mod prompts;
pub mod spinner;
pub mod table;
pub mod terminal;
pub mod theme;

pub use mock::{MockSpinner, MockUI};
pub use non_interactive::NonInteractiveUI;
pub use output::{Output, OutputMode};
pub use preflight::PreflightCollector;
pub use progress::{format_duration, format_relative_time, StepProgress};
pub use prompts::prompt_user;
pub use spinner::{step_spinner, ProgressSpinner};
pub use table::Table;
pub use terminal::{create_ui, TerminalUI};
pub use theme::{should_use_colors, BivvyTheme};

use crate::error::Result;

/// Trait for user interface interactions.
///
/// This trait allows mocking the UI in tests.
pub trait UserInterface {
    /// Get the current output mode.
    fn output_mode(&self) -> OutputMode;

    /// Display a message to the user.
    fn message(&mut self, msg: &str);

    /// Display a success message.
    fn success(&mut self, msg: &str);

    /// Display a warning message.
    fn warning(&mut self, msg: &str);

    /// Display an error message.
    fn error(&mut self, msg: &str);

    /// Show a prompt and get user input.
    fn prompt(&mut self, prompt: &Prompt) -> Result<PromptResult>;

    /// Start a spinner for an operation.
    fn start_spinner(&mut self, message: &str) -> Box<dyn SpinnerHandle>;

    /// Show a header/banner.
    fn show_header(&mut self, title: &str);

    /// Show progress (e.g., "Step 3 of 7").
    fn show_progress(&mut self, current: usize, total: usize);

    /// Check if running in interactive mode.
    fn is_interactive(&self) -> bool;
}

/// Handle for controlling a spinner.
pub trait SpinnerHandle {
    /// Update the spinner message.
    fn set_message(&mut self, msg: &str);

    /// Mark the operation as successful.
    fn finish_success(&mut self, msg: &str);

    /// Mark the operation as failed.
    fn finish_error(&mut self, msg: &str);

    /// Mark as skipped.
    fn finish_skipped(&mut self, msg: &str);
}

/// A prompt to show to the user.
#[derive(Debug, Clone)]
pub struct Prompt {
    /// Unique key for the prompt (used for caching/lookup).
    pub key: String,
    /// The question to display.
    pub question: String,
    /// The type of prompt.
    pub prompt_type: PromptType,
    /// Default value if user just presses enter.
    pub default: Option<String>,
}

/// The type of prompt.
#[derive(Debug, Clone)]
pub enum PromptType {
    /// Yes/no confirmation.
    Confirm,
    /// Free-form text input.
    Input,
    /// Select one from a list of options.
    Select { options: Vec<PromptOption> },
    /// Select multiple from a list of options.
    MultiSelect { options: Vec<PromptOption> },
}

/// An option in a select prompt.
#[derive(Debug, Clone)]
pub struct PromptOption {
    /// Display label.
    pub label: String,
    /// Value returned when selected.
    pub value: String,
}

/// Result of a prompt.
#[derive(Debug, Clone)]
pub enum PromptResult {
    /// Boolean result from confirm.
    Bool(bool),
    /// String result from input or select.
    String(String),
    /// Multiple string results from multi-select.
    Strings(Vec<String>),
}

impl PromptResult {
    /// Get as string, suitable for interpolation.
    pub fn as_string(&self) -> String {
        match self {
            Self::Bool(b) => b.to_string(),
            Self::String(s) => s.clone(),
            Self::Strings(v) => v.join(","),
        }
    }

    /// Get as bool if this is a Bool result.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_result_as_string_bool() {
        assert_eq!(PromptResult::Bool(true).as_string(), "true");
        assert_eq!(PromptResult::Bool(false).as_string(), "false");
    }

    #[test]
    fn prompt_result_as_string_string() {
        assert_eq!(
            PromptResult::String("hello".to_string()).as_string(),
            "hello"
        );
    }

    #[test]
    fn prompt_result_as_string_strings() {
        assert_eq!(
            PromptResult::Strings(vec!["a".to_string(), "b".to_string()]).as_string(),
            "a,b"
        );
    }

    #[test]
    fn prompt_result_as_bool() {
        assert_eq!(PromptResult::Bool(true).as_bool(), Some(true));
        assert_eq!(PromptResult::String("test".to_string()).as_bool(), None);
    }

    #[test]
    fn prompt_option_creation() {
        let opt = PromptOption {
            label: "Display Text".to_string(),
            value: "value".to_string(),
        };
        assert_eq!(opt.label, "Display Text");
        assert_eq!(opt.value, "value");
    }

    #[test]
    fn prompt_type_variants() {
        let confirm = PromptType::Confirm;
        let input = PromptType::Input;
        let select = PromptType::Select { options: vec![] };
        let multi = PromptType::MultiSelect { options: vec![] };

        // Just verify they can be created
        assert!(matches!(confirm, PromptType::Confirm));
        assert!(matches!(input, PromptType::Input));
        assert!(matches!(select, PromptType::Select { .. }));
        assert!(matches!(multi, PromptType::MultiSelect { .. }));
    }

    #[test]
    fn multiselect_empty_selection_returns_empty_vec() {
        let result = PromptResult::Strings(vec![]);
        assert_eq!(result.as_string(), "");
    }

    #[test]
    fn multiselect_single_selection() {
        let result = PromptResult::Strings(vec!["db".to_string()]);
        assert_eq!(result.as_string(), "db");
    }

    #[test]
    fn multiselect_multiple_selections() {
        let result = PromptResult::Strings(vec![
            "db".to_string(),
            "cache".to_string(),
            "search".to_string(),
        ]);
        assert_eq!(result.as_string(), "db,cache,search");
    }

    #[test]
    fn prompt_type_select_stores_options() {
        let options = vec![
            PromptOption {
                label: "Option A".to_string(),
                value: "a".to_string(),
            },
            PromptOption {
                label: "Option B".to_string(),
                value: "b".to_string(),
            },
        ];

        let prompt_type = PromptType::Select {
            options: options.clone(),
        };

        if let PromptType::Select { options: stored } = prompt_type {
            assert_eq!(stored.len(), 2);
            assert_eq!(stored[0].value, "a");
        } else {
            panic!("Expected Select variant");
        }
    }

    #[test]
    fn prompt_type_multiselect_stores_options() {
        let options = vec![PromptOption {
            label: "Feature 1".to_string(),
            value: "f1".to_string(),
        }];

        let prompt_type = PromptType::MultiSelect { options };

        if let PromptType::MultiSelect { options: stored } = prompt_type {
            assert_eq!(stored.len(), 1);
        } else {
            panic!("Expected MultiSelect variant");
        }
    }

    #[test]
    fn prompt_result_strings_preserves_order() {
        let result = PromptResult::Strings(vec![
            "first".to_string(),
            "second".to_string(),
            "third".to_string(),
        ]);
        assert_eq!(result.as_string(), "first,second,third");
    }

    #[test]
    fn prompt_result_as_bool_returns_none_for_strings() {
        let result = PromptResult::Strings(vec!["true".to_string()]);
        assert_eq!(result.as_bool(), None);
    }
}
