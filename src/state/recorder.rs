//! Event-driven state recorder.
//!
//! Implements [`EventConsumer`] to update persistent state in response to
//! step lifecycle events. This decouples state recording from the orchestrator
//! — the orchestrator emits events, the recorder consumes them.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::logging::events::{BivvyEvent, EventConsumer};

use super::store::{StateStore, StepStatus};

/// Records step results by consuming events from the event bus.
///
/// Listens for `StepCompleted` and `StepSkipped` events and updates the
/// underlying [`StateStore`] accordingly. Uses `Arc<Mutex<StateStore>>`
/// so the caller can share access for reads and saves.
pub struct StateRecorder {
    store: Arc<Mutex<StateStore>>,
}

impl StateRecorder {
    /// Create a new recorder wrapping a shared state store.
    pub fn new(store: Arc<Mutex<StateStore>>) -> Self {
        Self { store }
    }

    /// Get the shared state store reference.
    pub fn store(&self) -> &Arc<Mutex<StateStore>> {
        &self.store
    }
}

impl EventConsumer for StateRecorder {
    fn on_event(&mut self, event: &BivvyEvent) {
        match event {
            BivvyEvent::StepCompleted {
                name,
                success,
                duration_ms,
                ..
            } => {
                let status = if *success {
                    StepStatus::Success
                } else {
                    StepStatus::Failed
                };
                if let Ok(mut store) = self.store.lock() {
                    store.record_step_result(name, status, Duration::from_millis(*duration_ms));
                }
            }
            BivvyEvent::StepSkipped { name, .. } => {
                if let Ok(mut store) = self.store.lock() {
                    store.record_step_result(name, StepStatus::Skipped, Duration::ZERO);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ProjectId;
    use tempfile::TempDir;

    fn make_shared_store() -> (Arc<Mutex<StateStore>>, TempDir) {
        let temp = TempDir::new().unwrap();
        let project_id = ProjectId::from_path(temp.path()).unwrap();
        (Arc::new(Mutex::new(StateStore::new(&project_id))), temp)
    }

    #[test]
    fn records_step_completion() {
        let (store, _tmp) = make_shared_store();
        let mut recorder = StateRecorder::new(store.clone());
        recorder.on_event(&BivvyEvent::StepCompleted {
            name: "build".to_string(),
            success: true,
            exit_code: Some(0),
            duration_ms: 1500,
            error: None,
        });
        assert!(store.lock().unwrap().is_step_complete("build"));
    }

    #[test]
    fn records_step_failure() {
        let (store, _tmp) = make_shared_store();
        let mut recorder = StateRecorder::new(store.clone());
        recorder.on_event(&BivvyEvent::StepCompleted {
            name: "build".to_string(),
            success: false,
            exit_code: Some(1),
            duration_ms: 500,
            error: Some("exit code 1".to_string()),
        });
        assert!(!store.lock().unwrap().is_step_complete("build"));
    }

    #[test]
    fn records_step_skipped() {
        let (store, _tmp) = make_shared_store();
        let mut recorder = StateRecorder::new(store.clone());
        recorder.on_event(&BivvyEvent::StepSkipped {
            name: "deploy".to_string(),
            reason: "user_declined".to_string(),
        });
        let s = store.lock().unwrap();
        let step = s.steps.get("deploy").unwrap();
        assert_eq!(step.status, StepStatus::Skipped);
    }

    #[test]
    fn ignores_unrelated_events() {
        let (store, _tmp) = make_shared_store();
        let mut recorder = StateRecorder::new(store.clone());
        recorder.on_event(&BivvyEvent::WorkflowStarted {
            name: "default".to_string(),
            step_count: 3,
        });
        assert!(store.lock().unwrap().steps.is_empty());
    }

    #[test]
    fn shared_store_accessible_after_recording() {
        let (store, _tmp) = make_shared_store();
        let mut recorder = StateRecorder::new(store.clone());
        recorder.on_event(&BivvyEvent::StepCompleted {
            name: "test".to_string(),
            success: true,
            exit_code: Some(0),
            duration_ms: 100,
            error: None,
        });
        // The shared store is still accessible
        assert!(store.lock().unwrap().is_step_complete("test"));
    }
}
