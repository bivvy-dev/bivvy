//! Persistent state storage.
//!
//! This module provides the main state storage for Bivvy projects,
//! including step states and run history.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use super::{ProjectId, RunRecord};

/// Persistent state for a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateStore {
    /// Schema version for migration.
    pub version: u32,

    /// Project identification.
    pub project: ProjectInfo,

    /// Last run timestamp.
    pub last_run: Option<DateTime<Utc>>,

    /// Last workflow executed.
    pub last_workflow: Option<String>,

    /// State for each step.
    pub steps: HashMap<String, StepState>,

    /// Run history (most recent first).
    #[serde(default)]
    pub runs: Vec<RunRecord>,
}

/// Project information stored in state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub path: String,
    pub git_remote: Option<String>,
    pub name: String,
}

/// State for a single step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepState {
    /// When this step last ran.
    pub last_run: Option<DateTime<Utc>>,

    /// Status of the last run.
    pub status: StepStatus,

    /// Duration of the last run in milliseconds.
    pub duration_ms: Option<u64>,

    /// Legacy v1 watches hash. Present only in v1 state files that had
    /// change-detection watches configured. Consumed during migration to
    /// populate SnapshotStore baselines, then cleared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watches_hash: Option<String>,
}

/// A baseline migration entry extracted from v1 state.
///
/// Each entry represents a `watches_hash` from a v1 `StepState` that
/// should be recorded in the `SnapshotStore` as a `_last_run` baseline.
#[derive(Debug, Clone)]
pub struct BaselineMigration {
    /// Step name.
    pub step_name: String,
    /// The v1 watches hash to use as the initial baseline.
    pub hash: String,
}

/// Status of a step execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StepStatus {
    Success,
    Failed,
    Skipped,
    NeverRun,
}

/// Entry in step history.
#[derive(Debug, Clone)]
pub struct StepHistoryEntry {
    pub timestamp: DateTime<Utc>,
    pub status: StepStatus,
    pub workflow: String,
}

/// Summary for status command.
#[derive(Debug, Clone)]
pub struct StatusSummary {
    pub last_run: Option<DateTime<Utc>>,
    pub last_workflow: Option<String>,
    pub step_count: usize,
    pub complete_count: usize,
}

/// Size information for state.
#[derive(Debug, Clone)]
pub struct StateSize {
    pub run_count: usize,
    pub step_count: usize,
}

impl StateStore {
    /// Current schema version.
    pub const CURRENT_VERSION: u32 = 2;

    /// Default number of runs to keep.
    pub const DEFAULT_HISTORY_RETENTION: usize = 50;

    /// Create a new state store for a project.
    pub fn new(project_id: &ProjectId) -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            project: ProjectInfo {
                path: project_id.path().to_string_lossy().to_string(),
                git_remote: project_id.git_remote().map(String::from),
                name: project_id.name().to_string(),
            },
            last_run: None,
            last_workflow: None,
            steps: HashMap::new(),
            runs: Vec::new(),
        }
    }

    /// Get the state directory for a project.
    pub fn state_dir(project_id: &ProjectId) -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("~"))
            .join(".bivvy")
            .join("projects")
            .join(project_id.hash())
    }

    /// Get the state file path.
    pub fn state_file(project_id: &ProjectId) -> PathBuf {
        Self::state_dir(project_id).join("state.yml")
    }

    /// Load state from disk, applying migrations if needed.
    ///
    /// Returns the state store and any baseline migrations that need to be
    /// applied to the `SnapshotStore` (from v1 `watches_hash` fields).
    pub fn load(project_id: &ProjectId) -> crate::error::Result<(Self, Vec<BaselineMigration>)> {
        let path = Self::state_file(project_id);

        if !path.exists() {
            return Ok((Self::new(project_id), Vec::new()));
        }

        let content = fs::read_to_string(&path)?;
        let mut state: Self = serde_yaml::from_str(&content).map_err(|e| {
            crate::error::BivvyError::ConfigParseError {
                path: path.clone(),
                message: e.to_string(),
            }
        })?;

        // Apply migrations
        let baselines = if state.version < Self::CURRENT_VERSION {
            state.migrate()
        } else {
            Vec::new()
        };

        Ok((state, baselines))
    }

    /// Apply any pending migrations to bring state to the current version.
    ///
    /// Returns baseline migrations extracted from v1 `watches_hash` fields.
    /// The caller should apply these to the `SnapshotStore`.
    fn migrate(&mut self) -> Vec<BaselineMigration> {
        let mut baselines = Vec::new();

        // v1 → v2: Extract watches_hash from steps into baseline migrations
        if self.version < 2 {
            for (step_name, step_state) in &mut self.steps {
                if let Some(hash) = step_state.watches_hash.take() {
                    baselines.push(BaselineMigration {
                        step_name: step_name.clone(),
                        hash,
                    });
                }
            }
        }

        self.version = Self::CURRENT_VERSION;
        baselines
    }

    /// Save state to disk using atomic write.
    ///
    /// Uses the write-to-temp-then-rename pattern to prevent corruption
    /// if the process crashes or loses power during the write operation.
    /// This ensures state files are never partially written.
    pub fn save(&self, project_id: &ProjectId) -> crate::error::Result<()> {
        let dir = Self::state_dir(project_id);
        fs::create_dir_all(&dir)?;

        let path = Self::state_file(project_id);
        let content = serde_yaml::to_string(self).map_err(|e| {
            crate::error::BivvyError::ConfigValidationError {
                message: format!("Failed to serialize state: {}", e),
            }
        })?;

        // Atomic write: write to temp file, then rename
        // This prevents corruption if process crashes mid-write
        let temp_path = path.with_extension("yml.tmp");
        fs::write(&temp_path, &content)?;
        fs::rename(&temp_path, &path)?;

        Ok(())
    }

    /// Get state for a step.
    pub fn get_step(&self, name: &str) -> Option<&StepState> {
        self.steps.get(name)
    }

    /// Update state for a step.
    pub fn update_step(&mut self, name: &str, state: StepState) {
        self.steps.insert(name.to_string(), state);
    }

    // --- Step State Tracking ---

    /// Record a step execution result.
    pub fn record_step_result(
        &mut self,
        step: &str,
        status: StepStatus,
        duration: std::time::Duration,
    ) {
        let state = StepState {
            last_run: Some(Utc::now()),
            status,
            duration_ms: Some(duration.as_millis() as u64),
            watches_hash: None,
        };
        self.steps.insert(step.to_string(), state);
    }

    /// Check if a step has been run successfully.
    pub fn is_step_complete(&self, step: &str) -> bool {
        self.steps
            .get(step)
            .map(|s| s.status == StepStatus::Success)
            .unwrap_or(false)
    }

    /// Get the last run time for a step.
    pub fn step_last_run(&self, step: &str) -> Option<DateTime<Utc>> {
        self.steps.get(step).and_then(|s| s.last_run)
    }

    /// Clear state for a specific step.
    pub fn clear_step(&mut self, step: &str) {
        self.steps.remove(step);
    }

    /// Clear all step states.
    pub fn clear_all_steps(&mut self) {
        self.steps.clear();
    }

    // --- Run History ---

    /// Record a completed run.
    pub fn record_run(&mut self, record: RunRecord) {
        self.last_run = Some(record.timestamp);
        self.last_workflow = Some(record.workflow.clone());
        self.runs.insert(0, record);
    }

    /// Get the most recent run.
    pub fn last_run_record(&self) -> Option<&RunRecord> {
        self.runs.first()
    }

    /// Get run history (most recent first).
    pub fn run_history(&self, limit: usize) -> &[RunRecord] {
        let len = self.runs.len().min(limit);
        &self.runs[..len]
    }

    // --- Query Methods ---

    /// Get last run details for a specific step.
    pub fn step_history(&self, step: &str) -> Vec<StepHistoryEntry> {
        self.runs
            .iter()
            .filter_map(|run| {
                if run.steps_run.contains(&step.to_string()) {
                    Some(StepHistoryEntry {
                        timestamp: run.timestamp,
                        status: if run.status == super::RunStatus::Success {
                            StepStatus::Success
                        } else {
                            StepStatus::Failed
                        },
                        workflow: run.workflow.clone(),
                    })
                } else if run.steps_skipped.contains(&step.to_string()) {
                    Some(StepHistoryEntry {
                        timestamp: run.timestamp,
                        status: StepStatus::Skipped,
                        workflow: run.workflow.clone(),
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get a summary for the status command.
    pub fn status_summary(&self) -> StatusSummary {
        StatusSummary {
            last_run: self.last_run,
            last_workflow: self.last_workflow.clone(),
            step_count: self.steps.len(),
            complete_count: self
                .steps
                .values()
                .filter(|s| s.status == StepStatus::Success)
                .count(),
        }
    }

    // --- History Pruning ---

    /// Prune old run history.
    pub fn prune_history(&mut self, keep: usize) {
        if self.runs.len() > keep {
            self.runs.truncate(keep);
        }
    }

    /// Clean up state (remove old data, prune history).
    pub fn cleanup(&mut self, retention: usize) {
        self.prune_history(retention);

        // Remove step state for steps not in recent runs
        let recent_steps: std::collections::HashSet<String> = self
            .runs
            .iter()
            .flat_map(|r| r.steps_run.iter().chain(r.steps_skipped.iter()))
            .cloned()
            .collect();

        self.steps.retain(|name, _| recent_steps.contains(name));
    }

    /// Get the size of the state (for diagnostics).
    pub fn size(&self) -> StateSize {
        StateSize {
            run_count: self.runs.len(),
            step_count: self.steps.len(),
        }
    }
}

impl Default for StepState {
    fn default() -> Self {
        Self {
            last_run: None,
            status: StepStatus::NeverRun,
            duration_ms: None,
            watches_hash: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn state_store_new() {
        let temp = TempDir::new().unwrap();
        let project = ProjectId::from_path(temp.path()).unwrap();
        let state = StateStore::new(&project);

        assert_eq!(state.version, 2);
        assert_eq!(state.version, StateStore::CURRENT_VERSION);
        assert!(state.last_run.is_none());
        assert!(state.steps.is_empty());
    }

    #[test]
    fn state_store_save_and_load() {
        let temp = TempDir::new().unwrap();
        let project = ProjectId::from_path(temp.path()).unwrap();

        let mut state = StateStore::new(&project);
        state.update_step(
            "test_step",
            StepState {
                status: StepStatus::Success,
                last_run: Some(Utc::now()),
                duration_ms: Some(1000),
                watches_hash: None,
            },
        );

        state.save(&project).unwrap();

        let (loaded, _) = StateStore::load(&project).unwrap();
        assert!(loaded.get_step("test_step").is_some());
        assert_eq!(
            loaded.get_step("test_step").unwrap().status,
            StepStatus::Success
        );
    }

    #[test]
    fn state_store_load_nonexistent_returns_new() {
        let temp = TempDir::new().unwrap();
        let project = ProjectId::from_path(temp.path()).unwrap();

        let (state, _) = StateStore::load(&project).unwrap();
        assert!(state.steps.is_empty());
    }

    #[test]
    fn step_status_serializes() {
        let state = StepState {
            status: StepStatus::Success,
            ..Default::default()
        };

        let yaml = serde_yaml::to_string(&state).unwrap();
        assert!(yaml.contains("Success"));
    }

    #[test]
    fn save_uses_atomic_write() {
        let temp = TempDir::new().unwrap();
        let project = ProjectId::from_path(temp.path()).unwrap();

        let mut state = StateStore::new(&project);
        state.update_step(
            "test_step",
            StepState {
                status: StepStatus::Success,
                ..Default::default()
            },
        );

        // Save state
        state.save(&project).unwrap();

        // Verify no temp file remains (it should have been renamed)
        let temp_path = StateStore::state_file(&project).with_extension("yml.tmp");
        assert!(
            !temp_path.exists(),
            "Temp file should not exist after successful save"
        );

        // Verify actual state file exists and is valid
        let (loaded, _) = StateStore::load(&project).unwrap();
        assert_eq!(
            loaded.get_step("test_step").unwrap().status,
            StepStatus::Success
        );
    }
}

#[cfg(test)]
mod step_state_tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn record_step_result() {
        let temp = TempDir::new().unwrap();
        let project = ProjectId::from_path(temp.path()).unwrap();
        let mut state = StateStore::new(&project);

        state.record_step_result(
            "test",
            StepStatus::Success,
            std::time::Duration::from_secs(5),
        );

        let step = state.get_step("test").unwrap();
        assert_eq!(step.status, StepStatus::Success);
        assert_eq!(step.duration_ms, Some(5000));
    }

    #[test]
    fn is_step_complete() {
        let temp = TempDir::new().unwrap();
        let project = ProjectId::from_path(temp.path()).unwrap();
        let mut state = StateStore::new(&project);

        assert!(!state.is_step_complete("test"));

        state.record_step_result(
            "test",
            StepStatus::Success,
            std::time::Duration::from_secs(1),
        );

        assert!(state.is_step_complete("test"));
    }

    #[test]
    fn is_step_complete_false_for_failed() {
        let temp = TempDir::new().unwrap();
        let project = ProjectId::from_path(temp.path()).unwrap();
        let mut state = StateStore::new(&project);

        state.record_step_result(
            "test",
            StepStatus::Failed,
            std::time::Duration::from_secs(1),
        );

        assert!(!state.is_step_complete("test"));
    }

    #[test]
    fn clear_step() {
        let temp = TempDir::new().unwrap();
        let project = ProjectId::from_path(temp.path()).unwrap();
        let mut state = StateStore::new(&project);

        state.record_step_result("test", StepStatus::Success, std::time::Duration::ZERO);
        assert!(state.get_step("test").is_some());

        state.clear_step("test");
        assert!(state.get_step("test").is_none());
    }

    #[test]
    fn clear_all_steps() {
        let temp = TempDir::new().unwrap();
        let project = ProjectId::from_path(temp.path()).unwrap();
        let mut state = StateStore::new(&project);

        state.record_step_result("a", StepStatus::Success, std::time::Duration::ZERO);
        state.record_step_result("b", StepStatus::Success, std::time::Duration::ZERO);

        state.clear_all_steps();

        assert!(state.steps.is_empty());
    }
}

#[cfg(test)]
mod query_tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn step_history_returns_entries() {
        let temp = TempDir::new().unwrap();
        let project = super::super::ProjectId::from_path(temp.path()).unwrap();
        let mut state = StateStore::new(&project);

        let record = super::super::RunRecord {
            timestamp: Utc::now(),
            workflow: "default".to_string(),
            duration_ms: 1000,
            status: super::super::RunStatus::Success,
            steps_run: vec!["test".to_string()],
            steps_skipped: vec![],
            error: None,
        };

        state.record_run(record);

        let history = state.step_history("test");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].status, StepStatus::Success);
        assert_eq!(history[0].workflow, "default");
    }

    #[test]
    fn step_history_includes_skipped() {
        let temp = TempDir::new().unwrap();
        let project = super::super::ProjectId::from_path(temp.path()).unwrap();
        let mut state = StateStore::new(&project);

        let record = super::super::RunRecord {
            timestamp: Utc::now(),
            workflow: "default".to_string(),
            duration_ms: 1000,
            status: super::super::RunStatus::Success,
            steps_run: vec![],
            steps_skipped: vec!["test".to_string()],
            error: None,
        };

        state.record_run(record);

        let history = state.step_history("test");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].status, StepStatus::Skipped);
    }

    #[test]
    fn step_history_empty_for_unknown_step() {
        let temp = TempDir::new().unwrap();
        let project = super::super::ProjectId::from_path(temp.path()).unwrap();
        let state = StateStore::new(&project);

        let history = state.step_history("unknown");
        assert!(history.is_empty());
    }

    #[test]
    fn status_summary() {
        let temp = TempDir::new().unwrap();
        let project = super::super::ProjectId::from_path(temp.path()).unwrap();
        let mut state = StateStore::new(&project);

        state.record_step_result("step1", StepStatus::Success, std::time::Duration::ZERO);
        state.record_step_result("step2", StepStatus::Failed, std::time::Duration::ZERO);

        let summary = state.status_summary();
        assert_eq!(summary.step_count, 2);
        assert_eq!(summary.complete_count, 1);
    }

    #[test]
    fn status_summary_with_last_run() {
        let temp = TempDir::new().unwrap();
        let project = super::super::ProjectId::from_path(temp.path()).unwrap();
        let mut state = StateStore::new(&project);

        let record = super::super::RunRecord {
            timestamp: Utc::now(),
            workflow: "custom".to_string(),
            duration_ms: 1000,
            status: super::super::RunStatus::Success,
            steps_run: vec![],
            steps_skipped: vec![],
            error: None,
        };

        state.record_run(record);

        let summary = state.status_summary();
        assert!(summary.last_run.is_some());
        assert_eq!(summary.last_workflow, Some("custom".to_string()));
    }
}

#[cfg(test)]
mod prune_tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn prune_history_keeps_recent() {
        let temp = TempDir::new().unwrap();
        let project = super::super::ProjectId::from_path(temp.path()).unwrap();
        let mut state = StateStore::new(&project);

        // Add 10 runs
        for i in 0..10 {
            let record = super::super::RunRecord {
                timestamp: Utc::now(),
                workflow: format!("run{}", i),
                duration_ms: 1000,
                status: super::super::RunStatus::Success,
                steps_run: vec![],
                steps_skipped: vec![],
                error: None,
            };
            state.record_run(record);
        }

        assert_eq!(state.runs.len(), 10);

        state.prune_history(5);

        assert_eq!(state.runs.len(), 5);
        // Most recent should still be there
        assert_eq!(state.runs[0].workflow, "run9");
    }

    #[test]
    fn prune_history_no_op_when_under_limit() {
        let temp = TempDir::new().unwrap();
        let project = super::super::ProjectId::from_path(temp.path()).unwrap();
        let mut state = StateStore::new(&project);

        // Add 3 runs
        for i in 0..3 {
            let record = super::super::RunRecord {
                timestamp: Utc::now(),
                workflow: format!("run{}", i),
                duration_ms: 1000,
                status: super::super::RunStatus::Success,
                steps_run: vec![],
                steps_skipped: vec![],
                error: None,
            };
            state.record_run(record);
        }

        state.prune_history(5);

        assert_eq!(state.runs.len(), 3);
    }

    #[test]
    fn cleanup_removes_orphaned_steps() {
        let temp = TempDir::new().unwrap();
        let project = super::super::ProjectId::from_path(temp.path()).unwrap();
        let mut state = StateStore::new(&project);

        // Add orphan step state (not in any run)
        state.record_step_result("orphan", StepStatus::Success, std::time::Duration::ZERO);

        // Add a run with a different step
        let record = super::super::RunRecord {
            timestamp: Utc::now(),
            workflow: "default".to_string(),
            duration_ms: 1000,
            status: super::super::RunStatus::Success,
            steps_run: vec!["kept".to_string()],
            steps_skipped: vec![],
            error: None,
        };
        state.record_run(record);

        // Add the kept step state
        state.record_step_result("kept", StepStatus::Success, std::time::Duration::ZERO);

        state.cleanup(50);

        assert!(state.get_step("orphan").is_none());
        assert!(state.get_step("kept").is_some());
    }

    #[test]
    fn cleanup_keeps_skipped_steps() {
        let temp = TempDir::new().unwrap();
        let project = super::super::ProjectId::from_path(temp.path()).unwrap();
        let mut state = StateStore::new(&project);

        let record = super::super::RunRecord {
            timestamp: Utc::now(),
            workflow: "default".to_string(),
            duration_ms: 1000,
            status: super::super::RunStatus::Success,
            steps_run: vec![],
            steps_skipped: vec!["skipped".to_string()],
            error: None,
        };
        state.record_run(record);

        state.record_step_result("skipped", StepStatus::Skipped, std::time::Duration::ZERO);

        state.cleanup(50);

        assert!(state.get_step("skipped").is_some());
    }

    #[test]
    fn size_returns_counts() {
        let temp = TempDir::new().unwrap();
        let project = super::super::ProjectId::from_path(temp.path()).unwrap();
        let mut state = StateStore::new(&project);

        state.record_step_result("a", StepStatus::Success, std::time::Duration::ZERO);
        state.record_step_result("b", StepStatus::Success, std::time::Duration::ZERO);

        let record = super::super::RunRecord {
            timestamp: Utc::now(),
            workflow: "default".to_string(),
            duration_ms: 1000,
            status: super::super::RunStatus::Success,
            steps_run: vec![],
            steps_skipped: vec![],
            error: None,
        };
        state.record_run(record);

        let size = state.size();
        assert_eq!(size.step_count, 2);
        assert_eq!(size.run_count, 1);
    }
}

#[cfg(test)]
mod migration_tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn v2_state_loads_correctly() {
        let temp = TempDir::new().unwrap();
        let project = ProjectId::from_path(temp.path()).unwrap();

        let mut state = StateStore::new(&project);
        assert_eq!(state.version, 2);
        state.update_step(
            "step1",
            StepState {
                status: StepStatus::Success,
                ..Default::default()
            },
        );
        state.save(&project).unwrap();

        let (loaded, _) = StateStore::load(&project).unwrap();
        assert_eq!(loaded.version, 2);
        assert_eq!(
            loaded.get_step("step1").unwrap().status,
            StepStatus::Success,
        );
    }

    #[test]
    fn migrate_v1_extracts_watches_hash_as_baseline() {
        let temp = TempDir::new().unwrap();
        let project = ProjectId::from_path(temp.path()).unwrap();

        // Write a v1 state file with watches_hash
        let state_dir = StateStore::state_dir(&project);
        std::fs::create_dir_all(&state_dir).unwrap();
        let state_file = state_dir.join("state.yml");
        std::fs::write(
            &state_file,
            r#"version: 1
project:
  path: /tmp/test
  name: test
last_run: null
last_workflow: null
steps:
  build:
    last_run: null
    status: Success
    duration_ms: 100
    watches_hash: "abc123def456"
  test:
    last_run: null
    status: NeverRun
    duration_ms: null
runs: []
"#,
        )
        .unwrap();

        let (loaded, baselines) = StateStore::load(&project).unwrap();
        assert_eq!(loaded.version, 2);
        assert_eq!(baselines.len(), 1);
        assert_eq!(baselines[0].step_name, "build");
        assert_eq!(baselines[0].hash, "abc123def456");

        // watches_hash should be cleared after migration
        assert!(loaded.get_step("build").unwrap().watches_hash.is_none());
    }

    #[test]
    fn migrate_v2_returns_no_baselines() {
        let temp = TempDir::new().unwrap();
        let project = ProjectId::from_path(temp.path()).unwrap();

        let mut state = StateStore::new(&project);
        state.update_step(
            "step1",
            StepState {
                status: StepStatus::Success,
                ..Default::default()
            },
        );
        state.save(&project).unwrap();

        let (_, baselines) = StateStore::load(&project).unwrap();
        assert!(baselines.is_empty());
    }
}
