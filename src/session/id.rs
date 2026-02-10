//! Session ID generation and parsing.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

/// A unique session identifier.
///
/// Format: `sess_{timestamp_ms}_{random_hex}`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionId {
    timestamp: DateTime<Utc>,
    random: [u8; 8],
}

impl SessionId {
    /// Generate a new session ID.
    pub fn new() -> Self {
        let mut random = [0u8; 8];
        getrandom::getrandom(&mut random).expect("Failed to generate random bytes");

        // Truncate to milliseconds for consistent serialization
        let now = Utc::now();
        let timestamp = DateTime::from_timestamp_millis(now.timestamp_millis()).unwrap_or(now);

        Self { timestamp, random }
    }

    /// Get the session timestamp.
    pub fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    /// Get the ID as a string.
    pub fn as_str(&self) -> String {
        self.to_string()
    }

    /// Parse a session ID from a string.
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.strip_prefix("sess_")?;
        let parts: Vec<&str> = s.split('_').collect();
        if parts.len() != 2 {
            return None;
        }

        let ts_millis: i64 = parts[0].parse().ok()?;
        let timestamp = DateTime::from_timestamp_millis(ts_millis)?;
        let random_hex = parts[1];
        let random_bytes = hex::decode(random_hex).ok()?;
        if random_bytes.len() != 8 {
            return None;
        }

        let mut random = [0u8; 8];
        random.copy_from_slice(&random_bytes);

        Some(Self { timestamp, random })
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "sess_{}_{}",
            self.timestamp.timestamp_millis(),
            hex::encode(self.random)
        )
    }
}

// Custom serialization to store as string
impl Serialize for SessionId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for SessionId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        SessionId::parse(&s).ok_or_else(|| serde::de::Error::custom("Invalid session ID format"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_id_generation() {
        let id1 = SessionId::new();
        let id2 = SessionId::new();

        assert_ne!(id1, id2);
        assert!(id1.as_str().starts_with("sess_"));
    }

    #[test]
    fn session_id_from_string() {
        let id = SessionId::new();
        let s = id.to_string();
        let parsed = SessionId::parse(&s).unwrap();

        assert_eq!(id, parsed);
    }

    #[test]
    fn session_id_timestamp_extraction() {
        let id = SessionId::new();
        let ts = id.timestamp();

        // Should be within last second
        let now = chrono::Utc::now();
        assert!(now.signed_duration_since(ts).num_seconds() < 2);
    }

    #[test]
    fn session_id_display() {
        let id = SessionId::new();
        let display = id.to_string();

        // Format: sess_{timestamp}_{hex}
        assert!(display.starts_with("sess_"));
        let parts: Vec<&str> = display.strip_prefix("sess_").unwrap().split('_').collect();
        assert_eq!(parts.len(), 2);
        // First part should be numeric (timestamp)
        assert!(parts[0].parse::<i64>().is_ok());
        // Second part should be 16 hex chars (8 bytes)
        assert_eq!(parts[1].len(), 16);
    }

    #[test]
    fn session_id_serialization() {
        let id = SessionId::new();
        let json = serde_json::to_string(&id).unwrap();
        let parsed: SessionId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn session_id_parse_invalid() {
        assert!(SessionId::parse("invalid").is_none());
        assert!(SessionId::parse("sess_").is_none());
        assert!(SessionId::parse("sess_123").is_none());
        assert!(SessionId::parse("sess_abc_xyz").is_none());
    }

    #[test]
    fn session_id_default() {
        let id = SessionId::default();
        assert!(id.as_str().starts_with("sess_"));
    }
}
