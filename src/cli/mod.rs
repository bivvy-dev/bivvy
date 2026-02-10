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
