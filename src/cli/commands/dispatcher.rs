//! Command dispatching.
//!
//! This module provides the core command infrastructure:
//! - [`Command`] trait for implementing commands
//! - [`CommandResult`] for uniform result reporting
//! - [`CommandDispatcher`] for routing CLI subcommands

use std::path::{Path, PathBuf};

use crate::cli::args::{Cli, Commands};
use crate::error::Result;
use crate::ui::UserInterface;

/// Trait for command implementations.
///
/// Each CLI subcommand implements this trait to provide its execution logic.
pub trait Command {
    /// Execute the command.
    ///
    /// # Arguments
    ///
    /// * `ui` - User interface for displaying output and prompts
    ///
    /// # Returns
    ///
    /// A [`CommandResult`] indicating success/failure and exit code.
    fn execute(&self, ui: &mut dyn UserInterface) -> Result<CommandResult>;
}

/// Result of command execution.
#[derive(Debug)]
pub struct CommandResult {
    /// Whether the command succeeded.
    pub success: bool,

    /// Exit code to use (0 for success, non-zero for failure).
    pub exit_code: i32,
}

impl CommandResult {
    /// Create a successful result.
    pub fn success() -> Self {
        Self {
            success: true,
            exit_code: 0,
        }
    }

    /// Create a failure result.
    pub fn failure(exit_code: i32) -> Self {
        Self {
            success: false,
            exit_code,
        }
    }
}

/// Dispatches CLI commands to their implementations.
pub struct CommandDispatcher {
    project_root: PathBuf,
}

impl CommandDispatcher {
    /// Create a new dispatcher for the given project root.
    pub fn new(project_root: PathBuf) -> Self {
        Self { project_root }
    }

    /// Get the project root path.
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    /// Dispatch and execute a command.
    ///
    /// Routes the CLI subcommand to the appropriate command implementation
    /// and executes it.
    pub fn dispatch(&self, cli: &Cli, ui: &mut dyn UserInterface) -> Result<CommandResult> {
        match &cli.command {
            Some(Commands::Run(args)) => {
                let cmd = super::run::RunCommand::new(&self.project_root, args.clone());
                cmd.execute(ui)
            }
            Some(Commands::Init(args)) => {
                let cmd = super::init::InitCommand::new(&self.project_root, args.clone());
                cmd.execute(ui)
            }
            Some(Commands::Status(args)) => {
                let cmd = super::status::StatusCommand::new(&self.project_root, args.clone());
                cmd.execute(ui)
            }
            Some(Commands::List(args)) => {
                let cmd = super::list::ListCommand::new(&self.project_root, args.clone());
                cmd.execute(ui)
            }
            Some(Commands::Last(args)) => {
                let cmd = super::last::LastCommand::new(&self.project_root, args.clone());
                cmd.execute(ui)
            }
            Some(Commands::History(args)) => {
                let cmd = super::history::HistoryCommand::new(&self.project_root, args.clone());
                cmd.execute(ui)
            }
            Some(Commands::Lint(args)) => {
                let cmd = super::lint::LintCommand::new(&self.project_root, args.clone());
                cmd.execute(ui)
            }
            Some(Commands::Config(args)) => {
                let cmd = super::config::ConfigCommand::new(&self.project_root, args.clone());
                cmd.execute(ui)
            }
            Some(Commands::Cache(args)) => {
                let cmd = super::cache::CacheCommand::new(args.clone());
                cmd.execute(ui)
            }
            Some(Commands::Feedback(args)) => {
                let cmd = super::feedback::FeedbackCommand::new(args.clone());
                cmd.execute(ui)
            }
            Some(Commands::Completions(args)) => {
                let cmd = super::completions::CompletionsCommand::new(args.clone());
                cmd.execute(ui)
            }
            None => {
                // Default to run command with default args
                let cmd = super::run::RunCommand::new(
                    &self.project_root,
                    crate::cli::args::RunArgs::default(),
                );
                cmd.execute(ui)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_result_success() {
        let result = CommandResult::success();
        assert!(result.success);
        assert_eq!(result.exit_code, 0);
    }

    #[test]
    fn command_result_failure() {
        let result = CommandResult::failure(1);
        assert!(!result.success);
        assert_eq!(result.exit_code, 1);
    }

    #[test]
    fn dispatcher_creation() {
        let dispatcher = CommandDispatcher::new(std::path::PathBuf::from("/test"));
        assert_eq!(dispatcher.project_root(), std::path::Path::new("/test"));
    }
}
