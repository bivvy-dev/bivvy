//! Change detection for watched files.
//!
//! This module provides the [`ChangeDetector`] for checking if watched
//! files have changed since a step was last run, and the [`ChangeStatus`]
//! enum for representing the result.

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

use super::StateStore;

/// Status of changes for a step.
#[derive(Debug, Clone, PartialEq)]
pub enum ChangeStatus {
    /// Step has never been run.
    NeverRun,

    /// No changes detected since last run.
    Current,

    /// Watched files have changed since last run.
    Stale {
        /// File that changed.
        file: String,
        /// When the file was last modified.
        changed: DateTime<Utc>,
        /// When the step was last run.
        step_run: DateTime<Utc>,
    },

    /// No watches configured (always considered changed).
    NoWatches,
}

/// Detects changes in watched files.
pub struct ChangeDetector<'a> {
    state: &'a StateStore,
    project_root: &'a Path,
}

impl<'a> ChangeDetector<'a> {
    /// Create a new change detector.
    pub fn new(state: &'a StateStore, project_root: &'a Path) -> Self {
        Self {
            state,
            project_root,
        }
    }

    /// Check if a step is stale (watched files have changed).
    pub fn check_step(&self, step: &str, watches: &[String]) -> ChangeStatus {
        // Get the step's last run info
        let step_state = match self.state.get_step(step) {
            Some(s) => s,
            None => return ChangeStatus::NeverRun,
        };

        let last_run = match step_state.last_run {
            Some(t) => t,
            None => return ChangeStatus::NeverRun,
        };

        // If no watches configured, report NoWatches
        if watches.is_empty() {
            return ChangeStatus::NoWatches;
        }

        // Check each watched file
        for watch_path in watches {
            let full_path = self.project_root.join(watch_path);

            if let Some(mtime) = self.get_file_mtime(&full_path) {
                if mtime > last_run {
                    return ChangeStatus::Stale {
                        file: watch_path.clone(),
                        changed: mtime,
                        step_run: last_run,
                    };
                }
            } else {
                // File doesn't exist - treat as changed
                return ChangeStatus::Stale {
                    file: watch_path.clone(),
                    changed: Utc::now(),
                    step_run: last_run,
                };
            }
        }

        ChangeStatus::Current
    }

    fn get_file_mtime(&self, path: &Path) -> Option<DateTime<Utc>> {
        fs::metadata(path)
            .ok()
            .and_then(|m| m.modified().ok())
            .map(DateTime::from)
    }

    // --- Watch File Hashing ---

    /// Compute a hash of all watched files.
    ///
    /// The hash includes:
    /// - File path
    /// - Modification time
    /// - File size
    /// - File content (for files under 1MB)
    pub fn compute_watches_hash(&self, watches: &[String]) -> Option<String> {
        if watches.is_empty() {
            return None;
        }

        let mut hasher = Sha256::new();

        for watch_path in watches {
            let full_path = self.project_root.join(watch_path);

            if let Ok(metadata) = fs::metadata(&full_path) {
                // Hash file path
                hasher.update(watch_path.as_bytes());

                // Hash mtime
                if let Ok(mtime) = metadata.modified() {
                    hasher.update(format!("{:?}", mtime).as_bytes());
                }

                // Hash file size
                hasher.update(metadata.len().to_le_bytes());

                // For small files, also hash content
                if metadata.len() < 1024 * 1024 {
                    // 1MB limit
                    if let Ok(content) = fs::read(&full_path) {
                        hasher.update(&content);
                    }
                }
            }
        }

        let result = hasher.finalize();
        Some(hex::encode(&result[..8]))
    }

    /// Check if watches hash has changed.
    pub fn has_watches_changed(&self, step: &str, watches: &[String]) -> bool {
        let current_hash = self.compute_watches_hash(watches);
        let stored_hash = self
            .state
            .get_step(step)
            .and_then(|s| s.watches_hash.as_ref());

        match (current_hash, stored_hash) {
            (Some(current), Some(stored)) => current != *stored,
            (None, None) => false,
            _ => true, // Either no current or no stored = changed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{ProjectId, StepStatus};
    use std::time::Duration;
    use tempfile::TempDir;

    #[test]
    fn change_status_never_run() {
        let temp = TempDir::new().unwrap();
        let project = ProjectId::from_path(temp.path()).unwrap();
        let state = StateStore::new(&project);

        let detector = ChangeDetector::new(&state, temp.path());
        let status = detector.check_step("test", &["file.txt".to_string()]);

        assert_eq!(status, ChangeStatus::NeverRun);
    }

    #[test]
    fn change_status_current_when_unchanged() {
        let temp = TempDir::new().unwrap();
        let project = ProjectId::from_path(temp.path()).unwrap();
        let mut state = StateStore::new(&project);

        // Create a file
        let file_path = temp.path().join("file.txt");
        fs::write(&file_path, "content").unwrap();

        // Wait a tiny bit then record the step
        std::thread::sleep(Duration::from_millis(10));
        state.record_step_result("test", StepStatus::Success, Duration::from_secs(1), None);

        let detector = ChangeDetector::new(&state, temp.path());
        let status = detector.check_step("test", &["file.txt".to_string()]);

        assert_eq!(status, ChangeStatus::Current);
    }

    #[test]
    fn change_status_stale_when_file_changed() {
        let temp = TempDir::new().unwrap();
        let project = ProjectId::from_path(temp.path()).unwrap();
        let mut state = StateStore::new(&project);

        // Record step first
        state.record_step_result("test", StepStatus::Success, Duration::from_secs(1), None);

        // Wait then modify file
        std::thread::sleep(Duration::from_millis(10));
        let file_path = temp.path().join("file.txt");
        fs::write(&file_path, "changed").unwrap();

        let detector = ChangeDetector::new(&state, temp.path());
        let status = detector.check_step("test", &["file.txt".to_string()]);

        assert!(matches!(status, ChangeStatus::Stale { .. }));
    }

    #[test]
    fn change_status_no_watches() {
        let temp = TempDir::new().unwrap();
        let project = ProjectId::from_path(temp.path()).unwrap();
        let mut state = StateStore::new(&project);

        state.record_step_result("test", StepStatus::Success, Duration::from_secs(1), None);

        let detector = ChangeDetector::new(&state, temp.path());
        let status = detector.check_step("test", &[]);

        assert_eq!(status, ChangeStatus::NoWatches);
    }

    #[test]
    fn change_status_stale_when_file_missing() {
        let temp = TempDir::new().unwrap();
        let project = ProjectId::from_path(temp.path()).unwrap();
        let mut state = StateStore::new(&project);

        state.record_step_result("test", StepStatus::Success, Duration::from_secs(1), None);

        let detector = ChangeDetector::new(&state, temp.path());
        let status = detector.check_step("test", &["nonexistent.txt".to_string()]);

        assert!(matches!(status, ChangeStatus::Stale { .. }));
    }
}

#[cfg(test)]
mod hash_tests {
    use super::*;
    use crate::state::{ProjectId, StepStatus};
    use std::time::Duration;
    use tempfile::TempDir;

    #[test]
    fn compute_watches_hash_empty() {
        let temp = TempDir::new().unwrap();
        let project = ProjectId::from_path(temp.path()).unwrap();
        let state = StateStore::new(&project);

        let detector = ChangeDetector::new(&state, temp.path());
        let hash = detector.compute_watches_hash(&[]);

        assert!(hash.is_none());
    }

    #[test]
    fn compute_watches_hash_consistent() {
        let temp = TempDir::new().unwrap();
        let project = ProjectId::from_path(temp.path()).unwrap();
        let state = StateStore::new(&project);

        // Create a file
        fs::write(temp.path().join("file.txt"), "content").unwrap();

        let detector = ChangeDetector::new(&state, temp.path());
        let hash1 = detector.compute_watches_hash(&["file.txt".to_string()]);
        let hash2 = detector.compute_watches_hash(&["file.txt".to_string()]);

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn compute_watches_hash_changes_with_content() {
        let temp = TempDir::new().unwrap();
        let project = ProjectId::from_path(temp.path()).unwrap();
        let state = StateStore::new(&project);

        let file_path = temp.path().join("file.txt");
        fs::write(&file_path, "content1").unwrap();

        let detector = ChangeDetector::new(&state, temp.path());
        let hash1 = detector.compute_watches_hash(&["file.txt".to_string()]);

        // Wait a bit and modify
        std::thread::sleep(Duration::from_millis(10));
        fs::write(&file_path, "content2").unwrap();

        let hash2 = detector.compute_watches_hash(&["file.txt".to_string()]);

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn has_watches_changed_no_stored_hash() {
        let temp = TempDir::new().unwrap();
        let project = ProjectId::from_path(temp.path()).unwrap();
        let mut state = StateStore::new(&project);

        fs::write(temp.path().join("file.txt"), "content").unwrap();

        state.record_step_result(
            "test",
            StepStatus::Success,
            Duration::ZERO,
            None, // No watches_hash stored
        );

        let detector = ChangeDetector::new(&state, temp.path());
        assert!(detector.has_watches_changed("test", &["file.txt".to_string()]));
    }

    #[test]
    fn has_watches_changed_matching_hash() {
        let temp = TempDir::new().unwrap();
        let project = ProjectId::from_path(temp.path()).unwrap();
        let mut state = StateStore::new(&project);

        fs::write(temp.path().join("file.txt"), "content").unwrap();

        // First compute hash with a temporary detector
        let hash = {
            let detector = ChangeDetector::new(&state, temp.path());
            detector.compute_watches_hash(&["file.txt".to_string()])
        };

        // Now we can mutate state
        state.record_step_result("test", StepStatus::Success, Duration::ZERO, hash);

        // Create a new detector to check
        let detector = ChangeDetector::new(&state, temp.path());
        assert!(!detector.has_watches_changed("test", &["file.txt".to_string()]));
    }

    #[test]
    fn has_watches_changed_no_step() {
        let temp = TempDir::new().unwrap();
        let project = ProjectId::from_path(temp.path()).unwrap();
        let state = StateStore::new(&project);

        fs::write(temp.path().join("file.txt"), "content").unwrap();

        let detector = ChangeDetector::new(&state, temp.path());
        // Step doesn't exist, so there's no stored hash - should return true
        assert!(detector.has_watches_changed("nonexistent", &["file.txt".to_string()]));
    }

    #[test]
    fn hash_includes_multiple_files() {
        let temp = TempDir::new().unwrap();
        let project = ProjectId::from_path(temp.path()).unwrap();
        let state = StateStore::new(&project);

        fs::write(temp.path().join("file1.txt"), "content1").unwrap();
        fs::write(temp.path().join("file2.txt"), "content2").unwrap();

        let detector = ChangeDetector::new(&state, temp.path());

        let hash_both =
            detector.compute_watches_hash(&["file1.txt".to_string(), "file2.txt".to_string()]);
        let hash_one = detector.compute_watches_hash(&["file1.txt".to_string()]);

        assert_ne!(hash_both, hash_one);
    }
}
