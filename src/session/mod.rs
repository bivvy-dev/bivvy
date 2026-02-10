//! Session tracking for all bivvy commands.
//!
//! This module provides session management that tracks:
//! - When CLI commands are run
//! - What commands were executed
//! - Duration and outcomes
//! - Error context for feedback

mod id;
mod metadata;
mod store;

pub use id::SessionId;
pub use metadata::{SessionContext, SessionMetadata, StepResultSummary};
pub use store::{Session, SessionStore};

/// Get the default session store path.
pub fn default_store_path() -> std::path::PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("bivvy")
        .join("sessions")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_store_path_valid() {
        let path = default_store_path();
        assert!(path.ends_with("sessions"));
    }
}
