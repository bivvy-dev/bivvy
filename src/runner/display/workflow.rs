//! Workflow-level display contract.
//!
//! Owns the run header, persistent progress bar (pinned at the bottom),
//! and the run summary. Delegates per-step rendering to
//! [`super::StepDisplay`].
//!
//! ## Construction
//!
//! - Interactive runs: [`TerminalWorkflowDisplay::new`] with an
//!   [`Arc<TerminalSurface>`] and [`OutputMode`].
//! - CI / non-interactive: [`NonInteractiveWorkflowDisplay::new`].

use std::sync::Arc;
use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};

use crate::ui::progress::format_duration;
use crate::ui::surface::{PinnedBar, TerminalSurface};
use crate::ui::theme::BivvyTheme;
use crate::ui::OutputMode;

use super::step::{NonInteractiveStepDisplay, StepDisplay, TerminalStepDisplay};
use super::{RunHeader, RunSummary};

/// Workflow-level display contract.
///
/// Implementations own the workflow's pinned progress bar (if any) and
/// produce per-step displays via [`Self::begin_step`].
pub trait WorkflowDisplay {
    /// Show the rich run header at the top of a run.
    fn show_run_header(&mut self, hdr: &RunHeader);

    /// Initialize the persistent progress bar.
    ///
    /// Called once before the step loop. Implementations that don't
    /// support a pinned bar (CI mode, silent mode) make this a no-op.
    fn start_progress(&mut self, total: usize);

    /// Update the progress bar.
    fn update_progress(&mut self, current: usize, total: usize, elapsed: Duration);

    /// Finish and clear the progress bar before the summary is rendered.
    fn finish_progress(&mut self);

    /// Show a run summary at the end of a run.
    fn show_run_summary(&mut self, summary: &RunSummary);

    /// Hand off to the step layer.
    ///
    /// The returned [`StepDisplay`] shares only the surface (or output
    /// stream) with the workflow — never any draw state.
    fn begin_step(&mut self, index: usize, total: usize) -> Box<dyn StepDisplay>;

    /// Print a status line above the pinned region (used for filter
    /// reasons, dry-run notes, etc.).
    fn message(&mut self, msg: &str);

    /// Print a hint (post-run guidance) above the pinned region.
    fn hint(&mut self, hint: &str);
}

// ────────────────────────── Interactive impl ──────────────────────────

/// Interactive workflow display backed by a [`TerminalSurface`].
pub struct TerminalWorkflowDisplay {
    surface: Arc<TerminalSurface>,
    theme: BivvyTheme,
    mode: OutputMode,
    pinned: Option<PinnedBar>,
}

impl TerminalWorkflowDisplay {
    /// Construct a new terminal workflow display.
    pub fn new(surface: Arc<TerminalSurface>, mode: OutputMode) -> Self {
        let theme = if crate::ui::should_use_colors() {
            BivvyTheme::new()
        } else {
            BivvyTheme::plain()
        };
        Self {
            surface,
            theme,
            mode,
            pinned: None,
        }
    }

    /// Format the bar message string with bracket bar + counts.
    fn format_bar(theme: &BivvyTheme, current: usize, total: usize, elapsed: Duration) -> String {
        let filled = if total > 0 { (current * 16) / total } else { 0 };
        let empty = 16usize.saturating_sub(filled);
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

impl WorkflowDisplay for TerminalWorkflowDisplay {
    fn show_run_header(&mut self, hdr: &RunHeader) {
        if !self.mode.shows_status() {
            return;
        }
        let step_label = if hdr.step_count == 1 { "step" } else { "steps" };
        self.surface.println(&format!(
            "\n{} {} {} {} {} {} {}\n",
            self.theme.header.apply_to("⛺"),
            self.theme.highlight.apply_to(&hdr.app_name),
            self.theme.dim.apply_to(format!("v{}", hdr.version)),
            self.theme.dim.apply_to("·"),
            self.theme
                .dim
                .apply_to(format!("{} workflow", hdr.workflow)),
            self.theme
                .dim
                .apply_to(format!("· {} {}", hdr.step_count, step_label)),
            self.theme.dim.apply_to(format!("· env: {}", hdr.env_name)),
        ));
    }

    fn start_progress(&mut self, total: usize) {
        if !self.mode.shows_status() {
            return;
        }
        let bar = ProgressBar::new(total as u64);
        bar.set_style(ProgressStyle::default_bar().template("{msg}").unwrap());
        bar.set_message(Self::format_bar(&self.theme, 0, total, Duration::ZERO));
        self.pinned = Some(self.surface.pin_bottom(bar));
    }

    fn update_progress(&mut self, current: usize, total: usize, elapsed: Duration) {
        if !self.mode.shows_status() {
            return;
        }
        if let Some(p) = &self.pinned {
            p.set_message(Self::format_bar(&self.theme, current, total, elapsed));
        }
    }

    fn finish_progress(&mut self) {
        // Drop clears the bar.
        self.pinned = None;
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
            self.surface.println(line);
        }
    }

    fn begin_step(&mut self, index: usize, total: usize) -> Box<dyn StepDisplay> {
        Box::new(TerminalStepDisplay::new(
            Arc::clone(&self.surface),
            self.mode,
            index,
            total,
        ))
    }

    fn message(&mut self, msg: &str) {
        if self.mode.shows_status() {
            self.surface.println(msg);
        }
    }

    fn hint(&mut self, hint: &str) {
        if !self.mode.shows_status() {
            return;
        }
        self.surface
            .println(&self.theme.hint.apply_to(hint).to_string());
        self.surface.println("");
    }
}

// ─────────────────────── Non-interactive impl ────────────────────────

/// Non-interactive (CI / headless) workflow display.
///
/// Prints structured lines to stdout/stderr; skips the pinned progress
/// bar entirely.
pub struct NonInteractiveWorkflowDisplay {
    mode: OutputMode,
    is_ci: bool,
}

impl NonInteractiveWorkflowDisplay {
    /// Construct a new non-interactive workflow display.
    pub fn new(mode: OutputMode) -> Self {
        Self {
            mode,
            is_ci: crate::shell::is_ci(),
        }
    }

    /// Construct a non-interactive workflow display with an explicit CI
    /// flag (test override).
    pub fn with_ci(mode: OutputMode, is_ci: bool) -> Self {
        Self { mode, is_ci }
    }
}

impl WorkflowDisplay for NonInteractiveWorkflowDisplay {
    fn show_run_header(&mut self, hdr: &RunHeader) {
        if !self.mode.shows_status() {
            return;
        }
        let step_label = if hdr.step_count == 1 { "step" } else { "steps" };
        println!(
            "\n⛺ {} v{} · {} workflow · {} {} · env: {}\n",
            hdr.app_name, hdr.version, hdr.workflow, hdr.step_count, step_label, hdr.env_name,
        );
    }

    fn start_progress(&mut self, _total: usize) {}

    fn update_progress(&mut self, current: usize, total: usize, elapsed: Duration) {
        if self.is_ci {
            return;
        }
        if !self.mode.shows_status() {
            return;
        }
        let filled = if total > 0 { (current * 16) / total } else { 0 };
        let empty = 16usize.saturating_sub(filled);
        let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));
        println!(
            "  [{}] {}/{} steps · {} elapsed",
            bar,
            current,
            total,
            format_duration(elapsed),
        );
    }

    fn finish_progress(&mut self) {}

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
        let satisfied_part = if summary.steps_satisfied > 0 {
            format!(" · {} already satisfied", summary.steps_satisfied)
        } else {
            String::new()
        };
        println!(
            "  │ Total: {} · {} run · {} skipped{}",
            format_duration(summary.total_duration),
            summary.steps_run,
            summary.steps_skipped,
            satisfied_part,
        );
        println!("  └────────────────────────────────────");
        // No trailing "Setup complete!" / "Setup failed" line — see the
        // matching note in `TerminalWorkflowDisplay::show_run_summary`.
    }

    fn begin_step(&mut self, index: usize, total: usize) -> Box<dyn StepDisplay> {
        Box::new(NonInteractiveStepDisplay::new(self.mode, index, total))
    }

    fn message(&mut self, msg: &str) {
        if self.mode.shows_status() {
            println!("{}", msg);
        }
    }

    fn hint(&mut self, hint: &str) {
        if self.mode.shows_status() {
            println!("  💡 {}", hint);
        }
    }
}

// ────────────────────────── Mock impl ──────────────────────────────

use std::cell::RefCell;
use std::rc::Rc;

/// Shared buffer between [`MockWorkflowDisplay`] and the
/// [`MockStepDisplay`]s it spawns. Tests can read this through the
/// workflow display to assert on every captured interaction.
#[derive(Default)]
pub struct MockState {
    /// Run headers shown.
    pub headers: Vec<RunHeader>,
    /// Run summaries shown.
    pub summaries: Vec<RunSummary>,
    /// Workflow progress events (current, total, elapsed).
    pub progress_events: Vec<(usize, usize, Duration)>,
    /// Workflow-level messages (above the progress region).
    pub workflow_messages: Vec<String>,
    /// Workflow-level hints.
    pub workflow_hints: Vec<String>,
    /// Per-step messages captured from `MockStepDisplay`.
    pub step_messages: Vec<String>,
    /// Per-step warnings.
    pub step_warnings: Vec<String>,
    /// Per-step error blocks (command, output, hint, indent).
    pub step_error_blocks: Vec<(String, String, Option<String>, usize)>,
    /// Per-step success lines (formatted).
    pub step_successes: Vec<String>,
    /// Per-step error lines (formatted).
    pub step_errors: Vec<String>,
    /// Per-step skipped lines (formatted).
    pub step_skipped: Vec<String>,
}

/// In-memory workflow display for tests.
///
/// Captures every interaction (workflow- and step-level) so assertions
/// can be made without depending on stdout capture.
pub struct MockWorkflowDisplay {
    state: Rc<RefCell<MockState>>,
}

impl Default for MockWorkflowDisplay {
    fn default() -> Self {
        Self::new()
    }
}

impl MockWorkflowDisplay {
    /// Construct a new mock workflow display.
    pub fn new() -> Self {
        Self {
            state: Rc::new(RefCell::new(MockState::default())),
        }
    }

    /// Borrow the captured state for assertions.
    pub fn state(&self) -> std::cell::Ref<'_, MockState> {
        self.state.borrow()
    }

    /// Whether any captured message contains `needle`. Searches across
    /// workflow- and step-level messages.
    pub fn has_message(&self, needle: &str) -> bool {
        let s = self.state.borrow();
        s.workflow_messages.iter().any(|m| m.contains(needle))
            || s.step_messages.iter().any(|m| m.contains(needle))
    }

    /// Whether any captured success line contains `needle`.
    pub fn has_success(&self, needle: &str) -> bool {
        let s = self.state.borrow();
        s.step_successes.iter().any(|m| m.contains(needle))
    }

    /// Whether any captured error contains `needle`.
    pub fn has_error(&self, needle: &str) -> bool {
        let s = self.state.borrow();
        s.step_errors.iter().any(|m| m.contains(needle))
    }

    /// Whether any captured warning contains `needle`.
    pub fn has_warning(&self, needle: &str) -> bool {
        let s = self.state.borrow();
        s.step_warnings.iter().any(|m| m.contains(needle))
    }

    /// All captured run headers.
    pub fn headers(&self) -> Vec<RunHeader> {
        self.state.borrow().headers.clone()
    }

    /// All captured run summaries.
    pub fn summaries(&self) -> Vec<RunSummary> {
        self.state.borrow().summaries.clone()
    }
}

impl WorkflowDisplay for MockWorkflowDisplay {
    fn show_run_header(&mut self, hdr: &RunHeader) {
        self.state.borrow_mut().headers.push(hdr.clone());
    }

    fn start_progress(&mut self, _total: usize) {}

    fn update_progress(&mut self, current: usize, total: usize, elapsed: Duration) {
        self.state
            .borrow_mut()
            .progress_events
            .push((current, total, elapsed));
    }

    fn finish_progress(&mut self) {}

    fn show_run_summary(&mut self, summary: &RunSummary) {
        self.state.borrow_mut().summaries.push(summary.clone());
    }

    fn begin_step(&mut self, index: usize, total: usize) -> Box<dyn StepDisplay> {
        Box::new(MockStepDisplay::new(Rc::clone(&self.state), index, total))
    }

    fn message(&mut self, msg: &str) {
        self.state
            .borrow_mut()
            .workflow_messages
            .push(msg.to_string());
    }

    fn hint(&mut self, hint: &str) {
        self.state
            .borrow_mut()
            .workflow_hints
            .push(hint.to_string());
    }
}

/// Per-step mock display that writes to a shared [`MockState`].
pub struct MockStepDisplay {
    state: Rc<RefCell<MockState>>,
    step_number: String,
    step_indent: usize,
}

impl MockStepDisplay {
    /// Construct a new mock step display sharing state with its
    /// parent [`MockWorkflowDisplay`].
    pub fn new(state: Rc<RefCell<MockState>>, index: usize, total: usize) -> Self {
        let step_number = format!("[{}/{}]", index + 1, total);
        let step_indent = step_number.chars().count() + 1;
        Self {
            state,
            step_number,
            step_indent,
        }
    }
}

impl crate::ui::OutputWriter for MockStepDisplay {
    fn message(&mut self, msg: &str) {
        self.state.borrow_mut().step_messages.push(msg.to_string());
    }

    fn success(&mut self, msg: &str) {
        self.state.borrow_mut().step_successes.push(msg.to_string());
    }

    fn warning(&mut self, msg: &str) {
        self.state.borrow_mut().step_warnings.push(msg.to_string());
    }

    fn error(&mut self, msg: &str) {
        self.state.borrow_mut().step_errors.push(msg.to_string());
    }

    fn show_error_block(&mut self, command: &str, output: &str, hint: Option<&str>, indent: usize) {
        self.state.borrow_mut().step_error_blocks.push((
            command.to_string(),
            output.to_string(),
            hint.map(str::to_string),
            indent,
        ));
    }
}

impl crate::ui::Prompter for MockStepDisplay {
    fn prompt(
        &mut self,
        prompt: &crate::ui::Prompt,
    ) -> crate::error::Result<crate::ui::PromptResult> {
        // Default mock prompt: use the prompt's default value if
        // provided, otherwise return a blank result. Real test mocks
        // pass through MockUI / dialog stubs.
        let is_multiselect = matches!(
            prompt.prompt_type,
            crate::ui::PromptType::MultiSelect { .. }
        );
        if let Some(default) = &prompt.default {
            if is_multiselect {
                let values: Vec<String> =
                    default.split(',').map(|s| s.trim().to_string()).collect();
                return Ok(crate::ui::PromptResult::Strings(values));
            }
            return Ok(crate::ui::PromptResult::String(default.clone()));
        }
        Ok(crate::ui::PromptResult::String(String::new()))
    }
}

impl super::step::StepDisplay for MockStepDisplay {
    fn show_step_header(&mut self, step_name: &str, _title: Option<&str>) {
        self.state
            .borrow_mut()
            .step_messages
            .push(format!("{} {}", self.step_number, step_name));
    }

    fn start_running(&mut self, command: &str) {
        self.state
            .borrow_mut()
            .step_messages
            .push(format!("Running `{}`...", command));
    }

    fn update_live_output(&mut self, _line: crate::shell::OutputLine) {}

    fn finish(
        &mut self,
        status: crate::steps::StepStatus,
        duration: Option<Duration>,
        _detail: Option<&str>,
    ) {
        let label = status.label();
        let body = match duration {
            Some(d) => format!("{} ({})", label, crate::ui::format_duration(d)),
            None => label.to_string(),
        };
        let mut state = self.state.borrow_mut();
        match status {
            crate::steps::StepStatus::Completed => state.step_successes.push(body),
            crate::steps::StepStatus::Failed => state.step_errors.push(body),
            crate::steps::StepStatus::Skipped => state.step_skipped.push(body),
            _ => state.step_messages.push(body),
        }
    }

    fn finish_and_clear(&mut self) {}

    fn output_mode(&self) -> OutputMode {
        OutputMode::Normal
    }

    fn step_indent(&self) -> usize {
        self.step_indent
    }

    fn step_number(&self) -> &str {
        &self.step_number
    }

    fn is_interactive(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header() -> RunHeader {
        RunHeader {
            app_name: "Test".to_string(),
            version: "1.0.0".to_string(),
            workflow: "default".to_string(),
            step_count: 3,
            env_name: "development".to_string(),
        }
    }

    #[test]
    fn terminal_workflow_display_construction_does_not_panic() {
        let surface = TerminalSurface::hidden();
        let _wd = TerminalWorkflowDisplay::new(surface, OutputMode::Normal);
    }

    #[test]
    fn terminal_progress_lifecycle() {
        let surface = TerminalSurface::hidden();
        let mut wd = TerminalWorkflowDisplay::new(surface, OutputMode::Normal);
        wd.start_progress(5);
        wd.update_progress(1, 5, Duration::from_secs(1));
        wd.update_progress(5, 5, Duration::from_secs(5));
        wd.finish_progress();
        // No panic = pass.
    }

    #[test]
    fn terminal_silent_mode_skips_progress() {
        let surface = TerminalSurface::hidden();
        let mut wd = TerminalWorkflowDisplay::new(surface, OutputMode::Silent);
        wd.start_progress(5);
        // Nothing pinned in silent mode.
        assert!(wd.pinned.is_none());
    }

    #[test]
    fn terminal_show_header_silent_skipped() {
        let surface = TerminalSurface::hidden();
        let mut wd = TerminalWorkflowDisplay::new(surface, OutputMode::Silent);
        wd.show_run_header(&header());
        // No assertion on output (hidden surface), but must not panic.
    }

    #[test]
    fn terminal_summary_renders_without_panic() {
        let surface = TerminalSurface::hidden();
        let mut wd = TerminalWorkflowDisplay::new(surface, OutputMode::Normal);
        let summary = RunSummary {
            step_results: vec![],
            total_duration: Duration::from_secs(2),
            steps_run: 2,
            steps_skipped: 0,
            steps_satisfied: 0,
            success: true,
            failed_steps: vec![],
        };
        wd.show_run_summary(&summary);
    }

    #[test]
    fn terminal_begin_step_returns_step_display() {
        let surface = TerminalSurface::hidden();
        let mut wd = TerminalWorkflowDisplay::new(surface, OutputMode::Normal);
        let _step = wd.begin_step(0, 3);
    }

    #[test]
    fn non_interactive_begin_step_returns_step_display() {
        let mut wd = NonInteractiveWorkflowDisplay::with_ci(OutputMode::Normal, false);
        let _step = wd.begin_step(0, 3);
    }

    #[test]
    fn non_interactive_ci_mode_skips_progress() {
        let mut wd = NonInteractiveWorkflowDisplay::with_ci(OutputMode::Normal, true);
        wd.update_progress(1, 3, Duration::from_secs(1));
        // Must not print in CI mode; no assertion since stdout capture
        // is awkward to set up — exercising the early return path is
        // still useful coverage.
    }
}
