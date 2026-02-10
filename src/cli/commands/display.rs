//! Shared display helpers for step status formatting.
//!
//! These helpers are used by `status`, `last`, and any other command that
//! needs to render [`StepStatus`] values consistently.

use std::collections::HashSet;

use crate::state::StepStatus;
use crate::ui::UserInterface;

/// Return a bracketed symbol for a step status.
pub fn status_symbol(status: &StepStatus) -> &'static str {
    match status {
        StepStatus::Success => "[ok]",
        StepStatus::Failed => "[FAIL]",
        StepStatus::Skipped => "[skip]",
        StepStatus::NeverRun => "[pending]",
    }
}

/// Return a short key string for a step status (used in legend tracking).
pub fn status_key(status: &StepStatus) -> &'static str {
    match status {
        StepStatus::Success => "ok",
        StepStatus::Failed => "fail",
        StepStatus::Skipped => "skip",
        StepStatus::NeverRun => "pending",
    }
}

/// Print a single step's status line, styled by severity.
pub fn show_step_status(ui: &mut dyn UserInterface, name: &str, status: &StepStatus) {
    let line = format!("  {} {}", status_symbol(status), name);
    match status {
        StepStatus::Success => ui.success(&line),
        StepStatus::Failed => ui.error(&line),
        StepStatus::Skipped => ui.warning(&line),
        StepStatus::NeverRun => ui.message(&line),
    }
}

/// Build a legend string for the statuses that appeared in output.
///
/// Returns `None` if `seen_statuses` is empty.
pub fn format_legend(seen_statuses: &HashSet<&str>) -> Option<String> {
    let mut parts = Vec::new();
    if seen_statuses.contains("ok") {
        parts.push("[ok] passed");
    }
    if seen_statuses.contains("fail") {
        parts.push("[FAIL] failed");
    }
    if seen_statuses.contains("skip") {
        parts.push("[skip] user-skipped");
    }
    if seen_statuses.contains("pending") {
        parts.push("[pending] not yet run");
    }
    if parts.is_empty() {
        None
    } else {
        Some(format!("Legend: {}", parts.join("  ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_symbol_values() {
        assert_eq!(status_symbol(&StepStatus::Success), "[ok]");
        assert_eq!(status_symbol(&StepStatus::Failed), "[FAIL]");
        assert_eq!(status_symbol(&StepStatus::Skipped), "[skip]");
        assert_eq!(status_symbol(&StepStatus::NeverRun), "[pending]");
    }

    #[test]
    fn status_key_values() {
        assert_eq!(status_key(&StepStatus::Success), "ok");
        assert_eq!(status_key(&StepStatus::Failed), "fail");
        assert_eq!(status_key(&StepStatus::Skipped), "skip");
        assert_eq!(status_key(&StepStatus::NeverRun), "pending");
    }

    #[test]
    fn format_legend_empty() {
        let seen = HashSet::new();
        assert!(format_legend(&seen).is_none());
    }

    #[test]
    fn format_legend_single() {
        let mut seen = HashSet::new();
        seen.insert("ok");
        let legend = format_legend(&seen).unwrap();
        assert!(legend.contains("[ok] passed"));
        assert!(!legend.contains("[FAIL]"));
    }

    #[test]
    fn format_legend_all() {
        let mut seen = HashSet::new();
        seen.insert("ok");
        seen.insert("fail");
        seen.insert("skip");
        seen.insert("pending");
        let legend = format_legend(&seen).unwrap();
        assert!(legend.contains("[ok] passed"));
        assert!(legend.contains("[FAIL] failed"));
        assert!(legend.contains("[skip] user-skipped"));
        assert!(legend.contains("[pending] not yet run"));
    }

    #[test]
    fn show_step_status_uses_correct_ui_method() {
        use crate::ui::MockUI;

        let mut ui = MockUI::new();
        show_step_status(&mut ui, "step1", &StepStatus::Success);
        assert!(ui.successes().iter().any(|m| m.contains("step1")));

        show_step_status(&mut ui, "step2", &StepStatus::Failed);
        assert!(ui.errors().iter().any(|m| m.contains("step2")));

        show_step_status(&mut ui, "step3", &StepStatus::Skipped);
        assert!(ui.warnings().iter().any(|m| m.contains("step3")));

        show_step_status(&mut ui, "step4", &StepStatus::NeverRun);
        assert!(ui.messages().iter().any(|m| m.contains("step4")));
    }
}
