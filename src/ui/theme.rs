//! Visual theme and styling.

use console::Style;

/// Bivvy's visual theme.
#[derive(Debug, Clone)]
pub struct BivvyTheme {
    /// Style for success messages.
    pub success: Style,
    /// Style for warning messages.
    pub warning: Style,
    /// Style for error messages.
    pub error: Style,
    /// Style for informational messages.
    pub info: Style,
    /// Style for dim/secondary text.
    pub dim: Style,
    /// Style for highlighted/important text.
    pub highlight: Style,
    /// Style for step titles.
    pub step_title: Style,
    /// Style for headers.
    pub header: Style,
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
            warning: Style::new().yellow(),
            error: Style::new().red().bold(),
            info: Style::new().cyan(),
            dim: Style::new().dim(),
            highlight: Style::new().bold(),
            step_title: Style::new().bold(),
            header: Style::new().bold().magenta(),
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
        }
    }

    /// Format a success message.
    pub fn format_success(&self, msg: &str) -> String {
        format!("{} {}", self.success.apply_to("✓"), msg)
    }

    /// Format a warning message.
    pub fn format_warning(&self, msg: &str) -> String {
        format!("{} {}", self.warning.apply_to("⚠"), msg)
    }

    /// Format an error message.
    pub fn format_error(&self, msg: &str) -> String {
        format!("{} {}", self.error.apply_to("✗"), msg)
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
}
