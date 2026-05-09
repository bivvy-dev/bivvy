//! Command-line interface for Bivvy.
//!
//! This module provides the CLI argument parsing using clap's derive macros
//! and command implementations.
//!
//! # Architecture
//!
//! - [`args`] - Argument definitions using clap derive macros
//! - [`commands`] - Command implementations
//! - [`session_wrapper`] - Session tracking integration

pub mod args;
pub mod commands;
pub mod session_wrapper;

pub use args::{
    Cli, Commands, ConfigArgs, HistoryArgs, InitArgs, LastArgs, LintArgs, ListArgs, RunArgs,
    StatusArgs,
};
pub use commands::{Command, CommandDispatcher, CommandResult};
pub use session_wrapper::SessionWrapper;

use std::path::PathBuf;

/// Resolve the project root from an explicit `--project` argument and
/// the current working directory.
///
/// Returns `Ok(path)` when either the explicit override is set or the
/// current working directory is readable. Returns `Err(message)` when
/// neither is available — this is the case the original `unwrap_or_default`
/// chain silently turned into `""`. Callers should print the error and
/// exit with a non-zero status code.
pub fn resolve_project_root(
    explicit: Option<PathBuf>,
    cwd: std::io::Result<PathBuf>,
) -> Result<PathBuf, String> {
    if let Some(p) = explicit {
        return Ok(p);
    }
    cwd.map_err(|e| {
        format!(
            "cannot determine current working directory: {}. \
             Pass --project <path> to set the project root explicitly.",
            e
        )
    })
}

#[cfg(test)]
mod tests {
    use super::resolve_project_root;
    use std::io;
    use std::path::PathBuf;

    #[test]
    fn explicit_project_path_wins() {
        let explicit = Some(PathBuf::from("/explicit"));
        let cwd = Err(io::Error::other("should be ignored"));
        let resolved = resolve_project_root(explicit, cwd).unwrap();
        assert_eq!(resolved, PathBuf::from("/explicit"));
    }

    #[test]
    fn falls_back_to_cwd() {
        let cwd = Ok(PathBuf::from("/some/cwd"));
        let resolved = resolve_project_root(None, cwd).unwrap();
        assert_eq!(resolved, PathBuf::from("/some/cwd"));
    }

    #[test]
    fn errors_when_neither_explicit_nor_cwd_available() {
        let cwd = Err(io::Error::other("cwd vanished"));
        let result = resolve_project_root(None, cwd);
        let message = result.expect_err("expected Err");
        assert!(
            message.contains("current working directory"),
            "message should explain the cause: {message}"
        );
        assert!(
            message.contains("--project"),
            "message should suggest --project: {message}"
        );
    }
}
