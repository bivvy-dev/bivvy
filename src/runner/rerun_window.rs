//! Rerun window — how long a previous successful run counts as "recent enough"
//! to consider a step satisfied.
//!
//! The rerun window is a duration that defines when execution history alone is
//! sufficient to skip a step. Outside the window, the step must be re-evaluated
//! via checks or re-run.
//!
//! # Parsing
//!
//! Accepts human-readable duration strings:
//! - `"4h"` — 4 hours
//! - `"30m"` — 30 minutes
//! - `"7d"` — 7 days
//! - `"0"` or `"never"` — execution history never satisfies this step
//! - `"forever"` — a previous successful run always counts
//!
//! # Default
//!
//! The default rerun window is 4 hours.

use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// How long a previous successful run counts as "recent enough" to satisfy a step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RerunWindow {
    /// A fixed duration window. The step is satisfied if it ran successfully
    /// within this duration.
    Duration(std::time::Duration),

    /// Execution history never satisfies this step. It must be satisfied by
    /// checks or `satisfied_when`, or it auto-runs every time.
    Never,

    /// A previous successful run always counts. Equivalent to an infinite window.
    Forever,
}

impl RerunWindow {
    /// Default rerun window: 4 hours.
    pub const DEFAULT_HOURS: u64 = 4;

    /// Check whether a timestamp falls within this rerun window.
    ///
    /// Returns `true` if the given `recorded_at` time is recent enough to be
    /// considered valid according to this window.
    pub fn is_within_window(&self, recorded_at: DateTime<Utc>) -> bool {
        match self {
            RerunWindow::Never => false,
            RerunWindow::Forever => true,
            RerunWindow::Duration(dur) => {
                let elapsed = Utc::now().signed_duration_since(recorded_at);
                // If recorded_at is in the future (clock skew), treat as within window
                if elapsed.num_milliseconds() < 0 {
                    return true;
                }
                let elapsed_std = elapsed.to_std().unwrap_or(std::time::Duration::MAX);
                elapsed_std <= *dur
            }
        }
    }
}

impl Default for RerunWindow {
    fn default() -> Self {
        RerunWindow::Duration(std::time::Duration::from_secs(Self::DEFAULT_HOURS * 3600))
    }
}

impl FromStr for RerunWindow {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        match s {
            "0" | "never" => return Ok(RerunWindow::Never),
            "forever" => return Ok(RerunWindow::Forever),
            _ => {}
        }

        // Parse duration strings like "4h", "30m", "7d"
        if s.is_empty() {
            return Err("empty rerun window string".to_string());
        }

        let (num_str, suffix) = s.split_at(s.len() - 1);
        let num: u64 = num_str
            .parse()
            .map_err(|_| format!("invalid rerun window '{}': expected a number followed by h/m/d/s (e.g., '4h', '30m', '7d')", s))?;

        let secs = match suffix {
            "s" => num,
            "m" => num * 60,
            "h" => num * 3600,
            "d" => num * 86400,
            _ => {
                return Err(format!(
                    "invalid rerun window '{}': unknown suffix '{}', expected h/m/d/s",
                    s, suffix
                ))
            }
        };

        Ok(RerunWindow::Duration(std::time::Duration::from_secs(secs)))
    }
}

impl fmt::Display for RerunWindow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RerunWindow::Never => write!(f, "never"),
            RerunWindow::Forever => write!(f, "forever"),
            RerunWindow::Duration(dur) => {
                let secs = dur.as_secs();
                if secs == 0 {
                    write!(f, "0s")
                } else if secs % 86400 == 0 {
                    write!(f, "{}d", secs / 86400)
                } else if secs % 3600 == 0 {
                    write!(f, "{}h", secs / 3600)
                } else if secs % 60 == 0 {
                    write!(f, "{}m", secs / 60)
                } else {
                    write!(f, "{}s", secs)
                }
            }
        }
    }
}

impl Serialize for RerunWindow {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for RerunWindow {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        RerunWindow::from_str(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn parse_hours() {
        let w = RerunWindow::from_str("4h").unwrap();
        assert_eq!(
            w,
            RerunWindow::Duration(std::time::Duration::from_secs(4 * 3600))
        );
    }

    #[test]
    fn parse_minutes() {
        let w = RerunWindow::from_str("30m").unwrap();
        assert_eq!(
            w,
            RerunWindow::Duration(std::time::Duration::from_secs(30 * 60))
        );
    }

    #[test]
    fn parse_days() {
        let w = RerunWindow::from_str("7d").unwrap();
        assert_eq!(
            w,
            RerunWindow::Duration(std::time::Duration::from_secs(7 * 86400))
        );
    }

    #[test]
    fn parse_seconds() {
        let w = RerunWindow::from_str("120s").unwrap();
        assert_eq!(
            w,
            RerunWindow::Duration(std::time::Duration::from_secs(120))
        );
    }

    #[test]
    fn parse_zero() {
        assert_eq!(RerunWindow::from_str("0").unwrap(), RerunWindow::Never);
    }

    #[test]
    fn parse_never() {
        assert_eq!(RerunWindow::from_str("never").unwrap(), RerunWindow::Never);
    }

    #[test]
    fn parse_forever() {
        assert_eq!(
            RerunWindow::from_str("forever").unwrap(),
            RerunWindow::Forever
        );
    }

    #[test]
    fn parse_invalid_empty() {
        assert!(RerunWindow::from_str("").is_err());
    }

    #[test]
    fn parse_invalid_no_suffix() {
        assert!(RerunWindow::from_str("42").is_err());
    }

    #[test]
    fn parse_invalid_suffix() {
        assert!(RerunWindow::from_str("4x").is_err());
    }

    #[test]
    fn parse_invalid_non_numeric() {
        assert!(RerunWindow::from_str("abch").is_err());
    }

    #[test]
    fn default_is_four_hours() {
        let w = RerunWindow::default();
        assert_eq!(
            w,
            RerunWindow::Duration(std::time::Duration::from_secs(4 * 3600))
        );
    }

    #[test]
    fn display_hours() {
        let w = RerunWindow::Duration(std::time::Duration::from_secs(4 * 3600));
        assert_eq!(w.to_string(), "4h");
    }

    #[test]
    fn display_days() {
        let w = RerunWindow::Duration(std::time::Duration::from_secs(7 * 86400));
        assert_eq!(w.to_string(), "7d");
    }

    #[test]
    fn display_minutes() {
        let w = RerunWindow::Duration(std::time::Duration::from_secs(30 * 60));
        assert_eq!(w.to_string(), "30m");
    }

    #[test]
    fn display_seconds() {
        let w = RerunWindow::Duration(std::time::Duration::from_secs(45));
        assert_eq!(w.to_string(), "45s");
    }

    #[test]
    fn display_zero_seconds() {
        let w = RerunWindow::Duration(std::time::Duration::from_secs(0));
        assert_eq!(w.to_string(), "0s");
    }

    #[test]
    fn display_never() {
        assert_eq!(RerunWindow::Never.to_string(), "never");
    }

    #[test]
    fn display_forever() {
        assert_eq!(RerunWindow::Forever.to_string(), "forever");
    }

    #[test]
    fn within_window_recent_timestamp() {
        let w = RerunWindow::Duration(std::time::Duration::from_secs(4 * 3600));
        let recorded = Utc::now() - Duration::hours(1);
        assert!(w.is_within_window(recorded));
    }

    #[test]
    fn outside_window_old_timestamp() {
        let w = RerunWindow::Duration(std::time::Duration::from_secs(4 * 3600));
        let recorded = Utc::now() - Duration::hours(5);
        assert!(!w.is_within_window(recorded));
    }

    #[test]
    fn within_window_at_boundary() {
        // At exactly the boundary, should be within window (<=)
        let w = RerunWindow::Duration(std::time::Duration::from_secs(3600));
        // Use a timestamp slightly less than 1 hour ago to avoid timing flakiness
        let recorded = Utc::now() - Duration::seconds(3599);
        assert!(w.is_within_window(recorded));
    }

    #[test]
    fn never_always_outside() {
        let recorded = Utc::now(); // just now
        assert!(!RerunWindow::Never.is_within_window(recorded));
    }

    #[test]
    fn forever_always_within() {
        let recorded = Utc::now() - Duration::days(365 * 10); // 10 years ago
        assert!(RerunWindow::Forever.is_within_window(recorded));
    }

    #[test]
    fn future_timestamp_within_window() {
        // Clock skew: recorded_at is in the future
        let w = RerunWindow::Duration(std::time::Duration::from_secs(3600));
        let recorded = Utc::now() + Duration::hours(1);
        assert!(w.is_within_window(recorded));
    }

    #[test]
    fn serde_round_trip_duration() {
        let w = RerunWindow::Duration(std::time::Duration::from_secs(4 * 3600));
        let json = serde_json::to_string(&w).unwrap();
        assert_eq!(json, "\"4h\"");
        let parsed: RerunWindow = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, w);
    }

    #[test]
    fn serde_round_trip_never() {
        let w = RerunWindow::Never;
        let json = serde_json::to_string(&w).unwrap();
        assert_eq!(json, "\"never\"");
        let parsed: RerunWindow = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, w);
    }

    #[test]
    fn serde_round_trip_forever() {
        let w = RerunWindow::Forever;
        let json = serde_json::to_string(&w).unwrap();
        assert_eq!(json, "\"forever\"");
        let parsed: RerunWindow = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, w);
    }

    #[test]
    fn serde_yaml_round_trip() {
        let w = RerunWindow::Duration(std::time::Duration::from_secs(24 * 3600));
        let yaml = serde_yaml::to_string(&w).unwrap();
        let parsed: RerunWindow = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed, w);
    }

    #[test]
    fn parse_with_whitespace() {
        let w = RerunWindow::from_str("  4h  ").unwrap();
        assert_eq!(
            w,
            RerunWindow::Duration(std::time::Duration::from_secs(4 * 3600))
        );
    }
}
