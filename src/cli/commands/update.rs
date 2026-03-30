//! Update command implementation.
//!
//! The `bivvy update` command checks for and installs updates.

use crate::error::Result;
use crate::ui::UserInterface;
use crate::updates::{check_for_updates_fresh, detect_install_method, VERSION};

use super::dispatcher::{Command, CommandResult};

/// Arguments for the `update` command.
#[derive(Debug, Clone, Default, clap::Args)]
pub struct UpdateArgs {
    /// Check for updates without installing
    #[arg(long)]
    pub check: bool,
}

/// The update command implementation.
pub struct UpdateCommand {
    args: UpdateArgs,
}

impl UpdateCommand {
    /// Create a new update command.
    pub fn new(args: UpdateArgs) -> Self {
        Self { args }
    }
}

impl Command for UpdateCommand {
    fn execute(&self, ui: &mut dyn UserInterface) -> Result<CommandResult> {
        ui.message(&format!("Current version: {}", VERSION));
        ui.message("Checking for updates...");

        let info = match check_for_updates_fresh() {
            Ok(info) => info,
            Err(e) => {
                ui.error(&format!("Failed to check for updates: {}", e));
                return Ok(CommandResult::failure(1));
            }
        };

        if !info.update_available {
            ui.message("You're on the latest version.");
            return Ok(CommandResult::success());
        }

        ui.message(&format!(
            "New version available: {} -> {}",
            info.current, info.latest
        ));

        if self.args.check {
            if let Some(url) = &info.release_url {
                ui.message(&format!("Release: {}", url));
            }
            return Ok(CommandResult::success());
        }

        let method = detect_install_method();

        if !method.supports_auto_update() {
            ui.message(&format!(
                "Auto-update is not supported for {} installs.",
                method.name()
            ));
            if let Some(url) = &info.release_url {
                ui.message(&format!("Download the latest version from: {}", url));
            }
            return Ok(CommandResult::success());
        }

        let update_cmd = method.update_command().unwrap();
        ui.message(&format!("Updating via: {}", update_cmd));

        let parts: Vec<&str> = update_cmd.split_whitespace().collect();
        let status = std::process::Command::new(parts[0])
            .args(&parts[1..])
            .status()?;

        if status.success() {
            ui.message("Update complete!");
            Ok(CommandResult::success())
        } else {
            ui.error("Update failed.");
            Ok(CommandResult::failure(1))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_command_creation() {
        let cmd = UpdateCommand::new(UpdateArgs::default());
        assert!(!cmd.args.check);
    }

    #[test]
    fn update_command_check_flag() {
        let cmd = UpdateCommand::new(UpdateArgs { check: true });
        assert!(cmd.args.check);
    }
}
