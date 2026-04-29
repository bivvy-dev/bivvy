//! State management for execution history and preferences.
//!
//! This module provides persistent state storage for Bivvy projects,
//! tracking execution history, step states, and user preferences.

pub mod history;
pub mod index;
pub mod preferences;
pub mod project;
pub mod recorder;
pub mod satisfaction;
pub mod store;

pub use history::{RunHistoryBuilder, RunRecord, RunStatus};
pub use index::{ProjectEntry, ProjectIndex};
pub use preferences::Preferences;
pub use project::ProjectId;
pub use recorder::StateRecorder;
pub use satisfaction::{
    PresenceKind, SatisfactionCache, SatisfactionEvidence, SatisfactionRecord, SatisfactionSource,
};
pub use store::{BaselineMigration, StateStore, StatusSummary, StepState, StepStatus};
