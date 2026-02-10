//! Project index management.
//!
//! This module provides the [`ProjectIndex`] for tracking all known
//! projects that Bivvy has been used with.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use super::ProjectId;

/// Index of all known projects.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectIndex {
    pub projects: HashMap<String, ProjectEntry>,
}

/// Entry for a project in the index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectEntry {
    pub path: String,
    pub git_remote: Option<String>,
    pub name: String,
    pub last_accessed: DateTime<Utc>,
}

impl ProjectIndex {
    /// Get the index file path.
    pub fn file_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("~"))
            .join(".bivvy")
            .join("projects")
            .join("index.yml")
    }

    /// Load the project index.
    pub fn load() -> crate::error::Result<Self> {
        let path = Self::file_path();

        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&path)?;
        let index: Self = serde_yaml::from_str(&content).map_err(|e| {
            crate::error::BivvyError::ConfigParseError {
                path,
                message: e.to_string(),
            }
        })?;

        Ok(index)
    }

    /// Save the project index using atomic write.
    ///
    /// Uses the write-to-temp-then-rename pattern to prevent corruption.
    pub fn save(&self) -> crate::error::Result<()> {
        let path = Self::file_path();

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = serde_yaml::to_string(self).map_err(|e| {
            crate::error::BivvyError::ConfigValidationError {
                message: format!("Failed to serialize index: {}", e),
            }
        })?;

        // Atomic write: write to temp file, then rename
        let temp_path = path.with_extension("yml.tmp");
        fs::write(&temp_path, &content)?;
        fs::rename(&temp_path, &path)?;

        Ok(())
    }

    /// Update or add a project to the index.
    pub fn update(&mut self, project_id: &ProjectId) {
        let entry = ProjectEntry {
            path: project_id.path().to_string_lossy().to_string(),
            git_remote: project_id.git_remote().map(String::from),
            name: project_id.name().to_string(),
            last_accessed: Utc::now(),
        };
        self.projects.insert(project_id.hash().to_string(), entry);
    }

    /// Remove projects that no longer exist on disk.
    pub fn prune(&mut self) -> Vec<String> {
        let mut removed = Vec::new();

        self.projects.retain(|hash, entry| {
            let exists = std::path::Path::new(&entry.path).exists();
            if !exists {
                removed.push(hash.clone());
            }
            exists
        });

        removed
    }

    /// List all projects.
    pub fn list(&self) -> impl Iterator<Item = (&str, &ProjectEntry)> {
        self.projects.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Get a project by hash.
    pub fn get(&self, hash: &str) -> Option<&ProjectEntry> {
        self.projects.get(hash)
    }

    /// Number of tracked projects.
    pub fn len(&self) -> usize {
        self.projects.len()
    }

    /// Check if index is empty.
    pub fn is_empty(&self) -> bool {
        self.projects.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn project_index_default() {
        let index = ProjectIndex::default();
        assert!(index.is_empty());
    }

    #[test]
    fn project_index_update() {
        let temp = TempDir::new().unwrap();
        let project = ProjectId::from_path(temp.path()).unwrap();

        let mut index = ProjectIndex::default();
        index.update(&project);

        assert!(index.projects.contains_key(project.hash()));
        assert_eq!(index.len(), 1);
    }

    #[test]
    fn project_index_prune_removes_missing() {
        let mut index = ProjectIndex::default();
        index.projects.insert(
            "test".to_string(),
            ProjectEntry {
                path: "/nonexistent/path".to_string(),
                git_remote: None,
                name: "test".to_string(),
                last_accessed: Utc::now(),
            },
        );

        let removed = index.prune();
        assert!(removed.contains(&"test".to_string()));
        assert!(index.projects.is_empty());
    }

    #[test]
    fn project_index_prune_keeps_existing() {
        let temp = TempDir::new().unwrap();

        let mut index = ProjectIndex::default();
        index.projects.insert(
            "test".to_string(),
            ProjectEntry {
                path: temp.path().to_string_lossy().to_string(),
                git_remote: None,
                name: "test".to_string(),
                last_accessed: Utc::now(),
            },
        );

        let removed = index.prune();
        assert!(removed.is_empty());
        assert_eq!(index.len(), 1);
    }

    #[test]
    fn project_index_list() {
        let temp = TempDir::new().unwrap();
        let project = ProjectId::from_path(temp.path()).unwrap();

        let mut index = ProjectIndex::default();
        index.update(&project);

        let entries: Vec<_> = index.list().collect();
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn project_index_get() {
        let temp = TempDir::new().unwrap();
        let project = ProjectId::from_path(temp.path()).unwrap();

        let mut index = ProjectIndex::default();
        index.update(&project);

        assert!(index.get(project.hash()).is_some());
        assert!(index.get("nonexistent").is_none());
    }

    #[test]
    fn project_entry_serializes() {
        let entry = ProjectEntry {
            path: "/path/to/project".to_string(),
            git_remote: Some("https://github.com/test/repo".to_string()),
            name: "project".to_string(),
            last_accessed: Utc::now(),
        };

        let yaml = serde_yaml::to_string(&entry).unwrap();
        assert!(yaml.contains("project"));
        assert!(yaml.contains("github.com"));
    }
}
