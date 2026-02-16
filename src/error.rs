//! Error types for Bivvy operations.
//!
//! This module defines [`BivvyError`], the primary error type used throughout
//! the application, and a [`Result`] type alias for convenience.
//!
//! # Error Handling Strategy
//!
//! - Use `BivvyError` for domain-specific errors that need distinct handling
//! - Use `anyhow::Error` (via `BivvyError::Other`) for unexpected errors
//! - All errors should provide actionable messages for users

use std::path::PathBuf;
use thiserror::Error;

/// Core error type for Bivvy operations.
#[derive(Debug, Error)]
pub enum BivvyError {
    /// Configuration file not found at expected location.
    #[error("Configuration not found: {path}")]
    ConfigNotFound { path: PathBuf },

    /// Failed to parse configuration file.
    #[error("Failed to parse config at {path}: {message}")]
    ConfigParseError { path: PathBuf, message: String },

    /// Invalid configuration structure or values.
    #[error("Invalid configuration: {message}")]
    ConfigValidationError { message: String },

    /// Referenced template does not exist.
    #[error("Unknown template: {name}")]
    UnknownTemplate { name: String },

    /// Step dependency cycle detected.
    #[error("Circular dependency detected: {cycle}")]
    CircularDependency { cycle: String },

    /// Step execution failed.
    #[error("Step '{step}' failed: {message}")]
    StepExecutionError { step: String, message: String },

    /// Shell command failed.
    #[error("Command failed with exit code {code:?}: {command}")]
    CommandFailed { command: String, code: Option<i32> },

    /// A required tool or service is missing and cannot be auto-installed.
    #[error("Missing requirement '{requirement}': {message}")]
    RequirementMissing {
        requirement: String,
        message: String,
    },

    /// A requirement check failed unexpectedly (e.g., network error during install).
    #[error("Requirement check failed for '{requirement}': {message}")]
    RequirementCheckFailed {
        requirement: String,
        message: String,
    },

    /// IO error wrapper.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Generic wrapped error for anyhow interop.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Result type alias for Bivvy operations.
pub type Result<T> = std::result::Result<T, BivvyError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_not_found_displays_path() {
        let err = BivvyError::ConfigNotFound {
            path: PathBuf::from("/foo/bar.yml"),
        };
        assert!(err.to_string().contains("/foo/bar.yml"));
    }

    #[test]
    fn config_parse_error_displays_path_and_message() {
        let err = BivvyError::ConfigParseError {
            path: PathBuf::from("/config.yml"),
            message: "invalid syntax".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("/config.yml"));
        assert!(msg.contains("invalid syntax"));
    }

    #[test]
    fn config_validation_error_displays_message() {
        let err = BivvyError::ConfigValidationError {
            message: "missing required field".into(),
        };
        assert!(err.to_string().contains("missing required field"));
    }

    #[test]
    fn unknown_template_displays_name() {
        let err = BivvyError::UnknownTemplate {
            name: "nonexistent".into(),
        };
        assert!(err.to_string().contains("nonexistent"));
    }

    #[test]
    fn circular_dependency_displays_cycle() {
        let err = BivvyError::CircularDependency {
            cycle: "a → b → a".into(),
        };
        assert!(err.to_string().contains("a → b → a"));
    }

    #[test]
    fn step_execution_error_displays_step_and_message() {
        let err = BivvyError::StepExecutionError {
            step: "install_deps".into(),
            message: "npm not found".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("install_deps"));
        assert!(msg.contains("npm not found"));
    }

    #[test]
    fn command_failed_displays_command_and_code() {
        let err = BivvyError::CommandFailed {
            command: "npm install".into(),
            code: Some(1),
        };
        let msg = err.to_string();
        assert!(msg.contains("npm install"));
        assert!(msg.contains("1"));
    }

    #[test]
    fn requirement_missing_displays_requirement_and_message() {
        let err = BivvyError::RequirementMissing {
            requirement: "ruby".into(),
            message: "Not found on PATH".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("ruby"));
        assert!(msg.contains("Not found on PATH"));
    }

    #[test]
    fn requirement_check_failed_displays_requirement_and_message() {
        let err = BivvyError::RequirementCheckFailed {
            requirement: "node".into(),
            message: "Install command exited with code 1".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("node"));
        assert!(msg.contains("Install command exited with code 1"));
    }

    #[test]
    fn io_error_converts_from_std() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let err: BivvyError = io_err.into();
        assert!(matches!(err, BivvyError::Io(_)));
    }

    #[test]
    fn result_type_alias_works() {
        fn returns_error() -> Result<()> {
            Err(BivvyError::ConfigValidationError {
                message: "test".into(),
            })
        }
        assert!(returns_error().is_err());
    }
}
