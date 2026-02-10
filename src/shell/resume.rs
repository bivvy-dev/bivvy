//! Shell reload resume state management.
//!
//! This module provides functionality for saving and restoring execution
//! state across shell reloads.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

// Override path for testing
static STATE_PATH_OVERRIDE: Mutex<Option<PathBuf>> = Mutex::new(None);

/// State saved for resuming after shell reload.
///
/// # Example
///
/// ```no_run
/// use bivvy::shell::ResumeState;
/// use std::path::PathBuf;
///
/// // Create a resume state
/// let state = ResumeState {
///     project_root: PathBuf::from("/test/project"),
///     workflow: "default".to_string(),
///     completed_steps: vec!["step1".to_string()],
///     pending_step: "step2".to_string(),
///     timestamp: chrono::Utc::now(),
/// };
///
/// // Save state to disk
/// state.save().unwrap();
///
/// // Later, load state back
/// let loaded = ResumeState::load().unwrap().unwrap();
/// assert_eq!(loaded.workflow, "default");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumeState {
    /// Project root path.
    pub project_root: PathBuf,
    /// Workflow being executed.
    pub workflow: String,
    /// Steps that have been completed.
    pub completed_steps: Vec<String>,
    /// Step that triggered the reload.
    pub pending_step: String,
    /// Timestamp when reload was requested.
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl ResumeState {
    /// Save resume state to disk.
    pub fn save(&self) -> Result<PathBuf> {
        let path = Self::state_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(path)
    }

    /// Load resume state if it exists.
    pub fn load() -> Result<Option<Self>> {
        let path = Self::state_path();
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&path)?;
        let state: Self = serde_json::from_str(&content)?;
        Ok(Some(state))
    }

    /// Clear saved resume state.
    pub fn clear() -> Result<()> {
        let path = Self::state_path();
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }

    /// Check if there is a resume state available.
    pub fn exists() -> bool {
        Self::state_path().exists()
    }

    /// Get the resume state file path.
    fn state_path() -> PathBuf {
        if let Ok(guard) = STATE_PATH_OVERRIDE.lock() {
            if let Some(ref path) = *guard {
                return path.clone();
            }
        }
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("~/.local/share"))
            .join("bivvy")
            .join("resume-state.json")
    }

    /// Set the state path override (for testing).
    #[cfg(test)]
    pub fn set_state_path_override(path: Option<PathBuf>) {
        if let Ok(mut guard) = STATE_PATH_OVERRIDE.lock() {
            *guard = path;
        }
    }
}

/// Options for shell reload prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReloadChoice {
    /// Reload shell and continue automatically.
    ReloadAndContinue,
    /// Exit and let user reload manually.
    ExitManual,
    /// Skip the step requiring reload.
    Skip,
}

impl ReloadChoice {
    /// Get the human-readable description for this choice.
    pub fn description(&self) -> &'static str {
        match self {
            Self::ReloadAndContinue => "Reload shell and continue (recommended)",
            Self::ExitManual => "Exit and reload manually",
            Self::Skip => "Skip this step",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tempfile::TempDir;

    // Mutex to serialize tests that use state path override
    static TEST_MUTEX: Mutex<()> = Mutex::new(());

    fn with_temp_state<F, R>(f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let _guard = TEST_MUTEX.lock().unwrap();
        let temp = TempDir::new().unwrap();
        let state_path = temp.path().join("resume-state.json");
        ResumeState::set_state_path_override(Some(state_path));
        let result = f();
        ResumeState::set_state_path_override(None);
        result
    }

    #[test]
    fn saves_and_loads_resume_state() {
        with_temp_state(|| {
            let state = ResumeState {
                project_root: PathBuf::from("/test/project"),
                workflow: "default".to_string(),
                completed_steps: vec!["step1".to_string()],
                pending_step: "step2".to_string(),
                timestamp: chrono::Utc::now(),
            };

            state.save().unwrap();
            let loaded = ResumeState::load().unwrap().unwrap();

            assert_eq!(loaded.workflow, "default");
            assert_eq!(loaded.completed_steps.len(), 1);
            assert_eq!(loaded.pending_step, "step2");
        });
    }

    #[test]
    fn clear_removes_state() {
        with_temp_state(|| {
            let state = ResumeState {
                project_root: PathBuf::from("/test"),
                workflow: "default".to_string(),
                completed_steps: vec![],
                pending_step: "step1".to_string(),
                timestamp: chrono::Utc::now(),
            };

            state.save().unwrap();
            assert!(ResumeState::exists());

            ResumeState::clear().unwrap();
            assert!(!ResumeState::exists());
            assert!(ResumeState::load().unwrap().is_none());
        });
    }

    #[test]
    fn load_returns_none_when_no_state() {
        with_temp_state(|| {
            let loaded = ResumeState::load().unwrap();
            assert!(loaded.is_none());
        });
    }

    #[test]
    fn exists_returns_false_when_no_state() {
        with_temp_state(|| {
            assert!(!ResumeState::exists());
        });
    }

    #[test]
    fn reload_choice_descriptions() {
        assert!(ReloadChoice::ReloadAndContinue
            .description()
            .contains("Reload"));
        assert!(ReloadChoice::ExitManual.description().contains("Exit"));
        assert!(ReloadChoice::Skip.description().contains("Skip"));
    }
}
