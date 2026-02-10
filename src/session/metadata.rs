//! Session metadata capture.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// Metadata for a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    /// Command that was run (e.g., "run", "list", "status").
    pub command: String,
    /// Command arguments.
    pub args: Vec<String>,
    /// Flags that were set.
    pub flags: HashMap<String, serde_json::Value>,
    /// Current working directory.
    pub cwd: Option<String>,
    /// Config file path used.
    pub config_path: Option<String>,
    /// Hash of config at time of session.
    pub config_hash: Option<String>,
    /// Session start time.
    pub start_time: Option<DateTime<Utc>>,
    /// Session end time.
    pub end_time: Option<DateTime<Utc>>,
    /// Exit code.
    pub exit_code: Option<i32>,
    /// Captured stdout.
    pub stdout: String,
    /// Captured stderr.
    pub stderr: String,
    /// Command-specific context.
    #[serde(default)]
    pub context: SessionContext,
}

/// Command-specific context data.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionContext {
    /// For run: step results.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub step_results: Vec<StepResultSummary>,
    /// For run: workflow name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow: Option<String>,
    /// For status/list: steps shown.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub steps_shown: Vec<String>,
    /// For init: files created.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub files_created: Vec<String>,
    /// Any errors that occurred.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub errors: Vec<String>,
}

/// Summary of a step result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResultSummary {
    /// Step name.
    pub name: String,
    /// Step status (success, failed, skipped).
    pub status: String,
    /// Duration in milliseconds.
    pub duration_ms: Option<u64>,
    /// Error message if failed.
    pub error: Option<String>,
}

impl SessionMetadata {
    /// Create new session metadata.
    pub fn new(command: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            command: command.into(),
            args,
            flags: HashMap::new(),
            cwd: std::env::current_dir()
                .ok()
                .map(|p| p.display().to_string()),
            config_path: None,
            config_hash: None,
            start_time: Some(Utc::now()),
            end_time: None,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            context: SessionContext::default(),
        }
    }

    /// Set a flag value.
    pub fn set_flag(&mut self, key: impl Into<String>, value: impl Into<serde_json::Value>) {
        self.flags.insert(key.into(), value.into());
    }

    /// Set config info.
    pub fn set_config(&mut self, path: impl Into<String>, hash: impl Into<String>) {
        self.config_path = Some(path.into());
        self.config_hash = Some(hash.into());
    }

    /// Finalize the session with results.
    pub fn finalize(&mut self, exit_code: i32, stdout: String, stderr: String) {
        self.end_time = Some(Utc::now());
        self.exit_code = Some(exit_code);
        self.stdout = stdout;
        self.stderr = stderr;
    }

    /// Get session duration.
    pub fn duration(&self) -> Option<Duration> {
        match (self.start_time, self.end_time) {
            (Some(start), Some(end)) => {
                let diff = end.signed_duration_since(start);
                Some(Duration::from_millis(diff.num_milliseconds() as u64))
            }
            _ => None,
        }
    }

    /// Add a step result summary.
    pub fn add_step_result(&mut self, result: StepResultSummary) {
        self.context.step_results.push(result);
    }

    /// Add an error message.
    pub fn add_error(&mut self, error: impl Into<String>) {
        self.context.errors.push(error.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_metadata_creation() {
        let meta = SessionMetadata::new("run", vec!["--verbose".to_string()]);

        assert_eq!(meta.command, "run");
        assert_eq!(meta.args, vec!["--verbose"]);
        assert!(meta.start_time.is_some());
    }

    #[test]
    fn session_metadata_finalize() {
        let mut meta = SessionMetadata::new("list", vec![]);
        meta.finalize(0, "Steps:\n  brew\n  mise".to_string(), String::new());

        assert!(meta.end_time.is_some());
        assert_eq!(meta.exit_code, Some(0));
        assert!(meta.stdout.contains("brew"));
    }

    #[test]
    fn session_metadata_duration() {
        let mut meta = SessionMetadata::new("status", vec![]);
        std::thread::sleep(std::time::Duration::from_millis(50));
        meta.finalize(0, String::new(), String::new());

        let duration = meta.duration().unwrap();
        assert!(duration.as_millis() >= 50);
    }

    #[test]
    fn session_metadata_set_flag() {
        let mut meta = SessionMetadata::new("run", vec![]);
        meta.set_flag("verbose", true);
        meta.set_flag("parallel", 4);

        assert_eq!(meta.flags.get("verbose"), Some(&serde_json::json!(true)));
        assert_eq!(meta.flags.get("parallel"), Some(&serde_json::json!(4)));
    }

    #[test]
    fn session_metadata_set_config() {
        let mut meta = SessionMetadata::new("run", vec![]);
        meta.set_config("/project/.bivvy/config.yml", "abc123");

        assert_eq!(
            meta.config_path,
            Some("/project/.bivvy/config.yml".to_string())
        );
        assert_eq!(meta.config_hash, Some("abc123".to_string()));
    }

    #[test]
    fn session_metadata_add_step_result() {
        let mut meta = SessionMetadata::new("run", vec![]);
        meta.add_step_result(StepResultSummary {
            name: "brew".to_string(),
            status: "success".to_string(),
            duration_ms: Some(1500),
            error: None,
        });

        assert_eq!(meta.context.step_results.len(), 1);
        assert_eq!(meta.context.step_results[0].name, "brew");
    }

    #[test]
    fn session_metadata_add_error() {
        let mut meta = SessionMetadata::new("run", vec![]);
        meta.add_error("Step failed: brew");
        meta.add_error("Config invalid");

        assert_eq!(meta.context.errors.len(), 2);
    }

    #[test]
    fn session_context_default() {
        let ctx = SessionContext::default();
        assert!(ctx.step_results.is_empty());
        assert!(ctx.workflow.is_none());
        assert!(ctx.steps_shown.is_empty());
        assert!(ctx.files_created.is_empty());
        assert!(ctx.errors.is_empty());
    }

    #[test]
    fn session_metadata_serialization() {
        let mut meta = SessionMetadata::new("run", vec!["--verbose".to_string()]);
        meta.finalize(0, "output".to_string(), String::new());

        let json = serde_json::to_string(&meta).unwrap();
        let parsed: SessionMetadata = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.command, "run");
        assert_eq!(parsed.exit_code, Some(0));
    }
}
