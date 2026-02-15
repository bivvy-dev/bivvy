//! Progress spinners.

use indicatif::{ProgressBar, ProgressStyle};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::shell::OutputLine;

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
                .template(&format!("{}{{spinner:.magenta}} {{msg}}", prefix))
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

    /// Get a clone of the inner progress bar for use in callbacks.
    ///
    /// This is useful for live output streaming: the cloned bar can be
    /// passed to a callback running on another thread, and `set_message`
    /// calls on it will update the spinner display in real-time.
    pub fn bar_clone(&self) -> ProgressBar {
        self.bar.clone()
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

    fn progress_bar(&self) -> Option<ProgressBar> {
        Some(self.bar.clone())
    }
}

/// Create a step-style spinner.
pub fn step_spinner(step_name: &str, description: &str) -> ProgressSpinner {
    let msg = format!("{} - {}", step_name, description);
    ProgressSpinner::new(&msg)
}

/// Create an output callback that updates a spinner with live output lines.
///
/// The callback maintains a ring buffer of the last `max_lines` output lines
/// and updates the spinner message to show the base message plus those lines.
/// This gives users feedback that a command is actually making progress.
///
/// # Arguments
/// * `bar` - A cloned `ProgressBar` from the spinner
/// * `base_message` - The primary spinner message (e.g., "Running install_deps...")
/// * `indent` - Number of spaces to indent live output lines
/// * `max_lines` - Maximum number of live output lines to show (2-3 typical)
pub fn live_output_callback(
    bar: ProgressBar,
    base_message: String,
    indent: usize,
    max_lines: usize,
) -> crate::shell::OutputCallback {
    let buffer: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::new()));
    let theme = BivvyTheme::new();

    Box::new(move |line: OutputLine| {
        let text = match &line {
            OutputLine::Stdout(s) => s.trim_end().to_string(),
            OutputLine::Stderr(s) => s.trim_end().to_string(),
        };

        // Skip empty lines
        if text.is_empty() {
            return;
        }

        // Truncate long lines for display
        let display_text = if text.len() > 72 {
            format!("{}...", &text[..69])
        } else {
            text
        };

        let mut buf = buffer.lock().unwrap();
        buf.push_back(display_text);
        while buf.len() > max_lines {
            buf.pop_front();
        }

        // Build multi-line message: base message + indented output lines
        let prefix = " ".repeat(indent);
        let mut msg = base_message.clone();
        for line in buf.iter() {
            msg.push('\n');
            msg.push_str(&prefix);
            msg.push_str(&theme.dim.apply_to(format!("» {}", line)).to_string());
        }

        bar.set_message(msg);
    })
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

    #[test]
    fn progress_bar_returns_clone() {
        let spinner = ProgressSpinner::new("Test");
        let bar = spinner.progress_bar();
        assert!(bar.is_some());
        bar.unwrap().finish();
    }

    #[test]
    fn hidden_spinner_progress_bar_returns_some() {
        // Hidden spinners still have a ProgressBar (just hidden)
        let spinner = ProgressSpinner::hidden();
        assert!(spinner.progress_bar().is_some());
    }

    #[test]
    fn live_output_callback_updates_bar() {
        let bar = ProgressBar::hidden();
        let callback = live_output_callback(bar.clone(), "Running...".to_string(), 4, 2);

        callback(OutputLine::Stdout("line 1".to_string()));
        let msg = bar.message();
        assert!(msg.contains("Running..."));
        assert!(msg.contains("line 1"));

        callback(OutputLine::Stderr("line 2".to_string()));
        let msg = bar.message();
        assert!(msg.contains("line 1"));
        assert!(msg.contains("line 2"));

        // Ring buffer evicts oldest line
        callback(OutputLine::Stdout("line 3".to_string()));
        let msg = bar.message();
        assert!(!msg.contains("line 1"));
        assert!(msg.contains("line 2"));
        assert!(msg.contains("line 3"));

        bar.finish();
    }

    #[test]
    fn live_output_callback_skips_empty_lines() {
        let bar = ProgressBar::hidden();
        let callback = live_output_callback(bar.clone(), "Running...".to_string(), 4, 2);

        // Send an empty line — should not cause a newline in the message
        callback(OutputLine::Stdout("".to_string()));

        // Now send a real line — the message should have exactly one output line
        callback(OutputLine::Stdout("real output".to_string()));
        let msg = bar.message();
        assert!(msg.contains("real output"));
        // Only one newline (base message + one output line)
        assert_eq!(msg.matches('\n').count(), 1);

        bar.finish();
    }

    #[test]
    fn live_output_callback_truncates_long_lines() {
        let bar = ProgressBar::hidden();
        let callback = live_output_callback(bar.clone(), "Running...".to_string(), 4, 2);

        let long_line = "x".repeat(100);
        callback(OutputLine::Stdout(long_line));
        let msg = bar.message();
        assert!(msg.contains("..."));
        // Should not contain the full 100-char line
        assert!(!msg.contains(&"x".repeat(100)));

        bar.finish();
    }
}
