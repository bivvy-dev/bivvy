//! Cache storage implementation.

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

use super::entry::CacheEntry;

/// Storage for cached templates.
pub struct CacheStore {
    /// Root directory for cache.
    root: PathBuf,
}

impl CacheStore {
    /// Create a new cache store.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Get the cache root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Ensure the cache directory exists.
    fn ensure_dir(&self) -> Result<()> {
        fs::create_dir_all(&self.root)
            .with_context(|| format!("Failed to create cache directory {:?}", self.root))
    }

    /// Get the path for storing an entry's content.
    pub fn content_path(&self, source_id: &str, template_name: &str) -> PathBuf {
        let key = format!("{}:{}", source_id, template_name);
        let hash = Sha256::digest(key.as_bytes());
        let hash_str = hex::encode(&hash[..16]);
        self.root.join(hash_str)
    }

    /// Get the metadata file path for an entry.
    fn metadata_path(&self, source_id: &str, template_name: &str) -> PathBuf {
        self.content_path(source_id, template_name)
            .with_extension("meta.json")
    }

    /// Store content and return a cache entry.
    pub fn store(
        &self,
        source_id: &str,
        template_name: &str,
        content: &str,
        ttl_seconds: u64,
    ) -> Result<CacheEntry> {
        self.ensure_dir()?;

        let content_path = self.content_path(source_id, template_name);
        fs::write(&content_path, content)?;

        let entry = CacheEntry::new(source_id, template_name, &content_path, ttl_seconds)
            .with_size(content.len() as u64);

        self.save_metadata(&entry)?;

        Ok(entry)
    }

    /// Load a cached entry's metadata.
    pub fn load(&self, source_id: &str, template_name: &str) -> Result<Option<CacheEntry>> {
        let meta_path = self.metadata_path(source_id, template_name);

        if !meta_path.exists() {
            return Ok(None);
        }

        let json = fs::read_to_string(&meta_path)?;
        let entry: CacheEntry = serde_json::from_str(&json)?;

        Ok(Some(entry))
    }

    /// Read the cached content.
    pub fn read_content(&self, entry: &CacheEntry) -> Result<String> {
        fs::read_to_string(&entry.content_path).with_context(|| {
            format!(
                "Failed to read cached content from {:?}",
                entry.content_path
            )
        })
    }

    /// Save entry metadata.
    fn save_metadata(&self, entry: &CacheEntry) -> Result<()> {
        let meta_path = self.metadata_path(&entry.source_id, &entry.template_name);
        let json = serde_json::to_string_pretty(entry)?;
        fs::write(&meta_path, json)?;
        Ok(())
    }

    /// Update an entry's metadata (e.g., after revalidation).
    pub fn update(&self, entry: &CacheEntry) -> Result<()> {
        self.save_metadata(entry)
    }

    /// Remove a cached entry.
    pub fn remove(&self, source_id: &str, template_name: &str) -> Result<bool> {
        let content_path = self.content_path(source_id, template_name);
        let meta_path = self.metadata_path(source_id, template_name);

        let mut removed = false;

        if content_path.exists() {
            fs::remove_file(&content_path)?;
            removed = true;
        }

        if meta_path.exists() {
            fs::remove_file(&meta_path)?;
            removed = true;
        }

        Ok(removed)
    }

    /// List all cached entries.
    pub fn list(&self) -> Result<Vec<CacheEntry>> {
        self.ensure_dir()?;

        let mut entries = Vec::new();

        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().is_some_and(|e| e == "json") {
                if let Ok(json) = fs::read_to_string(&path) {
                    if let Ok(cache_entry) = serde_json::from_str::<CacheEntry>(&json) {
                        entries.push(cache_entry);
                    }
                }
            }
        }

        entries.sort_by(|a, b| b.metadata.cached_at.cmp(&a.metadata.cached_at));
        Ok(entries)
    }

    /// Clear all cached entries.
    pub fn clear(&self) -> Result<usize> {
        let entries = self.list()?;
        let count = entries.len();

        for entry in entries {
            let _ = self.remove(&entry.source_id, &entry.template_name);
        }

        Ok(count)
    }

    /// Get total cache size in bytes.
    pub fn total_size(&self) -> Result<u64> {
        let entries = self.list()?;
        Ok(entries.iter().map(|e| e.metadata.size_bytes).sum())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn cache_store_creation() {
        let temp = TempDir::new().unwrap();
        let store = CacheStore::new(temp.path());

        assert_eq!(store.root(), temp.path());
    }

    #[test]
    fn store_and_load() {
        let temp = TempDir::new().unwrap();
        let store = CacheStore::new(temp.path());

        let entry = store
            .store("http:test", "template1", "template content", 3600)
            .unwrap();

        assert_eq!(entry.source_id, "http:test");
        assert_eq!(entry.template_name, "template1");

        let loaded = store.load("http:test", "template1").unwrap().unwrap();
        assert_eq!(loaded.source_id, entry.source_id);
        assert_eq!(loaded.template_name, entry.template_name);
    }

    #[test]
    fn read_content() {
        let temp = TempDir::new().unwrap();
        let store = CacheStore::new(temp.path());

        let entry = store
            .store("http:test", "template1", "hello world", 3600)
            .unwrap();

        let content = store.read_content(&entry).unwrap();
        assert_eq!(content, "hello world");
    }

    #[test]
    fn load_nonexistent_returns_none() {
        let temp = TempDir::new().unwrap();
        let store = CacheStore::new(temp.path());

        let result = store.load("http:test", "nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn remove_entry() {
        let temp = TempDir::new().unwrap();
        let store = CacheStore::new(temp.path());

        store
            .store("http:test", "template1", "content", 3600)
            .unwrap();

        let removed = store.remove("http:test", "template1").unwrap();
        assert!(removed);

        let loaded = store.load("http:test", "template1").unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn remove_nonexistent_returns_false() {
        let temp = TempDir::new().unwrap();
        let store = CacheStore::new(temp.path());

        let removed = store.remove("http:test", "nonexistent").unwrap();
        assert!(!removed);
    }

    #[test]
    fn list_entries() {
        let temp = TempDir::new().unwrap();
        let store = CacheStore::new(temp.path());

        store
            .store("http:test", "template1", "content1", 3600)
            .unwrap();
        store
            .store("http:test", "template2", "content2", 3600)
            .unwrap();

        let entries = store.list().unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn clear_cache() {
        let temp = TempDir::new().unwrap();
        let store = CacheStore::new(temp.path());

        store
            .store("http:test", "template1", "content1", 3600)
            .unwrap();
        store
            .store("http:test", "template2", "content2", 3600)
            .unwrap();

        let cleared = store.clear().unwrap();
        assert_eq!(cleared, 2);

        let entries = store.list().unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn total_size_calculation() {
        let temp = TempDir::new().unwrap();
        let store = CacheStore::new(temp.path());

        store.store("http:test", "t1", "12345", 3600).unwrap(); // 5 bytes
        store.store("http:test", "t2", "1234567890", 3600).unwrap(); // 10 bytes

        let total = store.total_size().unwrap();
        assert_eq!(total, 15);
    }

    #[test]
    fn content_path_is_deterministic() {
        let temp = TempDir::new().unwrap();
        let store = CacheStore::new(temp.path());

        let path1 = store.content_path("http:test", "template");
        let path2 = store.content_path("http:test", "template");

        assert_eq!(path1, path2);
    }

    #[test]
    fn different_entries_have_different_paths() {
        let temp = TempDir::new().unwrap();
        let store = CacheStore::new(temp.path());

        let path1 = store.content_path("http:test", "template1");
        let path2 = store.content_path("http:test", "template2");

        assert_ne!(path1, path2);
    }
}
