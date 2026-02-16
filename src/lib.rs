//! Bivvy - Interactive development environment setup automation.
//!
//! Bivvy is a CLI tool that replaces ad-hoc `bin/setup` scripts with
//! declarative YAML configuration and a polished interactive CLI experience.
//!
//! # Modules
//!
//! - [`cli`] - Command-line interface and argument parsing
//! - [`config`] - Configuration loading, parsing, and validation
//! - [`environment`] - Environment detection and resolution
//! - [`error`] - Error types and result aliases
//! - [`lint`] - Configuration validation and linting
//! - [`registry`] - Template registry and resolution
//! - [`runner`] - Step execution orchestration and dependency management
//! - [`shell`] - Shell command execution
//! - [`state`] - State management for execution history and preferences
//! - [`secrets`] - Secret detection and output masking
//! - [`steps`] - Step resolution and execution
//! - [`ui`] - Interactive prompts, spinners, and terminal output
//!
//! # Example
//!
//! ```
//! use bivvy::config::{InterpolationContext, resolve_string};
//!
//! // Resolve variables in a command
//! let mut ctx = InterpolationContext::new();
//! ctx.prompts.insert("mode".to_string(), "development".to_string());
//! let command = resolve_string("rails s -e ${mode}", &ctx).unwrap();
//! assert_eq!(command, "rails s -e development");
//! ```
//!
//! For file-based config loading, see the integration tests.

pub mod cache;
pub mod cli;
pub mod config;
pub mod detection;
pub mod environment;
pub mod error;
pub mod feedback;
pub mod lint;
pub mod registry;
pub mod requirements;
pub mod runner;
pub mod secrets;
pub mod session;
pub mod shell;
pub mod state;
pub mod steps;
pub mod ui;
pub mod updates;

pub use error::{BivvyError, Result};
