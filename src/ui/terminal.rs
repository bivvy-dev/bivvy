//! Interactive terminal UI.

use console::Term;
use std::io::Write;

use crate::error::Result;

use super::{
    prompt_user, should_use_colors, BivvyTheme, NonInteractiveUI, OutputMode, ProgressSpinner,
    Prompt, PromptResult, SpinnerHandle, UserInterface,
};

/// Interactive terminal UI implementation.
pub struct TerminalUI {
    term: Term,
    theme: BivvyTheme,
    mode: OutputMode,
}

impl TerminalUI {
    /// Create a new terminal UI.
    pub fn new(mode: OutputMode) -> Self {
        let theme = if should_use_colors() {
            BivvyTheme::new()
        } else {
            BivvyTheme::plain()
        };

        Self {
            term: Term::stdout(),
            theme,
            mode,
        }
    }
}

impl UserInterface for TerminalUI {
    fn output_mode(&self) -> OutputMode {
        self.mode
    }

    fn message(&mut self, msg: &str) {
        if self.mode.shows_status() {
            writeln!(self.term, "{}", msg).ok();
        }
    }

    fn success(&mut self, msg: &str) {
        if self.mode.shows_status() {
            writeln!(self.term, "{}", self.theme.format_success(msg)).ok();
        }
    }

    fn warning(&mut self, msg: &str) {
        if self.mode.shows_status() {
            writeln!(self.term, "{}", self.theme.format_warning(msg)).ok();
        }
    }

    fn error(&mut self, msg: &str) {
        writeln!(self.term, "{}", self.theme.format_error(msg)).ok();
    }

    fn prompt(&mut self, prompt: &Prompt) -> Result<PromptResult> {
        prompt_user(prompt, &self.term)
    }

    fn start_spinner(&mut self, message: &str) -> Box<dyn SpinnerHandle> {
        if self.mode.shows_spinners() {
            Box::new(ProgressSpinner::new(message))
        } else {
            Box::new(ProgressSpinner::hidden())
        }
    }

    fn show_header(&mut self, title: &str) {
        if self.mode.shows_status() {
            writeln!(self.term, "\n{}\n", self.theme.format_header(title)).ok();
        }
    }

    fn show_progress(&mut self, current: usize, total: usize) {
        if self.mode.shows_status() {
            writeln!(
                self.term,
                "{}",
                self.theme.dim.apply_to(format!("[{}/{}]", current, total))
            )
            .ok();
        }
    }

    fn is_interactive(&self) -> bool {
        self.term.is_term()
    }

    fn set_output_mode(&mut self, mode: OutputMode) {
        self.mode = mode;
    }
}

/// Create the appropriate UI based on context.
pub fn create_ui(interactive: bool, mode: OutputMode) -> Box<dyn UserInterface> {
    if interactive && Term::stdout().is_term() {
        Box::new(TerminalUI::new(mode))
    } else {
        Box::new(NonInteractiveUI::new(mode))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_ui_creation() {
        let ui = TerminalUI::new(OutputMode::Normal);
        drop(ui);
    }

    #[test]
    fn terminal_ui_output_mode() {
        let ui = TerminalUI::new(OutputMode::Quiet);
        assert_eq!(ui.output_mode(), OutputMode::Quiet);
    }

    #[test]
    fn create_ui_non_interactive() {
        let ui = create_ui(false, OutputMode::Normal);
        assert!(!ui.is_interactive());
    }

    #[test]
    fn create_ui_respects_mode() {
        let ui = create_ui(false, OutputMode::Silent);
        assert_eq!(ui.output_mode(), OutputMode::Silent);
    }

    #[test]
    fn create_ui_verbose_mode() {
        let ui = create_ui(false, OutputMode::Verbose);
        assert_eq!(ui.output_mode(), OutputMode::Verbose);
    }
}
