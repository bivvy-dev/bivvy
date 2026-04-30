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

    /// Trust all remote extends URLs without prompting
    #[arg(long, global = true)]
    pub trust: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

use super::commands::cache::CacheArgs;
use super::commands::feedback::FeedbackArgs;
use super::commands::snapshot::SnapshotArgs;
use super::commands::update::UpdateArgs;

/// Available subcommands.
#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Run setup workflow (default if no command specified)
    Run(RunArgs),

    /// Initialize Bivvy configuration for a project
    Init(InitArgs),

    /// Add a template step to the configuration
    Add(AddArgs),

    /// Show current setup status
    Status(StatusArgs),

    /// List available steps and workflows
    List(ListArgs),

    /// List available templates
    Templates(TemplatesArgs),

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

    /// Manage change check snapshots
    Snapshot(SnapshotArgs),

    /// Check for and install updates
    Update(UpdateArgs),

    /// Generate shell completions
    Completions(CompletionsArgs),

    /// Print JSON Schema for config.yml
    Schema(SchemaArgs),
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

    /// Force re-run of every step in the workflow, bypassing all checks
    /// and step-level configuration
    #[arg(long)]
    pub force_all: bool,

    /// Resume an interrupted run
    #[arg(long)]
    pub resume: bool,

    /// Save prompt answers for future runs
    #[arg(long)]
    pub save_preferences: bool,

    /// Preview commands without executing
    #[arg(long)]
    pub dry_run: bool,

    /// Discard all persisted satisfaction records and evaluate everything fresh
    #[arg(long)]
    pub fresh: bool,

    /// Use defaults, no prompts
    #[arg(long)]
    pub non_interactive: bool,

    /// Deprecated: use --non-interactive and --env ci instead
    #[arg(long, hide = true)]
    pub ci: bool,

    /// Target environment (e.g., development, ci, staging)
    #[arg(short, long, value_name = "ENV")]
    pub env: Option<String>,

    /// Use the diagnostic funnel for error recovery (overrides config)
    #[arg(long)]
    pub diagnostic_funnel: bool,

    /// Disable the diagnostic funnel, use legacy pattern matching
    #[arg(long, conflicts_with = "diagnostic_funnel")]
    pub no_diagnostic_funnel: bool,

    /// Suppress run header (used when chaining from init)
    #[arg(skip)]
    pub suppress_header: bool,
}

impl Default for RunArgs {
    fn default() -> Self {
        Self {
            workflow: "default".to_string(),
            only: Vec::new(),
            skip: Vec::new(),
            skip_behavior: "skip_with_dependents".to_string(),
            force: Vec::new(),
            force_all: false,
            resume: false,
            save_preferences: false,
            dry_run: false,
            non_interactive: false,
            ci: false,
            env: None,
            diagnostic_funnel: false,
            no_diagnostic_funnel: false,
            fresh: false,
            suppress_header: false,
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
    /// Workflow to show status for. When provided, the workflow's
    /// portable steps are visible alongside the project-level ones.
    pub workflow: Option<String>,

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
    /// Show details for one workflow (parses just that file).
    pub target: Option<String>,

    /// List only steps
    #[arg(long)]
    pub steps_only: bool,

    /// List only workflows
    #[arg(long)]
    pub workflows_only: bool,

    /// Show every step and workflow from the merged configuration
    /// (legacy behavior). Without this flag, output is built from
    /// discovery + headers and does not deep-merge.
    #[arg(long)]
    pub all: bool,

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

    /// Delete this project's run history (only logs that belong to this
    /// project; logs from other projects are left untouched)
    #[arg(long)]
    pub clear: bool,

    /// Skip the confirmation prompt when used with `--clear`
    #[arg(short, long)]
    pub force: bool,
}

/// Arguments for the `lint` command.
#[derive(Debug, Clone, Default, clap::Args)]
pub struct LintArgs {
    /// Workflow or step name to lint. Resolves to .bivvy/workflows/<name>.yml
    /// first, then .bivvy/steps/<name>.yml.
    pub target: Option<String>,

    /// Force lookup as a workflow file: .bivvy/workflows/<NAME>.yml
    #[arg(long, value_name = "NAME", conflicts_with_all = ["step", "config_only", "all"])]
    pub workflow: Option<String>,

    /// Force lookup as a step file: .bivvy/steps/<NAME>.yml
    #[arg(long, value_name = "NAME", conflicts_with_all = ["workflow", "config_only", "all"])]
    pub step: Option<String>,

    /// Lint .bivvy/config.yml only (the default when no target is given).
    /// Named `--config-only` rather than `--config` to avoid collision
    /// with the global `-c, --config <PATH>` option.
    #[arg(long = "config-only", conflicts_with_all = ["workflow", "step", "all"])]
    pub config_only: bool,

    /// Lint every file in the merged state — equivalent to the legacy
    /// "lint everything" behavior, now opt-in
    #[arg(long, conflicts_with_all = ["workflow", "step", "config_only"])]
    pub all: bool,

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

/// Arguments for the `schema` command.
#[derive(Debug, Clone, Default, clap::Args)]
pub struct SchemaArgs {
    /// Write schema to a file instead of stdout
    #[arg(short, long)]
    pub output: Option<PathBuf>,
}

/// Arguments for the `completions` command.
#[derive(Debug, Clone, clap::Args)]
pub struct CompletionsArgs {
    /// Shell to generate completions for
    #[arg(value_enum)]
    pub shell: Shell,
}

/// Arguments for the `templates` command.
#[derive(Debug, Clone, Default, clap::Args)]
pub struct TemplatesArgs {
    /// Filter by category (e.g., ruby, node, python)
    #[arg(long)]
    pub category: Option<String>,
}

/// Arguments for the `add` command.
#[derive(Debug, Clone, Default, clap::Args)]
pub struct AddArgs {
    /// Template to add
    pub template: String,

    /// Step name to use in config (defaults to template name)
    #[arg(long = "as", value_name = "NAME")]
    pub step_name: Option<String>,

    /// Workflow to add the step to (defaults to "default")
    #[arg(long)]
    pub workflow: Option<String>,

    /// Insert after this step in the workflow
    #[arg(long)]
    pub after: Option<String>,

    /// Don't add to any workflow
    #[arg(long)]
    pub no_workflow: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Debug, Parser)]
    struct TestRun {
        #[command(flatten)]
        args: RunArgs,
    }

    #[test]
    fn run_args_force_parses_step_list() {
        let cli = TestRun::parse_from(["test", "--force", "install,build"]);
        assert_eq!(cli.args.force, vec!["install", "build"]);
        assert!(!cli.args.force_all);
    }

    #[test]
    fn run_args_force_all_is_false_by_default() {
        let cli = TestRun::parse_from(["test"]);
        assert!(!cli.args.force_all);
        assert!(cli.args.force.is_empty());
    }

    #[test]
    fn run_args_force_all_flag_sets_field_true() {
        let cli = TestRun::parse_from(["test", "--force-all"]);
        assert!(cli.args.force_all);
        assert!(cli.args.force.is_empty());
    }

    #[test]
    fn run_args_force_and_force_all_can_coexist() {
        let cli = TestRun::parse_from(["test", "--force", "install", "--force-all"]);
        assert!(cli.args.force_all);
        assert_eq!(cli.args.force, vec!["install"]);
    }
}
