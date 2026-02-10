//! Run history recording.
//!
//! This module provides types for recording workflow execution history,
//! including the [`RunRecord`] struct and [`RunHistoryBuilder`] for
//! building records during execution.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A record of a single Bivvy run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRecord {
    /// When the run started.
    pub timestamp: DateTime<Utc>,

    /// Which workflow was executed.
    pub workflow: String,

    /// Total duration in milliseconds.
    pub duration_ms: u64,

    /// Overall status.
    pub status: RunStatus,

    /// Steps that were executed (not skipped).
    pub steps_run: Vec<String>,

    /// Steps that were skipped.
    pub steps_skipped: Vec<String>,

    /// Any error message if failed.
    pub error: Option<String>,
}

/// Status of a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunStatus {
    Success,
    Failed,
    Interrupted,
}

/// Helper for building run history.
pub struct RunHistoryBuilder {
    workflow: String,
    start_time: DateTime<Utc>,
    steps_run: Vec<String>,
    steps_skipped: Vec<String>,
}

impl RunHistoryBuilder {
    /// Start a new run record.
    pub fn start(workflow: &str) -> Self {
        Self {
            workflow: workflow.to_string(),
            start_time: Utc::now(),
            steps_run: Vec::new(),
            steps_skipped: Vec::new(),
        }
    }

    /// Record a step as executed.
    pub fn step_run(&mut self, step: &str) {
        self.steps_run.push(step.to_string());
    }

    /// Record a step as skipped.
    pub fn step_skipped(&mut self, step: &str) {
        self.steps_skipped.push(step.to_string());
    }

    /// Finish with success.
    pub fn finish_success(self) -> RunRecord {
        RunRecord {
            timestamp: self.start_time,
            workflow: self.workflow,
            duration_ms: (Utc::now() - self.start_time).num_milliseconds() as u64,
            status: RunStatus::Success,
            steps_run: self.steps_run,
            steps_skipped: self.steps_skipped,
            error: None,
        }
    }

    /// Finish with failure.
    pub fn finish_failed(self, error: &str) -> RunRecord {
        RunRecord {
            timestamp: self.start_time,
            workflow: self.workflow,
            duration_ms: (Utc::now() - self.start_time).num_milliseconds() as u64,
            status: RunStatus::Failed,
            steps_run: self.steps_run,
            steps_skipped: self.steps_skipped,
            error: Some(error.to_string()),
        }
    }

    /// Finish as interrupted.
    pub fn finish_interrupted(self) -> RunRecord {
        RunRecord {
            timestamp: self.start_time,
            workflow: self.workflow,
            duration_ms: (Utc::now() - self.start_time).num_milliseconds() as u64,
            status: RunStatus::Interrupted,
            steps_run: self.steps_run,
            steps_skipped: self.steps_skipped,
            error: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_history_builder_success() {
        let mut builder = RunHistoryBuilder::start("default");
        builder.step_run("step1");
        builder.step_run("step2");
        builder.step_skipped("step3");

        let record = builder.finish_success();

        assert_eq!(record.workflow, "default");
        assert_eq!(record.status, RunStatus::Success);
        assert_eq!(record.steps_run, vec!["step1", "step2"]);
        assert_eq!(record.steps_skipped, vec!["step3"]);
        assert!(record.error.is_none());
    }

    #[test]
    fn run_history_builder_failed() {
        let builder = RunHistoryBuilder::start("default");
        let record = builder.finish_failed("Step failed");

        assert_eq!(record.status, RunStatus::Failed);
        assert_eq!(record.error, Some("Step failed".to_string()));
    }

    #[test]
    fn run_history_builder_interrupted() {
        let builder = RunHistoryBuilder::start("default");
        let record = builder.finish_interrupted();

        assert_eq!(record.status, RunStatus::Interrupted);
        assert!(record.error.is_none());
    }

    #[test]
    fn run_record_serializes() {
        let record = RunRecord {
            timestamp: Utc::now(),
            workflow: "default".to_string(),
            duration_ms: 1000,
            status: RunStatus::Success,
            steps_run: vec!["a".to_string()],
            steps_skipped: vec![],
            error: None,
        };

        let yaml = serde_yaml::to_string(&record).unwrap();
        assert!(yaml.contains("default"));
        assert!(yaml.contains("Success"));
    }

    #[test]
    fn run_status_equality() {
        assert_eq!(RunStatus::Success, RunStatus::Success);
        assert_ne!(RunStatus::Success, RunStatus::Failed);
        assert_ne!(RunStatus::Failed, RunStatus::Interrupted);
    }

    #[test]
    fn run_history_builder_empty_steps() {
        let builder = RunHistoryBuilder::start("empty");
        let record = builder.finish_success();

        assert!(record.steps_run.is_empty());
        assert!(record.steps_skipped.is_empty());
    }
}
