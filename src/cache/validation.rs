//! Cache validation and invalidation logic.

use anyhow::Result;
use chrono::{Duration, Utc};

use super::{CacheEntry, CacheStore};
use crate::registry::source::RemoteCacheStrategy;

/// Cache validator for checking entry freshness.
pub struct CacheValidator<'a> {
    store: &'a CacheStore,
}

/// Result of cache validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationResult {
    /// Entry is fresh and valid.
    Fresh,
    /// Entry expired (TTL).
    Expired,
    /// Entry needs revalidation (ETag/git check).
    NeedsRevalidation,
    /// Entry not found.
    NotFound,
}

impl<'a> CacheValidator<'a> {
    /// Create a new cache validator.
    pub fn new(store: &'a CacheStore) -> Self {
        Self { store }
    }

    /// Validate a cache entry based on strategy.
    pub fn validate(
        &self,
        source_id: &str,
        template_name: &str,
        strategy: RemoteCacheStrategy,
    ) -> Result<ValidationResult> {
        let entry = match self.store.load(source_id, template_name)? {
            Some(e) => e,
            None => return Ok(ValidationResult::NotFound),
        };

        match strategy {
            RemoteCacheStrategy::Ttl => self.validate_ttl(&entry),
            RemoteCacheStrategy::Etag => self.validate_etag(&entry),
            RemoteCacheStrategy::Git => self.validate_git(&entry),
        }
    }

    /// Validate using TTL only.
    fn validate_ttl(&self, entry: &CacheEntry) -> Result<ValidationResult> {
        if entry.is_expired() {
            Ok(ValidationResult::Expired)
        } else {
            Ok(ValidationResult::Fresh)
        }
    }

    /// Validate with ETag strategy.
    ///
    /// If TTL expired, needs revalidation via HTTP.
    fn validate_etag(&self, entry: &CacheEntry) -> Result<ValidationResult> {
        if entry.is_expired() {
            if entry.metadata.etag.is_some() {
                Ok(ValidationResult::NeedsRevalidation)
            } else {
                Ok(ValidationResult::Expired)
            }
        } else {
            Ok(ValidationResult::Fresh)
        }
    }

    /// Validate with git strategy.
    ///
    /// If TTL expired, needs revalidation via git.
    fn validate_git(&self, entry: &CacheEntry) -> Result<ValidationResult> {
        if entry.is_expired() {
            if entry.metadata.commit_sha.is_some() {
                Ok(ValidationResult::NeedsRevalidation)
            } else {
                Ok(ValidationResult::Expired)
            }
        } else {
            Ok(ValidationResult::Fresh)
        }
    }

    /// Clean up expired entries (TTL-only cleanup).
    pub fn cleanup_expired(&self) -> Result<usize> {
        let entries = self.store.list()?;
        let mut removed = 0;

        for entry in entries {
            if entry.is_expired() && self.store.remove(&entry.source_id, &entry.template_name)? {
                removed += 1;
            }
        }

        Ok(removed)
    }

    /// Get entries older than a duration.
    pub fn entries_older_than(&self, age: Duration) -> Result<Vec<CacheEntry>> {
        let entries = self.store.list()?;
        let cutoff = Utc::now() - age;

        Ok(entries
            .into_iter()
            .filter(|e| e.metadata.cached_at < cutoff)
            .collect())
    }
}

/// Parse a TTL string like "7d", "24h", "30m".
pub fn parse_ttl(ttl: &str) -> Result<Duration> {
    let ttl = ttl.trim().to_lowercase();

    if let Some(days) = ttl.strip_suffix('d') {
        let n: i64 = days.parse()?;
        Ok(Duration::days(n))
    } else if let Some(hours) = ttl.strip_suffix('h') {
        let n: i64 = hours.parse()?;
        Ok(Duration::hours(n))
    } else if let Some(mins) = ttl.strip_suffix('m') {
        let n: i64 = mins.parse()?;
        Ok(Duration::minutes(n))
    } else if let Some(secs) = ttl.strip_suffix('s') {
        let n: i64 = secs.parse()?;
        Ok(Duration::seconds(n))
    } else {
        // Assume seconds if no suffix
        let n: i64 = ttl.parse()?;
        Ok(Duration::seconds(n))
    }
}

/// Format a duration for display.
pub fn format_duration(duration: Duration) -> String {
    let secs = duration.num_seconds();

    if secs >= 86400 {
        let days = secs / 86400;
        format!("{}d", days)
    } else if secs >= 3600 {
        let hours = secs / 3600;
        format!("{}h", hours)
    } else if secs >= 60 {
        let mins = secs / 60;
        format!("{}m", mins)
    } else {
        format!("{}s", secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn validate_fresh_entry_ttl() {
        let temp = TempDir::new().unwrap();
        let store = CacheStore::new(temp.path());

        store
            .store("http:test", "template", "content", 3600)
            .unwrap();

        let validator = CacheValidator::new(&store);
        let result = validator
            .validate("http:test", "template", RemoteCacheStrategy::Ttl)
            .unwrap();

        assert_eq!(result, ValidationResult::Fresh);
    }

    #[test]
    fn validate_expired_entry_ttl() {
        let temp = TempDir::new().unwrap();
        let store = CacheStore::new(temp.path());

        store.store("http:test", "template", "content", 0).unwrap();

        let validator = CacheValidator::new(&store);
        let result = validator
            .validate("http:test", "template", RemoteCacheStrategy::Ttl)
            .unwrap();

        assert_eq!(result, ValidationResult::Expired);
    }

    #[test]
    fn validate_not_found() {
        let temp = TempDir::new().unwrap();
        let store = CacheStore::new(temp.path());

        let validator = CacheValidator::new(&store);
        let result = validator
            .validate("http:test", "nonexistent", RemoteCacheStrategy::Ttl)
            .unwrap();

        assert_eq!(result, ValidationResult::NotFound);
    }

    #[test]
    fn validate_etag_needs_revalidation() {
        let temp = TempDir::new().unwrap();
        let store = CacheStore::new(temp.path());

        let mut entry = store.store("http:test", "template", "content", 0).unwrap();
        entry.metadata.etag = Some("\"abc123\"".to_string());
        store.update(&entry).unwrap();

        let validator = CacheValidator::new(&store);
        let result = validator
            .validate("http:test", "template", RemoteCacheStrategy::Etag)
            .unwrap();

        assert_eq!(result, ValidationResult::NeedsRevalidation);
    }

    #[test]
    fn validate_git_needs_revalidation() {
        let temp = TempDir::new().unwrap();
        let store = CacheStore::new(temp.path());

        let mut entry = store.store("http:test", "template", "content", 0).unwrap();
        entry.metadata.commit_sha = Some("abc123".to_string());
        store.update(&entry).unwrap();

        let validator = CacheValidator::new(&store);
        let result = validator
            .validate("http:test", "template", RemoteCacheStrategy::Git)
            .unwrap();

        assert_eq!(result, ValidationResult::NeedsRevalidation);
    }

    #[test]
    fn parse_ttl_days() {
        let duration = parse_ttl("7d").unwrap();
        assert_eq!(duration.num_days(), 7);
    }

    #[test]
    fn parse_ttl_hours() {
        let duration = parse_ttl("24h").unwrap();
        assert_eq!(duration.num_hours(), 24);
    }

    #[test]
    fn parse_ttl_minutes() {
        let duration = parse_ttl("30m").unwrap();
        assert_eq!(duration.num_minutes(), 30);
    }

    #[test]
    fn parse_ttl_seconds() {
        let duration = parse_ttl("3600s").unwrap();
        assert_eq!(duration.num_seconds(), 3600);
    }

    #[test]
    fn parse_ttl_no_suffix() {
        let duration = parse_ttl("3600").unwrap();
        assert_eq!(duration.num_seconds(), 3600);
    }

    #[test]
    fn format_duration_days() {
        assert_eq!(format_duration(Duration::days(7)), "7d");
    }

    #[test]
    fn format_duration_hours() {
        // 24 hours is 1 day, so test with a non-day-divisible value
        assert_eq!(format_duration(Duration::hours(12)), "12h");
    }

    #[test]
    fn format_duration_minutes() {
        assert_eq!(format_duration(Duration::minutes(30)), "30m");
    }

    #[test]
    fn format_duration_seconds() {
        assert_eq!(format_duration(Duration::seconds(45)), "45s");
    }

    #[test]
    fn cleanup_removes_expired() {
        let temp = TempDir::new().unwrap();
        let store = CacheStore::new(temp.path());

        store.store("http:test", "fresh", "content", 3600).unwrap();
        store.store("http:test", "stale", "content", 0).unwrap();

        let validator = CacheValidator::new(&store);
        let removed = validator.cleanup_expired().unwrap();

        assert_eq!(removed, 1);

        let entries = store.list().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].template_name, "fresh");
    }
}
