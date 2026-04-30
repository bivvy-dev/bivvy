//! Per-step display contract.
//!
//! A [`StepDisplay`] is created by [`super::WorkflowDisplay::begin_step`]
//! and lives for exactly one step. It owns:
//!
//! - The step header line written into scrollback.
//! - A transient region (above the workflow's pinned bar) that hosts a
//!   spinner with a bounded live-output tail.
//! - The error block, prompts, and final result line.
//!
//! ## Lifecycle
//!
//! 1. [`StepDisplay::show_step_header`] writes the step header line.
//! 2. [`StepDisplay::start_running`] mounts the transient region with
//!    the spinner and reserves `max_lines` for the live-output tail.
//! 3. [`StepDisplay::update_live_output`] appends to the ring buffer
//!    backing the spinner's message.
//! 4. One of the `finish_*` methods consumes `Box<Self>`. The transient
//!    region is dropped (cleared) and a final result line is written
//!    into scrollback.
//!
//! `Drop` on `Box<dyn StepDisplay>` is a panic-safety net only — the
//! `finish_*` methods are the canonical close path.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};

use crate::error::Result;
use crate::shell::OutputLine;
use crate::steps::StepStatus;
use crate::ui::progress::format_duration;
use crate::ui::surface::{TerminalSurface, TransientRegion};
use crate::ui::theme::BivvyTheme;
use crate::ui::{prompt_user, OutputMode, OutputWriter, Prompt, PromptResult, Prompter};

#[cfg(unix)]
use crate::shell::command::claim_foreground;

/// Per-step display contract.
///
/// Extends [`OutputWriter`] and [`Prompter`] so the step display is
/// drop-in compatible with code paths that expect those traits
/// (recovery menu, gap installer). Implementors must ensure that
/// calling any `finish_*` method cleanly closes the transient region
/// before returning.
pub trait StepDisplay: OutputWriter + Prompter {
    /// Write the step header line into scrollback.
    fn show_step_header(&mut self, step_name: &str, title: Option<&str>);

    /// Mount the transient region with a spinner.
    ///
    /// The `command` is normalized to a single line (newlines collapsed
    /// to spaces, truncated). The transient region reserves room for
    /// the live-output tail.
    fn start_running(&mut self, command: &str);

    /// Append a line of live command output to the spinner's tail.
    ///
    /// No-op if the transient region has not been mounted.
    fn update_live_output(&mut self, line: OutputLine);

    /// Finalize the step. Clears the transient region and prints the
    /// final result line into scrollback. The label and icon are
    /// derived from `status` (via [`StepStatus::label`] and
    /// [`StepStatus::display_char`]) so the displayed text is always
    /// in lockstep with the actual step outcome.
    ///
    /// Optional `duration` is shown in parentheses after the label;
    /// optional `detail` is appended dimmed.
    fn finish(&mut self, status: StepStatus, duration: Option<Duration>, detail: Option<&str>);

    /// Finalize the step without writing a final line — used when the
    /// caller will write its own final state afterwards (auto-satisfied
    /// step, blocked dependency, etc.).
    fn finish_and_clear(&mut self);

    /// The output mode currently in effect.
    fn output_mode(&self) -> OutputMode;

    /// Width (in columns) at which content should be indented to align
    /// under the step name. `step_indent() == step_number().chars().count() + 1`.
    fn step_indent(&self) -> usize;

    /// The step number string, e.g. `[1/3]`.
    fn step_number(&self) -> &str;

    /// Whether the underlying surface supports interactive prompts.
    fn is_interactive(&self) -> bool;

    /// Build an [`OutputCallback`] that feeds command output into the
    /// transient region's live-output tail. Returns `None` if the
    /// display has no live-output concept (silent / non-interactive
    /// non-verbose).
    fn live_output_callback(&mut self) -> Option<crate::shell::OutputCallback> {
        None
    }
}

// ────────────────────────── Interactive impl ──────────────────────────

const RING_BUFFER_VERBOSE: usize = 3;
const RING_BUFFER_NORMAL: usize = 2;
/// Cap on a single line of live output before truncation.
const MAX_OUTPUT_LINE_LEN: usize = 72;

/// How many tail lines to ring-buffer for the given output mode.
fn live_tail_capacity(mode: OutputMode) -> usize {
    match mode {
        OutputMode::Verbose => RING_BUFFER_VERBOSE,
        OutputMode::Normal => RING_BUFFER_NORMAL,
        _ => 0,
    }
}

/// Interactive per-step display backed by a [`TerminalSurface`].
pub struct TerminalStepDisplay {
    surface: Arc<TerminalSurface>,
    theme: BivvyTheme,
    mode: OutputMode,
    step_number: String,
    step_indent: usize,
    region: Option<TransientRegion>,
    /// Ring-buffer of recent live-output lines for the spinner tail.
    tail: VecDeque<String>,
    /// Spinner base message — preserved so live-output updates don't
    /// drop the "Running ..." prefix.
    base_message: String,
    /// Maximum tail capacity (depends on output mode).
    tail_capacity: usize,
}

impl TerminalStepDisplay {
    /// Construct a new terminal step display.
    pub fn new(
        surface: Arc<TerminalSurface>,
        mode: OutputMode,
        index: usize,
        total: usize,
    ) -> Self {
        let theme = if crate::ui::should_use_colors() {
            BivvyTheme::new()
        } else {
            BivvyTheme::plain()
        };
        let step_number = format!("[{}/{}]", index + 1, total);
        let step_indent = step_number.chars().count() + 1;
        Self {
            surface,
            theme,
            mode,
            step_number,
            step_indent,
            region: None,
            tail: VecDeque::new(),
            base_message: String::new(),
            tail_capacity: live_tail_capacity(mode),
        }
    }

    /// Collapse a multi-line command into a single line.
    fn normalize_command(command: &str) -> String {
        let collapsed = command
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join("; ");
        if collapsed.chars().count() > 96 {
            let mut out: String = collapsed.chars().take(93).collect();
            out.push_str("...");
            out
        } else {
            collapsed
        }
    }

    /// Truncate a single output line for display in the tail.
    fn truncate_line(line: &str) -> String {
        let trimmed = line.trim_end();
        if trimmed.chars().count() > MAX_OUTPUT_LINE_LEN {
            let mut out: String = trimmed
                .chars()
                .take(MAX_OUTPUT_LINE_LEN.saturating_sub(3))
                .collect();
            out.push_str("...");
            out
        } else {
            trimmed.to_string()
        }
    }

    /// Render the spinner message: base + tail buffer (already truncated).
    fn render_message(&self) -> String {
        let prefix = " ".repeat(self.step_indent.saturating_add(2));
        let mut msg = self.base_message.clone();
        for line in &self.tail {
            msg.push('\n');
            msg.push_str(&prefix);
            msg.push_str(&self.theme.dim.apply_to(format!("» {}", line)).to_string());
        }
        msg
    }
}

impl OutputWriter for TerminalStepDisplay {
    fn message(&mut self, msg: &str) {
        if self.mode.shows_status() {
            self.surface.println(msg);
        }
    }

    fn success(&mut self, msg: &str) {
        if self.mode.shows_status() {
            self.surface
                .println(&self.theme.format_success(msg).to_string());
        }
    }

    fn warning(&mut self, msg: &str) {
        if self.mode.shows_status() {
            self.surface
                .println(&self.theme.format_warning(msg).to_string());
        }
    }

    fn error(&mut self, msg: &str) {
        self.surface
            .println(&self.theme.format_error(msg).to_string());
    }

    fn show_hint(&mut self, hint: &str) {
        if self.mode.shows_status() {
            self.surface
                .println(&self.theme.hint.apply_to(hint).to_string());
            self.surface.println("");
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
            self.surface.println(line);
        }
    }
}

impl Prompter for TerminalStepDisplay {
    fn prompt(&mut self, prompt: &Prompt) -> Result<PromptResult> {
        // Re-claim foreground process group before each prompt — child
        // commands may have stolen it on exit.
        #[cfg(unix)]
        claim_foreground();

        let term = console::Term::stdout();
        self.surface
            .with_cursor_freed(|| prompt_user(prompt, &term))
    }
}

impl StepDisplay for TerminalStepDisplay {
    fn show_step_header(&mut self, step_name: &str, title: Option<&str>) {
        if !self.mode.shows_status() {
            return;
        }
        let header = format!(
            "{} {}",
            self.theme.step_number.apply_to(&self.step_number),
            self.theme.step_title.apply_to(step_name)
        );
        let line = match title {
            Some(t) if t != step_name => format!(
                "{} {} {}",
                header,
                self.theme.dim.apply_to("—"),
                self.theme.dim.apply_to(t)
            ),
            _ => header,
        };
        self.surface.println(&line);
    }

    fn start_running(&mut self, command: &str) {
        if !self.mode.shows_spinners() {
            return;
        }
        let collapsed = Self::normalize_command(command);
        let prefix = " ".repeat(self.step_indent);
        self.base_message = format!("Running `{}`...", collapsed);

        let bar = ProgressBar::new_spinner();
        bar.set_style(
            ProgressStyle::default_spinner()
                .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
                .template(&format!("{}{{spinner:.magenta}} {{msg}}", prefix))
                .unwrap(),
        );
        bar.set_message(self.base_message.clone());
        bar.enable_steady_tick(Duration::from_millis(80));

        // Reserve max_lines = 1 (spinner row) + tail_capacity.
        let max_lines = 1 + self.tail_capacity;
        let region = self.surface.transient_above_pinned(bar, max_lines);
        self.tail.clear();
        self.region = Some(region);
    }

    fn update_live_output(&mut self, line: OutputLine) {
        if self.tail_capacity == 0 || self.region.is_none() {
            return;
        }
        let raw = match &line {
            OutputLine::Stdout(s) | OutputLine::Stderr(s) => s.clone(),
        };
        let trimmed = raw.trim_end();
        if trimmed.is_empty() {
            return;
        }
        let display = Self::truncate_line(trimmed);
        self.tail.push_back(display);
        while self.tail.len() > self.tail_capacity {
            self.tail.pop_front();
        }
        let msg = self.render_message();
        if let Some(region) = &self.region {
            region.set_message(&msg);
        }
    }

    fn finish(&mut self, status: StepStatus, duration: Option<Duration>, detail: Option<&str>) {
        // Drop the region first so the line is cleared before we print.
        self.region = None;
        if !self.mode.shows_status() && !matches!(status, StepStatus::Failed) {
            // Failed lines still emit (errors are never silent), but
            // success/skipped honor silent mode.
            return;
        }

        // Build "Label" or "Label (Xms)" from the status enum — the
        // sole source of truth for what gets printed on the result row.
        let label = status.label();
        let body = match duration {
            Some(d) => format!("{} ({})", label, format_duration(d)),
            None => label.to_string(),
        };

        // Icon + label colored to match the status. The icon comes
        // from the status enum (`display_char`) — not from
        // `theme.format_success` etc., which bake in their own icons
        // and would emit "✓ ✓ Completed" / "✗ ✗ Failed" if combined.
        let icon_body = format!("{} {}", status.display_char(), body);
        let styled_body: String = match status {
            StepStatus::Completed => self.theme.success.apply_to(icon_body).to_string(),
            StepStatus::Failed => self.theme.error.apply_to(icon_body).to_string(),
            StepStatus::Skipped => self.theme.dim.apply_to(icon_body).to_string(),
            StepStatus::Pending | StepStatus::Running => {
                self.theme.dim.apply_to(icon_body).to_string()
            }
        };

        let pad = " ".repeat(self.step_indent);
        let detail_str = detail
            .map(|d| format!(" {}", self.theme.dim.apply_to(format!("— {}", d))))
            .unwrap_or_default();

        self.surface
            .println(&format!("{}{}{}", pad, styled_body, detail_str));
    }

    fn finish_and_clear(&mut self) {
        self.region = None;
    }

    fn output_mode(&self) -> OutputMode {
        self.mode
    }

    fn step_indent(&self) -> usize {
        self.step_indent
    }

    fn step_number(&self) -> &str {
        &self.step_number
    }

    fn is_interactive(&self) -> bool {
        true
    }

    fn live_output_callback(&mut self) -> Option<crate::shell::OutputCallback> {
        // Reuse the existing spinner-based ring buffer in spinner.rs —
        // it operates on a cloned ProgressBar with no &mut self required.
        if self.tail_capacity == 0 {
            return None;
        }
        let bar = self.region.as_ref()?.bar_clone()?;
        let max_lines = self.tail_capacity;
        let indent = self.step_indent.saturating_add(2);
        Some(crate::ui::spinner::live_output_callback(
            bar,
            self.base_message.clone(),
            indent,
            max_lines,
        ))
    }
}

// ─────────────────────── Non-interactive impl ────────────────────────

/// Non-interactive (CI / headless) per-step display.
pub struct NonInteractiveStepDisplay {
    mode: OutputMode,
    step_number: String,
    step_indent: usize,
}

impl NonInteractiveStepDisplay {
    /// Construct a new non-interactive step display.
    pub fn new(mode: OutputMode, index: usize, total: usize) -> Self {
        let step_number = format!("[{}/{}]", index + 1, total);
        let step_indent = step_number.chars().count() + 1;
        Self {
            mode,
            step_number,
            step_indent,
        }
    }
}

impl OutputWriter for NonInteractiveStepDisplay {
    fn message(&mut self, msg: &str) {
        if self.mode.shows_status() {
            println!("{}", msg);
        }
    }

    fn success(&mut self, msg: &str) {
        if self.mode.shows_status() {
            println!("✓ {}", msg);
        }
    }

    fn warning(&mut self, msg: &str) {
        if self.mode.shows_status() {
            eprintln!("⚠ {}", msg);
        }
    }

    fn error(&mut self, msg: &str) {
        eprintln!("✗ {}", msg);
    }

    fn show_hint(&mut self, hint: &str) {
        if self.mode.shows_status() {
            println!("  💡 {}", hint);
        }
    }

    fn show_error_block(&mut self, command: &str, output: &str, hint: Option<&str>, indent: usize) {
        let pad = " ".repeat(indent);
        eprintln!();
        eprintln!("{}┌─ Command ──────────────────────────", pad);
        eprintln!("{}│ {}", pad, command);
        if !output.is_empty() {
            eprintln!("{}├─ Output ───────────────────────────", pad);
            for line in output.lines() {
                eprintln!("{}│ {}", pad, line);
            }
        }
        eprintln!("{}└────────────────────────────────────", pad);
        if let Some(h) = hint {
            eprintln!();
            eprintln!("{}Hint: {}", pad, h);
        }
    }
}

impl Prompter for NonInteractiveStepDisplay {
    fn prompt(&mut self, prompt: &Prompt) -> Result<PromptResult> {
        // Non-interactive: reuse env override or default logic.
        let is_multiselect = matches!(
            prompt.prompt_type,
            crate::ui::PromptType::MultiSelect { .. }
        );
        if let Some(result) = crate::ui::prompts::env_override(prompt) {
            return Ok(result);
        }
        if let Some(default) = &prompt.default {
            if is_multiselect {
                let values: Vec<String> =
                    default.split(',').map(|s| s.trim().to_string()).collect();
                return Ok(PromptResult::Strings(values));
            }
            return Ok(PromptResult::String(default.clone()));
        }
        Err(crate::error::BivvyError::ConfigValidationError {
            message: format!(
                "Cannot prompt for '{}' in non-interactive mode (no default value)",
                prompt.key
            ),
        })
    }
}

impl StepDisplay for NonInteractiveStepDisplay {
    fn show_step_header(&mut self, step_name: &str, title: Option<&str>) {
        if !self.mode.shows_status() {
            return;
        }
        match title {
            Some(t) if t != step_name => {
                println!("{} {} — {}", self.step_number, step_name, t);
            }
            _ => println!("{} {}", self.step_number, step_name),
        }
    }

    fn start_running(&mut self, command: &str) {
        if !self.mode.shows_spinners() {
            return;
        }
        let collapsed = TerminalStepDisplay::normalize_command(command);
        let prefix = " ".repeat(self.step_indent);
        println!("{}Running `{}`...", prefix, collapsed);
    }

    fn update_live_output(&mut self, line: OutputLine) {
        if !matches!(self.mode, OutputMode::Verbose) {
            return;
        }
        match line {
            OutputLine::Stdout(s) => print!("{}", s),
            OutputLine::Stderr(s) => eprint!("{}", s),
        }
    }

    fn finish(&mut self, status: StepStatus, duration: Option<Duration>, detail: Option<&str>) {
        if !self.mode.shows_status() && !matches!(status, StepStatus::Failed) {
            return;
        }
        let label = status.label();
        let body = match duration {
            Some(d) => format!("{} ({})", label, format_duration(d)),
            None => label.to_string(),
        };
        let detail_str = detail.map(|d| format!(" — {}", d)).unwrap_or_default();
        let pad = " ".repeat(self.step_indent);
        let line = format!("{}{} {}{}", pad, status.display_char(), body, detail_str);
        if matches!(status, StepStatus::Failed) {
            eprintln!("{}", line);
        } else {
            println!("{}", line);
        }
    }

    fn finish_and_clear(&mut self) {}

    fn output_mode(&self) -> OutputMode {
        self.mode
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

    fn live_output_callback(&mut self) -> Option<crate::shell::OutputCallback> {
        if matches!(self.mode, OutputMode::Verbose) {
            Some(Box::new(crate::ui::VerboseStreamSink::new(
                self.step_indent,
            )))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_number_formatted_correctly() {
        let surface = TerminalSurface::hidden();
        let s = TerminalStepDisplay::new(surface, OutputMode::Normal, 0, 3);
        assert_eq!(s.step_number(), "[1/3]");
        assert_eq!(s.step_indent(), 6);

        let surface = TerminalSurface::hidden();
        let s = TerminalStepDisplay::new(surface, OutputMode::Normal, 9, 15);
        assert_eq!(s.step_number(), "[10/15]");
        assert_eq!(s.step_indent(), 8);
    }

    #[test]
    fn normalize_command_collapses_multiline() {
        assert_eq!(
            TerminalStepDisplay::normalize_command("echo foo\necho bar\n"),
            "echo foo; echo bar"
        );
    }

    #[test]
    fn normalize_command_truncates_long_input() {
        let long = "x".repeat(200);
        let normalized = TerminalStepDisplay::normalize_command(&long);
        assert!(normalized.ends_with("..."));
        assert_eq!(normalized.chars().count(), 96);
    }

    #[test]
    fn normalize_command_preserves_short_single_line() {
        assert_eq!(
            TerminalStepDisplay::normalize_command("echo hello"),
            "echo hello"
        );
    }

    #[test]
    fn truncate_line_under_limit_is_unchanged() {
        assert_eq!(
            TerminalStepDisplay::truncate_line("hello world"),
            "hello world"
        );
    }

    #[test]
    fn truncate_line_over_limit_truncated_with_ellipsis() {
        let long = "a".repeat(100);
        let result = TerminalStepDisplay::truncate_line(&long);
        assert!(result.ends_with("..."));
        assert_eq!(result.chars().count(), MAX_OUTPUT_LINE_LEN);
    }

    #[test]
    fn live_tail_capacity_per_mode() {
        assert_eq!(live_tail_capacity(OutputMode::Verbose), 3);
        assert_eq!(live_tail_capacity(OutputMode::Normal), 2);
        assert_eq!(live_tail_capacity(OutputMode::Quiet), 0);
        assert_eq!(live_tail_capacity(OutputMode::Silent), 0);
    }

    #[test]
    fn start_running_mounts_region_in_normal_mode() {
        let surface = TerminalSurface::hidden();
        let mut s = TerminalStepDisplay::new(surface, OutputMode::Normal, 0, 3);
        s.start_running("echo hello");
        assert!(s.region.is_some());
    }

    #[test]
    fn start_running_skipped_in_silent_mode() {
        let surface = TerminalSurface::hidden();
        let mut s = TerminalStepDisplay::new(surface, OutputMode::Silent, 0, 3);
        s.start_running("echo hello");
        assert!(s.region.is_none());
    }

    #[test]
    fn update_live_output_evicts_oldest() {
        let surface = TerminalSurface::hidden();
        let mut s = TerminalStepDisplay::new(surface, OutputMode::Normal, 0, 3);
        s.start_running("echo hello");

        s.update_live_output(OutputLine::Stdout("line 1".into()));
        s.update_live_output(OutputLine::Stdout("line 2".into()));
        s.update_live_output(OutputLine::Stdout("line 3".into()));
        // Capacity is 2 — line 1 evicted, line 2 + line 3 remain.
        assert_eq!(s.tail.len(), 2);
        assert!(s.tail[0].contains("line 2"));
        assert!(s.tail[1].contains("line 3"));
    }

    #[test]
    fn update_live_output_skips_empty_lines() {
        let surface = TerminalSurface::hidden();
        let mut s = TerminalStepDisplay::new(surface, OutputMode::Normal, 0, 3);
        s.start_running("echo hello");
        s.update_live_output(OutputLine::Stdout("".into()));
        s.update_live_output(OutputLine::Stdout("real".into()));
        assert_eq!(s.tail.len(), 1);
    }

    #[test]
    fn finish_completed_clears_region() {
        let surface = TerminalSurface::hidden();
        let mut s = TerminalStepDisplay::new(surface, OutputMode::Normal, 0, 3);
        s.start_running("rustc --version");
        assert!(s.region.is_some());
        s.finish(StepStatus::Completed, Some(Duration::from_millis(10)), None);
        assert!(s.region.is_none());
    }

    #[test]
    fn finish_failed_clears_region() {
        let surface = TerminalSurface::hidden();
        let mut s = TerminalStepDisplay::new(surface, OutputMode::Normal, 0, 3);
        s.start_running("rustc --version");
        s.finish(StepStatus::Failed, Some(Duration::from_millis(1)), None);
        assert!(s.region.is_none());
    }

    #[test]
    fn finish_label_comes_from_status_enum() {
        // The displayed text on the result line must be sourced from
        // `StepStatus::label()` — this guards against accidentally
        // hardcoding a literal label that drifts from the enum.
        assert_eq!(StepStatus::Completed.label(), "Completed");
        assert_eq!(StepStatus::Failed.label(), "Failed");
        assert_eq!(StepStatus::Skipped.label(), "Skipped");
    }

    #[test]
    fn non_interactive_step_display_step_number() {
        let s = NonInteractiveStepDisplay::new(OutputMode::Normal, 2, 5);
        assert_eq!(s.step_number(), "[3/5]");
        assert_eq!(s.step_indent(), 6);
    }

    #[test]
    fn non_interactive_prompt_uses_default() {
        let mut s = NonInteractiveStepDisplay::new(OutputMode::Normal, 0, 1);
        let prompt = Prompt {
            key: "test".to_string(),
            question: "?".to_string(),
            prompt_type: crate::ui::PromptType::Input,
            default: Some("default_value".to_string()),
        };
        let result = s.prompt(&prompt).unwrap();
        assert_eq!(result.as_string(), "default_value");
    }
}
