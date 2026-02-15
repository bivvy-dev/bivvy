//! Contextual hint generation for Bivvy commands.
//!
//! Provides smart hint text that suggests the logical next action
//! based on the current state of the project and command results.

/// Generate a hint after a successful run.
pub fn after_successful_run() -> &'static str {
    "Run `bivvy status` to verify setup health."
}

/// Generate a hint after a failed run with specific failed steps.
pub fn after_failed_run(failed_steps: &[String]) -> String {
    if failed_steps.is_empty() {
        return "Run `bivvy run` to retry.".to_string();
    }
    format!(
        "Fix and re-run: `bivvy run --only={}`",
        failed_steps.join(",")
    )
}

/// Generate a hint when all steps are pending (never run).
pub fn all_steps_pending() -> &'static str {
    "Run `bivvy run` to start setup."
}

/// Generate a hint when some steps are pending.
pub fn some_steps_pending(pending: &[String]) -> String {
    format!(
        "Run `bivvy run --only={}` to run remaining steps.",
        pending.join(",")
    )
}

/// Generate a hint after init when the user declines to run.
pub fn after_init() -> &'static str {
    "Run `bivvy run` when you're ready to start setup."
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn after_successful_run_hint() {
        let hint = after_successful_run();
        assert!(hint.contains("bivvy status"));
    }

    #[test]
    fn after_failed_run_with_steps() {
        let hint = after_failed_run(&["build".to_string(), "deploy".to_string()]);
        assert!(hint.contains("bivvy run --only=build,deploy"));
    }

    #[test]
    fn after_failed_run_no_steps() {
        let hint = after_failed_run(&[]);
        assert!(hint.contains("bivvy run"));
    }

    #[test]
    fn all_pending_hint() {
        let hint = all_steps_pending();
        assert!(hint.contains("bivvy run"));
    }

    #[test]
    fn some_pending_hint() {
        let hint = some_steps_pending(&["setup_db".to_string()]);
        assert!(hint.contains("bivvy run --only=setup_db"));
    }

    #[test]
    fn after_init_hint() {
        let hint = after_init();
        assert!(hint.contains("bivvy run"));
    }
}
