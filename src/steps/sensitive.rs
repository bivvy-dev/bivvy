//! Sensitive step handling.
//!
//! This module provides special handling for steps that process sensitive data.

/// Manages sensitive step execution.
///
/// # Example
///
/// ```
/// use bivvy::steps::SensitiveStepHandler;
///
/// // Configure handler based on execution mode
/// let handler = SensitiveStepHandler::new(false, false);
///
/// // Dry-run hides the actual command
/// let handler_dry = SensitiveStepHandler::new(true, false);
/// let display = handler_dry.dry_run_display("vault read secret/key");
/// assert!(display.contains("SENSITIVE"));
///
/// // Sensitive output is not logged
/// assert!(!handler.should_log_output());
///
/// // In non-interactive mode, no confirmation needed
/// let handler_non_interactive = SensitiveStepHandler::new(false, true);
/// assert!(!handler_non_interactive.should_confirm("fetch-secrets"));
/// ```
pub struct SensitiveStepHandler {
    /// Whether running in dry-run mode.
    is_dry_run: bool,
    /// Whether running in non-interactive mode.
    is_non_interactive: bool,
}

impl SensitiveStepHandler {
    /// Create a new handler with the given settings.
    pub fn new(is_dry_run: bool, is_non_interactive: bool) -> Self {
        Self {
            is_dry_run,
            is_non_interactive,
        }
    }

    /// Check if user should confirm before running sensitive step.
    ///
    /// Confirmation is skipped in non-interactive mode or CI.
    pub fn should_confirm(&self, _step_name: &str) -> bool {
        // Don't confirm in non-interactive mode or CI
        if self.is_non_interactive {
            return false;
        }
        if std::env::var("CI").is_ok() {
            return false;
        }
        // Always confirm sensitive steps otherwise
        true
    }

    /// Get display text for dry-run output.
    ///
    /// The actual command is hidden to prevent exposure in logs.
    pub fn dry_run_display(&self, _command: &str) -> String {
        "[SENSITIVE - command hidden in dry-run]".to_string()
    }

    /// Check if command output should be logged.
    ///
    /// Sensitive steps suppress output logging to prevent data exposure.
    pub fn should_log_output(&self) -> bool {
        false
    }

    /// Check if running in dry-run mode.
    pub fn is_dry_run(&self) -> bool {
        self.is_dry_run
    }

    /// Check if running in non-interactive mode.
    pub fn is_non_interactive(&self) -> bool {
        self.is_non_interactive
    }
}

impl Default for SensitiveStepHandler {
    fn default() -> Self {
        Self::new(false, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Mutex to serialize tests that modify CI env var
    static CI_MUTEX: Mutex<()> = Mutex::new(());

    fn with_ci_env<F, R>(ci_value: Option<&str>, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let _guard = CI_MUTEX.lock().unwrap();
        let old_ci = std::env::var("CI").ok();
        match ci_value {
            Some(v) => std::env::set_var("CI", v),
            None => std::env::remove_var("CI"),
        }
        let result = f();
        match old_ci {
            Some(v) => std::env::set_var("CI", v),
            None => std::env::remove_var("CI"),
        }
        result
    }

    #[test]
    fn dry_run_hides_sensitive_command() {
        let handler = SensitiveStepHandler::new(true, false);

        let display = handler.dry_run_display("vault read secret/key");

        assert!(!display.contains("vault"));
        assert!(display.contains("SENSITIVE"));
    }

    #[test]
    fn sensitive_output_not_logged() {
        let handler = SensitiveStepHandler::new(false, false);

        assert!(!handler.should_log_output());
    }

    #[test]
    fn no_confirm_in_non_interactive() {
        let handler = SensitiveStepHandler::new(false, true);

        assert!(!handler.should_confirm("any-step"));
    }

    #[test]
    fn confirm_in_interactive_mode() {
        with_ci_env(None, || {
            let handler = SensitiveStepHandler::new(false, false);
            assert!(handler.should_confirm("sensitive-step"));
        });
    }

    #[test]
    fn no_confirm_in_ci() {
        with_ci_env(Some("true"), || {
            let handler = SensitiveStepHandler::new(false, false);
            assert!(!handler.should_confirm("sensitive-step"));
        });
    }

    #[test]
    fn is_dry_run_accessor() {
        let handler = SensitiveStepHandler::new(true, false);
        assert!(handler.is_dry_run());

        let handler = SensitiveStepHandler::new(false, false);
        assert!(!handler.is_dry_run());
    }

    #[test]
    fn is_non_interactive_accessor() {
        let handler = SensitiveStepHandler::new(false, true);
        assert!(handler.is_non_interactive());

        let handler = SensitiveStepHandler::new(false, false);
        assert!(!handler.is_non_interactive());
    }

    #[test]
    fn default_handler() {
        let handler = SensitiveStepHandler::default();
        assert!(!handler.is_dry_run());
        assert!(!handler.is_non_interactive());
    }
}
