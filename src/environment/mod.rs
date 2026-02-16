//! Environment detection and resolution.
//!
//! Determines which environment (development, ci, staging, etc.) bivvy
//! is running in. The priority chain is:
//!
//! 1. Explicit `--env` flag
//! 2. Config `default_environment`
//! 3. Auto-detection (CI env vars, Docker, Codespace)
//! 4. Fallback to "development"

pub mod detection;
pub mod resolver;

pub use detection::{BuiltinDetector, DetectedEnvironment};
pub use resolver::{EnvironmentSource, ResolvedEnvironment};
