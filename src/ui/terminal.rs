//! Interactive terminal UI.

use console::Term;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::io::Write;
use std::time::Duration;

use crate::error::Result;

use super::progress::format_duration;
use super::{
    prompt_user, should_use_colors, BivvyTheme, NonInteractiveUI, OutputMode, OutputWriter,
    ProgressDisplay, ProgressSpinner, Prompt, PromptResult, Prompter, RunSummary, SpinnerFactory,
    SpinnerHandle, UiState, UserInterface, WorkflowDisplay,
};

#[cfg(unix)]
use crate::shell::command::claim_foreground;

/// Interactive terminal UI implementation.
///
/// When a workflow is running, a [`MultiProgress`] pins a progress bar at the
/// bottom of the terminal. All other output is printed *above* it via
/// `multi.println()`, so the bar stays in place and updates after each step.
pub struct TerminalUI {
    term: Term,
    theme: BivvyTheme,
    mode: OutputMode,
    /// Active multi-progress manager (set during workflow runs).
    multi: Option<MultiProgress>,
    /// The pinned workflow progress bar (child of `multi`).
    workflow_bar: Option<ProgressBar>,
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
            multi: None,
            workflow_bar: None,
        }
    }

    /// Print a line above the pinned progress bar, or directly to the terminal
    /// if no multi-progress is active.
    fn println(&mut self, line: &str) {
        if let Some(ref multi) = self.multi {
            multi.println(line).ok();
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

    fn show_error_block(&mut self, command: &str, output: &str, hint: Option<&str>) {
        let b = &self.theme.border;
        let mut lines = vec![
            format!(
                "    {} {}",
                b.apply_to("┌─"),
                b.apply_to("Command ──────────────────────────")
            ),
            format!(
                "    {} {}",
                b.apply_to("│"),
                self.theme.command.apply_to(command)
            ),
        ];

        if !output.is_empty() {
            lines.push(format!(
                "    {} {}",
                b.apply_to("├─"),
                b.apply_to("Output ───────────────────────────")
            ));
            for line in output.lines() {
                lines.push(format!("    {} {}", b.apply_to("│"), line));
            }
        }

        lines.push(format!(
            "    {}",
            b.apply_to("└────────────────────────────────────")
        ));

        if let Some(h) = hint {
            lines.push(String::new());
            lines.push(format!(
                "    {} {}",
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
        // Re-claim the terminal foreground process group before each prompt.
        // Child processes (check commands, step commands) may steal the
        // foreground group when they run. After they exit, nobody restores
        // it, so our next read_key() call gets EIO. Fix: always re-claim
        // before prompting.
        #[cfg(unix)]
        claim_foreground();

        // Suspend the multi-progress so dialoguer can draw its own prompts
        // without conflicting with the pinned progress bar.
        if let Some(ref multi) = self.multi {
            multi.set_draw_target(indicatif::ProgressDrawTarget::hidden());
        }
        let result = prompt_user(prompt, &self.term);
        if let Some(ref multi) = self.multi {
            multi.set_draw_target(indicatif::ProgressDrawTarget::stderr());
        }
        result
    }
}

impl SpinnerFactory for TerminalUI {
    fn start_spinner(&mut self, message: &str) -> Box<dyn SpinnerHandle> {
        if self.mode.shows_spinners() {
            let spinner = match self.multi {
                Some(ref multi) => ProgressSpinner::with_multi(message, 0, multi),
                None => ProgressSpinner::new(message),
            };
            Box::new(spinner)
        } else {
            Box::new(ProgressSpinner::hidden())
        }
    }

    fn start_spinner_indented(&mut self, message: &str, indent: usize) -> Box<dyn SpinnerHandle> {
        if self.mode.shows_spinners() {
            let spinner = match self.multi {
                Some(ref multi) => ProgressSpinner::with_multi(message, indent, multi),
                None => ProgressSpinner::with_indent(message, indent),
            };
            Box::new(spinner)
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

impl WorkflowDisplay for TerminalUI {
    fn show_run_header(
        &mut self,
        app_name: &str,
        workflow: &str,
        step_count: usize,
        version: &str,
    ) {
        if self.mode.shows_status() {
            let step_label = if step_count == 1 { "step" } else { "steps" };
            self.println(&format!(
                "\n{} {} {} {} {} {}\n",
                self.theme.header.apply_to("⛺"),
                self.theme.highlight.apply_to(app_name),
                self.theme.dim.apply_to(format!("v{}", version)),
                self.theme.dim.apply_to("·"),
                self.theme.dim.apply_to(format!("{} workflow", workflow)),
                self.theme
                    .dim
                    .apply_to(format!("· {} {}", step_count, step_label)),
            ));
        }
    }

    fn init_workflow_progress(&mut self, total: usize) {
        if !self.mode.shows_status() {
            return;
        }
        let multi = MultiProgress::new();
        let bar = multi.add(ProgressBar::new(total as u64));
        bar.set_style(ProgressStyle::default_bar().template("{msg}").unwrap());
        // Set initial message (0/N)
        let bar_text = Self::format_bar_message(&self.theme, 0, total, Duration::ZERO);
        bar.set_message(bar_text);
        self.workflow_bar = Some(bar);
        self.multi = Some(multi);
    }

    fn show_workflow_progress(&mut self, current: usize, total: usize, elapsed: Duration) {
        if !self.mode.shows_status() {
            return;
        }
        if let Some(ref bar) = self.workflow_bar {
            let bar_text = Self::format_bar_message(&self.theme, current, total, elapsed);
            bar.set_message(bar_text);
        } else {
            // Fallback for calls without init (shouldn't happen in practice)
            let filled = if total > 0 { (current * 16) / total } else { 0 };
            let empty = 16 - filled;
            let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));
            self.println(&format!(
                "{} {}/{} steps {} {}",
                self.theme.info.apply_to(format!("[{}]", bar)),
                current,
                total,
                self.theme.dim.apply_to("·"),
                self.theme
                    .duration
                    .apply_to(format!("{} elapsed", format_duration(elapsed))),
            ));
        }
    }

    fn finish_workflow_progress(&mut self) {
        if let Some(bar) = self.workflow_bar.take() {
            bar.finish_and_clear();
        }
        if let Some(multi) = self.multi.take() {
            multi.clear().ok();
        }
    }

    fn show_run_summary(&mut self, summary: &RunSummary) {
        if !self.mode.shows_status() {
            return;
        }

        let b = &self.theme.border;
        let mut lines = vec![
            String::new(),
            format!(
                "{} {}",
                b.apply_to("┌─"),
                b.apply_to("Summary ──────────────────────────")
            ),
        ];

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

            lines.push(format!(
                "{} {} {:<20} {}",
                b.apply_to("│"),
                icon,
                step.name,
                right_side,
            ));
        }

        // Footer
        let satisfied_part = if summary.steps_satisfied > 0 {
            format!(
                " {} {} already satisfied",
                self.theme.dim.apply_to("·"),
                summary.steps_satisfied,
            )
        } else {
            String::new()
        };
        lines.push(
            b.apply_to("├────────────────────────────────────")
                .to_string(),
        );
        lines.push(format!(
            "{} Total: {} {} {} run {} {} skipped{}",
            b.apply_to("│"),
            self.theme
                .duration
                .apply_to(format_duration(summary.total_duration)),
            self.theme.dim.apply_to("·"),
            summary.steps_run,
            self.theme.dim.apply_to("·"),
            summary.steps_skipped,
            satisfied_part,
        ));
        lines.push(
            b.apply_to("└────────────────────────────────────")
                .to_string(),
        );

        for line in &lines {
            self.println(line);
        }
    }
}

impl TerminalUI {
    /// Format the progress bar message string.
    ///
    /// Produces: `\n[██████░░░░░░░░░░] 2/5 steps · 1.2s elapsed`
    /// The leading blank line provides visual padding between step output and the bar.
    fn format_bar_message(
        theme: &BivvyTheme,
        current: usize,
        total: usize,
        elapsed: Duration,
    ) -> String {
        let filled = if total > 0 { (current * 16) / total } else { 0 };
        let empty = 16 - filled;
        let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));
        format!(
            "\n{} {}/{} steps {} {}",
            theme.info.apply_to(format!("[{}]", bar)),
            current,
            total,
            theme.dim.apply_to("·"),
            theme
                .duration
                .apply_to(format!("{} elapsed", format_duration(elapsed))),
        )
    }
}

impl UiState for TerminalUI {
    fn output_mode(&self) -> OutputMode {
        self.mode
    }

    fn is_interactive(&self) -> bool {
        self.term.is_term()
    }

    fn clear_lines(&mut self, count: usize) {
        // When multi-progress is active, lines printed via multi.println()
        // are above the progress bar and can't be cleared with term ops.
        // The clear_lines calls in the orchestrator are used to collapse
        // prompt output after a selection — this still works because prompts
        // suspend the multi-progress and draw directly to the terminal.
        if !super::is_dumb_term() {
            self.term.clear_last_lines(count).ok();
        }
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
