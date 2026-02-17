//! Bivvy CLI entry point.

use std::process::ExitCode;

use bivvy::cli::{Cli, CommandDispatcher, Commands};
use bivvy::shell::is_ci;
use bivvy::ui::{create_ui, OutputMode};
use clap::Parser;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Initialize the tracing subscriber for logging.
///
/// Log level is controlled by:
/// 1. `--debug` flag sets level to DEBUG
/// 2. `RUST_LOG` environment variable (if set)
/// 3. Default is INFO
fn init_tracing(debug: bool) {
    let filter = if debug {
        EnvFilter::new("bivvy=debug")
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("bivvy=info"))
    };

    tracing_subscriber::registry()
        .with(fmt::layer().with_target(false))
        .with(filter)
        .init();
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    init_tracing(cli.debug);

    tracing::debug!("Bivvy starting with args: {:?}", cli);

    // Determine output mode
    let output_mode = if cli.quiet {
        OutputMode::Quiet
    } else if cli.verbose {
        OutputMode::Verbose
    } else {
        OutputMode::Normal
    };

    // Handle --no-color
    if cli.no_color {
        std::env::set_var("NO_COLOR", "1");
    }

    // Determine project root
    let project_root = cli
        .project
        .as_ref()
        .cloned()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    // Check if non-interactive (CI mode or explicit flag)
    let is_interactive = match &cli.command {
        Some(Commands::Run(args)) => !args.non_interactive && !args.ci && !is_ci(),
        _ => !is_ci(),
    };

    // Create UI
    let mut ui = create_ui(is_interactive, output_mode);

    // Dispatch command
    let dispatcher = CommandDispatcher::new(project_root);

    match dispatcher.dispatch(&cli, ui.as_mut()) {
        Ok(result) => ExitCode::from(result.exit_code as u8),
        Err(e) => {
            ui.error(&format!("Error: {}", e));
            ExitCode::from(1)
        }
    }
}
