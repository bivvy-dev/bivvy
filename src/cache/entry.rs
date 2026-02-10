//! Cache entry and metadata types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A cached template entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    /// Source ID (e.g., "http:https://example.com").
    pub source_id: String,
    /// Template name within the source.
    pub template_name: String,
    /// Path to the cached content.
    pub content_path: PathBuf,
    /// Cache metadata for validation.
    pub metadata: CacheMetadata,
}

/// Metadata for cache validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMetadata {
    /// When this entry was cached.
    pub cached_at: DateTime<Utc>,
    /// When the cached entry expires.
    pub expires_at: DateTime<Utc>,
    /// ETag from HTTP response, if available.
    pub etag: Option<String>,
    /// Git commit SHA, if from a git source.
    pub commit_sha: Option<String>,
    /// Size in bytes.
    pub size_bytes: u64,
}

impl CacheEntry {
    /// Create a new cache entry.
    pub fn new(
        source_id: impl Into<String>,
        template_name: impl Into<String>,
        content_path: impl Into<PathBuf>,
        ttl_seconds: u64,
    ) -> Self {
        let now = Utc::now();
        let expires_at = now + chrono::Duration::seconds(ttl_seconds as i64);

        Self {
            source_id: source_id.into(),
            template_name: template_name.into(),
            content_path: content_path.into(),
            metadata: CacheMetadata {
                cached_at: now,
                expires_at,
                etag: None,
                commit_sha: None,
                size_bytes: 0,
            },
        }
    }

    /// Check if the entry has expired based on TTL.
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.metadata.expires_at
    }

    /// Get the age of this entry.
    pub fn age(&self) -> chrono::Duration {
        Utc::now().signed_duration_since(self.metadata.cached_at)
    }

    /// Set ETag for HTTP validation.
    pub fn with_etag(mut self, etag: impl Into<String>) -> Self {
        self.metadata.etag = Some(etag.into());
        self
    }

    /// Set commit SHA for git validation.
    pub fn with_commit_sha(mut self, sha: impl Into<String>) -> Self {
        self.metadata.commit_sha = Some(sha.into());
        self
    }

    /// Set size in bytes.
    pub fn with_size(mut self, size: u64) -> Self {
        self.metadata.size_bytes = size;
        self
    }

    /// Create a unique cache key.
    pub fn cache_key(&self) -> String {
        format!("{}:{}", self.source_id, self.template_name)
    }
}

impl CacheMetadata {
    /// Calculate remaining TTL in seconds.
    pub fn remaining_ttl(&self) -> i64 {
        self.expires_at
            .signed_duration_since(Utc::now())
            .num_seconds()
            .max(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_entry_creation() {
        let entry = CacheEntry::new(
            "http:https://example.com",
            "test-template",
            "/tmp/cache/test",
            3600,
        );

        assert_eq!(entry.source_id, "http:https://example.com");
        assert_eq!(entry.template_name, "test-template");
        assert!(!entry.is_expired());
    }

    #[test]
    fn cache_entry_expiration() {
        let entry = CacheEntry::new("test", "test", "/tmp", 0);

        // With 0 TTL, should be expired immediately
        assert!(entry.is_expired());
    }

    #[test]
    fn cache_entry_with_etag() {
        let entry = CacheEntry::new("test", "test", "/tmp", 3600).with_etag("\"abc123\"");

        assert_eq!(entry.metadata.etag, Some("\"abc123\"".to_string()));
    }

    #[test]
    fn cache_entry_with_commit_sha() {
        let entry = CacheEntry::new("test", "test", "/tmp", 3600).with_commit_sha("abc123def");

        assert_eq!(entry.metadata.commit_sha, Some("abc123def".to_string()));
    }

    #[test]
    fn cache_entry_with_size() {
        let entry = CacheEntry::new("test", "test", "/tmp", 3600).with_size(1024);

        assert_eq!(entry.metadata.size_bytes, 1024);
    }

    #[test]
    fn cache_key_format() {
        let entry = CacheEntry::new("http:https://example.com", "my-template", "/tmp", 3600);

        assert_eq!(entry.cache_key(), "http:https://example.com:my-template");
    }

    #[test]
    fn remaining_ttl_calculation() {
        let entry = CacheEntry::new("test", "test", "/tmp", 3600);

        let remaining = entry.metadata.remaining_ttl();
        assert!(remaining > 3590);
        assert!(remaining <= 3600);
    }

    #[test]
    fn expired_entry_has_zero_remaining_ttl() {
        let entry = CacheEntry::new("test", "test", "/tmp", 0);

        assert_eq!(entry.metadata.remaining_ttl(), 0);
    }

    #[test]
    fn cache_entry_age() {
        let entry = CacheEntry::new("test", "test", "/tmp", 3600);

        // Age should be very small (< 1 second)
        assert!(entry.age().num_seconds() < 1);
    }
}
