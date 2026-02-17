//! Visual theme and styling.

use console::Style;

/// Bivvy's visual theme.
///
/// See `src/ui/README.md` for color and typography norms.
#[derive(Debug, Clone)]
pub struct BivvyTheme {
    /// Style for success messages (green).
    pub success: Style,
    /// Style for warning messages (orange).
    pub warning: Style,
    /// Style for error messages (red bold).
    pub error: Style,
    /// Style for informational/running elements (fuchsia/magenta).
    pub info: Style,
    /// Style for dim/secondary text.
    pub dim: Style,
    /// Style for highlighted/important text (bold).
    pub highlight: Style,
    /// Style for step titles (bold).
    pub step_title: Style,
    /// Style for headers (fuchsia bold).
    pub header: Style,
    /// Style for step numbers and counters (dim).
    pub step_number: Style,
    /// Style for durations and timestamps (dim).
    pub duration: Style,
    /// Style for commands shown in output (dim italic).
    pub command: Style,
    /// Style for box-drawing borders (dim).
    pub border: Style,
    /// Style for contextual hints (fuchsia dim).
    pub hint: Style,
    /// Style for key labels in key-value displays (bold).
    pub key: Style,
    /// Style for values in key-value displays (normal).
    pub value: Style,
    /// Style for blocked status (orange).
    pub blocked: Style,
}

impl Default for BivvyTheme {
    fn default() -> Self {
        Self::new()
    }
}

impl BivvyTheme {
    /// Create the default Bivvy theme.
    pub fn new() -> Self {
        Self {
            success: Style::new().green(),
            warning: Style::new().color256(208),
            error: Style::new().red().bold(),
            info: Style::new().magenta(),
            dim: Style::new().dim(),
            highlight: Style::new().bold(),
            step_title: Style::new().bold(),
            header: Style::new().bold().magenta(),
            step_number: Style::new().dim(),
            duration: Style::new().dim(),
            command: Style::new().dim().italic(),
            border: Style::new().dim(),
            hint: Style::new().magenta().dim(),
            key: Style::new().bold(),
            value: Style::new(),
            blocked: Style::new().color256(208),
        }
    }

    /// Create a theme without colors (for non-TTY or --no-color).
    pub fn plain() -> Self {
        Self {
            success: Style::new(),
            warning: Style::new(),
            error: Style::new(),
            info: Style::new(),
            dim: Style::new(),
            highlight: Style::new(),
            step_title: Style::new(),
            header: Style::new(),
            step_number: Style::new(),
            duration: Style::new(),
            command: Style::new(),
            border: Style::new(),
            hint: Style::new(),
            key: Style::new(),
            value: Style::new(),
            blocked: Style::new(),
        }
    }

    /// Format a success message (icon + text in green).
    pub fn format_success(&self, msg: &str) -> String {
        format!("{}", self.success.apply_to(format!("✓ {}", msg)))
    }

    /// Format a warning message (icon + text in orange).
    pub fn format_warning(&self, msg: &str) -> String {
        format!("{}", self.warning.apply_to(format!("⚠ {}", msg)))
    }

    /// Format an error message (icon + text in red bold).
    pub fn format_error(&self, msg: &str) -> String {
        format!("{}", self.error.apply_to(format!("✗ {}", msg)))
    }

    /// Format a skipped message (icon + text in dim).
    pub fn format_skipped(&self, msg: &str) -> String {
        format!("{}", self.dim.apply_to(format!("○ {}", msg)))
    }

    /// Format a step title.
    pub fn format_step(&self, name: &str, description: &str) -> String {
        format!(
            "{} {}",
            self.step_title.apply_to(format!("◆ {}", name)),
            self.dim.apply_to(description)
        )
    }

    /// Format a header banner.
    pub fn format_header(&self, title: &str) -> String {
        format!(
            "{} {}",
            self.header.apply_to("⛺"),
            self.highlight.apply_to(title)
        )
    }
}

/// Check if colors should be enabled.
pub fn should_use_colors() -> bool {
    // Check NO_COLOR env var (https://no-color.org/)
    if std::env::var("NO_COLOR").is_ok() {
        return false;
    }

    // Check if stdout is a TTY
    console::Term::stdout().is_term()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_formats_success() {
        let theme = BivvyTheme::plain();
        let msg = theme.format_success("Complete");
        assert!(msg.contains("✓"));
        assert!(msg.contains("Complete"));
    }

    #[test]
    fn theme_formats_warning() {
        let theme = BivvyTheme::plain();
        let msg = theme.format_warning("Caution");
        assert!(msg.contains("⚠"));
        assert!(msg.contains("Caution"));
    }

    #[test]
    fn theme_formats_error() {
        let theme = BivvyTheme::plain();
        let msg = theme.format_error("Failed");
        assert!(msg.contains("✗"));
        assert!(msg.contains("Failed"));
    }

    #[test]
    fn theme_formats_skipped() {
        let theme = BivvyTheme::plain();
        let msg = theme.format_skipped("Skipped");
        assert!(msg.contains("○"));
        assert!(msg.contains("Skipped"));
    }

    #[test]
    fn theme_formats_step() {
        let theme = BivvyTheme::plain();
        let msg = theme.format_step("database", "Setup database");
        assert!(msg.contains("◆"));
        assert!(msg.contains("database"));
    }

    #[test]
    fn theme_formats_header() {
        let theme = BivvyTheme::plain();
        let msg = theme.format_header("MyApp");
        assert!(msg.contains("MyApp"));
        assert!(msg.contains("⛺"));
    }

    #[test]
    fn plain_theme_creates_without_panic() {
        let theme = BivvyTheme::plain();
        let _ = theme.format_success("test");
    }

    #[test]
    fn default_theme_creates_without_panic() {
        let theme = BivvyTheme::new();
        let _ = theme.format_success("test");
    }

    #[test]
    fn default_impl_matches_new() {
        let default = BivvyTheme::default();
        let new = BivvyTheme::new();
        // Both should produce the same formatted output
        assert_eq!(default.format_success("test"), new.format_success("test"));
    }

    #[test]
    fn new_theme_slots_exist() {
        let theme = BivvyTheme::new();
        // Verify the new style slots can be used without panic
        let _ = theme.step_number.apply_to("[2/7]");
        let _ = theme.duration.apply_to("1.2s");
        let _ = theme.command.apply_to("npm install");
        let _ = theme.border.apply_to("│");
        let _ = theme.hint.apply_to("Run bivvy status");
        let _ = theme.key.apply_to("Workflow:");
        let _ = theme.value.apply_to("default");
        let _ = theme.blocked.apply_to("⊘");
    }

    #[test]
    fn plain_theme_new_slots_exist() {
        let theme = BivvyTheme::plain();
        let _ = theme.step_number.apply_to("[2/7]");
        let _ = theme.duration.apply_to("1.2s");
        let _ = theme.command.apply_to("npm install");
        let _ = theme.border.apply_to("│");
        let _ = theme.hint.apply_to("Run bivvy status");
        let _ = theme.key.apply_to("Workflow:");
        let _ = theme.value.apply_to("default");
        let _ = theme.blocked.apply_to("⊘");
    }
}
