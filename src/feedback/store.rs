//! Feedback storage (JSONL format).

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

use anyhow::{Context, Result};

use super::{FeedbackEntry, FeedbackStatus};
use crate::session::SessionId;

/// Storage for feedback entries.
pub struct FeedbackStore {
    path: PathBuf,
}

impl FeedbackStore {
    /// Create a new feedback store.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Get the store path.
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Ensure the parent directory exists.
    fn ensure_dir(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory {:?}", parent))?;
        }
        Ok(())
    }

    /// Append a feedback entry.
    pub fn append(&self, entry: &FeedbackEntry) -> Result<()> {
        self.ensure_dir()?;

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .with_context(|| format!("Failed to open {:?}", self.path))?;

        let json = serde_json::to_string(entry)?;
        writeln!(file, "{}", json)?;

        Ok(())
    }

    /// List all feedback entries.
    pub fn list_all(&self) -> Result<Vec<FeedbackEntry>> {
        self.read_entries(|_| true)
    }

    /// List feedback by status.
    pub fn list_by_status(&self, status: FeedbackStatus) -> Result<Vec<FeedbackEntry>> {
        self.read_entries(|e| e.status == status)
    }

    /// List feedback by tag.
    pub fn list_by_tag(&self, tag: &str) -> Result<Vec<FeedbackEntry>> {
        self.read_entries(|e| e.tags.iter().any(|t| t == tag))
    }

    /// List feedback by session.
    pub fn list_by_session(&self, session_id: &SessionId) -> Result<Vec<FeedbackEntry>> {
        self.read_entries(|e| e.session_id.as_ref() == Some(session_id))
    }

    /// Update an entry by ID.
    pub fn update(&self, id: &str, updater: impl FnOnce(&mut FeedbackEntry)) -> Result<bool> {
        let mut entries = self.list_all()?;
        let mut found = false;

        for entry in &mut entries {
            if entry.id == id {
                updater(entry);
                found = true;
                break;
            }
        }

        if found {
            self.rewrite_all(&entries)?;
        }

        Ok(found)
    }

    /// Get an entry by ID.
    pub fn get(&self, id: &str) -> Result<Option<FeedbackEntry>> {
        let entries = self.read_entries(|e| e.id == id)?;
        Ok(entries.into_iter().next())
    }

    /// Read entries matching a predicate.
    fn read_entries(
        &self,
        predicate: impl Fn(&FeedbackEntry) -> bool,
    ) -> Result<Vec<FeedbackEntry>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(&self.path)?;
        let reader = BufReader::new(file);
        let mut entries = Vec::new();

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(entry) = serde_json::from_str::<FeedbackEntry>(&line) {
                if predicate(&entry) {
                    entries.push(entry);
                }
            }
        }

        Ok(entries)
    }

    /// Rewrite all entries (for updates).
    fn rewrite_all(&self, entries: &[FeedbackEntry]) -> Result<()> {
        self.ensure_dir()?;

        let mut file = File::create(&self.path)?;
        for entry in entries {
            let json = serde_json::to_string(entry)?;
            writeln!(file, "{}", json)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn feedback_store_save_and_load() {
        let temp = TempDir::new().unwrap();
        let store = FeedbackStore::new(temp.path().join("feedback.jsonl"));

        let entry = FeedbackEntry::new("test feedback");
        store.append(&entry).unwrap();

        let entries = store.list_all().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].message, "test feedback");
    }

    #[test]
    fn feedback_store_filter_by_status() {
        let temp = TempDir::new().unwrap();
        let store = FeedbackStore::new(temp.path().join("feedback.jsonl"));

        let entry1 = FeedbackEntry::new("open issue");
        let mut entry2 = FeedbackEntry::new("resolved issue");
        entry2.resolve("fixed");

        store.append(&entry1).unwrap();
        store.append(&entry2).unwrap();

        let open = store.list_by_status(FeedbackStatus::Open).unwrap();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].message, "open issue");
    }

    #[test]
    fn feedback_store_filter_by_tag() {
        let temp = TempDir::new().unwrap();
        let store = FeedbackStore::new(temp.path().join("feedback.jsonl"));

        let entry1 = FeedbackEntry::new("ux issue").with_tags(vec!["ux"]);
        let entry2 = FeedbackEntry::new("perf issue").with_tags(vec!["performance"]);

        store.append(&entry1).unwrap();
        store.append(&entry2).unwrap();

        let ux = store.list_by_tag("ux").unwrap();
        assert_eq!(ux.len(), 1);
        assert_eq!(ux[0].message, "ux issue");
    }

    #[test]
    fn feedback_store_get_by_session() {
        let temp = TempDir::new().unwrap();
        let store = FeedbackStore::new(temp.path().join("feedback.jsonl"));

        let session_id = SessionId::new();
        let entry = FeedbackEntry::new("session feedback").with_session(session_id.clone());

        store.append(&entry).unwrap();

        let session_feedback = store.list_by_session(&session_id).unwrap();
        assert_eq!(session_feedback.len(), 1);
    }

    #[test]
    fn feedback_store_update() {
        let temp = TempDir::new().unwrap();
        let store = FeedbackStore::new(temp.path().join("feedback.jsonl"));

        let entry = FeedbackEntry::new("issue");
        let entry_id = entry.id.clone();
        store.append(&entry).unwrap();

        let found = store
            .update(&entry_id, |e| {
                e.resolve("Fixed!");
            })
            .unwrap();

        assert!(found);

        let entries = store.list_all().unwrap();
        assert_eq!(entries[0].status, FeedbackStatus::Resolved);
        assert_eq!(entries[0].resolution, Some("Fixed!".to_string()));
    }

    #[test]
    fn feedback_store_update_nonexistent() {
        let temp = TempDir::new().unwrap();
        let store = FeedbackStore::new(temp.path().join("feedback.jsonl"));

        let found = store.update("nonexistent", |_| {}).unwrap();
        assert!(!found);
    }

    #[test]
    fn feedback_store_get_by_id() {
        let temp = TempDir::new().unwrap();
        let store = FeedbackStore::new(temp.path().join("feedback.jsonl"));

        let entry = FeedbackEntry::new("test");
        let entry_id = entry.id.clone();
        store.append(&entry).unwrap();

        let found = store.get(&entry_id).unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().message, "test");

        let not_found = store.get("nonexistent").unwrap();
        assert!(not_found.is_none());
    }

    #[test]
    fn feedback_store_empty() {
        let temp = TempDir::new().unwrap();
        let store = FeedbackStore::new(temp.path().join("feedback.jsonl"));

        let entries = store.list_all().unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn feedback_store_multiple_entries() {
        let temp = TempDir::new().unwrap();
        let store = FeedbackStore::new(temp.path().join("feedback.jsonl"));

        for i in 0..5 {
            store
                .append(&FeedbackEntry::new(format!("entry {}", i)))
                .unwrap();
        }

        let entries = store.list_all().unwrap();
        assert_eq!(entries.len(), 5);
    }
}
