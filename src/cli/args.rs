//! CLI argument definitions.
//!
//! This module defines all CLI arguments using clap's derive macros.
//! The main entry point is the [`Cli`] struct.

use clap::{Parser, Subcommand};
use clap_complete::Shell;
use std::path::PathBuf;

/// Bivvy - Development environment setup automation.
#[derive(Debug, Parser)]
#[command(name = "bivvy")]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    /// Path to config file (overrides default .bivvy/config.yml)
    #[arg(short, long, global = true)]
    pub config: Option<PathBuf>,

    /// Path to project root (overrides current directory)
    #[arg(short, long, global = true)]
    pub project: Option<PathBuf>,

    /// Show verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Minimal output
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// Disable colored output
    #[arg(long, global = true)]
    pub no_color: bool,

    /// Enable debug logging
    #[arg(long, global = true)]
    pub debug: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

use super::commands::cache::CacheArgs;
use super::commands::feedback::FeedbackArgs;

/// Available subcommands.
#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Run setup workflow (default if no command specified)
    Run(RunArgs),

    /// Initialize Bivvy configuration for a project
    Init(InitArgs),

    /// Show current setup status
    Status(StatusArgs),

    /// List available steps and workflows
    List(ListArgs),

    /// Show last run information
    Last(LastArgs),

    /// Show execution history
    History(HistoryArgs),

    /// Validate configuration files
    Lint(LintArgs),

    /// Show resolved configuration
    Config(ConfigArgs),

    /// Manage template cache
    Cache(CacheArgs),

    /// Capture and manage feedback
    Feedback(FeedbackArgs),

    /// Generate shell completions
    Completions(CompletionsArgs),
}

/// Arguments for the `run` command.
#[derive(Debug, Clone, clap::Args)]
pub struct RunArgs {
    /// Workflow to run
    #[arg(short, long, default_value = "default")]
    pub workflow: String,

    /// Run only specified steps (comma-separated)
    #[arg(long, value_delimiter = ',')]
    pub only: Vec<String>,

    /// Skip specified steps (comma-separated)
    #[arg(long, value_delimiter = ',')]
    pub skip: Vec<String>,

    /// How to handle skipped step dependencies
    #[arg(long, default_value = "skip_with_dependents")]
    pub skip_behavior: String,

    /// Force re-run of specified steps (comma-separated)
    #[arg(short, long, value_delimiter = ',')]
    pub force: Vec<String>,

    /// Resume an interrupted run
    #[arg(long)]
    pub resume: bool,

    /// Save prompt answers for future runs
    #[arg(long)]
    pub save_preferences: bool,

    /// Preview commands without executing
    #[arg(long)]
    pub dry_run: bool,

    /// Use defaults, no prompts
    #[arg(long)]
    pub non_interactive: bool,

    /// Deprecated: use --non-interactive and --env ci instead
    #[arg(long, hide = true)]
    pub ci: bool,

    /// Target environment (e.g., development, ci, staging)
    #[arg(long, value_name = "ENV")]
    pub env: Option<String>,
}

impl Default for RunArgs {
    fn default() -> Self {
        Self {
            workflow: "default".to_string(),
            only: Vec::new(),
            skip: Vec::new(),
            skip_behavior: "skip_with_dependents".to_string(),
            force: Vec::new(),
            resume: false,
            save_preferences: false,
            dry_run: false,
            non_interactive: false,
            ci: false,
            env: None,
        }
    }
}

/// Arguments for the `init` command.
#[derive(Debug, Clone, Default, clap::Args)]
pub struct InitArgs {
    /// Generate minimal config without prompts
    #[arg(long)]
    pub minimal: bool,

    /// Start from a specific template
    #[arg(long)]
    pub template: Option<String>,

    /// Copy configuration from another project
    #[arg(long)]
    pub from: Option<String>,

    /// Overwrite existing configuration
    #[arg(long)]
    pub force: bool,
}

/// Arguments for the `status` command.
#[derive(Debug, Clone, Default, clap::Args)]
pub struct StatusArgs {
    /// Output as JSON
    #[arg(long)]
    pub json: bool,

    /// Show status for specific step
    #[arg(long)]
    pub step: Option<String>,

    /// Target environment (e.g., development, ci, staging)
    #[arg(long, value_name = "ENV")]
    pub env: Option<String>,
}

/// Arguments for the `list` command.
#[derive(Debug, Clone, Default, clap::Args)]
pub struct ListArgs {
    /// List only steps
    #[arg(long)]
    pub steps_only: bool,

    /// List only workflows
    #[arg(long)]
    pub workflows_only: bool,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,

    /// Target environment (e.g., development, ci, staging)
    #[arg(long, value_name = "ENV")]
    pub env: Option<String>,
}

/// Arguments for the `last` command.
#[derive(Debug, Clone, Default, clap::Args)]
pub struct LastArgs {
    /// Output as JSON
    #[arg(long)]
    pub json: bool,

    /// Show details for specific step
    #[arg(long)]
    pub step: Option<String>,

    /// Show all runs
    #[arg(long)]
    pub all: bool,

    /// Include command output
    #[arg(long)]
    pub output: bool,
}

/// Arguments for the `history` command.
#[derive(Debug, Clone, Default, clap::Args)]
pub struct HistoryArgs {
    /// Filter by step name
    #[arg(long)]
    pub step: Option<String>,

    /// Number of runs to show
    #[arg(long)]
    pub limit: Option<usize>,

    /// Show runs since duration (e.g., "1h", "7d", "30m")
    #[arg(long)]
    pub since: Option<String>,

    /// Show detailed view with steps for each run
    #[arg(long)]
    pub detail: bool,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

/// Arguments for the `lint` command.
#[derive(Debug, Clone, Default, clap::Args)]
pub struct LintArgs {
    /// Output format: human, json, sarif
    #[arg(long, default_value = "human")]
    pub format: String,

    /// Auto-fix simple issues
    #[arg(long)]
    pub fix: bool,

    /// Treat warnings as errors
    #[arg(long)]
    pub strict: bool,
}

/// Arguments for the `config` command.
#[derive(Debug, Clone, Default, clap::Args)]
pub struct ConfigArgs {
    /// Output as JSON
    #[arg(long)]
    pub json: bool,

    /// Output as YAML
    #[arg(long)]
    pub yaml: bool,

    /// Show fully merged config
    #[arg(long)]
    pub merged: bool,
}

/// Arguments for the `completions` command.
#[derive(Debug, Clone, clap::Args)]
pub struct CompletionsArgs {
    /// Shell to generate completions for
    #[arg(value_enum)]
    pub shell: Shell,
}
