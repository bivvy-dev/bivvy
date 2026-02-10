//! Cache revalidation for ETag and git strategies.

use anyhow::Result;

use super::{CacheEntry, CacheStore, ValidationResult};
use crate::registry::fetch::{GitFetcher, HttpFetcher};

/// Performs cache revalidation using ETag or git checking.
pub struct CacheRevalidator<'a> {
    store: &'a CacheStore,
    http: HttpFetcher,
    git: GitFetcher,
}

/// Result of revalidation.
#[derive(Debug)]
pub enum RevalidationResult {
    /// Content unchanged, cache extended.
    Unchanged,
    /// Content changed, new content available.
    Updated(String),
    /// Revalidation failed (fallback to cache or error).
    Failed(String),
}

impl<'a> CacheRevalidator<'a> {
    /// Create a new revalidator.
    pub fn new(store: &'a CacheStore, git_clone_dir: impl Into<std::path::PathBuf>) -> Self {
        Self {
            store,
            http: HttpFetcher::new(),
            git: GitFetcher::new(git_clone_dir),
        }
    }

    /// Revalidate an entry using its ETag.
    pub fn revalidate_etag(
        &self,
        url: &str,
        entry: &mut CacheEntry,
        new_ttl_seconds: u64,
    ) -> Result<RevalidationResult> {
        let etag = entry.metadata.etag.as_deref();

        match self.http.fetch_if_changed(url, etag)? {
            None => {
                // 304 Not Modified - extend TTL
                entry.metadata.expires_at =
                    chrono::Utc::now() + chrono::Duration::seconds(new_ttl_seconds as i64);
                self.store.update(entry)?;
                Ok(RevalidationResult::Unchanged)
            }
            Some(response) => {
                // Content changed - update cache
                let content_path = self
                    .store
                    .content_path(&entry.source_id, &entry.template_name);
                std::fs::write(&content_path, &response.content)?;

                entry.metadata.etag = response.etag;
                entry.metadata.size_bytes = response.content.len() as u64;
                entry.metadata.cached_at = chrono::Utc::now();
                entry.metadata.expires_at =
                    chrono::Utc::now() + chrono::Duration::seconds(new_ttl_seconds as i64);
                self.store.update(entry)?;

                Ok(RevalidationResult::Updated(response.content))
            }
        }
    }

    /// Revalidate an entry using git commit comparison.
    pub fn revalidate_git(
        &self,
        url: &str,
        git_ref: Option<&str>,
        entry: &mut CacheEntry,
        new_ttl_seconds: u64,
    ) -> Result<RevalidationResult> {
        let current_sha = match &entry.metadata.commit_sha {
            Some(sha) => sha.clone(),
            None => {
                // No SHA to compare, must re-fetch
                return self.fetch_git(url, git_ref, entry, new_ttl_seconds);
            }
        };

        let has_updates = self.git.has_updates(url, git_ref, &current_sha)?;

        if has_updates {
            self.fetch_git(url, git_ref, entry, new_ttl_seconds)
        } else {
            // No updates - extend TTL
            entry.metadata.expires_at =
                chrono::Utc::now() + chrono::Duration::seconds(new_ttl_seconds as i64);
            self.store.update(entry)?;
            Ok(RevalidationResult::Unchanged)
        }
    }

    /// Fetch from git and update the cache entry.
    fn fetch_git(
        &self,
        url: &str,
        git_ref: Option<&str>,
        entry: &mut CacheEntry,
        new_ttl_seconds: u64,
    ) -> Result<RevalidationResult> {
        let result = self.git.fetch(url, git_ref)?;

        // Copy content from cloned repo to cache
        let content_path = self
            .store
            .content_path(&entry.source_id, &entry.template_name);

        // For git sources, we store a marker that the content is at the git path
        let content = format!("git-path:{}", result.local_path.display());
        std::fs::write(&content_path, &content)?;

        entry.metadata.commit_sha = Some(result.commit_sha);
        entry.metadata.cached_at = chrono::Utc::now();
        entry.metadata.expires_at =
            chrono::Utc::now() + chrono::Duration::seconds(new_ttl_seconds as i64);
        self.store.update(entry)?;

        Ok(RevalidationResult::Updated(content))
    }
}

/// Check if a validation result needs revalidation.
pub fn needs_revalidation(result: &ValidationResult) -> bool {
    matches!(result, ValidationResult::NeedsRevalidation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn needs_revalidation_check() {
        assert!(!needs_revalidation(&ValidationResult::Fresh));
        assert!(!needs_revalidation(&ValidationResult::Expired));
        assert!(needs_revalidation(&ValidationResult::NeedsRevalidation));
        assert!(!needs_revalidation(&ValidationResult::NotFound));
    }

    #[test]
    fn revalidator_creation() {
        let temp = TempDir::new().unwrap();
        let store = CacheStore::new(temp.path());

        let git_dir = temp.path().join("git");
        let _revalidator = CacheRevalidator::new(&store, git_dir);
    }

    #[test]
    fn revalidation_result_variants() {
        let unchanged = RevalidationResult::Unchanged;
        let updated = RevalidationResult::Updated("new content".to_string());
        let failed = RevalidationResult::Failed("network error".to_string());

        // Just check we can pattern match
        match unchanged {
            RevalidationResult::Unchanged => {}
            _ => panic!("expected Unchanged"),
        }

        match updated {
            RevalidationResult::Updated(content) => assert_eq!(content, "new content"),
            _ => panic!("expected Updated"),
        }

        match failed {
            RevalidationResult::Failed(msg) => assert_eq!(msg, "network error"),
            _ => panic!("expected Failed"),
        }
    }
}
