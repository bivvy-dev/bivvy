//! CLI command implementations.
//!
//! Each command implements the [`Command`] trait, which provides a uniform
//! interface for executing commands and reporting results.
//!
//! # Architecture
//!
//! Commands are dispatched via [`CommandDispatcher`], which routes CLI
//! subcommands to their implementations. This allows:
//! - Single binary with subcommands (`bivvy run`, `bivvy status`)
//! - Shared initialization logic
//! - Consistent global flag handling

pub mod cache;
pub mod completions;
pub mod config;
pub mod dispatcher;
pub mod display;
pub mod feedback;
pub mod history;
pub mod init;
pub mod last;
pub mod lint;
pub mod list;
pub mod run;
pub mod status;

pub use dispatcher::{Command, CommandDispatcher, CommandResult};
