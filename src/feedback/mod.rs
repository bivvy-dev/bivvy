//! Feedback capture for dogfooding.
//!
//! This module provides structured capture of friction points with session correlation.

mod entry;
mod store;

pub use entry::{FeedbackEntry, FeedbackStatus};
pub use store::FeedbackStore;

/// Get the default feedback store path.
pub fn default_store_path() -> std::path::PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("bivvy")
        .join("feedback.jsonl")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_store_path_valid() {
        let path = default_store_path();
        assert!(path.ends_with("feedback.jsonl"));
    }
}
