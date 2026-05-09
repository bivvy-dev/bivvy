//! Session ID generation and parsing.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

/// Why a session id string could not be parsed.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SessionIdParseError {
    /// The string did not start with the `sess_` prefix.
    #[error("missing 'sess_' prefix")]
    MissingPrefix,
    /// The remainder did not have exactly two `_`-separated parts.
    #[error("expected `sess_<timestamp>_<hex>` (got {found} part(s))")]
    WrongPartCount { found: usize },
    /// The timestamp portion was not a valid `i64` milliseconds value.
    #[error("invalid timestamp: {0}")]
    InvalidTimestamp(String),
    /// The random portion was not valid hex.
    #[error("invalid hex random: {0}")]
    InvalidHex(String),
    /// The decoded random bytes were not exactly 8 long.
    #[error("expected 8 random bytes (got {found})")]
    WrongRandomLength { found: usize },
}

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
        crate::sys::random_bytes(&mut random)
            .expect("BUG: platform RNG unavailable; session ID cannot be generated");

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
    ///
    /// Returns `None` for any malformed input. Logs the structured cause
    /// at `debug` so a corrupt id on disk leaves a forensic trail without
    /// changing the caller's `Option`-returning contract. Use [`try_parse`]
    /// to inspect the cause directly.
    ///
    /// [`try_parse`]: Self::try_parse
    pub fn parse(s: &str) -> Option<Self> {
        match Self::try_parse(s) {
            Ok(id) => Some(id),
            Err(e) => {
                tracing::debug!("invalid session id {:?}: {}", s, e);
                None
            }
        }
    }

    /// Parse a session ID from a string, returning the structured cause
    /// of any failure.
    pub fn try_parse(s: &str) -> Result<Self, SessionIdParseError> {
        let s = s
            .strip_prefix("sess_")
            .ok_or(SessionIdParseError::MissingPrefix)?;
        let parts: Vec<&str> = s.split('_').collect();
        if parts.len() != 2 {
            return Err(SessionIdParseError::WrongPartCount { found: parts.len() });
        }

        let ts_millis: i64 = parts[0]
            .parse()
            .map_err(|_| SessionIdParseError::InvalidTimestamp(parts[0].to_string()))?;
        let timestamp = DateTime::from_timestamp_millis(ts_millis)
            .ok_or_else(|| SessionIdParseError::InvalidTimestamp(parts[0].to_string()))?;
        let random_hex = parts[1];
        let random_bytes = hex::decode(random_hex)
            .map_err(|_| SessionIdParseError::InvalidHex(random_hex.to_string()))?;
        if random_bytes.len() != 8 {
            return Err(SessionIdParseError::WrongRandomLength {
                found: random_bytes.len(),
            });
        }

        let mut random = [0u8; 8];
        random.copy_from_slice(&random_bytes);

        Ok(Self { timestamp, random })
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
        SessionId::try_parse(&s)
            .map_err(|e| serde::de::Error::custom(format!("Invalid session ID {s:?}: {e}")))
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

    // --- M8: structured parse errors via try_parse ---

    #[test]
    fn try_parse_invalid_prefix_returns_err() {
        let err = SessionId::try_parse("nope_123_abcd").unwrap_err();
        assert_eq!(err, SessionIdParseError::MissingPrefix);
    }

    #[test]
    fn try_parse_wrong_part_count_returns_err() {
        // No underscore after sess_ → one part
        match SessionId::try_parse("sess_123abc").unwrap_err() {
            SessionIdParseError::WrongPartCount { found } => assert_eq!(found, 1),
            other => panic!("expected WrongPartCount, got {other:?}"),
        }
        // Three parts
        match SessionId::try_parse("sess_123_abc_extra").unwrap_err() {
            SessionIdParseError::WrongPartCount { found } => assert_eq!(found, 3),
            other => panic!("expected WrongPartCount, got {other:?}"),
        }
    }

    #[test]
    fn try_parse_invalid_timestamp_returns_err() {
        match SessionId::try_parse("sess_notanumber_abcdef0123456789").unwrap_err() {
            SessionIdParseError::InvalidTimestamp(s) => assert_eq!(s, "notanumber"),
            other => panic!("expected InvalidTimestamp, got {other:?}"),
        }
    }

    #[test]
    fn try_parse_invalid_hex_returns_err() {
        match SessionId::try_parse("sess_1700000000000_zzznothex").unwrap_err() {
            SessionIdParseError::InvalidHex(s) => assert_eq!(s, "zzznothex"),
            other => panic!("expected InvalidHex, got {other:?}"),
        }
    }

    #[test]
    fn try_parse_wrong_random_length_returns_err() {
        // Hex decodes but isn't 8 bytes (4 hex chars = 2 bytes).
        match SessionId::try_parse("sess_1700000000000_abcd").unwrap_err() {
            SessionIdParseError::WrongRandomLength { found } => assert_eq!(found, 2),
            other => panic!("expected WrongRandomLength, got {other:?}"),
        }
    }

    #[test]
    fn try_parse_round_trips_with_parse() {
        let id = SessionId::new();
        let s = id.to_string();
        let parsed = SessionId::try_parse(&s).unwrap();
        assert_eq!(id, parsed);
    }
}
