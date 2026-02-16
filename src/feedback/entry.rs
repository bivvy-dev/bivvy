//! Feedback entry types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::session::SessionId;

/// A feedback entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackEntry {
    /// Unique feedback ID.
    pub id: String,
    /// Timestamp when feedback was created.
    pub timestamp: Option<DateTime<Utc>>,
    /// The feedback message.
    pub message: String,
    /// Tags for categorization.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Associated session ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    /// Feedback status.
    #[serde(default)]
    pub status: FeedbackStatus,
    /// Resolution note (if resolved).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
    /// Resolution timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<DateTime<Utc>>,
    /// Whether this entry was delivered externally.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivered: Option<bool>,
}

/// Feedback status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FeedbackStatus {
    /// Feedback is open and needs attention.
    #[default]
    Open,
    /// Feedback is being worked on.
    InProgress,
    /// Feedback has been resolved.
    Resolved,
    /// Feedback won't be addressed.
    WontFix,
}

impl FeedbackEntry {
    /// Create a new feedback entry.
    pub fn new(message: impl Into<String>) -> Self {
        let mut id_bytes = [0u8; 6];
        getrandom::getrandom(&mut id_bytes).expect("Failed to generate random bytes");

        Self {
            id: format!("fb_{}", hex::encode(id_bytes)),
            timestamp: Some(Utc::now()),
            message: message.into(),
            tags: Vec::new(),
            session_id: None,
            status: FeedbackStatus::Open,
            resolution: None,
            resolved_at: None,
            delivered: None,
        }
    }

    /// Attach a session ID.
    pub fn with_session(mut self, session_id: SessionId) -> Self {
        self.session_id = Some(session_id);
        self
    }

    /// Add tags.
    pub fn with_tags(mut self, tags: Vec<impl Into<String>>) -> Self {
        self.tags = tags.into_iter().map(Into::into).collect();
        self
    }

    /// Mark as resolved.
    pub fn resolve(&mut self, resolution: impl Into<String>) {
        self.status = FeedbackStatus::Resolved;
        self.resolution = Some(resolution.into());
        self.resolved_at = Some(Utc::now());
    }

    /// Mark as won't fix.
    pub fn wont_fix(&mut self, reason: impl Into<String>) {
        self.status = FeedbackStatus::WontFix;
        self.resolution = Some(reason.into());
        self.resolved_at = Some(Utc::now());
    }

    /// Mark as in progress.
    pub fn start_progress(&mut self) {
        self.status = FeedbackStatus::InProgress;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feedback_entry_creation() {
        let entry = FeedbackEntry::new("the error message was confusing");

        assert_eq!(entry.message, "the error message was confusing");
        assert!(entry.id.starts_with("fb_"));
        assert!(entry.timestamp.is_some());
    }

    #[test]
    fn feedback_entry_with_session() {
        let session_id = SessionId::new();
        let entry = FeedbackEntry::new("error unclear").with_session(session_id.clone());

        assert_eq!(entry.session_id, Some(session_id));
    }

    #[test]
    fn feedback_entry_with_tags() {
        let entry = FeedbackEntry::new("slow startup").with_tags(vec!["performance", "ux"]);

        assert_eq!(entry.tags, vec!["performance", "ux"]);
    }

    #[test]
    fn feedback_entry_status() {
        let mut entry = FeedbackEntry::new("issue");
        assert_eq!(entry.status, FeedbackStatus::Open);

        entry.resolve("Fixed in commit abc123");
        assert_eq!(entry.status, FeedbackStatus::Resolved);
        assert!(entry.resolution.is_some());
    }

    #[test]
    fn feedback_entry_wont_fix() {
        let mut entry = FeedbackEntry::new("issue");
        entry.wont_fix("Working as intended");

        assert_eq!(entry.status, FeedbackStatus::WontFix);
        assert_eq!(entry.resolution, Some("Working as intended".to_string()));
        assert!(entry.resolved_at.is_some());
    }

    #[test]
    fn feedback_entry_in_progress() {
        let mut entry = FeedbackEntry::new("issue");
        entry.start_progress();

        assert_eq!(entry.status, FeedbackStatus::InProgress);
    }

    #[test]
    fn feedback_entry_id_format() {
        let entry = FeedbackEntry::new("test");
        // ID should be fb_ followed by 12 hex chars (6 bytes)
        assert!(entry.id.starts_with("fb_"));
        assert_eq!(entry.id.len(), 3 + 12); // "fb_" + 12 hex chars
    }

    #[test]
    fn feedback_status_default() {
        let status = FeedbackStatus::default();
        assert_eq!(status, FeedbackStatus::Open);
    }

    #[test]
    fn feedback_entry_serialization() {
        let entry = FeedbackEntry::new("test message").with_tags(vec!["tag1", "tag2"]);

        let json = serde_json::to_string(&entry).unwrap();
        let parsed: FeedbackEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.message, "test message");
        assert_eq!(parsed.tags, vec!["tag1", "tag2"]);
    }
}
