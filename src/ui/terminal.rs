//! Interactive terminal UI.

use console::Term;
use std::io::Write;
use std::time::Duration;

use crate::error::Result;

use super::progress::format_duration;
use super::{
    prompt_user, should_use_colors, BivvyTheme, NonInteractiveUI, OutputMode, ProgressSpinner,
    Prompt, PromptResult, RunSummary, SpinnerHandle, UserInterface,
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

    fn start_spinner_indented(&mut self, message: &str, indent: usize) -> Box<dyn SpinnerHandle> {
        if self.mode.shows_spinners() {
            Box::new(ProgressSpinner::with_indent(message, indent))
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

    fn show_run_header(&mut self, app_name: &str, workflow: &str, step_count: usize) {
        if self.mode.shows_status() {
            let step_label = if step_count == 1 { "step" } else { "steps" };
            writeln!(
                self.term,
                "\n{} {} {} {} {}\n",
                self.theme.header.apply_to("⛺"),
                self.theme.highlight.apply_to(app_name),
                self.theme.dim.apply_to("·"),
                self.theme.dim.apply_to(format!("{} workflow", workflow)),
                self.theme
                    .dim
                    .apply_to(format!("· {} {}", step_count, step_label)),
            )
            .ok();
        }
    }

    fn show_workflow_progress(&mut self, current: usize, total: usize, elapsed: Duration) {
        if self.mode.shows_status() {
            let filled = if total > 0 { (current * 16) / total } else { 0 };
            let empty = 16 - filled;
            let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty),);
            writeln!(
                self.term,
                "  {} {}/{} steps {} {}",
                self.theme.info.apply_to(format!("[{}]", bar)),
                current,
                total,
                self.theme.dim.apply_to("·"),
                self.theme
                    .duration
                    .apply_to(format!("{} elapsed", format_duration(elapsed))),
            )
            .ok();
        }
    }

    fn show_hint(&mut self, hint: &str) {
        if self.mode.shows_status() {
            writeln!(self.term, "  {}", self.theme.hint.apply_to(hint)).ok();
            writeln!(self.term).ok();
        }
    }

    fn show_error_block(&mut self, command: &str, output: &str, hint: Option<&str>) {
        let b = &self.theme.border;
        let top = format!(
            "    {} {}",
            b.apply_to("┌─"),
            b.apply_to("Command ──────────────────────────")
        );
        let cmd_line = format!(
            "    {} {}",
            b.apply_to("│"),
            self.theme.command.apply_to(command)
        );
        writeln!(self.term, "{}", top).ok();
        writeln!(self.term, "{}", cmd_line).ok();

        if !output.is_empty() {
            writeln!(
                self.term,
                "    {} {}",
                b.apply_to("├─"),
                b.apply_to("Output ───────────────────────────")
            )
            .ok();
            for line in output.lines() {
                writeln!(self.term, "    {} {}", b.apply_to("│"), line).ok();
            }
        }

        writeln!(
            self.term,
            "    {}",
            b.apply_to("└────────────────────────────────────")
        )
        .ok();

        if let Some(h) = hint {
            writeln!(self.term).ok();
            writeln!(
                self.term,
                "    {} {}",
                self.theme.hint.apply_to("Hint:"),
                self.theme.hint.apply_to(h)
            )
            .ok();
        }
    }

    fn show_run_summary(&mut self, summary: &RunSummary) {
        if !self.mode.shows_status() {
            return;
        }

        let b = &self.theme.border;

        writeln!(self.term).ok();
        writeln!(
            self.term,
            "  {} {}",
            b.apply_to("┌─"),
            b.apply_to("Summary ──────────────────────────")
        )
        .ok();

        for step in &summary.step_results {
            let icon = step.status.styled(&self.theme);
            let duration_str = step.duration.map(format_duration).unwrap_or_default();
            let detail_str = step.detail.as_deref().unwrap_or("");

            let right_side = if !duration_str.is_empty() {
                self.theme.duration.apply_to(duration_str).to_string()
            } else if !detail_str.is_empty() {
                self.theme.dim.apply_to(detail_str).to_string()
            } else {
                String::new()
            };

            writeln!(
                self.term,
                "  {} {} {:<20} {}",
                b.apply_to("│"),
                icon,
                step.name,
                right_side,
            )
            .ok();
        }

        // Footer
        writeln!(
            self.term,
            "  {}",
            b.apply_to("├────────────────────────────────────")
        )
        .ok();
        writeln!(
            self.term,
            "  {} Total: {} {} {} run {} {} skipped",
            b.apply_to("│"),
            self.theme
                .duration
                .apply_to(format_duration(summary.total_duration)),
            self.theme.dim.apply_to("·"),
            summary.steps_run,
            self.theme.dim.apply_to("·"),
            summary.steps_skipped,
        )
        .ok();
        writeln!(
            self.term,
            "  {}",
            b.apply_to("└────────────────────────────────────")
        )
        .ok();
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
