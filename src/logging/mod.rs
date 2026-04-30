//! Structured event logging for bivvy sessions.
//!
//! Records every event during a bivvy session as a JSONL (JSON Lines) entry.
//! One file per session. Logs are stored in `~/.bivvy/logs/` with automatic
//! retention management (age + size limits).
//!
//! # Architecture
//!
//! The event logger is one of three independent event consumers:
//!
//! 1. **Event logger** (this module) — writes all events to JSONL for
//!    debugging, feedback, and auditing
//! 2. **State recorder** — updates persistent state on step completion
//! 3. **Presenter** — shows real-time terminal output
//!
//! Each consumer handles only the events it cares about. They don't know
//! about each other.
//!
//! # Log Format
//!
//! Each line is a JSON object with:
//! - `ts` — ISO 8601 timestamp with milliseconds
//! - `session` — unique session ID
//! - `event` — the event data (tagged by `type`)
//!
//! # Retention
//!
//! Logs expire automatically. Default: 30 days or 500 MB total, whichever
//! comes first. Cleanup runs at the start of each session.

pub mod bus;
pub mod events;

pub use bus::EventBus;
pub use events::{
    BehaviorFlags, BivvyEvent, DecisionTrace, DependencyStatus, EventConsumer, FilterResult,
    InputMethod, NamedCheckResult, RequirementGapInfo, RerunInfo, SatisfactionResult,
    StepOutcomeKind, TraceCheckResult,
};

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::Serialize;

/// Default log retention: 30 days.
pub const DEFAULT_RETENTION_DAYS: u32 = 30;

/// Default log retention: 500 MB.
pub const DEFAULT_RETENTION_MB: u64 = 500;

/// Get the default log directory path.
///
/// Returns `~/.bivvy/logs/`.
pub fn default_log_dir() -> PathBuf {
    crate::sys::home_dir()
        .unwrap_or_else(|| PathBuf::from("~"))
        .join(".bivvy")
        .join("logs")
}

/// Extract the `working_directory` recorded in a log file's
/// `session_started` event.
///
/// Returns `None` if the file cannot be read, contains no `session_started`
/// event, or that event does not carry a `working_directory` field. Used by
/// `bivvy last` and `bivvy history` to scope log scanning to the current
/// project.
pub fn log_working_directory(log_path: &Path) -> Option<PathBuf> {
    let content = std::fs::read_to_string(log_path).ok()?;
    for line in content.lines() {
        let value: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if value.get("type").and_then(|t| t.as_str()) == Some("session_started") {
            return value
                .get("working_directory")
                .and_then(|w| w.as_str())
                .map(PathBuf::from);
        }
    }
    None
}

/// List all JSONL log files in `log_dir` that belong to `canonical_project`.
///
/// Order is unspecified. Returns an empty vector if the directory does not
/// exist or cannot be read. Used by `bivvy history --clear` to enumerate
/// the files that should be deleted for the current project, leaving logs
/// from other projects untouched.
pub fn list_project_logs(log_dir: &Path, canonical_project: &Path) -> Vec<PathBuf> {
    if !log_dir.exists() {
        return Vec::new();
    }
    fs::read_dir(log_dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "jsonl"))
        .filter(|p| log_belongs_to_project(p, canonical_project))
        .collect()
}

/// Test whether a log file belongs to the project rooted at `canonical_project`.
///
/// Compares the canonical project path against the `working_directory`
/// recorded in the log's `session_started` event. Falls back to a literal
/// path comparison when the recorded directory no longer exists (e.g. logs
/// from temp-dir tests or moved projects).
pub fn log_belongs_to_project(log_path: &Path, canonical_project: &Path) -> bool {
    let wd = match log_working_directory(log_path) {
        Some(p) => p,
        None => return false,
    };
    if wd == canonical_project {
        return true;
    }
    matches!(wd.canonicalize(), Ok(canon) if canon == canonical_project)
}

/// A single JSONL log entry.
#[derive(Debug, Serialize)]
struct LogEntry<'a> {
    /// ISO 8601 timestamp with milliseconds.
    ts: String,
    /// Session ID.
    session: &'a str,
    /// The event data.
    #[serde(flatten)]
    event: &'a BivvyEvent,
}

/// Retention policy for log files.
#[derive(Debug, Clone)]
pub struct RetentionPolicy {
    /// Maximum age in days. Files older than this are deleted.
    pub max_age_days: u32,
    /// Maximum total size in megabytes. Oldest files are deleted first
    /// when the total exceeds this.
    pub max_size_mb: u64,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            max_age_days: DEFAULT_RETENTION_DAYS,
            max_size_mb: DEFAULT_RETENTION_MB,
        }
    }
}

/// Structured event logger that writes JSONL to disk.
///
/// Created at the start of every bivvy session. Writes synchronously
/// (the overhead of one JSON line per event is negligible). The file
/// is flushed when [`EventLogger::flush`] is called or on drop.
///
/// Steps marked `sensitive: true` have their command and output lines
/// redacted in the log (replaced with `"[SENSITIVE]"` and `"[REDACTED]"`).
///
/// # Usage
///
/// ```no_run
/// use bivvy::logging::{EventLogger, BivvyEvent, RetentionPolicy};
///
/// let logger = EventLogger::new(
///     "/tmp/bivvy-logs",
///     "sess_123_abc",
///     RetentionPolicy::default(),
/// ).unwrap();
/// ```
pub struct EventLogger {
    writer: BufWriter<File>,
    session_id: String,
    log_path: PathBuf,
    /// Step names that are marked `sensitive: true` in the config.
    /// Events for these steps have their content redacted.
    sensitive_steps: std::collections::HashSet<String>,
}

impl EventLogger {
    /// Create a new event logger for a session.
    ///
    /// Creates the log directory if needed, runs retention cleanup, and
    /// opens a new JSONL file for this session.
    ///
    /// The log file is named `{timestamp}_{session_suffix}.jsonl` where
    /// the timestamp is ISO 8601 (with dashes instead of colons for
    /// filesystem compatibility) and the session suffix provides uniqueness.
    pub fn new(
        log_dir: impl AsRef<Path>,
        session_id: &str,
        retention: RetentionPolicy,
    ) -> std::io::Result<Self> {
        let log_dir = log_dir.as_ref();
        fs::create_dir_all(log_dir)?;

        // Run retention cleanup before starting the new session
        if let Err(e) = cleanup_logs(log_dir, &retention) {
            tracing::debug!("Log retention cleanup failed: {}", e);
        }

        // Generate filename: 2026-04-25T10-00-00_{session_suffix}.jsonl
        let now = Utc::now();
        let timestamp = now.format("%Y-%m-%dT%H-%M-%S").to_string();

        // Extract a short suffix from the session ID for uniqueness
        let suffix = session_id
            .strip_prefix("sess_")
            .and_then(|s| s.rsplit('_').next())
            .unwrap_or(session_id);
        let filename = format!("{}_{}.jsonl", timestamp, suffix);
        let log_path = log_dir.join(&filename);

        let file = File::create(&log_path)?;
        let writer = BufWriter::new(file);

        Ok(Self {
            writer,
            session_id: session_id.to_string(),
            log_path,
            sensitive_steps: std::collections::HashSet::new(),
        })
    }

    /// Returns the path to the log file.
    pub fn log_path(&self) -> &Path {
        &self.log_path
    }

    /// Register step names that should have their content redacted in logs.
    ///
    /// Call this after loading the config, before any events are emitted.
    pub fn set_sensitive_steps(&mut self, steps: impl IntoIterator<Item = String>) {
        self.sensitive_steps = steps.into_iter().collect();
    }

    /// Flush the log buffer to disk.
    pub fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }

    /// Returns true if the given step name is marked sensitive.
    fn is_sensitive(&self, step_name: &str) -> bool {
        self.sensitive_steps.contains(step_name)
    }

    /// Redact sensitive content in an event, returning a new event if
    /// redaction was needed or None if the event can be logged as-is.
    fn redact_event(&self, event: &BivvyEvent) -> Option<BivvyEvent> {
        match event {
            BivvyEvent::StepOutput { name, stream, .. } if self.is_sensitive(name) => {
                Some(BivvyEvent::StepOutput {
                    name: name.clone(),
                    stream: stream.clone(),
                    line: "[REDACTED]".to_string(),
                })
            }
            BivvyEvent::StepCompleted {
                name,
                success,
                exit_code,
                duration_ms,
                error,
            } if self.is_sensitive(name) => Some(BivvyEvent::StepCompleted {
                name: name.clone(),
                success: *success,
                exit_code: *exit_code,
                duration_ms: *duration_ms,
                error: error.as_ref().map(|_| "[REDACTED]".to_string()),
            }),
            BivvyEvent::RecoveryStarted { step, .. } if self.is_sensitive(step) => {
                Some(BivvyEvent::RecoveryStarted {
                    step: step.clone(),
                    error: "[REDACTED]".to_string(),
                })
            }
            BivvyEvent::RecoveryActionTaken {
                step,
                action,
                command,
            } if self.is_sensitive(step) => Some(BivvyEvent::RecoveryActionTaken {
                step: step.clone(),
                action: action.clone(),
                command: command.as_ref().map(|_| "[SENSITIVE]".to_string()),
            }),
            _ => None,
        }
    }

    /// Write a single event as a JSONL line.
    fn write_event(&mut self, event: &BivvyEvent) {
        let entry = LogEntry {
            ts: Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
            session: &self.session_id,
            event,
        };

        // Write JSON + newline. Errors are logged but don't propagate —
        // logging failures should never break the main workflow.
        if let Err(e) = serde_json::to_writer(&mut self.writer, &entry) {
            tracing::debug!("Failed to write event to log: {}", e);
            return;
        }
        if let Err(e) = writeln!(self.writer) {
            tracing::debug!("Failed to write newline to log: {}", e);
        }
    }
}

impl EventConsumer for EventLogger {
    fn on_event(&mut self, event: &BivvyEvent) {
        if let Some(redacted) = self.redact_event(event) {
            self.write_event(&redacted);
        } else {
            self.write_event(event);
        }
    }
}

impl Drop for EventLogger {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

/// Clean up expired log files based on the retention policy.
///
/// Deletes files that are:
/// 1. Older than `max_age_days`
/// 2. Excess files when total size exceeds `max_size_mb` (oldest first)
///
/// Only targets `.jsonl` files in the log directory.
pub fn cleanup_logs(log_dir: &Path, policy: &RetentionPolicy) -> std::io::Result<()> {
    if !log_dir.exists() {
        return Ok(());
    }

    let max_age = chrono::Duration::days(i64::from(policy.max_age_days));
    let max_bytes = policy.max_size_mb * 1024 * 1024;
    let now = Utc::now();

    // Collect all .jsonl files with their metadata
    let mut log_files: Vec<(PathBuf, fs::Metadata)> = Vec::new();
    for entry in fs::read_dir(log_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "jsonl") {
            if let Ok(meta) = entry.metadata() {
                log_files.push((path, meta));
            }
        }
    }

    // Phase 1: Delete files older than max_age_days
    let mut remaining: Vec<(PathBuf, u64)> = Vec::new();
    for (path, meta) in &log_files {
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| chrono::DateTime::<Utc>::from(t).into());

        let is_expired = modified
            .map(|m: chrono::DateTime<Utc>| now.signed_duration_since(m) > max_age)
            .unwrap_or(false);

        if is_expired {
            tracing::debug!("Deleting expired log: {}", path.display());
            let _ = fs::remove_file(path);
        } else {
            remaining.push((path.clone(), meta.len()));
        }
    }

    // Phase 2: If total size exceeds limit, delete oldest files first
    let total_size: u64 = remaining.iter().map(|(_, size)| size).sum();
    if total_size > max_bytes {
        // Sort by modification time (oldest first)
        remaining.sort_by(|(a, _), (b, _)| {
            let a_time = fs::metadata(a).and_then(|m| m.modified()).ok();
            let b_time = fs::metadata(b).and_then(|m| m.modified()).ok();
            a_time.cmp(&b_time)
        });

        let mut current_size = total_size;
        for (path, size) in &remaining {
            if current_size <= max_bytes {
                break;
            }
            tracing::debug!(
                "Deleting log for size limit ({} MB > {} MB): {}",
                current_size / (1024 * 1024),
                policy.max_size_mb,
                path.display()
            );
            if fs::remove_file(path).is_ok() {
                current_size = current_size.saturating_sub(*size);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn event_logger_creates_log_file() {
        let dir = TempDir::new().unwrap();
        let logger =
            EventLogger::new(dir.path(), "sess_123_abcdef01", RetentionPolicy::default()).unwrap();

        assert!(logger.log_path().exists());
        assert!(logger
            .log_path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .ends_with("_abcdef01.jsonl"));
    }

    #[test]
    fn event_logger_creates_directory_if_missing() {
        let dir = TempDir::new().unwrap();
        let nested = dir.path().join("nested").join("logs");
        let logger =
            EventLogger::new(&nested, "sess_123_abcdef01", RetentionPolicy::default()).unwrap();

        assert!(nested.exists());
        assert!(logger.log_path().exists());
    }

    #[test]
    fn event_logger_writes_jsonl() {
        let dir = TempDir::new().unwrap();
        let mut logger =
            EventLogger::new(dir.path(), "sess_123_abcdef01", RetentionPolicy::default()).unwrap();

        logger.on_event(&BivvyEvent::SessionStarted {
            command: "run".to_string(),
            args: vec!["--verbose".to_string()],
            version: "1.9.0".to_string(),
            os: None,
            working_directory: None,
        });
        logger.on_event(&BivvyEvent::StepStarting {
            name: "build".to_string(),
        });
        logger.on_event(&BivvyEvent::SessionEnded {
            exit_code: 0,
            duration_ms: 5000,
        });
        logger.flush().unwrap();

        let content = fs::read_to_string(logger.log_path()).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 3);

        // Verify each line is valid JSON with expected fields
        for line in &lines {
            let value: serde_json::Value = serde_json::from_str(line).unwrap();
            assert!(value.get("ts").is_some(), "Missing 'ts' field");
            assert_eq!(value["session"], "sess_123_abcdef01");
            assert!(value.get("type").is_some(), "Missing 'type' field");
        }

        // Verify first event is session_started
        let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(first["type"], "session_started");
        assert_eq!(first["command"], "run");

        // Verify last event is session_ended
        let last: serde_json::Value = serde_json::from_str(lines[2]).unwrap();
        assert_eq!(last["type"], "session_ended");
        assert_eq!(last["exit_code"], 0);
    }

    #[test]
    fn event_logger_flushes_on_drop() {
        let dir = TempDir::new().unwrap();
        let log_path;
        {
            let mut logger =
                EventLogger::new(dir.path(), "sess_123_abcdef01", RetentionPolicy::default())
                    .unwrap();
            logger.on_event(&BivvyEvent::SessionStarted {
                command: "lint".to_string(),
                args: vec![],
                version: "1.0.0".to_string(),
                os: None,
                working_directory: None,
            });
            log_path = logger.log_path().to_path_buf();
            // logger drops here
        }

        let content = fs::read_to_string(&log_path).unwrap();
        assert!(!content.is_empty());
        let value: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(value["type"], "session_started");
    }

    #[test]
    fn log_entry_format_has_ts_session_and_event() {
        let dir = TempDir::new().unwrap();
        let mut logger =
            EventLogger::new(dir.path(), "sess_999_deadbeef", RetentionPolicy::default()).unwrap();

        logger.on_event(&BivvyEvent::StepCompleted {
            name: "build".to_string(),
            success: true,
            exit_code: Some(0),
            duration_ms: 1234,
            error: None,
        });
        logger.flush().unwrap();

        let content = fs::read_to_string(logger.log_path()).unwrap();
        let value: serde_json::Value = serde_json::from_str(content.trim()).unwrap();

        // ts is ISO 8601
        let ts = value["ts"].as_str().unwrap();
        assert!(ts.ends_with('Z'), "Timestamp should end with Z: {}", ts);
        assert!(ts.contains('T'), "Timestamp should contain T: {}", ts);

        // session is the session ID
        assert_eq!(value["session"], "sess_999_deadbeef");

        // event fields are flattened into the top level
        assert_eq!(value["type"], "step_completed");
        assert_eq!(value["name"], "build");
        assert_eq!(value["success"], true);
    }

    #[test]
    fn cleanup_deletes_old_files() {
        let dir = TempDir::new().unwrap();

        // Create a fake old log file
        let old_file = dir.path().join("2020-01-01T00-00-00_old.jsonl");
        fs::write(&old_file, "{}\n").unwrap();

        // Set modification time to the past by creating a new file (the old
        // one will have "now" mtime, so we use a very short retention)
        let policy = RetentionPolicy {
            max_age_days: 0, // Expire immediately
            max_size_mb: 500,
        };

        cleanup_logs(dir.path(), &policy).unwrap();
        assert!(!old_file.exists(), "Old log file should have been deleted");
    }

    #[test]
    fn cleanup_respects_size_limit() {
        let dir = TempDir::new().unwrap();

        // Create files that together exceed the size limit.
        // Each file is 600 bytes, limit is 1 KB.
        let data = "x".repeat(600);
        for i in 0..3 {
            let path = dir
                .path()
                .join(format!("2026-04-{:02}T00-00-00_{}.jsonl", i + 1, i));
            fs::write(&path, &data).unwrap();
            // Small delay to ensure different mtimes
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let policy = RetentionPolicy {
            max_age_days: 365, // Don't expire by age
            max_size_mb: 0,    // 0 MB limit — should delete until under
        };

        cleanup_logs(dir.path(), &policy).unwrap();

        // All files should be deleted since 0 MB limit
        let remaining: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "jsonl"))
            .collect();
        assert_eq!(
            remaining.len(),
            0,
            "All files should be deleted with 0 MB limit"
        );
    }

    #[test]
    fn cleanup_ignores_non_jsonl_files() {
        let dir = TempDir::new().unwrap();

        let txt_file = dir.path().join("notes.txt");
        fs::write(&txt_file, "keep me").unwrap();

        let policy = RetentionPolicy {
            max_age_days: 0, // Expire everything
            max_size_mb: 0,
        };

        cleanup_logs(dir.path(), &policy).unwrap();
        assert!(txt_file.exists(), "Non-JSONL files should be preserved");
    }

    #[test]
    fn cleanup_handles_empty_directory() {
        let dir = TempDir::new().unwrap();
        let policy = RetentionPolicy::default();
        cleanup_logs(dir.path(), &policy).unwrap();
        // No error
    }

    #[test]
    fn cleanup_handles_nonexistent_directory() {
        let policy = RetentionPolicy::default();
        cleanup_logs(Path::new("/tmp/bivvy-nonexistent-dir-test"), &policy).unwrap();
        // No error
    }

    #[test]
    fn default_log_dir_ends_with_bivvy_logs() {
        let dir = default_log_dir();
        assert!(dir.ends_with(".bivvy/logs"));
    }

    #[test]
    fn retention_policy_defaults() {
        let policy = RetentionPolicy::default();
        assert_eq!(policy.max_age_days, 30);
        assert_eq!(policy.max_size_mb, 500);
    }

    #[test]
    fn multiple_events_produce_separate_lines() {
        let dir = TempDir::new().unwrap();
        let mut logger =
            EventLogger::new(dir.path(), "sess_123_aabbccdd", RetentionPolicy::default()).unwrap();

        for i in 0..10 {
            logger.on_event(&BivvyEvent::StepPlanned {
                name: format!("step_{}", i),
                index: i,
                total: 10,
            });
        }
        logger.flush().unwrap();

        let content = fs::read_to_string(logger.log_path()).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 10);

        // Each line should be independently parseable JSON
        for (i, line) in lines.iter().enumerate() {
            let value: serde_json::Value = serde_json::from_str(line).unwrap();
            assert_eq!(value["type"], "step_planned");
            assert_eq!(value["name"], format!("step_{}", i));
            assert_eq!(value["index"], i);
        }
    }

    #[test]
    fn session_id_without_sess_prefix_still_works() {
        let dir = TempDir::new().unwrap();
        let logger =
            EventLogger::new(dir.path(), "custom_id_123", RetentionPolicy::default()).unwrap();

        // Falls back to using the whole ID as suffix
        let filename = logger
            .log_path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(filename.ends_with("_custom_id_123.jsonl"));
    }

    #[test]
    fn complex_event_round_trips_through_jsonl() {
        let dir = TempDir::new().unwrap();
        let mut logger =
            EventLogger::new(dir.path(), "sess_100_aabb0011", RetentionPolicy::default()).unwrap();

        let event = BivvyEvent::WorkflowCompleted {
            name: "default".to_string(),
            success: false,
            aborted: true,
            steps_run: 3,
            steps_skipped: 2,
            duration_ms: 45678,
        };
        logger.on_event(&event);
        logger.flush().unwrap();

        let content = fs::read_to_string(logger.log_path()).unwrap();
        let value: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(value["type"], "workflow_completed");
        assert_eq!(value["name"], "default");
        assert_eq!(value["success"], false);
        assert_eq!(value["aborted"], true);
        assert_eq!(value["steps_run"], 3);
        assert_eq!(value["steps_skipped"], 2);
        assert_eq!(value["duration_ms"], 45678);
    }

    #[test]
    fn sensitive_step_output_redacted() {
        let dir = TempDir::new().unwrap();
        let mut logger =
            EventLogger::new(dir.path(), "sess_100_sensitive", RetentionPolicy::default()).unwrap();
        logger.set_sensitive_steps(vec!["secret_step".to_string()]);

        logger.on_event(&BivvyEvent::StepOutput {
            name: "secret_step".to_string(),
            stream: "stdout".to_string(),
            line: "password=hunter2".to_string(),
        });
        logger.flush().unwrap();

        let content = fs::read_to_string(logger.log_path()).unwrap();
        let value: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(value["line"], "[REDACTED]");
        assert!(!content.contains("hunter2"));
    }

    #[test]
    fn sensitive_step_completed_error_redacted() {
        let dir = TempDir::new().unwrap();
        let mut logger = EventLogger::new(
            dir.path(),
            "sess_100_sensitive2",
            RetentionPolicy::default(),
        )
        .unwrap();
        logger.set_sensitive_steps(vec!["secret_step".to_string()]);

        logger.on_event(&BivvyEvent::StepCompleted {
            name: "secret_step".to_string(),
            success: false,
            exit_code: Some(1),
            duration_ms: 500,
            error: Some("secret token expired".to_string()),
        });
        logger.flush().unwrap();

        let content = fs::read_to_string(logger.log_path()).unwrap();
        let value: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(value["error"], "[REDACTED]");
        assert!(!content.contains("secret token"));
    }

    #[test]
    fn non_sensitive_step_not_redacted() {
        let dir = TempDir::new().unwrap();
        let mut logger =
            EventLogger::new(dir.path(), "sess_100_nonsens", RetentionPolicy::default()).unwrap();
        logger.set_sensitive_steps(vec!["secret_step".to_string()]);

        logger.on_event(&BivvyEvent::StepOutput {
            name: "public_step".to_string(),
            stream: "stdout".to_string(),
            line: "hello world".to_string(),
        });
        logger.flush().unwrap();

        let content = fs::read_to_string(logger.log_path()).unwrap();
        assert!(content.contains("hello world"));
    }

    #[test]
    fn log_working_directory_extracts_from_session_started() {
        let dir = TempDir::new().unwrap();
        let log_path = dir.path().join("test.jsonl");
        let line = serde_json::json!({
            "ts": "2026-04-25T10:00:00.000Z",
            "session": "sess_test",
            "type": "session_started",
            "command": "run",
            "args": [],
            "version": "1.9.0",
            "working_directory": "/some/project"
        });
        fs::write(&log_path, format!("{}\n", line)).unwrap();

        let wd = log_working_directory(&log_path).unwrap();
        assert_eq!(wd, PathBuf::from("/some/project"));
    }

    #[test]
    fn log_working_directory_returns_none_without_session_started() {
        let dir = TempDir::new().unwrap();
        let log_path = dir.path().join("no_session.jsonl");
        let line = serde_json::json!({
            "ts": "2026-04-25T10:00:00.000Z",
            "session": "sess_test",
            "type": "step_planned",
            "name": "x",
            "index": 0,
            "total": 1
        });
        fs::write(&log_path, format!("{}\n", line)).unwrap();

        assert!(log_working_directory(&log_path).is_none());
    }

    #[test]
    fn log_working_directory_returns_none_when_field_missing() {
        let dir = TempDir::new().unwrap();
        let log_path = dir.path().join("no_wd.jsonl");
        let line = serde_json::json!({
            "ts": "2026-04-25T10:00:00.000Z",
            "session": "sess_test",
            "type": "session_started",
            "command": "run",
            "args": [],
            "version": "1.9.0"
        });
        fs::write(&log_path, format!("{}\n", line)).unwrap();

        assert!(log_working_directory(&log_path).is_none());
    }

    #[test]
    fn log_belongs_to_project_matches_canonical_path() {
        let project = TempDir::new().unwrap();
        let canonical = project.path().canonicalize().unwrap();

        let logs = TempDir::new().unwrap();
        let log_path = logs.path().join("match.jsonl");
        let line = serde_json::json!({
            "ts": "2026-04-25T10:00:00.000Z",
            "session": "sess_test",
            "type": "session_started",
            "command": "run",
            "args": [],
            "version": "1.9.0",
            "working_directory": project.path().display().to_string()
        });
        fs::write(&log_path, format!("{}\n", line)).unwrap();

        assert!(log_belongs_to_project(&log_path, &canonical));
    }

    #[test]
    fn log_belongs_to_project_rejects_other_project() {
        let project_a = TempDir::new().unwrap();
        let project_b = TempDir::new().unwrap();
        let canonical_a = project_a.path().canonicalize().unwrap();

        let logs = TempDir::new().unwrap();
        let log_path = logs.path().join("other.jsonl");
        let line = serde_json::json!({
            "ts": "2026-04-25T10:00:00.000Z",
            "session": "sess_test",
            "type": "session_started",
            "command": "run",
            "args": [],
            "version": "1.9.0",
            "working_directory": project_b.path().display().to_string()
        });
        fs::write(&log_path, format!("{}\n", line)).unwrap();

        assert!(!log_belongs_to_project(&log_path, &canonical_a));
    }

    #[test]
    fn list_project_logs_returns_empty_for_missing_dir() {
        let logs = list_project_logs(
            Path::new("/tmp/bivvy-list-project-logs-missing"),
            Path::new("/some/project"),
        );
        assert!(logs.is_empty());
    }

    #[test]
    fn list_project_logs_includes_only_matching_project() {
        let project_a = TempDir::new().unwrap();
        let project_b = TempDir::new().unwrap();
        let canonical_a = project_a.path().canonicalize().unwrap();
        let canonical_b = project_b.path().canonicalize().unwrap();
        let log_dir = TempDir::new().unwrap();

        let line_for = |wd: &Path| {
            serde_json::json!({
                "ts": "2026-04-25T10:00:00.000Z",
                "session": "sess_test",
                "type": "session_started",
                "command": "run",
                "args": [],
                "version": "1.9.0",
                "working_directory": wd.display().to_string(),
            })
            .to_string()
        };

        fs::write(
            log_dir.path().join("a1.jsonl"),
            format!("{}\n", line_for(&canonical_a)),
        )
        .unwrap();
        fs::write(
            log_dir.path().join("b1.jsonl"),
            format!("{}\n", line_for(&canonical_b)),
        )
        .unwrap();
        fs::write(
            log_dir.path().join("a2.jsonl"),
            format!("{}\n", line_for(&canonical_a)),
        )
        .unwrap();
        // Non-jsonl noise file — must be ignored.
        fs::write(log_dir.path().join("notes.txt"), "ignore me").unwrap();

        let mut logs_a = list_project_logs(log_dir.path(), &canonical_a);
        logs_a.sort();
        assert_eq!(logs_a.len(), 2);
        assert!(logs_a
            .iter()
            .all(|p| p.extension().and_then(|e| e.to_str()) == Some("jsonl")));
        assert!(logs_a.iter().all(|p| p
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap()
            .starts_with('a')));

        let logs_b = list_project_logs(log_dir.path(), &canonical_b);
        assert_eq!(logs_b.len(), 1);
    }

    #[test]
    fn list_project_logs_skips_files_without_session_started() {
        let project = TempDir::new().unwrap();
        let canonical = project.path().canonicalize().unwrap();
        let log_dir = TempDir::new().unwrap();

        // No session_started — must be excluded under project scoping.
        let event_line = serde_json::json!({
            "ts": "2026-04-25T10:00:00.000Z",
            "session": "sess_test",
            "type": "workflow_completed",
            "name": "default",
            "success": true,
            "aborted": false,
            "steps_run": 1,
            "steps_skipped": 0,
            "duration_ms": 1000
        });
        fs::write(
            log_dir.path().join("orphan.jsonl"),
            format!("{}\n", event_line),
        )
        .unwrap();

        assert!(list_project_logs(log_dir.path(), &canonical).is_empty());
    }

    #[test]
    fn log_belongs_to_project_falls_back_to_literal_match_for_missing_dir() {
        let canonical = PathBuf::from("/no/longer/exists");

        let logs = TempDir::new().unwrap();
        let log_path = logs.path().join("gone.jsonl");
        let line = serde_json::json!({
            "ts": "2026-04-25T10:00:00.000Z",
            "session": "sess_test",
            "type": "session_started",
            "command": "run",
            "args": [],
            "version": "1.9.0",
            "working_directory": "/no/longer/exists"
        });
        fs::write(&log_path, format!("{}\n", line)).unwrap();

        assert!(log_belongs_to_project(&log_path, &canonical));
    }
}
