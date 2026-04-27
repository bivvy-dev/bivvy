//! Event-driven presenter for display-only operations.
//!
//! Implements [`EventConsumer`] to handle non-interactive UI output in
//! response to events. Interactive prompts remain in the orchestrator —
//! this presenter handles only display operations that don't require
//! user input.
//!
//! # Event handling
//!
//! | Event | Action |
//! |-------|--------|
//! | `StepPlanned` | Log step inclusion in plan |
//! | `StepFilteredOut` | Log step exclusion |
//! | `StepStarting` | Log step start |
//! | `StepCompleted` | Log step result |
//! | `StepSkipped` | Log skip reason |
//! | `WorkflowStarted` | Log workflow start |
//! | `WorkflowCompleted` | Log workflow summary |
//! | `CheckEvaluated` | Log check result |

use crate::logging::events::{BivvyEvent, EventConsumer};

/// Non-interactive presenter that formats events for display.
///
/// Currently used for logging/tracing. The orchestrator still handles
/// direct terminal output. As the architecture evolves, more display
/// logic will migrate from the orchestrator into this presenter.
pub struct EventPresenter {
    /// Collected messages for inspection (testing) or deferred display.
    messages: Vec<String>,
}

impl EventPresenter {
    /// Create a new event presenter.
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
        }
    }

    /// Get the collected messages.
    pub fn messages(&self) -> &[String] {
        &self.messages
    }

    /// Drain and return all collected messages.
    pub fn drain_messages(&mut self) -> Vec<String> {
        std::mem::take(&mut self.messages)
    }
}

impl Default for EventPresenter {
    fn default() -> Self {
        Self::new()
    }
}

impl EventConsumer for EventPresenter {
    fn on_event(&mut self, event: &BivvyEvent) {
        match event {
            BivvyEvent::WorkflowStarted { name, step_count } => {
                self.messages.push(format!(
                    "Workflow '{}' started ({} steps)",
                    name, step_count
                ));
            }
            BivvyEvent::StepStarting { name } => {
                self.messages.push(format!("Starting: {}", name));
            }
            BivvyEvent::StepCompleted {
                name,
                success,
                duration_ms,
                error,
                ..
            } => {
                if *success {
                    self.messages
                        .push(format!("\u{2713} {} ({}ms)", name, duration_ms));
                } else {
                    let detail = error.as_deref().unwrap_or("failed");
                    self.messages.push(format!(
                        "\u{2717} {} — {} ({}ms)",
                        name, detail, duration_ms
                    ));
                }
            }
            BivvyEvent::StepSkipped { name, reason } => {
                self.messages
                    .push(format!("\u{25CB} {} — {}", name, reason));
            }
            BivvyEvent::StepFilteredOut { name, reason } => {
                self.messages
                    .push(format!("\u{2298} {} ({})", name, reason));
            }
            BivvyEvent::CheckEvaluated {
                step, description, ..
            } => {
                self.messages
                    .push(format!("  check: {} — {}", step, description));
            }
            BivvyEvent::WorkflowCompleted {
                name,
                success,
                steps_run,
                steps_skipped,
                duration_ms,
                ..
            } => {
                let status = if *success { "succeeded" } else { "failed" };
                self.messages.push(format!(
                    "Workflow '{}' {}: {} ran, {} skipped ({}ms)",
                    name, status, steps_run, steps_skipped, duration_ms
                ));
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_workflow_started() {
        let mut presenter = EventPresenter::new();
        presenter.on_event(&BivvyEvent::WorkflowStarted {
            name: "default".to_string(),
            step_count: 3,
        });
        assert_eq!(presenter.messages().len(), 1);
        assert!(presenter.messages()[0].contains("default"));
        assert!(presenter.messages()[0].contains("3 steps"));
    }

    #[test]
    fn captures_step_completed_success() {
        let mut presenter = EventPresenter::new();
        presenter.on_event(&BivvyEvent::StepCompleted {
            name: "build".to_string(),
            success: true,
            exit_code: Some(0),
            duration_ms: 1500,
            error: None,
        });
        assert!(presenter.messages()[0].contains("\u{2713}"));
        assert!(presenter.messages()[0].contains("build"));
    }

    #[test]
    fn captures_step_completed_failure() {
        let mut presenter = EventPresenter::new();
        presenter.on_event(&BivvyEvent::StepCompleted {
            name: "build".to_string(),
            success: false,
            exit_code: Some(1),
            duration_ms: 500,
            error: Some("cargo build failed".to_string()),
        });
        assert!(presenter.messages()[0].contains("\u{2717}"));
        assert!(presenter.messages()[0].contains("cargo build failed"));
    }

    #[test]
    fn captures_step_skipped() {
        let mut presenter = EventPresenter::new();
        presenter.on_event(&BivvyEvent::StepSkipped {
            name: "deploy".to_string(),
            reason: "user_declined".to_string(),
        });
        assert!(presenter.messages()[0].contains("deploy"));
        assert!(presenter.messages()[0].contains("user_declined"));
    }

    #[test]
    fn ignores_unrelated_events() {
        let mut presenter = EventPresenter::new();
        presenter.on_event(&BivvyEvent::SessionStarted {
            command: "run".to_string(),
            args: vec![],
            version: "1.0.0".to_string(),
            os: None,
            working_directory: None,
        });
        assert!(presenter.messages().is_empty());
    }

    #[test]
    fn drain_messages_clears() {
        let mut presenter = EventPresenter::new();
        presenter.on_event(&BivvyEvent::StepStarting {
            name: "build".to_string(),
        });
        let msgs = presenter.drain_messages();
        assert_eq!(msgs.len(), 1);
        assert!(presenter.messages().is_empty());
    }

    #[test]
    fn captures_workflow_completed() {
        let mut presenter = EventPresenter::new();
        presenter.on_event(&BivvyEvent::WorkflowCompleted {
            name: "default".to_string(),
            success: true,
            aborted: false,
            steps_run: 4,
            steps_skipped: 1,
            duration_ms: 5000,
        });
        assert!(presenter.messages()[0].contains("succeeded"));
        assert!(presenter.messages()[0].contains("4 ran"));
    }
}
