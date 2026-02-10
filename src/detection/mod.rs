//! Project and technology detection.

pub mod command_detection;
pub mod conflicts;
pub mod environment;
pub mod file_detection;
pub mod package_manager;
pub mod project;
pub mod runner;
pub mod types;

pub use conflicts::ConflictDetector;
pub use environment::EnvironmentDetector;
pub use package_manager::PackageManagerDetector;
pub use project::ProjectDetector;
pub use runner::{DetectionRunner, FullDetection, SuggestedTemplate};
pub use types::{Detection, DetectionKind, DetectionResult};
