//! Session wrapper for CLI commands.
//!
//! This module provides session tracking integration for CLI command execution.

use anyhow::Result;

use crate::session::{default_store_path, Session, SessionId, SessionMetadata, SessionStore};

/// Wraps command execution with session tracking.
pub struct SessionWrapper {
    store: SessionStore,
    current_id: SessionId,
    metadata: SessionMetadata,
    stdout_buffer: Vec<u8>,
    stderr_buffer: Vec<u8>,
}

impl SessionWrapper {
    /// Create a new session wrapper.
    pub fn new(command: &str, args: Vec<String>) -> Self {
        Self {
            store: SessionStore::new(default_store_path()),
            current_id: SessionId::new(),
            metadata: SessionMetadata::new(command, args),
            stdout_buffer: Vec::new(),
            stderr_buffer: Vec::new(),
        }
    }

    /// Get the current session ID.
    pub fn session_id(&self) -> &SessionId {
        &self.current_id
    }

    /// Get mutable access to metadata for adding context.
    pub fn metadata_mut(&mut self) -> &mut SessionMetadata {
        &mut self.metadata
    }

    /// Capture stdout.
    pub fn capture_stdout(&mut self, s: &str) {
        self.stdout_buffer.extend_from_slice(s.as_bytes());
    }

    /// Capture stderr.
    pub fn capture_stderr(&mut self, s: &str) {
        self.stderr_buffer.extend_from_slice(s.as_bytes());
    }

    /// Finalize and save the session.
    pub fn finalize(mut self, exit_code: i32) -> Result<SessionId> {
        let stdout = String::from_utf8_lossy(&self.stdout_buffer).to_string();
        let stderr = String::from_utf8_lossy(&self.stderr_buffer).to_string();

        self.metadata.finalize(exit_code, stdout, stderr);

        let session = Session {
            id: self.current_id.clone(),
            metadata: self.metadata,
        };

        self.store.save(&session)?;

        // Cleanup old sessions (keep last 100)
        let _ = self.store.cleanup(100);

        Ok(self.current_id)
    }

    /// Get the latest session ID (for feedback command).
    pub fn get_latest_session_id() -> Result<Option<SessionId>> {
        let store = Self::get_store();
        Ok(store.get_latest()?.map(|s| s.id))
    }

    /// Get the session store for reading sessions.
    pub fn get_store() -> SessionStore {
        SessionStore::new(default_store_path())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // Use a custom store for testing to avoid polluting the real store
    fn create_test_wrapper(
        command: &str,
        args: Vec<String>,
        store_path: std::path::PathBuf,
    ) -> SessionWrapper {
        SessionWrapper {
            store: SessionStore::new(store_path),
            current_id: SessionId::new(),
            metadata: SessionMetadata::new(command, args),
            stdout_buffer: Vec::new(),
            stderr_buffer: Vec::new(),
        }
    }

    #[test]
    fn session_wrapper_creation() {
        let wrapper = SessionWrapper::new("run", vec!["--verbose".to_string()]);
        assert!(wrapper.session_id().as_str().starts_with("sess_"));
    }

    #[test]
    fn session_wrapper_capture_output() {
        let temp = TempDir::new().unwrap();
        let mut wrapper = create_test_wrapper("list", vec![], temp.path().join("sessions"));

        wrapper.capture_stdout("Hello\n");
        wrapper.capture_stdout("World\n");
        wrapper.capture_stderr("Warning: something\n");

        let id = wrapper.finalize(0).unwrap();

        // Verify the session was saved
        let store = SessionStore::new(temp.path().join("sessions"));
        let session = store.load(&id).unwrap();

        assert!(session.metadata.stdout.contains("Hello"));
        assert!(session.metadata.stdout.contains("World"));
        assert!(session.metadata.stderr.contains("Warning"));
    }

    #[test]
    fn session_wrapper_metadata_modification() {
        let temp = TempDir::new().unwrap();
        let mut wrapper = create_test_wrapper("run", vec![], temp.path().join("sessions"));

        wrapper.metadata_mut().set_flag("verbose", true);
        wrapper.metadata_mut().context.workflow = Some("default".to_string());

        let id = wrapper.finalize(0).unwrap();

        let store = SessionStore::new(temp.path().join("sessions"));
        let session = store.load(&id).unwrap();

        assert_eq!(
            session.metadata.flags.get("verbose"),
            Some(&serde_json::json!(true))
        );
        assert_eq!(
            session.metadata.context.workflow,
            Some("default".to_string())
        );
    }

    #[test]
    fn session_wrapper_finalize_exit_code() {
        let temp = TempDir::new().unwrap();
        let wrapper = create_test_wrapper("run", vec![], temp.path().join("sessions"));

        let id = wrapper.finalize(1).unwrap();

        let store = SessionStore::new(temp.path().join("sessions"));
        let session = store.load(&id).unwrap();

        assert_eq!(session.metadata.exit_code, Some(1));
    }

    #[test]
    fn session_wrapper_cleanup_old_sessions() {
        let temp = TempDir::new().unwrap();
        let store_path = temp.path().join("sessions");

        // Create 105 sessions
        for i in 0..105 {
            let wrapper = create_test_wrapper(&format!("cmd{}", i), vec![], store_path.clone());
            wrapper.finalize(0).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }

        let store = SessionStore::new(&store_path);
        let sessions = store.list_recent(usize::MAX).unwrap();

        // Should have at most 100 sessions due to cleanup
        assert!(sessions.len() <= 100);
    }

    #[test]
    fn session_wrapper_get_store() {
        // Test that get_store returns a valid store
        let store = SessionWrapper::get_store();
        // Just check the path is set correctly
        assert!(store.path().ends_with("sessions"));
    }

    #[test]
    fn session_wrapper_get_latest_session_id() {
        // This tests the static method - may or may not have sessions
        let result = SessionWrapper::get_latest_session_id();
        // Should not error
        assert!(result.is_ok());
    }

    #[test]
    fn session_wrapper_capture_multiple_calls() {
        let temp = TempDir::new().unwrap();
        let mut wrapper = create_test_wrapper("test", vec![], temp.path().join("sessions"));

        // Multiple capture calls
        wrapper.capture_stdout("line1\n");
        wrapper.capture_stdout("line2\n");
        wrapper.capture_stdout("line3\n");
        wrapper.capture_stderr("err1\n");
        wrapper.capture_stderr("err2\n");

        let id = wrapper.finalize(0).unwrap();

        let store = SessionStore::new(temp.path().join("sessions"));
        let session = store.load(&id).unwrap();

        assert!(session.metadata.stdout.contains("line1"));
        assert!(session.metadata.stdout.contains("line2"));
        assert!(session.metadata.stdout.contains("line3"));
        assert!(session.metadata.stderr.contains("err1"));
        assert!(session.metadata.stderr.contains("err2"));
    }

    #[test]
    fn session_wrapper_session_id_accessor() {
        let temp = TempDir::new().unwrap();
        let wrapper = create_test_wrapper("cmd", vec![], temp.path().join("sessions"));

        let id = wrapper.session_id();
        assert!(id.as_str().starts_with("sess_"));
    }

    #[test]
    fn session_wrapper_with_args() {
        let temp = TempDir::new().unwrap();
        let wrapper = create_test_wrapper(
            "run",
            vec!["--verbose".to_string(), "--force".to_string()],
            temp.path().join("sessions"),
        );

        let id = wrapper.finalize(0).unwrap();

        let store = SessionStore::new(temp.path().join("sessions"));
        let session = store.load(&id).unwrap();

        assert_eq!(session.metadata.command, "run");
        assert_eq!(session.metadata.args, vec!["--verbose", "--force"]);
    }
}
