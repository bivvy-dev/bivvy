//! User preferences persistence.
//!
//! This module provides the [`Preferences`] struct for storing
//! saved user choices like prompt answers and skip behaviors.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use super::{ProjectId, StateStore};

/// Saved user preferences for a project.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Preferences {
    /// Saved prompt answers.
    #[serde(default)]
    pub prompts: HashMap<String, String>,

    /// Skip behavior preferences.
    #[serde(default)]
    pub skip_behavior: HashMap<String, String>,

    /// Template source preferences (for collision resolution).
    #[serde(default)]
    pub template_sources: HashMap<String, String>,

    /// Other arbitrary preferences.
    #[serde(default)]
    pub other: HashMap<String, String>,
}

impl Preferences {
    /// Get the preferences file path.
    pub fn file_path(project_id: &ProjectId) -> PathBuf {
        StateStore::state_dir(project_id).join("preferences.yml")
    }

    /// Load preferences from disk.
    pub fn load(project_id: &ProjectId) -> crate::error::Result<Self> {
        let path = Self::file_path(project_id);

        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&path)?;
        let prefs: Self = serde_yaml::from_str(&content).map_err(|e| {
            crate::error::BivvyError::ConfigParseError {
                path,
                message: e.to_string(),
            }
        })?;

        Ok(prefs)
    }

    /// Save preferences to disk using atomic write.
    ///
    /// Uses the write-to-temp-then-rename pattern to prevent corruption.
    pub fn save(&self, project_id: &ProjectId) -> crate::error::Result<()> {
        let dir = StateStore::state_dir(project_id);
        fs::create_dir_all(&dir)?;

        let path = Self::file_path(project_id);
        let content = serde_yaml::to_string(self).map_err(|e| {
            crate::error::BivvyError::ConfigValidationError {
                message: format!("Failed to serialize preferences: {}", e),
            }
        })?;

        // Atomic write: write to temp file, then rename
        let temp_path = path.with_extension("yml.tmp");
        fs::write(&temp_path, &content)?;
        fs::rename(&temp_path, &path)?;

        Ok(())
    }

    /// Get a saved prompt value.
    pub fn get_prompt(&self, key: &str) -> Option<&str> {
        self.prompts.get(key).map(|s| s.as_str())
    }

    /// Save a prompt value.
    pub fn set_prompt(&mut self, key: &str, value: &str) {
        self.prompts.insert(key.to_string(), value.to_string());
    }

    /// Get skip behavior for a step.
    pub fn get_skip_behavior(&self, step: &str) -> Option<&str> {
        self.skip_behavior.get(step).map(|s| s.as_str())
    }

    /// Save skip behavior for a step.
    pub fn set_skip_behavior(&mut self, step: &str, behavior: &str) {
        self.skip_behavior
            .insert(step.to_string(), behavior.to_string());
    }

    /// Get template source for a template.
    pub fn get_template_source(&self, template: &str) -> Option<&str> {
        self.template_sources.get(template).map(|s| s.as_str())
    }

    /// Save template source preference.
    pub fn set_template_source(&mut self, template: &str, source: &str) {
        self.template_sources
            .insert(template.to_string(), source.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn preferences_default() {
        let prefs = Preferences::default();
        assert!(prefs.prompts.is_empty());
        assert!(prefs.skip_behavior.is_empty());
    }

    #[test]
    fn preferences_save_and_load() {
        let temp = TempDir::new().unwrap();
        let project = ProjectId::from_path(temp.path()).unwrap();

        let mut prefs = Preferences::default();
        prefs.set_prompt("install_mode", "frozen");
        prefs.set_skip_behavior("seeds", "skip_only");

        prefs.save(&project).unwrap();

        let loaded = Preferences::load(&project).unwrap();
        assert_eq!(loaded.get_prompt("install_mode"), Some("frozen"));
        assert_eq!(loaded.get_skip_behavior("seeds"), Some("skip_only"));
    }

    #[test]
    fn preferences_load_nonexistent_returns_default() {
        let temp = TempDir::new().unwrap();
        let project = ProjectId::from_path(temp.path()).unwrap();

        let prefs = Preferences::load(&project).unwrap();
        assert!(prefs.prompts.is_empty());
    }

    #[test]
    fn preferences_serializes_correctly() {
        let mut prefs = Preferences::default();
        prefs.set_prompt("key", "value");

        let yaml = serde_yaml::to_string(&prefs).unwrap();
        assert!(yaml.contains("key"));
        assert!(yaml.contains("value"));
    }

    #[test]
    fn preferences_template_sources() {
        let mut prefs = Preferences::default();
        prefs.set_template_source("rails", "builtin");

        assert_eq!(prefs.get_template_source("rails"), Some("builtin"));
        assert!(prefs.get_template_source("unknown").is_none());
    }

    #[test]
    fn preferences_save_uses_atomic_write() {
        let temp = TempDir::new().unwrap();
        let project = ProjectId::from_path(temp.path()).unwrap();

        let mut prefs = Preferences::default();
        prefs.set_prompt("test", "value");

        prefs.save(&project).unwrap();

        // Verify no temp file remains
        let temp_path = Preferences::file_path(&project).with_extension("yml.tmp");
        assert!(
            !temp_path.exists(),
            "Temp file should not exist after successful save"
        );

        // Verify the file was actually saved
        let loaded = Preferences::load(&project).unwrap();
        assert_eq!(loaded.get_prompt("test"), Some("value"));
    }
}
