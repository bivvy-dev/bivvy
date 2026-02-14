//! Progress spinners.

use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

use super::theme::BivvyTheme;
use super::SpinnerHandle;

/// A progress spinner for long-running operations.
pub struct ProgressSpinner {
    bar: ProgressBar,
    indent: usize,
}

impl ProgressSpinner {
    /// Create a new spinner with a message.
    pub fn new(message: &str) -> Self {
        Self::with_indent(message, 0)
    }

    /// Create a new spinner with indentation.
    pub fn with_indent(message: &str, indent: usize) -> Self {
        let bar = ProgressBar::new_spinner();
        let prefix = " ".repeat(indent);
        bar.set_style(
            ProgressStyle::default_spinner()
                .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
                .template(&format!("{}{{spinner:.cyan}} {{msg}}", prefix))
                .unwrap(),
        );
        bar.set_message(message.to_string());
        bar.enable_steady_tick(Duration::from_millis(80));

        Self { bar, indent }
    }

    /// Create a spinner that doesn't show (for silent mode).
    pub fn hidden() -> Self {
        let bar = ProgressBar::hidden();
        Self { bar, indent: 0 }
    }
}

impl SpinnerHandle for ProgressSpinner {
    fn set_message(&mut self, msg: &str) {
        self.bar.set_message(msg.to_string());
    }

    fn finish_success(&mut self, msg: &str) {
        let prefix = " ".repeat(self.indent);
        let theme = BivvyTheme::new();
        self.bar
            .set_style(ProgressStyle::default_spinner().template("{msg}").unwrap());
        self.bar
            .finish_with_message(format!("{}{}", prefix, theme.format_success(msg)));
    }

    fn finish_error(&mut self, msg: &str) {
        let prefix = " ".repeat(self.indent);
        let theme = BivvyTheme::new();
        self.bar
            .set_style(ProgressStyle::default_spinner().template("{msg}").unwrap());
        self.bar
            .finish_with_message(format!("{}{}", prefix, theme.format_error(msg)));
    }

    fn finish_skipped(&mut self, msg: &str) {
        let prefix = " ".repeat(self.indent);
        let theme = BivvyTheme::new();
        self.bar
            .set_style(ProgressStyle::default_spinner().template("{msg}").unwrap());
        self.bar
            .finish_with_message(format!("{}{}", prefix, theme.format_skipped(msg)));
    }
}

/// Create a step-style spinner.
pub fn step_spinner(step_name: &str, description: &str) -> ProgressSpinner {
    let msg = format!("{} - {}", step_name, description);
    ProgressSpinner::new(&msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spinner_creation() {
        let spinner = ProgressSpinner::new("Testing...");
        drop(spinner);
    }

    #[test]
    fn hidden_spinner() {
        let spinner = ProgressSpinner::hidden();
        drop(spinner);
    }

    #[test]
    fn spinner_finish_success() {
        let mut spinner = ProgressSpinner::new("Testing...");
        spinner.finish_success("Done");
    }

    #[test]
    fn spinner_finish_error() {
        let mut spinner = ProgressSpinner::new("Testing...");
        spinner.finish_error("Failed");
    }

    #[test]
    fn spinner_finish_skipped() {
        let mut spinner = ProgressSpinner::new("Testing...");
        spinner.finish_skipped("Skipped");
    }

    #[test]
    fn spinner_set_message() {
        let mut spinner = ProgressSpinner::new("Initial");
        spinner.set_message("Updated");
        spinner.finish_success("Done");
    }

    #[test]
    fn step_spinner_creates_formatted_message() {
        let spinner = step_spinner("database", "Setup database");
        spinner.bar.finish();
    }
}
