//! Git template fetching.
//!
//! Provides functionality for cloning and updating git repositories
//! to fetch templates, with support for specific refs and update detection.

use anyhow::{bail, Result};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Fetches templates from git repositories.
pub struct GitFetcher {
    /// Directory for cloned repositories.
    clone_dir: PathBuf,
}

/// Information about a git fetch.
#[derive(Debug)]
pub struct GitFetchResult {
    /// Path to the fetched content.
    pub local_path: PathBuf,
    /// Current commit SHA.
    pub commit_sha: String,
}

impl GitFetcher {
    /// Create a new git fetcher.
    pub fn new(clone_dir: impl Into<PathBuf>) -> Self {
        Self {
            clone_dir: clone_dir.into(),
        }
    }

    /// Get the clone directory.
    pub fn clone_dir(&self) -> &PathBuf {
        &self.clone_dir
    }

    /// Clone or update a repository.
    pub fn fetch(&self, url: &str, git_ref: Option<&str>) -> Result<GitFetchResult> {
        let repo_path = self.repo_path(url);

        if repo_path.exists() {
            self.update_repo(&repo_path, git_ref)?;
        } else {
            self.clone_repo(url, &repo_path, git_ref)?;
        }

        let commit_sha = self.get_head_sha(&repo_path)?;

        Ok(GitFetchResult {
            local_path: repo_path,
            commit_sha,
        })
    }

    /// Check if a repository has new commits.
    pub fn has_updates(&self, url: &str, git_ref: Option<&str>, current_sha: &str) -> Result<bool> {
        let remote_sha = self.resolve_ref(url, git_ref)?;
        Ok(remote_sha != current_sha)
    }

    /// Get the current commit SHA for a ref using ls-remote.
    pub fn resolve_ref(&self, url: &str, git_ref: Option<&str>) -> Result<String> {
        let refspec = git_ref.unwrap_or("HEAD");

        let output = std::process::Command::new("git")
            .args(["ls-remote", url, refspec])
            .output()?;

        if !output.status.success() {
            bail!(
                "Failed to query remote: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let sha = stdout
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().next())
            .ok_or_else(|| anyhow::anyhow!("Could not parse ls-remote output"))?;

        Ok(sha.to_string())
    }

    fn clone_repo(&self, url: &str, path: &Path, git_ref: Option<&str>) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut cmd = std::process::Command::new("git");
        cmd.args(["clone", "--depth", "1"]);

        if let Some(r) = git_ref {
            cmd.args(["--branch", r]);
        }

        cmd.args([url, &path.to_string_lossy()]);

        let output = cmd.output()?;
        if !output.status.success() {
            bail!(
                "Git clone failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        Ok(())
    }

    fn update_repo(&self, path: &PathBuf, git_ref: Option<&str>) -> Result<()> {
        let output = std::process::Command::new("git")
            .args(["fetch", "origin"])
            .current_dir(path)
            .output()?;

        if !output.status.success() {
            bail!(
                "Git fetch failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let refspec = if let Some(r) = git_ref {
            format!("origin/{}", r)
        } else {
            "origin/HEAD".to_string()
        };

        let output = std::process::Command::new("git")
            .args(["reset", "--hard", &refspec])
            .current_dir(path)
            .output()?;

        if !output.status.success() {
            bail!(
                "Git reset failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        Ok(())
    }

    fn get_head_sha(&self, path: &PathBuf) -> Result<String> {
        let output = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(path)
            .output()?;

        if !output.status.success() {
            bail!("Git rev-parse failed");
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Get the local path for a repository.
    ///
    /// Uses a hash of the URL to create a deterministic, unique path.
    pub fn repo_path(&self, url: &str) -> PathBuf {
        let hash = Sha256::digest(url.as_bytes());
        let hash_str = hex::encode(&hash[..8]);
        self.clone_dir.join(hash_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn creates_fetcher_with_clone_dir() {
        let temp = TempDir::new().unwrap();
        let fetcher = GitFetcher::new(temp.path());

        assert_eq!(fetcher.clone_dir(), temp.path());
    }

    #[test]
    fn git_fetch_result_fields() {
        let result = GitFetchResult {
            local_path: PathBuf::from("/tmp/repo"),
            commit_sha: "abc123def456".to_string(),
        };

        assert_eq!(result.local_path, PathBuf::from("/tmp/repo"));
        assert_eq!(result.commit_sha, "abc123def456");
    }

    #[test]
    fn repo_path_is_deterministic() {
        let temp = TempDir::new().unwrap();
        let fetcher = GitFetcher::new(temp.path());

        let path1 = fetcher.repo_path("https://github.com/org/repo.git");
        let path2 = fetcher.repo_path("https://github.com/org/repo.git");

        assert_eq!(path1, path2);
    }

    #[test]
    fn different_repos_have_different_paths() {
        let temp = TempDir::new().unwrap();
        let fetcher = GitFetcher::new(temp.path());

        let path1 = fetcher.repo_path("https://github.com/org/repo1.git");
        let path2 = fetcher.repo_path("https://github.com/org/repo2.git");

        assert_ne!(path1, path2);
    }

    #[test]
    fn repo_path_is_within_clone_dir() {
        let temp = TempDir::new().unwrap();
        let fetcher = GitFetcher::new(temp.path());

        let path = fetcher.repo_path("https://github.com/org/repo.git");

        assert!(path.starts_with(temp.path()));
    }

    #[test]
    fn repo_path_uses_hex_hash() {
        let temp = TempDir::new().unwrap();
        let fetcher = GitFetcher::new(temp.path());

        let path = fetcher.repo_path("https://github.com/org/repo.git");
        let filename = path.file_name().unwrap().to_string_lossy();

        // Should be 16 hex characters (8 bytes encoded)
        assert_eq!(filename.len(), 16);
        assert!(filename.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
