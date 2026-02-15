//! Shared display helpers for step status formatting.
//!
//! These helpers are used by `status`, `last`, and any other command that
//! needs to render [`StepStatus`] values consistently.

use crate::state::StepStatus;
use crate::ui::{StatusKind, UserInterface};

/// Return a StatusKind for a step status.
pub fn status_kind(status: &StepStatus) -> StatusKind {
    StatusKind::from(*status)
}

/// Return the icon string for a step status (TTY output).
pub fn status_icon(status: &StepStatus) -> &'static str {
    status_kind(status).icon()
}

/// Return a bracketed symbol for a step status (non-TTY output).
pub fn status_symbol(status: &StepStatus) -> &'static str {
    status_kind(status).bracketed()
}

/// Print a single step's status line, styled by severity.
pub fn show_step_status(ui: &mut dyn UserInterface, name: &str, status: &StepStatus) {
    let kind = status_kind(status);
    let line = format!("  {} {}", kind.icon(), name);
    match status {
        StepStatus::Success => ui.success(&line),
        StepStatus::Failed => ui.error(&line),
        StepStatus::Skipped => ui.warning(&line),
        StepStatus::NeverRun => ui.message(&line),
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
    fn status_icon_values() {
        assert_eq!(status_icon(&StepStatus::Success), "✓");
        assert_eq!(status_icon(&StepStatus::Failed), "✗");
        assert_eq!(status_icon(&StepStatus::Skipped), "○");
        assert_eq!(status_icon(&StepStatus::NeverRun), "◌");
    }

    #[test]
    fn status_kind_conversion() {
        assert_eq!(status_kind(&StepStatus::Success), StatusKind::Success);
        assert_eq!(status_kind(&StepStatus::Failed), StatusKind::Failed);
        assert_eq!(status_kind(&StepStatus::Skipped), StatusKind::Skipped);
        assert_eq!(status_kind(&StepStatus::NeverRun), StatusKind::Pending);
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

    #[test]
    fn show_step_status_includes_icons() {
        use crate::ui::MockUI;

        let mut ui = MockUI::new();
        show_step_status(&mut ui, "step1", &StepStatus::Success);
        assert!(ui.successes().iter().any(|m| m.contains("✓")));

        show_step_status(&mut ui, "step2", &StepStatus::Failed);
        assert!(ui.errors().iter().any(|m| m.contains("✗")));

        show_step_status(&mut ui, "step3", &StepStatus::Skipped);
        assert!(ui.warnings().iter().any(|m| m.contains("○")));

        show_step_status(&mut ui, "step4", &StepStatus::NeverRun);
        assert!(ui.messages().iter().any(|m| m.contains("◌")));
    }
}
