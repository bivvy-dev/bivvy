//! Interactive terminal UI.
//!
//! [`TerminalUI`] is the generic interactive UI used by every command
//! (init, status, list, run, …). For run-path workflow chrome and step
//! rendering, see [`crate::runner::display`].

use console::Term;
use std::io::Write;
use std::sync::Arc;

use crate::error::Result;
use crate::ui::surface::TerminalSurface;

use super::{
    prompt_user, should_use_colors, BivvyTheme, NonInteractiveUI, OutputMode, OutputWriter,
    ProgressDisplay, ProgressSpinner, Prompt, PromptResult, Prompter, SpinnerFactory,
    SpinnerHandle, UiState, UserInterface,
};

#[cfg(unix)]
use crate::shell::command::claim_foreground;

/// Interactive terminal UI implementation.
///
/// During a workflow run an [`Arc<TerminalSurface>`] is attached via
/// [`TerminalUI::with_surface`]; output then flows through the surface
/// so it stays coordinated with the workflow's pinned bar and the
/// active step's transient region. Outside of a run the surface is
/// `None` and output is written directly to the terminal.
pub struct TerminalUI {
    term: Term,
    theme: BivvyTheme,
    mode: OutputMode,
    surface: Option<Arc<TerminalSurface>>,
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
            surface: None,
        }
    }

    /// Print a line through the surface (above bars) or directly to
    /// the terminal if no surface is attached.
    fn println(&mut self, line: &str) {
        if let Some(surface) = &self.surface {
            surface.println(line);
        } else {
            writeln!(self.term, "{}", line).ok();
        }
    }
}

impl OutputWriter for TerminalUI {
    fn message(&mut self, msg: &str) {
        if self.mode.shows_status() {
            self.println(msg);
        }
    }

    fn success(&mut self, msg: &str) {
        if self.mode.shows_status() {
            self.println(&self.theme.format_success(msg).to_string());
        }
    }

    fn warning(&mut self, msg: &str) {
        if self.mode.shows_status() {
            self.println(&self.theme.format_warning(msg).to_string());
        }
    }

    fn error(&mut self, msg: &str) {
        self.println(&self.theme.format_error(msg).to_string());
    }

    fn show_hint(&mut self, hint: &str) {
        if self.mode.shows_status() {
            self.println(&self.theme.hint.apply_to(hint).to_string());
            self.println("");
        }
    }

    fn show_error_block(&mut self, command: &str, output: &str, hint: Option<&str>, indent: usize) {
        let b = &self.theme.border;
        let pad = " ".repeat(indent);
        let mut lines = vec![
            format!(
                "{}{} {}",
                pad,
                b.apply_to("┌─"),
                b.apply_to("Command ──────────────────────────")
            ),
            format!(
                "{}{} {}",
                pad,
                b.apply_to("│"),
                self.theme.command.apply_to(command)
            ),
        ];

        if !output.is_empty() {
            lines.push(format!(
                "{}{} {}",
                pad,
                b.apply_to("├─"),
                b.apply_to("Output ───────────────────────────")
            ));
            for line in output.lines() {
                lines.push(format!("{}{} {}", pad, b.apply_to("│"), line));
            }
        }

        lines.push(format!(
            "{}{}",
            pad,
            b.apply_to("└────────────────────────────────────")
        ));

        if let Some(h) = hint {
            lines.push(String::new());
            lines.push(format!(
                "{}{} {}",
                pad,
                self.theme.hint.apply_to("Hint:"),
                self.theme.hint.apply_to(h)
            ));
        }

        for line in &lines {
            self.println(line);
        }
    }
}

impl Prompter for TerminalUI {
    fn prompt(&mut self, prompt: &Prompt) -> Result<PromptResult> {
        // Re-claim foreground process group: child commands may have
        // stolen it on exit, leaving us unable to read input.
        #[cfg(unix)]
        claim_foreground();

        if let Some(surface) = &self.surface {
            surface.with_cursor_freed(|| prompt_user(prompt, &self.term))
        } else {
            prompt_user(prompt, &self.term)
        }
    }
}

impl SpinnerFactory for TerminalUI {
    fn start_spinner(&mut self, message: &str) -> Box<dyn SpinnerHandle> {
        if self.mode.shows_spinners() {
            // Outside a run path: ad-hoc spinner with no surface
            // coordination. Used by init/status/etc. for one-off
            // operations.
            Box::new(ProgressSpinner::new(message))
        } else {
            Box::new(ProgressSpinner::hidden())
        }
    }

    fn start_spinner_indented(&mut self, message: &str, indent: usize) -> Box<dyn SpinnerHandle> {
        if self.mode.shows_spinners() {
            Box::new(ProgressSpinner::with_indent(message, indent))
        } else {
            Box::new(ProgressSpinner::hidden())
        }
    }
}

impl ProgressDisplay for TerminalUI {
    fn show_header(&mut self, title: &str) {
        if self.mode.shows_status() {
            self.println(&format!("\n{}\n", self.theme.format_header(title)));
        }
    }

    fn show_progress(&mut self, current: usize, total: usize) {
        if self.mode.shows_status() {
            self.println(
                &self
                    .theme
                    .dim
                    .apply_to(format!("[{}/{}]", current, total))
                    .to_string(),
            );
        }
    }
}

impl UiState for TerminalUI {
    fn output_mode(&self) -> OutputMode {
        self.mode
    }

    fn is_interactive(&self) -> bool {
        self.term.is_term()
    }

    fn set_output_mode(&mut self, mode: OutputMode) {
        self.mode = mode;
    }

    fn attach_surface(&mut self, surface: Arc<TerminalSurface>) {
        self.surface = Some(surface);
    }

    fn detach_surface(&mut self) {
        self.surface = None;
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

    #[test]
    fn attach_surface_routes_output() {
        let mut ui = TerminalUI::new(OutputMode::Normal);
        let surface = TerminalSurface::hidden();
        UiState::attach_surface(&mut ui, Arc::clone(&surface));
        ui.message("hello");
        // No assertion — hidden surface drops output. Verifies no panic.
        UiState::detach_surface(&mut ui);
    }
}
