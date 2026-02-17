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

        // Try the refspec directly, then with refs/heads/ and refs/tags/ prefixes.
        // Short refspecs like "v1.0" may not match "refs/tags/v1.0" in all git versions.
        let candidates = if refspec == "HEAD" {
            vec![refspec.to_string()]
        } else {
            vec![
                refspec.to_string(),
                format!("refs/heads/{refspec}"),
                format!("refs/tags/{refspec}"),
            ]
        };

        for candidate in &candidates {
            let output = std::process::Command::new("git")
                .args(["ls-remote", url, candidate])
                .output()?;

            if !output.status.success() {
                bail!(
                    "Failed to query remote: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some(sha) = stdout
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().next())
            {
                return Ok(sha.to_string());
            }
        }

        bail!("Could not resolve ref '{refspec}' from {url}")
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
    use std::sync::Mutex;
    use tempfile::TempDir;

    // Serialize git-process tests to avoid flaky failures under parallel execution
    static GIT_LOCK: Mutex<()> = Mutex::new(());

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

    // --- Local bare repo git tests ---

    /// Create a bare git repo with an initial commit containing a template file.
    /// Returns the path to the bare repo.
    fn create_bare_repo(parent: &Path) -> PathBuf {
        let bare_path = parent.join("test-repo.git");

        // Create a temporary working directory for the initial commit
        let work_dir = parent.join("work");
        std::fs::create_dir_all(&work_dir).unwrap();

        // Initialize bare repo with explicit default branch
        let output = std::process::Command::new("git")
            .args([
                "init",
                "--bare",
                "--initial-branch=main",
                bare_path.to_string_lossy().as_ref(),
            ])
            .output()
            .unwrap();
        assert!(output.status.success(), "bare init failed");

        // Clone bare to working dir
        let output = std::process::Command::new("git")
            .args([
                "clone",
                bare_path.to_string_lossy().as_ref(),
                work_dir.to_string_lossy().as_ref(),
            ])
            .output()
            .unwrap();
        assert!(output.status.success(), "clone failed");

        // Configure git user for commits
        for (key, val) in [("user.name", "Test"), ("user.email", "test@test.com")] {
            let output = std::process::Command::new("git")
                .args(["config", key, val])
                .current_dir(&work_dir)
                .output()
                .unwrap();
            assert!(output.status.success(), "git config {key} failed");
        }

        // Create a template file and commit
        let templates_dir = work_dir.join("templates");
        std::fs::create_dir_all(&templates_dir).unwrap();
        std::fs::write(
            templates_dir.join("node.yml"),
            "name: node\ncommand: npm install\n",
        )
        .unwrap();

        let output = std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&work_dir)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git add failed in create_bare_repo"
        );

        let output = std::process::Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(&work_dir)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git commit failed in create_bare_repo: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let output = std::process::Command::new("git")
            .args(["push", "origin", "HEAD:main"])
            .current_dir(&work_dir)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git push failed in create_bare_repo: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        bare_path
    }

    /// Push a new commit to the bare repo.
    fn push_commit_to_bare(parent: &Path, bare_path: &Path) {
        let work_dir = parent.join("work2");

        let output = std::process::Command::new("git")
            .args([
                "clone",
                &bare_path.to_string_lossy(),
                &work_dir.to_string_lossy(),
            ])
            .output()
            .unwrap();
        assert!(output.status.success(), "clone for push failed");

        for (key, val) in [("user.name", "Test"), ("user.email", "test@test.com")] {
            std::process::Command::new("git")
                .args(["config", key, val])
                .current_dir(&work_dir)
                .output()
                .unwrap();
        }

        std::fs::write(work_dir.join("new-file.txt"), "new content").unwrap();

        let output = std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&work_dir)
            .output()
            .unwrap();
        assert!(output.status.success(), "git add failed");

        let output = std::process::Command::new("git")
            .args(["commit", "-m", "Second commit"])
            .current_dir(&work_dir)
            .output()
            .unwrap();
        assert!(output.status.success(), "git commit failed");

        let output = std::process::Command::new("git")
            .args(["push", "origin", "HEAD:main"])
            .current_dir(&work_dir)
            .output()
            .unwrap();
        assert!(output.status.success(), "git push failed");
    }

    #[test]
    fn clone_from_local_bare_repo() {
        let _lock = GIT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = TempDir::new().unwrap();
        let bare_path = create_bare_repo(temp.path());

        let clone_dir = temp.path().join("clones");
        let fetcher = GitFetcher::new(&clone_dir);

        let result = fetcher
            .fetch(&bare_path.to_string_lossy(), Some("main"))
            .unwrap();

        assert!(!result.commit_sha.is_empty());
        assert!(result.local_path.exists());
        // Template file should exist in clone
        assert!(result.local_path.join("templates/node.yml").exists());
    }

    #[test]
    fn has_updates_false_when_no_new_commits() {
        let _lock = GIT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = TempDir::new().unwrap();
        let bare_path = create_bare_repo(temp.path());

        let clone_dir = temp.path().join("clones");
        let fetcher = GitFetcher::new(&clone_dir);

        // Clone first
        let result = fetcher
            .fetch(&bare_path.to_string_lossy(), Some("main"))
            .unwrap();

        // No changes have been made
        let has_updates = fetcher
            .has_updates(
                &bare_path.to_string_lossy(),
                Some("main"),
                &result.commit_sha,
            )
            .unwrap();

        assert!(!has_updates);
    }

    #[test]
    fn has_updates_true_after_push() {
        let _lock = GIT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = TempDir::new().unwrap();
        let bare_path = create_bare_repo(temp.path());

        let clone_dir = temp.path().join("clones");
        let fetcher = GitFetcher::new(&clone_dir);

        // Clone and get initial SHA
        let result = fetcher
            .fetch(&bare_path.to_string_lossy(), Some("main"))
            .unwrap();
        let initial_sha = result.commit_sha;

        // Push a new commit
        push_commit_to_bare(temp.path(), &bare_path);

        // Now should detect updates
        let has_updates = fetcher
            .has_updates(&bare_path.to_string_lossy(), Some("main"), &initial_sha)
            .unwrap();

        assert!(has_updates);
    }

    #[test]
    fn ref_resolution_branch() {
        let _lock = GIT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = TempDir::new().unwrap();
        let bare_path = create_bare_repo(temp.path());

        let fetcher = GitFetcher::new(temp.path().join("clones"));

        let sha = fetcher
            .resolve_ref(&bare_path.to_string_lossy(), Some("main"))
            .unwrap();

        assert!(!sha.is_empty());
        // SHA should be 40 hex characters
        assert_eq!(sha.len(), 40);
        assert!(sha.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn ref_resolution_tag() {
        let _lock = GIT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = TempDir::new().unwrap();
        let bare_path = create_bare_repo(temp.path());

        // Create a tag
        let work_dir = temp.path().join("tag-work");
        std::process::Command::new("git")
            .args([
                "clone",
                &bare_path.to_string_lossy(),
                &work_dir.to_string_lossy(),
            ])
            .output()
            .unwrap();

        for (key, val) in [("user.name", "Test"), ("user.email", "test@test.com")] {
            std::process::Command::new("git")
                .args(["config", key, val])
                .current_dir(&work_dir)
                .output()
                .unwrap();
        }

        let output = std::process::Command::new("git")
            .args(["tag", "v1.0"])
            .current_dir(&work_dir)
            .output()
            .unwrap();
        assert!(output.status.success(), "tag creation failed");

        let output = std::process::Command::new("git")
            .args(["push", "origin", "v1.0"])
            .current_dir(&work_dir)
            .output()
            .unwrap();
        assert!(output.status.success(), "tag push failed");

        let fetcher = GitFetcher::new(temp.path().join("clones"));

        let sha = fetcher
            .resolve_ref(&bare_path.to_string_lossy(), Some("v1.0"))
            .unwrap();

        assert!(!sha.is_empty());
        assert_eq!(sha.len(), 40);
    }

    #[test]
    fn invalid_repo_url_returns_error() {
        let _lock = GIT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = TempDir::new().unwrap();
        let fetcher = GitFetcher::new(temp.path().join("clones"));

        let result = fetcher.fetch("/nonexistent/path/repo.git", Some("main"));

        assert!(result.is_err());
    }

    #[test]
    fn fetch_specific_subdirectory() {
        let _lock = GIT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = TempDir::new().unwrap();
        let bare_path = create_bare_repo(temp.path());

        let clone_dir = temp.path().join("clones");
        let fetcher = GitFetcher::new(&clone_dir);

        let result = fetcher
            .fetch(&bare_path.to_string_lossy(), Some("main"))
            .unwrap();

        // The "templates" subdirectory should contain our template
        let templates_path = result.local_path.join("templates");
        assert!(templates_path.is_dir());

        let content = std::fs::read_to_string(templates_path.join("node.yml")).unwrap();
        assert!(content.contains("npm install"));
    }
}
