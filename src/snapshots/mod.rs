//! Snapshot and baseline management for change checks.
//!
//! The [`SnapshotStore`] manages baseline hashes used by change checks
//! to detect whether targets have changed. Baselines can be:
//! - Per-run (updated after each successful step execution)
//! - Named snapshots (captured via `bivvy snapshot <slug>`)
//!
//! Snapshots are stored per-project in `~/.bivvy/projects/{project_hash}/snapshots/`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Key for looking up baselines in the snapshot store.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SnapshotKey {
    /// Step name.
    pub step_name: String,
    /// Scope: "project" (default) or "workflow:{name}".
    pub scope: String,
    /// Hash of the step's check configuration. Ensures different
    /// check configs produce isolated baselines.
    pub config_hash: String,
}

impl SnapshotKey {
    /// Create a project-scoped key.
    pub fn project(step_name: impl Into<String>, config_hash: impl Into<String>) -> Self {
        Self {
            step_name: step_name.into(),
            scope: "project".to_string(),
            config_hash: config_hash.into(),
        }
    }

    /// Create a workflow-scoped key.
    pub fn workflow(
        step_name: impl Into<String>,
        workflow_name: impl Into<String>,
        config_hash: impl Into<String>,
    ) -> Self {
        Self {
            step_name: step_name.into(),
            scope: format!("workflow:{}", workflow_name.into()),
            config_hash: config_hash.into(),
        }
    }

    /// Returns the filename for this key's storage.
    fn filename(&self) -> String {
        if self.scope == "project" {
            format!("{}_{}.yml", self.step_name, self.config_hash)
        } else {
            let workflow = self.scope.strip_prefix("workflow:").unwrap_or("unknown");
            format!(
                "{}_{}__wf_{}.yml",
                self.step_name, self.config_hash, workflow
            )
        }
    }
}

/// Information about a single baseline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineEntry {
    /// The computed hash.
    pub hash: String,
    /// When this baseline was captured.
    pub captured_at: String,
    /// What was hashed.
    pub target: String,
}

/// Stored snapshot data for a step.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SnapshotData {
    /// Step name.
    pub step: String,
    /// Scope.
    pub scope: String,
    /// Named and run-based baselines.
    #[serde(default)]
    pub baselines: HashMap<String, BaselineEntry>,
}

/// Information about a named snapshot for listing.
#[derive(Debug, Clone)]
pub struct SnapshotInfo {
    /// The snapshot slug.
    pub slug: String,
    /// Step name.
    pub step: String,
    /// Target that was hashed.
    pub target: String,
    /// Hash value.
    pub hash: String,
    /// When captured.
    pub captured_at: String,
}

/// Manages baseline hashes for change check comparisons.
///
/// The store reads/writes YAML files in a snapshots directory.
/// Each step+config combination gets its own file.
pub struct SnapshotStore {
    /// Directory where snapshot files are stored.
    dir: PathBuf,
    /// In-memory cache of loaded snapshot data.
    cache: HashMap<String, SnapshotData>,
}

impl SnapshotStore {
    /// Create a new store backed by the given directory.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            cache: HashMap::new(),
        }
    }

    /// Get the baseline hash for a change check.
    ///
    /// The `baseline_name` parameter selects which baseline to compare against:
    /// - `"_last_run"` for `each_run` baseline config
    /// - `"_first_run"` for `first_run` baseline config
    /// - A named slug for `snapshot:<slug>` baseline config
    pub fn get_baseline(&mut self, key: &SnapshotKey, baseline_name: &str) -> Option<String> {
        self.ensure_loaded(key);
        let filename = key.filename();
        self.cache
            .get(&filename)
            .and_then(|data| data.baselines.get(baseline_name))
            .map(|entry| entry.hash.clone())
    }

    /// Record a new baseline after successful step execution.
    pub fn record_baseline(
        &mut self,
        key: &SnapshotKey,
        baseline_name: &str,
        hash: String,
        target: String,
    ) {
        self.ensure_loaded(key);
        let filename = key.filename();
        let data = self.cache.entry(filename).or_insert_with(|| SnapshotData {
            step: key.step_name.clone(),
            scope: key.scope.clone(),
            baselines: HashMap::new(),
        });

        data.baselines.insert(
            baseline_name.to_string(),
            BaselineEntry {
                hash,
                captured_at: now_iso8601(),
                target,
            },
        );
    }

    /// Capture a named snapshot.
    pub fn capture_named(&mut self, key: &SnapshotKey, slug: &str, hash: String, target: String) {
        self.record_baseline(key, slug, hash, target);
    }

    /// List all named snapshots (excludes internal baselines like _last_run).
    pub fn list_named(&mut self) -> Vec<SnapshotInfo> {
        self.load_all();
        let mut result = Vec::new();
        for data in self.cache.values() {
            for (name, entry) in &data.baselines {
                if !name.starts_with('_') {
                    result.push(SnapshotInfo {
                        slug: name.clone(),
                        step: data.step.clone(),
                        target: entry.target.clone(),
                        hash: entry.hash.clone(),
                        captured_at: entry.captured_at.clone(),
                    });
                }
            }
        }
        result.sort_by(|a, b| a.slug.cmp(&b.slug));
        result
    }

    /// Delete a named snapshot.
    pub fn delete_named(&mut self, slug: &str) -> bool {
        self.load_all();
        let mut found = false;
        for data in self.cache.values_mut() {
            if data.baselines.remove(slug).is_some() {
                found = true;
            }
        }
        found
    }

    /// Save all modified data to disk.
    pub fn save(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        for (filename, data) in &self.cache {
            let path = self.dir.join(filename);
            let yaml = serde_yaml::to_string(data).map_err(std::io::Error::other)?;
            std::fs::write(path, yaml)?;
        }
        Ok(())
    }

    /// Load snapshot data for a key if not already cached.
    fn ensure_loaded(&mut self, key: &SnapshotKey) {
        let filename = key.filename();
        if self.cache.contains_key(&filename) {
            return;
        }
        let path = self.dir.join(&filename);
        if path.exists() {
            if let Ok(contents) = std::fs::read_to_string(&path) {
                if let Ok(data) = serde_yaml::from_str::<SnapshotData>(&contents) {
                    self.cache.insert(filename, data);
                    return;
                }
            }
        }
        // Initialize empty
        self.cache.insert(
            filename,
            SnapshotData {
                step: key.step_name.clone(),
                scope: key.scope.clone(),
                baselines: HashMap::new(),
            },
        );
    }

    /// Load all snapshot files from disk.
    fn load_all(&mut self) {
        if !self.dir.exists() {
            return;
        }
        if let Ok(entries) = std::fs::read_dir(&self.dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "yml") {
                    let filename = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    if self.cache.contains_key(&filename) {
                        continue;
                    }
                    if let Ok(contents) = std::fs::read_to_string(&path) {
                        if let Ok(data) = serde_yaml::from_str::<SnapshotData>(&contents) {
                            self.cache.insert(filename, data);
                        }
                    }
                }
            }
        }
    }
}

fn now_iso8601() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_key() -> SnapshotKey {
        SnapshotKey::project("bundle_install", "abc12345")
    }

    fn workflow_key() -> SnapshotKey {
        SnapshotKey::workflow("bundle_install", "ci", "abc12345")
    }

    #[test]
    fn snapshot_key_project_filename() {
        let key = test_key();
        assert_eq!(key.filename(), "bundle_install_abc12345.yml");
    }

    #[test]
    fn snapshot_key_workflow_filename() {
        let key = workflow_key();
        assert_eq!(key.filename(), "bundle_install_abc12345__wf_ci.yml");
    }

    #[test]
    fn record_and_retrieve_baseline() {
        let temp = TempDir::new().unwrap();
        let mut store = SnapshotStore::new(temp.path());
        let key = test_key();

        store.record_baseline(
            &key,
            "_last_run",
            "sha256:abc123".to_string(),
            "Gemfile.lock".to_string(),
        );

        let hash = store.get_baseline(&key, "_last_run");
        assert_eq!(hash, Some("sha256:abc123".to_string()));
    }

    #[test]
    fn get_baseline_returns_none_when_missing() {
        let temp = TempDir::new().unwrap();
        let mut store = SnapshotStore::new(temp.path());
        let key = test_key();

        assert!(store.get_baseline(&key, "_last_run").is_none());
    }

    #[test]
    fn record_updates_existing_baseline() {
        let temp = TempDir::new().unwrap();
        let mut store = SnapshotStore::new(temp.path());
        let key = test_key();

        store.record_baseline(
            &key,
            "_last_run",
            "sha256:old".to_string(),
            "Gemfile.lock".to_string(),
        );
        store.record_baseline(
            &key,
            "_last_run",
            "sha256:new".to_string(),
            "Gemfile.lock".to_string(),
        );

        assert_eq!(
            store.get_baseline(&key, "_last_run"),
            Some("sha256:new".to_string())
        );
    }

    #[test]
    fn capture_named_snapshot() {
        let temp = TempDir::new().unwrap();
        let mut store = SnapshotStore::new(temp.path());
        let key = test_key();

        store.capture_named(
            &key,
            "v1.0",
            "sha256:release".to_string(),
            "Gemfile.lock".to_string(),
        );

        assert_eq!(
            store.get_baseline(&key, "v1.0"),
            Some("sha256:release".to_string())
        );
    }

    #[test]
    fn list_named_excludes_internal() {
        let temp = TempDir::new().unwrap();
        let mut store = SnapshotStore::new(temp.path());
        let key = test_key();

        store.record_baseline(
            &key,
            "_last_run",
            "sha256:abc".to_string(),
            "Gemfile.lock".to_string(),
        );
        store.record_baseline(
            &key,
            "_first_run",
            "sha256:def".to_string(),
            "Gemfile.lock".to_string(),
        );
        store.capture_named(
            &key,
            "v1.0",
            "sha256:ghi".to_string(),
            "Gemfile.lock".to_string(),
        );

        let named = store.list_named();
        assert_eq!(named.len(), 1);
        assert_eq!(named[0].slug, "v1.0");
    }

    #[test]
    fn delete_named_snapshot() {
        let temp = TempDir::new().unwrap();
        let mut store = SnapshotStore::new(temp.path());
        let key = test_key();

        store.capture_named(
            &key,
            "v1.0",
            "sha256:abc".to_string(),
            "Gemfile.lock".to_string(),
        );
        assert!(store.delete_named("v1.0"));
        assert!(store.get_baseline(&key, "v1.0").is_none());
    }

    #[test]
    fn delete_nonexistent_returns_false() {
        let temp = TempDir::new().unwrap();
        let mut store = SnapshotStore::new(temp.path());
        assert!(!store.delete_named("nonexistent"));
    }

    #[test]
    fn save_and_reload() {
        let temp = TempDir::new().unwrap();
        let key = test_key();

        // Save
        {
            let mut store = SnapshotStore::new(temp.path());
            store.record_baseline(
                &key,
                "_last_run",
                "sha256:persisted".to_string(),
                "Gemfile.lock".to_string(),
            );
            store.save().unwrap();
        }

        // Reload
        {
            let mut store = SnapshotStore::new(temp.path());
            let hash = store.get_baseline(&key, "_last_run");
            assert_eq!(hash, Some("sha256:persisted".to_string()));
        }
    }

    #[test]
    fn project_and_workflow_scopes_are_isolated() {
        let temp = TempDir::new().unwrap();
        let mut store = SnapshotStore::new(temp.path());
        let project_key = test_key();
        let workflow_key = workflow_key();

        store.record_baseline(
            &project_key,
            "_last_run",
            "sha256:project".to_string(),
            "Gemfile.lock".to_string(),
        );
        store.record_baseline(
            &workflow_key,
            "_last_run",
            "sha256:workflow".to_string(),
            "Gemfile.lock".to_string(),
        );

        assert_eq!(
            store.get_baseline(&project_key, "_last_run"),
            Some("sha256:project".to_string())
        );
        assert_eq!(
            store.get_baseline(&workflow_key, "_last_run"),
            Some("sha256:workflow".to_string())
        );
    }

    #[test]
    fn different_config_hashes_are_isolated() {
        let temp = TempDir::new().unwrap();
        let mut store = SnapshotStore::new(temp.path());

        let key1 = SnapshotKey::project("step", "config_a");
        let key2 = SnapshotKey::project("step", "config_b");

        store.record_baseline(
            &key1,
            "_last_run",
            "sha256:a".to_string(),
            "file.txt".to_string(),
        );
        store.record_baseline(
            &key2,
            "_last_run",
            "sha256:b".to_string(),
            "file.txt".to_string(),
        );

        assert_eq!(
            store.get_baseline(&key1, "_last_run"),
            Some("sha256:a".to_string())
        );
        assert_eq!(
            store.get_baseline(&key2, "_last_run"),
            Some("sha256:b".to_string())
        );
    }

    #[test]
    fn empty_dir_load_all_works() {
        let temp = TempDir::new().unwrap();
        let mut store = SnapshotStore::new(temp.path());
        let named = store.list_named();
        assert!(named.is_empty());
    }
}
