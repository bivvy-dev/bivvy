//! Unified status vocabulary for consistent CLI output.
//!
//! `StatusKind` provides a single canonical set of status icons and
//! colors used across all commands and display contexts. This replaces
//! the previous split between theme-level symbols and display.rs brackets.

use super::theme::BivvyTheme;

/// Canonical status kinds used across all Bivvy output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StatusKind {
    /// Operation completed successfully.
    Success,
    /// Operation failed.
    Failed,
    /// Operation was skipped.
    Skipped,
    /// Operation has not been run yet.
    Pending,
    /// Operation is currently running.
    Running,
    /// Operation is blocked (dependency failed).
    Blocked,
    /// Non-fatal warning.
    Warning,
}

impl StatusKind {
    /// Unicode icon for TTY output.
    pub fn icon(self) -> &'static str {
        match self {
            Self::Success => "✓",
            Self::Failed => "✗",
            Self::Skipped => "○",
            Self::Pending => "◌",
            Self::Running => "◆",
            Self::Blocked => "⊘",
            Self::Warning => "⚠",
        }
    }

    /// Bracketed text for non-TTY output.
    pub fn bracketed(self) -> &'static str {
        match self {
            Self::Success => "[ok]",
            Self::Failed => "[FAIL]",
            Self::Skipped => "[skip]",
            Self::Pending => "[pending]",
            Self::Running => "[run]",
            Self::Blocked => "[blocked]",
            Self::Warning => "[warn]",
        }
    }

    /// Styled icon string using the given theme.
    pub fn styled(self, theme: &BivvyTheme) -> String {
        let icon = self.icon();
        match self {
            Self::Success => theme.success.apply_to(icon).to_string(),
            Self::Failed => theme.error.apply_to(icon).to_string(),
            Self::Skipped => theme.dim.apply_to(icon).to_string(),
            Self::Pending => theme.dim.apply_to(icon).to_string(),
            Self::Running => theme.info.apply_to(icon).to_string(),
            Self::Blocked => theme.warning.apply_to(icon).to_string(),
            Self::Warning => theme.warning.apply_to(icon).to_string(),
        }
    }

    /// Format a status line: styled icon + message.
    pub fn format(self, theme: &BivvyTheme, msg: &str) -> String {
        format!("{} {}", self.styled(theme), msg)
    }

    /// Format a status line for non-TTY: bracketed + message.
    pub fn format_plain(self, msg: &str) -> String {
        format!("{} {}", self.bracketed(), msg)
    }
}

impl From<crate::state::StepStatus> for StatusKind {
    fn from(status: crate::state::StepStatus) -> Self {
        match status {
            crate::state::StepStatus::Success => Self::Success,
            crate::state::StepStatus::Failed => Self::Failed,
            crate::state::StepStatus::Skipped => Self::Skipped,
            crate::state::StepStatus::NeverRun => Self::Pending,
        }
    }
}

impl From<crate::steps::StepStatus> for StatusKind {
    fn from(status: crate::steps::StepStatus) -> Self {
        match status {
            crate::steps::StepStatus::Pending => Self::Pending,
            crate::steps::StepStatus::Running => Self::Running,
            crate::steps::StepStatus::Completed => Self::Success,
            crate::steps::StepStatus::Failed => Self::Failed,
            crate::steps::StepStatus::Skipped => Self::Skipped,
        }
    }
}

impl From<crate::state::RunStatus> for StatusKind {
    fn from(status: crate::state::RunStatus) -> Self {
        match status {
            crate::state::RunStatus::Success => Self::Success,
            crate::state::RunStatus::Failed => Self::Failed,
            crate::state::RunStatus::Interrupted => Self::Warning,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_returns_unicode_symbols() {
        assert_eq!(StatusKind::Success.icon(), "✓");
        assert_eq!(StatusKind::Failed.icon(), "✗");
        assert_eq!(StatusKind::Skipped.icon(), "○");
        assert_eq!(StatusKind::Pending.icon(), "◌");
        assert_eq!(StatusKind::Running.icon(), "◆");
        assert_eq!(StatusKind::Blocked.icon(), "⊘");
        assert_eq!(StatusKind::Warning.icon(), "⚠");
    }

    #[test]
    fn bracketed_returns_text_labels() {
        assert_eq!(StatusKind::Success.bracketed(), "[ok]");
        assert_eq!(StatusKind::Failed.bracketed(), "[FAIL]");
        assert_eq!(StatusKind::Skipped.bracketed(), "[skip]");
        assert_eq!(StatusKind::Pending.bracketed(), "[pending]");
        assert_eq!(StatusKind::Running.bracketed(), "[run]");
        assert_eq!(StatusKind::Blocked.bracketed(), "[blocked]");
        assert_eq!(StatusKind::Warning.bracketed(), "[warn]");
    }

    #[test]
    fn styled_returns_string_with_icon() {
        let theme = BivvyTheme::plain();
        for kind in [
            StatusKind::Success,
            StatusKind::Failed,
            StatusKind::Skipped,
            StatusKind::Pending,
            StatusKind::Running,
            StatusKind::Blocked,
            StatusKind::Warning,
        ] {
            let styled = kind.styled(&theme);
            assert!(
                styled.contains(kind.icon()),
                "styled({:?}) missing icon",
                kind
            );
        }
    }

    #[test]
    fn format_includes_icon_and_message() {
        let theme = BivvyTheme::plain();
        let result = StatusKind::Success.format(&theme, "install_deps");
        assert!(result.contains("✓"));
        assert!(result.contains("install_deps"));
    }

    #[test]
    fn format_plain_uses_brackets() {
        let result = StatusKind::Failed.format_plain("build");
        assert_eq!(result, "[FAIL] build");
    }

    #[test]
    fn from_state_step_status() {
        use crate::state::StepStatus;

        assert_eq!(StatusKind::from(StepStatus::Success), StatusKind::Success);
        assert_eq!(StatusKind::from(StepStatus::Failed), StatusKind::Failed);
        assert_eq!(StatusKind::from(StepStatus::Skipped), StatusKind::Skipped);
        assert_eq!(StatusKind::from(StepStatus::NeverRun), StatusKind::Pending);
    }

    #[test]
    fn from_steps_step_status() {
        use crate::steps::StepStatus;

        assert_eq!(StatusKind::from(StepStatus::Pending), StatusKind::Pending);
        assert_eq!(StatusKind::from(StepStatus::Running), StatusKind::Running);
        assert_eq!(StatusKind::from(StepStatus::Completed), StatusKind::Success);
        assert_eq!(StatusKind::from(StepStatus::Failed), StatusKind::Failed);
        assert_eq!(StatusKind::from(StepStatus::Skipped), StatusKind::Skipped);
    }

    #[test]
    fn from_run_status() {
        use crate::state::RunStatus;

        assert_eq!(StatusKind::from(RunStatus::Success), StatusKind::Success);
        assert_eq!(StatusKind::from(RunStatus::Failed), StatusKind::Failed);
        assert_eq!(
            StatusKind::from(RunStatus::Interrupted),
            StatusKind::Warning
        );
    }

    #[test]
    fn all_variants_have_unique_icons() {
        let icons: Vec<&str> = [
            StatusKind::Success,
            StatusKind::Failed,
            StatusKind::Skipped,
            StatusKind::Pending,
            StatusKind::Running,
            StatusKind::Blocked,
            StatusKind::Warning,
        ]
        .iter()
        .map(|k| k.icon())
        .collect();

        // Warning and Blocked can share ⚠/⊘ — just ensure Success, Failed, etc. are distinct
        let mut unique = icons.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), icons.len(), "All icons should be unique");
    }

    #[test]
    fn all_variants_have_unique_brackets() {
        let brackets: Vec<&str> = [
            StatusKind::Success,
            StatusKind::Failed,
            StatusKind::Skipped,
            StatusKind::Pending,
            StatusKind::Running,
            StatusKind::Blocked,
            StatusKind::Warning,
        ]
        .iter()
        .map(|k| k.bracketed())
        .collect();

        let mut unique = brackets.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(
            unique.len(),
            brackets.len(),
            "All brackets should be unique"
        );
    }
}
