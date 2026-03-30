//! Trust verification for remote extends URLs.
//!
//! This module provides a trust store that tracks which remote URLs
//! have been explicitly approved by the user. When a config uses
//! `extends:` with a remote URL, the trust store is checked before
//! fetching. Untrusted URLs require user confirmation (or the
//! `--trust` CLI flag).
//!
//! Trusted URLs are persisted in `~/.bivvy/trusted_urls.yml`.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

/// Persistent store of trusted remote extends URLs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrustStore {
    /// Set of URLs the user has explicitly trusted.
    #[serde(default)]
    pub urls: HashSet<String>,
}

impl TrustStore {
    /// Get the default path for the trust store file.
    pub fn default_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("~"))
            .join(".bivvy")
            .join("trusted_urls.yml")
    }

    /// Load trusted URLs from the given path.
    pub fn load(path: &std::path::Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(path)?;
        let store: Self = serde_yaml::from_str(&content)?;
        Ok(store)
    }

    /// Load from the default path.
    pub fn load_default() -> Result<Self> {
        Self::load(&Self::default_path())
    }

    /// Save trusted URLs to the given path.
    pub fn save(&self, path: &std::path::Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = serde_yaml::to_string(self)?;

        // Atomic write
        let temp_path = path.with_extension("yml.tmp");
        fs::write(&temp_path, &content)?;
        fs::rename(&temp_path, path)?;

        Ok(())
    }

    /// Save to the default path.
    pub fn save_default(&self) -> Result<()> {
        self.save(&Self::default_path())
    }

    /// Check if a URL is trusted.
    pub fn is_trusted(&self, url: &str) -> bool {
        self.urls.contains(url)
    }

    /// Add a URL to the trusted set.
    pub fn trust(&mut self, url: &str) {
        self.urls.insert(url.to_string());
    }

    /// Remove a URL from the trusted set.
    pub fn revoke(&mut self, url: &str) {
        self.urls.remove(url);
    }
}

/// Policy for how to handle untrusted URLs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustPolicy {
    /// Prompt the user for each untrusted URL (interactive mode).
    Prompt,
    /// Automatically trust all URLs (--trust flag).
    TrustAll,
    /// Reject untrusted URLs with an error (non-interactive mode).
    Reject,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn trust_store_default_is_empty() {
        let store = TrustStore::default();
        assert!(store.urls.is_empty());
    }

    #[test]
    fn trust_and_check() {
        let mut store = TrustStore::default();
        let url = "https://example.com/config.yml";

        assert!(!store.is_trusted(url));
        store.trust(url);
        assert!(store.is_trusted(url));
    }

    #[test]
    fn revoke_removes_trust() {
        let mut store = TrustStore::default();
        let url = "https://example.com/config.yml";

        store.trust(url);
        assert!(store.is_trusted(url));

        store.revoke(url);
        assert!(!store.is_trusted(url));
    }

    #[test]
    fn save_and_load() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("trusted_urls.yml");

        let mut store = TrustStore::default();
        store.trust("https://example.com/base.yml");
        store.trust("https://corp.example.com/shared.yml");
        store.save(&path).unwrap();

        let loaded = TrustStore::load(&path).unwrap();
        assert!(loaded.is_trusted("https://example.com/base.yml"));
        assert!(loaded.is_trusted("https://corp.example.com/shared.yml"));
        assert!(!loaded.is_trusted("https://other.com/config.yml"));
    }

    #[test]
    fn load_nonexistent_returns_default() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("nonexistent.yml");

        let store = TrustStore::load(&path).unwrap();
        assert!(store.urls.is_empty());
    }

    #[test]
    fn save_creates_parent_directories() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("nested").join("dir").join("trusted.yml");

        let mut store = TrustStore::default();
        store.trust("https://example.com/config.yml");
        store.save(&path).unwrap();

        assert!(path.exists());
        let loaded = TrustStore::load(&path).unwrap();
        assert!(loaded.is_trusted("https://example.com/config.yml"));
    }

    #[test]
    fn save_uses_atomic_write() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("trusted_urls.yml");

        let mut store = TrustStore::default();
        store.trust("https://example.com/config.yml");
        store.save(&path).unwrap();

        // Temp file should not remain
        let temp_path = path.with_extension("yml.tmp");
        assert!(!temp_path.exists());
    }

    #[test]
    fn trust_policy_variants() {
        assert_eq!(TrustPolicy::Prompt, TrustPolicy::Prompt);
        assert_ne!(TrustPolicy::TrustAll, TrustPolicy::Reject);
    }
}
