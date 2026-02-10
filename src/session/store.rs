//! Session storage.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};

use super::{SessionId, SessionMetadata};

/// A complete session record.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Session {
    /// Unique session identifier.
    pub id: SessionId,
    /// Session metadata.
    pub metadata: SessionMetadata,
}

/// Storage for sessions.
pub struct SessionStore {
    path: PathBuf,
}

impl SessionStore {
    /// Create a new session store.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Ensure the store directory exists.
    fn ensure_dir(&self) -> Result<()> {
        fs::create_dir_all(&self.path)
            .with_context(|| format!("Failed to create session store at {:?}", self.path))
    }

    /// Get the path for a session file.
    fn session_path(&self, id: &SessionId) -> PathBuf {
        self.path.join(format!("{}.json", id))
    }

    /// Save a session.
    pub fn save(&self, session: &Session) -> Result<()> {
        self.ensure_dir()?;
        let path = self.session_path(&session.id);
        let json = serde_json::to_string_pretty(session)?;
        fs::write(&path, json).with_context(|| format!("Failed to write session to {:?}", path))
    }

    /// Load a session by ID.
    pub fn load(&self, id: &SessionId) -> Result<Session> {
        let path = self.session_path(id);
        let json =
            fs::read_to_string(&path).with_context(|| format!("Session not found: {:?}", path))?;
        serde_json::from_str(&json).context("Failed to parse session")
    }

    /// Get the most recent session.
    pub fn get_latest(&self) -> Result<Option<Session>> {
        let sessions = self.list_recent(1)?;
        Ok(sessions.into_iter().next())
    }

    /// List recent sessions.
    pub fn list_recent(&self, limit: usize) -> Result<Vec<Session>> {
        self.ensure_dir()?;

        let mut sessions = Vec::new();
        for entry in fs::read_dir(&self.path)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                if let Ok(json) = fs::read_to_string(&path) {
                    if let Ok(session) = serde_json::from_str::<Session>(&json) {
                        sessions.push(session);
                    }
                }
            }
        }

        // Sort by timestamp descending
        sessions.sort_by(|a, b| b.id.timestamp().cmp(&a.id.timestamp()));
        sessions.truncate(limit);

        Ok(sessions)
    }

    /// Clean up old sessions (keep last N).
    pub fn cleanup(&self, keep: usize) -> Result<usize> {
        let sessions = self.list_recent(usize::MAX)?;
        let mut removed = 0;

        for session in sessions.into_iter().skip(keep) {
            let path = self.session_path(&session.id);
            if fs::remove_file(&path).is_ok() {
                removed += 1;
            }
        }

        Ok(removed)
    }

    /// Get the store path.
    pub fn path(&self) -> &PathBuf {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn session_store_save_and_load() {
        let temp = TempDir::new().unwrap();
        let store = SessionStore::new(temp.path().join("sessions"));

        let id = SessionId::new();
        let mut meta = SessionMetadata::new("list", vec![]);
        meta.finalize(0, "output".to_string(), String::new());

        let session = Session {
            id: id.clone(),
            metadata: meta,
        };
        store.save(&session).unwrap();

        let loaded = store.load(&id).unwrap();
        assert_eq!(loaded.metadata.command, "list");
    }

    #[test]
    fn session_store_list_recent() {
        let temp = TempDir::new().unwrap();
        let store = SessionStore::new(temp.path().join("sessions"));

        // Create 3 sessions
        for cmd in ["list", "status", "run"] {
            let id = SessionId::new();
            let mut meta = SessionMetadata::new(cmd, vec![]);
            meta.finalize(0, String::new(), String::new());
            store.save(&Session { id, metadata: meta }).unwrap();
            // Small delay to ensure different timestamps
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        let recent = store.list_recent(10).unwrap();
        assert_eq!(recent.len(), 3);
        // Should be sorted by timestamp descending
        assert_eq!(recent[0].metadata.command, "run");
    }

    #[test]
    fn session_store_get_latest() {
        let temp = TempDir::new().unwrap();
        let store = SessionStore::new(temp.path().join("sessions"));

        let id1 = SessionId::new();
        let mut meta1 = SessionMetadata::new("list", vec![]);
        meta1.finalize(0, String::new(), String::new());
        store
            .save(&Session {
                id: id1,
                metadata: meta1,
            })
            .unwrap();

        std::thread::sleep(std::time::Duration::from_millis(10));

        let id2 = SessionId::new();
        let mut meta2 = SessionMetadata::new("run", vec![]);
        meta2.finalize(0, String::new(), String::new());
        store
            .save(&Session {
                id: id2.clone(),
                metadata: meta2,
            })
            .unwrap();

        let latest = store.get_latest().unwrap().unwrap();
        assert_eq!(latest.id, id2);
    }

    #[test]
    fn session_store_cleanup() {
        let temp = TempDir::new().unwrap();
        let store = SessionStore::new(temp.path().join("sessions"));

        // Create 5 sessions
        for i in 0..5 {
            let id = SessionId::new();
            let mut meta = SessionMetadata::new(format!("cmd{}", i), vec![]);
            meta.finalize(0, String::new(), String::new());
            store.save(&Session { id, metadata: meta }).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        // Keep only last 2
        let removed = store.cleanup(2).unwrap();
        assert_eq!(removed, 3);

        let remaining = store.list_recent(10).unwrap();
        assert_eq!(remaining.len(), 2);
    }

    #[test]
    fn session_store_empty_list() {
        let temp = TempDir::new().unwrap();
        let store = SessionStore::new(temp.path().join("sessions"));

        let recent = store.list_recent(10).unwrap();
        assert!(recent.is_empty());
    }

    #[test]
    fn session_store_get_latest_empty() {
        let temp = TempDir::new().unwrap();
        let store = SessionStore::new(temp.path().join("sessions"));

        let latest = store.get_latest().unwrap();
        assert!(latest.is_none());
    }

    #[test]
    fn session_store_load_nonexistent() {
        let temp = TempDir::new().unwrap();
        let store = SessionStore::new(temp.path().join("sessions"));

        let id = SessionId::new();
        let result = store.load(&id);
        assert!(result.is_err());
    }
}
