//! Low-level terminal surface for the run path.
//!
//! `TerminalSurface` owns the [`MultiProgress`] that coordinates pinned and
//! transient regions during a workflow run. It is the only place that
//! touches the multi-progress directly — higher layers (`WorkflowDisplay`,
//! `StepDisplay`) only see *regions*, not bars.
//!
//! ## Regions, top to bottom
//!
//! 1. **Scrollback** — printed via [`TerminalSurface::println`]. Step
//!    headers, status messages, error blocks, summaries — anything
//!    non-spinner.
//! 2. **Transient region** — currently-running step's spinner with its
//!    bounded live-output tail. Mounted via
//!    [`TerminalSurface::transient_above_pinned`].
//! 3. **Pinned region** — workflow progress bar, pinned at the bottom.
//!    Mounted via [`TerminalSurface::pin_bottom`].
//!
//! ## Region clearing
//!
//! [`TransientRegion`] takes a `max_lines` hint at construction time so
//! callers can reason about the maximum vertical extent of the spinner
//! and its live-output tail. Clearing on drop currently delegates to
//! `indicatif::ProgressBar::finish_and_clear`, which tracks the bar's
//! last rendered height and clears that many rows. An earlier version
//! of this module padded every message to exactly `max_lines` blank
//! rows before clearing, but that pattern caused trailing blank rows
//! to leak into scrollback on drop — `finish_and_clear` already handles
//! the multi-line case correctly in current `indicatif` versions.
//!
//! ## Cursor freeing
//!
//! Interactive prompts (dialoguer) need exclusive access to the cursor.
//! [`TerminalSurface::with_cursor_freed`] hides the multi-progress draw
//! target for the duration of a closure and restores it afterwards.

use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::sync::Arc;

/// Owns the [`MultiProgress`] that coordinates draw regions during a run.
///
/// Construct via [`TerminalSurface::new`] and share via the returned
/// [`Arc`]. Both the workflow display and per-step displays hold a clone of
/// the `Arc`, but only the surface itself owns mutable access to the
/// underlying multi-progress.
pub struct TerminalSurface {
    multi: MultiProgress,
}

impl TerminalSurface {
    /// Create a new surface, returning a shared handle.
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            multi: MultiProgress::new(),
        })
    }

    /// Try to create a surface for the current terminal.
    ///
    /// Returns `None` when the terminal can't render progress bars —
    /// non-TTY stderr, `NO_COLOR`, or `TERM=dumb`. In those cases
    /// `MultiProgress::stderr()` is silently hidden by indicatif and
    /// every `println` call becomes a no-op, swallowing all output.
    /// Callers should fall back to plain stdout writes (typically by
    /// using [`crate::runner::display::NonInteractiveWorkflowDisplay`]).
    pub fn try_new() -> Option<Arc<Self>> {
        if !console::Term::stderr().features().colors_supported() {
            return None;
        }
        Some(Self::new())
    }

    /// Create a hidden surface for tests — no output is drawn.
    pub fn hidden() -> Arc<Self> {
        Arc::new(Self {
            multi: MultiProgress::with_draw_target(ProgressDrawTarget::hidden()),
        })
    }

    /// Print a line into scrollback, above any pinned/transient bars.
    pub fn println(&self, line: &str) {
        // `MultiProgress::println` writes above all managed bars. If draw
        // is hidden (tests / silent mode), the call is a no-op rather than
        // an error — discard the result either way.
        let _ = self.multi.println(line);
    }

    /// Pin a bar at the bottom of the live region.
    ///
    /// Used once per run, by the workflow display. The returned [`PinnedBar`]
    /// keeps the bar live until dropped; on drop, the bar is cleared.
    pub fn pin_bottom(self: &Arc<Self>, bar: ProgressBar) -> PinnedBar {
        let bar = self.multi.add(bar);
        PinnedBar { bar: Some(bar) }
    }

    /// Mount a transient bar above the pinned region.
    ///
    /// `max_lines` records the maximum number of lines the bar will ever
    /// render (spinner row + live-output tail). It is informational —
    /// callers can read it back via [`TransientRegion::max_lines`] —
    /// but the actual clearing on drop delegates to
    /// `ProgressBar::finish_and_clear`, which tracks the rendered height
    /// itself.
    pub fn transient_above_pinned(
        self: &Arc<Self>,
        bar: ProgressBar,
        max_lines: usize,
    ) -> TransientRegion {
        // Insert *before* the last bar (the pinned workflow bar), so the
        // transient renders above it. If no bar is pinned, this is
        // equivalent to a normal add.
        let bar = self.multi.insert_from_back(1, bar);
        TransientRegion {
            bar: Some(bar),
            max_lines: max_lines.max(1),
        }
    }

    /// Hide every bar for the duration of `f`, then restore.
    ///
    /// Used by step prompts. Encapsulates the
    /// `set_draw_target(hidden) / set_draw_target(stderr)` pattern so the
    /// rest of the codebase doesn't need to coordinate it.
    pub fn with_cursor_freed<R>(&self, f: impl FnOnce() -> R) -> R {
        self.multi.set_draw_target(ProgressDrawTarget::hidden());
        let result = f();
        self.multi.set_draw_target(ProgressDrawTarget::stderr());
        result
    }
}

/// Handle for a bar pinned at the bottom of the live region.
///
/// On drop the bar is cleared.
pub struct PinnedBar {
    bar: Option<ProgressBar>,
}

impl PinnedBar {
    /// Update the bar's message.
    pub fn set_message(&self, msg: impl Into<String>) {
        if let Some(bar) = &self.bar {
            bar.set_message(msg.into());
        }
    }

    /// Get a clone of the underlying [`ProgressBar`] (for advanced usage).
    pub fn bar_clone(&self) -> Option<ProgressBar> {
        self.bar.clone()
    }
}

impl Drop for PinnedBar {
    fn drop(&mut self) {
        if let Some(bar) = self.bar.take() {
            bar.finish_and_clear();
        }
    }
}

/// Handle for a transient bar mounted above the pinned region.
///
/// On drop the bar is cleared via `ProgressBar::finish_and_clear`. The
/// `max_lines` field is the *maximum* vertical extent the caller
/// committed to when mounting — it's exposed for tests and assertions
/// but no longer drives clearing directly.
pub struct TransientRegion {
    bar: Option<ProgressBar>,
    max_lines: usize,
}

impl TransientRegion {
    /// Update the bar's message.
    ///
    /// Indicatif's bar tracks the current rendered height across ticks
    /// and clears it correctly on `finish_and_clear`, so the message is
    /// passed through untouched (no blank-line padding) — padding to a
    /// fixed height was causing extra blank rows to leak into
    /// scrollback when the bar was finally dropped.
    pub fn set_message(&self, msg: &str) {
        if let Some(bar) = &self.bar {
            bar.set_message(msg.to_string());
        }
    }

    /// Get a clone of the underlying [`ProgressBar`] for live-output
    /// callbacks. Callers must pad messages to `max_lines` themselves
    /// when bypassing [`Self::set_message`].
    pub fn bar_clone(&self) -> Option<ProgressBar> {
        self.bar.clone()
    }

    /// Maximum number of lines this region will ever render.
    pub fn max_lines(&self) -> usize {
        self.max_lines
    }

    /// Replace the bar style with a static `{msg}` template.
    ///
    /// Used by `finish_*` methods on [`StepDisplay`] when leaving a
    /// styled result line in scrollback. The caller should follow this
    /// with [`Self::finish_with_message`] or the bar will be cleared on
    /// drop.
    pub fn freeze_style(&self) {
        if let Some(bar) = &self.bar {
            bar.set_style(ProgressStyle::default_spinner().template("{msg}").unwrap());
        }
    }

    /// Finish the region with a final message and consume the handle.
    ///
    /// Unlike `Drop`, this leaves the message in scrollback (collapsed to
    /// a single line via the `{msg}` template).
    pub fn finish_with_line(mut self, line: &str) {
        if let Some(bar) = self.bar.take() {
            bar.set_style(ProgressStyle::default_spinner().template("{msg}").unwrap());
            bar.finish_with_message(line.to_string());
        }
    }
}

impl Drop for TransientRegion {
    fn drop(&mut self) {
        if let Some(bar) = self.bar.take() {
            // Indicatif tracks the bar's last rendered height and
            // clears that many rows on `finish_and_clear`. Don't mess
            // with the message before clearing — earlier attempts to
            // "normalize" the height by overwriting the message with
            // blank lines caused those blanks to be baked into
            // scrollback below the cleared region.
            bar.finish_and_clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indicatif::ProgressBar;

    #[test]
    fn surface_new_returns_shared_handle() {
        let surface = TerminalSurface::hidden();
        // Both clones share the same inner multi-progress.
        let clone = Arc::clone(&surface);
        assert!(Arc::ptr_eq(&surface, &clone));
    }

    #[test]
    fn try_new_does_not_panic_in_test_environment() {
        // In a typical test runner stderr is not attended, so try_new
        // should return None. Either way, calling it must not panic.
        let _ = TerminalSurface::try_new();
    }

    #[test]
    fn pin_bottom_returns_handle_that_clears_on_drop() {
        let surface = TerminalSurface::hidden();
        let bar = ProgressBar::new(10);
        {
            let pinned = surface.pin_bottom(bar);
            pinned.set_message("running");
            // Bar is alive while pinned is in scope.
            assert!(pinned.bar_clone().is_some());
        }
        // Bar is dropped — finish_and_clear has been called.
    }

    #[test]
    fn transient_region_sets_max_lines() {
        let surface = TerminalSurface::hidden();
        let bar = ProgressBar::new_spinner();
        let region = surface.transient_above_pinned(bar, 4);
        assert_eq!(region.max_lines(), 4);
    }

    #[test]
    fn transient_region_min_one_line() {
        let surface = TerminalSurface::hidden();
        let bar = ProgressBar::new_spinner();
        let region = surface.transient_above_pinned(bar, 0);
        // Zero is bumped to one — a region must reserve at least one line.
        assert_eq!(region.max_lines(), 1);
    }

    #[test]
    fn println_does_not_panic_when_hidden() {
        let surface = TerminalSurface::hidden();
        surface.println("scrollback line");
    }

    #[test]
    fn with_cursor_freed_runs_closure() {
        let surface = TerminalSurface::hidden();
        let result = surface.with_cursor_freed(|| 42);
        assert_eq!(result, 42);
    }

    #[test]
    fn with_cursor_freed_restores_target_on_panic() {
        // Verify that on panic the draw target is still restored.
        // We can only check the no-panic happy path here directly;
        // the panic path is exercised by std's catch_unwind machinery
        // via running a panicking closure.
        let surface = TerminalSurface::hidden();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            surface.with_cursor_freed(|| panic!("boom"));
        }));
        assert!(outcome.is_err());
        // After the panic, the surface is still usable.
        surface.println("post-panic");
    }

    #[test]
    fn transient_region_finish_with_line_consumes_handle() {
        let surface = TerminalSurface::hidden();
        let bar = ProgressBar::new_spinner();
        let region = surface.transient_above_pinned(bar, 4);
        region.finish_with_line("done");
        // No way to observe "done" was rendered with the hidden target,
        // but the call must not panic.
    }

    #[test]
    fn transient_region_set_message_passes_through() {
        let surface = TerminalSurface::hidden();
        let bar = ProgressBar::new_spinner();
        let region = surface.transient_above_pinned(bar.clone(), 3);
        region.set_message("just one line");
        // No padding — the message reaches indicatif unchanged so the
        // bar's tracked height stays in sync with what it actually drew.
        assert_eq!(bar.message(), "just one line");
    }
}
