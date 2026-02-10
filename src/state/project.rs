//! Project identification and hashing.
//!
//! This module provides unique project identification using SHA256 hashing
//! of the project path and git remote URL.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

use crate::error::Result;

/// Unique identifier for a project.
///
/// Projects are identified by a combination of their absolute path and
/// git remote URL (if available). This produces a stable hash that can
/// be used as a directory name for storing state.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProjectId {
    /// SHA256 hash of project identification data.
    hash: String,
    /// Absolute path to the project root.
    path: PathBuf,
    /// Git remote URL if available.
    git_remote: Option<String>,
}

impl ProjectId {
    /// Create a ProjectId from a project path.
    ///
    /// The path will be canonicalized to an absolute path. Git remote
    /// detection is attempted using system git.
    ///
    /// # Errors
    ///
    /// Returns an error if the path cannot be canonicalized (e.g., doesn't exist).
    pub fn from_path(path: &Path) -> Result<Self> {
        let abs_path = path.canonicalize().map_err(crate::error::BivvyError::Io)?;

        let git_remote = Self::detect_git_remote(&abs_path);
        let hash = Self::compute_hash(&abs_path, git_remote.as_deref());

        Ok(Self {
            hash,
            path: abs_path,
            git_remote,
        })
    }

    /// Get the hash as a string.
    ///
    /// This is a 16-character hex string (8 bytes of SHA256).
    pub fn hash(&self) -> &str {
        &self.hash
    }

    /// Get the project path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Get the git remote URL if available.
    pub fn git_remote(&self) -> Option<&str> {
        self.git_remote.as_deref()
    }

    /// Get the project name (directory name).
    pub fn name(&self) -> &str {
        self.path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
    }

    fn detect_git_remote(path: &Path) -> Option<String> {
        let output = std::process::Command::new("git")
            .args(["remote", "get-url", "origin"])
            .current_dir(path)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let url = String::from_utf8(output.stdout).ok()?;
        let url = url.trim();
        if url.is_empty() {
            None
        } else {
            Some(url.to_string())
        }
    }

    fn compute_hash(path: &Path, git_remote: Option<&str>) -> String {
        let mut hasher = Sha256::new();

        hasher.update(path.to_string_lossy().as_bytes());

        if let Some(remote) = git_remote {
            hasher.update(remote.as_bytes());
        }

        let result = hasher.finalize();
        hex::encode(&result[..8]) // Use first 8 bytes (16 hex chars)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn project_id_from_path() {
        let temp = TempDir::new().unwrap();
        let project = ProjectId::from_path(temp.path()).unwrap();

        assert!(!project.hash().is_empty());
        assert_eq!(project.hash().len(), 16); // 8 bytes = 16 hex chars
    }

    #[test]
    fn project_id_name_is_directory() {
        let temp = TempDir::new().unwrap();
        let project = ProjectId::from_path(temp.path()).unwrap();

        assert!(!project.name().is_empty());
    }

    #[test]
    fn project_id_same_path_same_hash() {
        let temp = TempDir::new().unwrap();
        let project1 = ProjectId::from_path(temp.path()).unwrap();
        let project2 = ProjectId::from_path(temp.path()).unwrap();

        assert_eq!(project1.hash(), project2.hash());
    }

    #[test]
    fn project_id_different_paths_different_hash() {
        let temp1 = TempDir::new().unwrap();
        let temp2 = TempDir::new().unwrap();

        let project1 = ProjectId::from_path(temp1.path()).unwrap();
        let project2 = ProjectId::from_path(temp2.path()).unwrap();

        assert_ne!(project1.hash(), project2.hash());
    }

    #[test]
    fn project_id_nonexistent_path_fails() {
        let result = ProjectId::from_path(Path::new("/nonexistent/path"));
        assert!(result.is_err());
    }

    #[test]
    fn project_id_path_accessor() {
        let temp = TempDir::new().unwrap();
        let project = ProjectId::from_path(temp.path()).unwrap();

        // Path should be canonicalized (absolute)
        assert!(project.path().is_absolute());
    }

    #[test]
    fn project_id_git_remote_is_none_for_non_git() {
        let temp = TempDir::new().unwrap();
        let project = ProjectId::from_path(temp.path()).unwrap();

        // Non-git directory should have no remote
        assert!(project.git_remote().is_none());
    }
}
