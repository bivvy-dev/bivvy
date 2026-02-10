//! Template caching system.
//!
//! This module provides disk-based caching for remote templates with
//! TTL-based and content-based (ETag/git) invalidation strategies.

pub mod entry;
pub mod revalidation;
pub mod store;
pub mod validation;

pub use entry::{CacheEntry, CacheMetadata};
pub use revalidation::{needs_revalidation, CacheRevalidator, RevalidationResult};
pub use store::CacheStore;
pub use validation::{format_duration, parse_ttl, CacheValidator, ValidationResult};

/// Get the default cache directory.
pub fn default_cache_dir() -> std::path::PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("bivvy")
        .join("templates")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_cache_dir_valid() {
        let path = default_cache_dir();
        assert!(path.ends_with("templates"));
    }
}
