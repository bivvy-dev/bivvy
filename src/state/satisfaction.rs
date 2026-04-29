//! Satisfaction cache types and persistence.
//!
//! The satisfaction cache is the core data structure of bivvy's decision engine.
//! It records what bivvy knows about each step's satisfaction state — whether the
//! step's purpose is already fulfilled — and persists that knowledge across sessions.
//!
//! # Two-Layer Architecture
//!
//! The cache operates at two layers:
//! - **Persisted layer** (`satisfaction.json`): survives across sessions
//! - **Runtime layer** (in-memory `HashMap`): working copy for one session
//!
//! The runtime layer is seeded from the persisted layer on startup, updated as
//! steps are evaluated, and flushed back to disk at the end.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A persisted record of a step's satisfaction state.
///
/// Contains the satisfaction decision, the signal that produced it, and the
/// verifiable evidence that supports it. Used for both runtime decisions and
/// cross-session persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SatisfactionRecord {
    /// Whether the step is satisfied.
    pub satisfied: bool,

    /// What signal produced the satisfaction (or lack thereof).
    pub source: SatisfactionSource,

    /// When this record was created or last updated.
    pub recorded_at: DateTime<Utc>,

    /// The specific evidence that supports this record.
    /// Used for fast cross-session validation.
    pub evidence: SatisfactionEvidence,

    /// Hash of the step's check configuration at the time of recording.
    /// If the config changes, this record is invalidated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_hash: Option<String>,

    /// Hash of the step's definition (command, deps, tools) at recording time.
    /// If the definition changes, this record is invalidated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_hash: Option<String>,
}

/// What signal produced a satisfaction decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SatisfactionSource {
    /// A file or binary exists at the expected path.
    PresenceCheck,
    /// Target hash matches baseline — nothing changed since last run.
    ChangeCheck,
    /// A check command succeeded.
    ExecutionCheck,
    /// Explicit `satisfied_when` conditions passed.
    ExplicitCondition,
    /// Step ran successfully within the rerun window.
    ExecutionHistory,
    /// No data yet — step has never been evaluated.
    NeverEvaluated,
}

/// The verifiable evidence behind a satisfaction decision.
///
/// Each variant carries enough data to re-validate cheaply across sessions.
/// Presence checks need a stat(), change checks need a hash compare,
/// execution history needs a timestamp check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SatisfactionEvidence {
    /// File or binary exists at this path.
    /// Validate: `stat()` the path.
    Presence { target: String, kind: PresenceKind },

    /// Target hash matches baseline.
    /// Validate: rehash the target, compare.
    ChangeBaseline { target: String, hash: String },

    /// Command exited 0.
    /// Validate: re-run the command (expensive — only for execution checks).
    CommandSuccess { command: String },

    /// Step ran successfully at this time.
    /// Validate: check if `ran_at` is within the rerun window.
    HistoricalRun {
        ran_at: DateTime<Utc>,
        exit_code: i32,
    },

    /// Multiple pieces of evidence (for `satisfied_when` with multiple conditions).
    Composite(Vec<SatisfactionEvidence>),

    /// No evidence (step has never been evaluated).
    None,
}

/// Kind of presence evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PresenceKind {
    /// A file path exists.
    File,
    /// A binary is found on PATH.
    Binary,
    /// A custom command check.
    Custom,
}

impl SatisfactionEvidence {
    /// Validate this evidence against the current state of the world.
    ///
    /// Returns `true` if the evidence still holds. Validation cost varies by type:
    /// - `Presence` — stat() call (~microseconds)
    /// - `ChangeBaseline` — rehash + compare (~milliseconds)
    /// - `HistoricalRun` — timestamp arithmetic (instant)
    /// - `CommandSuccess` — requires re-running the command (expensive, returns false to force re-evaluation)
    /// - `Composite` — validates each sub-evidence
    /// - `None` — always invalid
    pub fn validate(&self, project_root: &Path) -> bool {
        match self {
            SatisfactionEvidence::Presence { target, kind } => match kind {
                PresenceKind::File => {
                    let path = if Path::new(target).is_absolute() {
                        std::path::PathBuf::from(target)
                    } else {
                        project_root.join(target)
                    };
                    path.exists()
                }
                PresenceKind::Binary => {
                    // Check if binary is on PATH
                    crate::sys::find_on_path(target).is_some()
                }
                PresenceKind::Custom => {
                    // Custom presence checks can't be cheaply validated
                    false
                }
            },
            SatisfactionEvidence::ChangeBaseline { .. } => {
                // Rehashing requires the snapshot store — defer to the caller
                // For now, return false to force re-evaluation
                false
            }
            SatisfactionEvidence::CommandSuccess { .. } => {
                // Re-running a command is expensive — always re-evaluate
                false
            }
            SatisfactionEvidence::HistoricalRun { ran_at, exit_code } => {
                // Valid if the run succeeded (exit 0) — the rerun window check
                // is done by the caller since it depends on step configuration
                *exit_code == 0 && *ran_at <= Utc::now()
            }
            SatisfactionEvidence::Composite(evidence_list) => {
                evidence_list.iter().all(|e| e.validate(project_root))
            }
            SatisfactionEvidence::None => false,
        }
    }
}

/// Two-layer satisfaction cache: persisted JSON on disk + runtime HashMap in memory.
///
/// During a `bivvy run`:
/// 1. Load persisted records from `satisfaction.json`
/// 2. As steps are evaluated, store results in the runtime layer
/// 3. At the end (or after each step for crash recovery), flush to disk
///
/// The runtime layer takes precedence over the persisted layer.
pub struct SatisfactionCache {
    /// Runtime layer — the working copy for this session.
    runtime: HashMap<String, SatisfactionRecord>,
    /// Persisted layer — loaded from disk at session start.
    persisted: HashMap<String, SatisfactionRecord>,
    /// Path to the satisfaction.json file.
    path: PathBuf,
}

impl SatisfactionCache {
    /// Create an empty cache (for new projects or `--fresh` mode).
    pub fn empty(path: PathBuf) -> Self {
        Self {
            runtime: HashMap::new(),
            persisted: HashMap::new(),
            path,
        }
    }

    /// Load the cache from a `satisfaction.json` file.
    ///
    /// If the file doesn't exist or is invalid, returns an empty cache.
    pub fn load(path: PathBuf) -> Self {
        let persisted = if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
                Err(_) => HashMap::new(),
            }
        } else {
            HashMap::new()
        };

        Self {
            runtime: HashMap::new(),
            persisted,
            path,
        }
    }

    /// Get a satisfaction record for a step.
    ///
    /// Checks the runtime layer first, then the persisted layer.
    /// Persisted records are validated before being returned — if the evidence
    /// is stale or the config/step hash doesn't match, the record is discarded.
    pub fn get(
        &self,
        step: &str,
        project_root: &Path,
        config_hash: Option<&str>,
        step_hash: Option<&str>,
    ) -> Option<&SatisfactionRecord> {
        // Runtime layer takes precedence — no validation needed (just computed)
        if let Some(record) = self.runtime.get(step) {
            return Some(record);
        }

        // Check persisted layer with validation
        if let Some(record) = self.persisted.get(step) {
            // Structural invalidation: config or step definition changed
            if let Some(expected) = config_hash {
                if record.config_hash.as_deref() != Some(expected) {
                    return None;
                }
            }
            if let Some(expected) = step_hash {
                if record.step_hash.as_deref() != Some(expected) {
                    return None;
                }
            }

            // Evidence validation
            if record.evidence.validate(project_root) {
                return Some(record);
            }
        }

        None
    }

    /// Get a satisfaction record from the runtime layer only (no validation).
    pub fn get_runtime(&self, step: &str) -> Option<&SatisfactionRecord> {
        self.runtime.get(step)
    }

    /// Store a satisfaction record in the runtime layer.
    pub fn store(&mut self, step: &str, record: SatisfactionRecord) {
        self.runtime.insert(step.to_string(), record);
    }

    /// Check if a step has any record (runtime or persisted, without validation).
    pub fn has_record(&self, step: &str) -> bool {
        self.runtime.contains_key(step) || self.persisted.contains_key(step)
    }

    /// Invalidate a specific step's records from both layers.
    pub fn invalidate(&mut self, step: &str) {
        self.runtime.remove(step);
        self.persisted.remove(step);
    }

    /// Invalidate all records (for `--fresh` mode).
    pub fn invalidate_all(&mut self) {
        self.runtime.clear();
        self.persisted.clear();
    }

    /// Flush the runtime cache to disk as `satisfaction.json`.
    ///
    /// Merges runtime records into persisted records (runtime wins on conflict),
    /// then writes the combined result to disk using atomic write.
    pub fn flush(&mut self) -> std::io::Result<()> {
        // Merge runtime into persisted
        for (step, record) in &self.runtime {
            self.persisted.insert(step.clone(), record.clone());
        }

        // Ensure parent directory exists
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Atomic write: temp file then rename
        let content =
            serde_json::to_string_pretty(&self.persisted).map_err(std::io::Error::other)?;
        let temp_path = self.path.with_extension("json.tmp");
        std::fs::write(&temp_path, &content)?;
        std::fs::rename(&temp_path, &self.path)?;

        Ok(())
    }

    /// Get the path to the satisfaction.json file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Get the number of records in the runtime layer.
    pub fn runtime_len(&self) -> usize {
        self.runtime.len()
    }

    /// Get the number of records in the persisted layer.
    pub fn persisted_len(&self) -> usize {
        self.persisted.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn satisfaction_record_json_round_trip() {
        let record = SatisfactionRecord {
            satisfied: true,
            source: SatisfactionSource::PresenceCheck,
            recorded_at: Utc::now(),
            evidence: SatisfactionEvidence::Presence {
                target: "node_modules".to_string(),
                kind: PresenceKind::File,
            },
            config_hash: Some("abc123".to_string()),
            step_hash: Some("def456".to_string()),
        };

        let json = serde_json::to_string(&record).unwrap();
        let parsed: SatisfactionRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.satisfied, record.satisfied);
        assert_eq!(parsed.source, record.source);
        assert_eq!(parsed.config_hash, record.config_hash);
        assert_eq!(parsed.step_hash, record.step_hash);
    }

    #[test]
    fn satisfaction_source_round_trip() {
        let sources = vec![
            SatisfactionSource::PresenceCheck,
            SatisfactionSource::ChangeCheck,
            SatisfactionSource::ExecutionCheck,
            SatisfactionSource::ExplicitCondition,
            SatisfactionSource::ExecutionHistory,
            SatisfactionSource::NeverEvaluated,
        ];
        for source in sources {
            let json = serde_json::to_string(&source).unwrap();
            let parsed: SatisfactionSource = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, source);
        }
    }

    #[test]
    fn evidence_presence_file_validates_when_exists() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("node_modules");
        std::fs::create_dir(&file_path).unwrap();

        let evidence = SatisfactionEvidence::Presence {
            target: "node_modules".to_string(),
            kind: PresenceKind::File,
        };
        assert!(evidence.validate(temp.path()));
    }

    #[test]
    fn evidence_presence_file_invalid_when_missing() {
        let temp = TempDir::new().unwrap();

        let evidence = SatisfactionEvidence::Presence {
            target: "node_modules".to_string(),
            kind: PresenceKind::File,
        };
        assert!(!evidence.validate(temp.path()));
    }

    #[test]
    fn evidence_presence_binary_validates_for_known_binary() {
        // "sh" should exist on any system
        let evidence = SatisfactionEvidence::Presence {
            target: "sh".to_string(),
            kind: PresenceKind::Binary,
        };
        assert!(evidence.validate(Path::new("/")));
    }

    #[test]
    fn evidence_presence_binary_invalid_for_unknown() {
        let evidence = SatisfactionEvidence::Presence {
            target: "nonexistent-binary-xyz-12345".to_string(),
            kind: PresenceKind::Binary,
        };
        assert!(!evidence.validate(Path::new("/")));
    }

    #[test]
    fn evidence_historical_run_validates_for_success() {
        let evidence = SatisfactionEvidence::HistoricalRun {
            ran_at: Utc::now() - chrono::Duration::hours(1),
            exit_code: 0,
        };
        assert!(evidence.validate(Path::new("/")));
    }

    #[test]
    fn evidence_historical_run_invalid_for_failure() {
        let evidence = SatisfactionEvidence::HistoricalRun {
            ran_at: Utc::now() - chrono::Duration::hours(1),
            exit_code: 1,
        };
        assert!(!evidence.validate(Path::new("/")));
    }

    #[test]
    fn evidence_command_success_always_needs_reeval() {
        let evidence = SatisfactionEvidence::CommandSuccess {
            command: "bundle check".to_string(),
        };
        assert!(!evidence.validate(Path::new("/")));
    }

    #[test]
    fn evidence_change_baseline_always_needs_reeval() {
        let evidence = SatisfactionEvidence::ChangeBaseline {
            target: "Gemfile.lock".to_string(),
            hash: "abc123".to_string(),
        };
        assert!(!evidence.validate(Path::new("/")));
    }

    #[test]
    fn evidence_none_always_invalid() {
        assert!(!SatisfactionEvidence::None.validate(Path::new("/")));
    }

    #[test]
    fn evidence_composite_all_valid() {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join("file_a"), "content").unwrap();
        std::fs::write(temp.path().join("file_b"), "content").unwrap();

        let evidence = SatisfactionEvidence::Composite(vec![
            SatisfactionEvidence::Presence {
                target: "file_a".to_string(),
                kind: PresenceKind::File,
            },
            SatisfactionEvidence::Presence {
                target: "file_b".to_string(),
                kind: PresenceKind::File,
            },
        ]);
        assert!(evidence.validate(temp.path()));
    }

    #[test]
    fn evidence_composite_one_invalid() {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join("file_a"), "content").unwrap();
        // file_b does not exist

        let evidence = SatisfactionEvidence::Composite(vec![
            SatisfactionEvidence::Presence {
                target: "file_a".to_string(),
                kind: PresenceKind::File,
            },
            SatisfactionEvidence::Presence {
                target: "file_b".to_string(),
                kind: PresenceKind::File,
            },
        ]);
        assert!(!evidence.validate(temp.path()));
    }

    #[test]
    fn evidence_composite_empty_is_valid() {
        let evidence = SatisfactionEvidence::Composite(vec![]);
        assert!(evidence.validate(Path::new("/")));
    }

    #[test]
    fn evidence_json_round_trip_all_variants() {
        let variants = vec![
            SatisfactionEvidence::Presence {
                target: "node_modules".to_string(),
                kind: PresenceKind::File,
            },
            SatisfactionEvidence::Presence {
                target: "ruby".to_string(),
                kind: PresenceKind::Binary,
            },
            SatisfactionEvidence::ChangeBaseline {
                target: "yarn.lock".to_string(),
                hash: "sha256:abc".to_string(),
            },
            SatisfactionEvidence::CommandSuccess {
                command: "bundle check".to_string(),
            },
            SatisfactionEvidence::HistoricalRun {
                ran_at: Utc::now(),
                exit_code: 0,
            },
            SatisfactionEvidence::Composite(vec![SatisfactionEvidence::None]),
            SatisfactionEvidence::None,
        ];

        for evidence in variants {
            let json = serde_json::to_string(&evidence).unwrap();
            let parsed: SatisfactionEvidence = serde_json::from_str(&json).unwrap();
            // Verify round-trip by re-serializing
            let json2 = serde_json::to_string(&parsed).unwrap();
            assert_eq!(json, json2);
        }
    }

    #[test]
    fn presence_kind_round_trip() {
        let kinds = vec![
            PresenceKind::File,
            PresenceKind::Binary,
            PresenceKind::Custom,
        ];
        for kind in kinds {
            let json = serde_json::to_string(&kind).unwrap();
            let parsed: PresenceKind = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, kind);
        }
    }

    #[test]
    fn evidence_presence_absolute_path() {
        let temp = TempDir::new().unwrap();
        let abs_path = temp.path().join("absolute_file");
        std::fs::write(&abs_path, "content").unwrap();

        let evidence = SatisfactionEvidence::Presence {
            target: abs_path.to_string_lossy().to_string(),
            kind: PresenceKind::File,
        };
        // Should work regardless of project_root since path is absolute
        assert!(evidence.validate(Path::new("/nonexistent")));
    }
}

#[cfg(test)]
mod cache_tests {
    use super::*;
    use tempfile::TempDir;

    fn make_record(satisfied: bool, source: SatisfactionSource) -> SatisfactionRecord {
        SatisfactionRecord {
            satisfied,
            source,
            recorded_at: Utc::now(),
            evidence: SatisfactionEvidence::None,
            config_hash: None,
            step_hash: None,
        }
    }

    fn make_presence_record(target: &str) -> SatisfactionRecord {
        SatisfactionRecord {
            satisfied: true,
            source: SatisfactionSource::PresenceCheck,
            recorded_at: Utc::now(),
            evidence: SatisfactionEvidence::Presence {
                target: target.to_string(),
                kind: PresenceKind::File,
            },
            config_hash: Some("hash1".to_string()),
            step_hash: Some("step1".to_string()),
        }
    }

    #[test]
    fn cache_store_and_retrieve_runtime() {
        let temp = TempDir::new().unwrap();
        let mut cache = SatisfactionCache::empty(temp.path().join("satisfaction.json"));

        let record = make_record(true, SatisfactionSource::ExecutionHistory);
        cache.store("install_deps", record);

        let result = cache.get_runtime("install_deps");
        assert!(result.is_some());
        assert!(result.unwrap().satisfied);
    }

    #[test]
    fn cache_runtime_not_found() {
        let temp = TempDir::new().unwrap();
        let cache = SatisfactionCache::empty(temp.path().join("satisfaction.json"));
        assert!(cache.get_runtime("missing").is_none());
    }

    #[test]
    fn cache_load_from_disk() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("satisfaction.json");

        // Write a file directly
        let mut records = HashMap::new();
        records.insert(
            "install_deps".to_string(),
            make_presence_record("node_modules"),
        );
        let json = serde_json::to_string_pretty(&records).unwrap();
        std::fs::write(&path, json).unwrap();

        // Create the target so evidence validates
        std::fs::create_dir(temp.path().join("node_modules")).unwrap();

        let cache = SatisfactionCache::load(path);
        assert_eq!(cache.persisted_len(), 1);

        let result = cache.get("install_deps", temp.path(), Some("hash1"), Some("step1"));
        assert!(result.is_some());
        assert!(result.unwrap().satisfied);
    }

    #[test]
    fn cache_load_nonexistent_returns_empty() {
        let temp = TempDir::new().unwrap();
        let cache = SatisfactionCache::load(temp.path().join("nonexistent.json"));
        assert_eq!(cache.persisted_len(), 0);
        assert_eq!(cache.runtime_len(), 0);
    }

    #[test]
    fn cache_load_invalid_json_returns_empty() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("satisfaction.json");
        std::fs::write(&path, "not valid json").unwrap();

        let cache = SatisfactionCache::load(path);
        assert_eq!(cache.persisted_len(), 0);
    }

    #[test]
    fn cache_runtime_takes_precedence() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("satisfaction.json");

        // Persisted says not satisfied
        let mut records = HashMap::new();
        records.insert(
            "step_a".to_string(),
            make_record(false, SatisfactionSource::NeverEvaluated),
        );
        let json = serde_json::to_string_pretty(&records).unwrap();
        std::fs::write(&path, json).unwrap();

        let mut cache = SatisfactionCache::load(path);

        // Runtime says satisfied
        cache.store(
            "step_a",
            make_record(true, SatisfactionSource::ExecutionHistory),
        );

        let result = cache.get("step_a", temp.path(), None, None);
        assert!(result.is_some());
        assert!(result.unwrap().satisfied);
    }

    #[test]
    fn cache_flush_writes_to_disk() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("satisfaction.json");

        let mut cache = SatisfactionCache::empty(path.clone());
        cache.store(
            "step_a",
            make_record(true, SatisfactionSource::PresenceCheck),
        );
        cache.store(
            "step_b",
            make_record(false, SatisfactionSource::NeverEvaluated),
        );

        cache.flush().unwrap();

        // Verify file was written
        assert!(path.exists());

        // Re-load and verify
        let reloaded = SatisfactionCache::load(path);
        assert_eq!(reloaded.persisted_len(), 2);
    }

    #[test]
    fn cache_flush_no_temp_file_remains() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("satisfaction.json");

        let mut cache = SatisfactionCache::empty(path.clone());
        cache.store(
            "step_a",
            make_record(true, SatisfactionSource::PresenceCheck),
        );
        cache.flush().unwrap();

        let temp_path = path.with_extension("json.tmp");
        assert!(!temp_path.exists());
    }

    #[test]
    fn cache_invalidate_step() {
        let temp = TempDir::new().unwrap();
        let mut cache = SatisfactionCache::empty(temp.path().join("satisfaction.json"));

        cache.store(
            "step_a",
            make_record(true, SatisfactionSource::PresenceCheck),
        );
        assert!(cache.has_record("step_a"));

        cache.invalidate("step_a");
        assert!(!cache.has_record("step_a"));
    }

    #[test]
    fn cache_invalidate_all() {
        let temp = TempDir::new().unwrap();
        let mut cache = SatisfactionCache::empty(temp.path().join("satisfaction.json"));

        cache.store(
            "step_a",
            make_record(true, SatisfactionSource::PresenceCheck),
        );
        cache.store("step_b", make_record(true, SatisfactionSource::ChangeCheck));

        cache.invalidate_all();
        assert_eq!(cache.runtime_len(), 0);
        assert_eq!(cache.persisted_len(), 0);
    }

    #[test]
    fn cache_structural_invalidation_config_hash() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("satisfaction.json");

        // Persisted with config_hash "old_hash"
        let mut records = HashMap::new();
        let mut record = make_presence_record("node_modules");
        record.config_hash = Some("old_hash".to_string());
        records.insert("step_a".to_string(), record);
        std::fs::write(&path, serde_json::to_string(&records).unwrap()).unwrap();
        std::fs::create_dir(temp.path().join("node_modules")).unwrap();

        let cache = SatisfactionCache::load(path);

        // Config hash changed — record should be invalidated
        let result = cache.get("step_a", temp.path(), Some("new_hash"), None);
        assert!(result.is_none());

        // Same hash — record should be valid
        let result = cache.get("step_a", temp.path(), Some("old_hash"), None);
        assert!(result.is_some());
    }

    #[test]
    fn cache_structural_invalidation_step_hash() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("satisfaction.json");

        let mut records = HashMap::new();
        let mut record = make_presence_record("node_modules");
        record.step_hash = Some("old_step".to_string());
        records.insert("step_a".to_string(), record);
        std::fs::write(&path, serde_json::to_string(&records).unwrap()).unwrap();
        std::fs::create_dir(temp.path().join("node_modules")).unwrap();

        let cache = SatisfactionCache::load(path);

        // Step hash changed — record should be invalidated
        let result = cache.get("step_a", temp.path(), None, Some("new_step"));
        assert!(result.is_none());

        // Same hash — record should be valid
        let result = cache.get("step_a", temp.path(), None, Some("old_step"));
        assert!(result.is_some());
    }

    #[test]
    fn cache_evidence_validation_fails_for_missing_file() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("satisfaction.json");

        let mut records = HashMap::new();
        records.insert(
            "step_a".to_string(),
            make_presence_record("nonexistent_dir"),
        );
        std::fs::write(&path, serde_json::to_string(&records).unwrap()).unwrap();

        let cache = SatisfactionCache::load(path);

        // File doesn't exist — evidence validation fails
        let result = cache.get("step_a", temp.path(), Some("hash1"), Some("step1"));
        assert!(result.is_none());
    }

    #[test]
    fn cache_flush_merges_runtime_into_persisted() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("satisfaction.json");

        // Start with a persisted record
        let mut records = HashMap::new();
        records.insert(
            "persisted_step".to_string(),
            make_record(true, SatisfactionSource::ExecutionHistory),
        );
        std::fs::write(&path, serde_json::to_string(&records).unwrap()).unwrap();

        let mut cache = SatisfactionCache::load(path.clone());

        // Add a runtime record
        cache.store(
            "runtime_step",
            make_record(true, SatisfactionSource::PresenceCheck),
        );

        cache.flush().unwrap();

        // Both should be in the file
        let reloaded = SatisfactionCache::load(path);
        assert_eq!(reloaded.persisted_len(), 2);
    }

    #[test]
    fn cache_has_record() {
        let temp = TempDir::new().unwrap();
        let mut cache = SatisfactionCache::empty(temp.path().join("satisfaction.json"));
        assert!(!cache.has_record("step_a"));

        cache.store(
            "step_a",
            make_record(true, SatisfactionSource::PresenceCheck),
        );
        assert!(cache.has_record("step_a"));
    }

    #[test]
    fn cache_get_with_no_hash_checks() {
        let temp = TempDir::new().unwrap();
        let mut cache = SatisfactionCache::empty(temp.path().join("satisfaction.json"));

        // Store a record with evidence that validates (None evidence always fails,
        // so use runtime which doesn't need validation)
        cache.store(
            "step_a",
            make_record(true, SatisfactionSource::PresenceCheck),
        );

        // get() with no hash checks should find runtime record
        let result = cache.get("step_a", temp.path(), None, None);
        assert!(result.is_some());
    }
}
