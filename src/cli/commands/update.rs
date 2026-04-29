//! Update command implementation.
//!
//! The `bivvy update` command checks for and installs updates.
//! It also provides flags to enable or disable automatic background updates.

use crate::error::Result;
use crate::ui::{OutputWriter, UserInterface};
use crate::updates::{
    auto_update::is_auto_update_enabled, check_for_updates_fresh, detect_install_method, VERSION,
};

use super::dispatcher::{Command, CommandResult};

/// Arguments for the `update` command.
#[derive(Debug, Clone, Default, clap::Args)]
pub struct UpdateArgs {
    /// Check for updates without installing
    #[arg(long)]
    pub check: bool,

    /// Enable automatic background updates
    #[arg(long, conflicts_with = "disable_auto_update")]
    pub enable_auto_update: bool,

    /// Disable automatic background updates
    #[arg(long, conflicts_with = "enable_auto_update")]
    pub disable_auto_update: bool,
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
        // Handle auto-update toggle flags
        if self.args.enable_auto_update {
            return set_auto_update(ui, true);
        }
        if self.args.disable_auto_update {
            return set_auto_update(ui, false);
        }

        ui.message(&format!("Current version: {}", VERSION));

        // Show auto-update status
        if is_auto_update_enabled() {
            ui.message("Auto-update: enabled");
        } else {
            ui.message("Auto-update: disabled");
        }

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

/// Write the auto_update setting to the system config at `~/.bivvy/config.yml`.
///
/// Only requires `OutputWriter` — displays confirmation messages but does not prompt.
fn set_auto_update(ui: &mut dyn OutputWriter, enabled: bool) -> Result<CommandResult> {
    let home = crate::sys::home_dir().ok_or_else(|| {
        crate::error::BivvyError::Other(anyhow::anyhow!("Could not determine home directory"))
    })?;

    let config_dir = home.join(".bivvy");
    let config_path = config_dir.join("config.yml");

    // Read existing config or start fresh
    let mut value: serde_yaml::Value = if config_path.exists() {
        let content = std::fs::read_to_string(&config_path).map_err(|e| {
            crate::error::BivvyError::Other(anyhow::anyhow!(
                "Failed to read {}: {}",
                config_path.display(),
                e
            ))
        })?;
        serde_yaml::from_str(&content)
            .unwrap_or(serde_yaml::Value::Mapping(serde_yaml::Mapping::new()))
    } else {
        serde_yaml::Value::Mapping(serde_yaml::Mapping::new())
    };

    // Ensure settings mapping exists
    let mapping = value.as_mapping_mut().ok_or_else(|| {
        crate::error::BivvyError::Other(anyhow::anyhow!("System config is not a YAML mapping"))
    })?;

    let settings_key = serde_yaml::Value::String("settings".to_string());
    if !mapping.contains_key(&settings_key) {
        mapping.insert(
            settings_key.clone(),
            serde_yaml::Value::Mapping(serde_yaml::Mapping::new()),
        );
    }

    let settings = mapping
        .get_mut(&settings_key)
        .and_then(|v| v.as_mapping_mut())
        .ok_or_else(|| {
            crate::error::BivvyError::Other(anyhow::anyhow!("settings is not a YAML mapping"))
        })?;

    settings.insert(
        serde_yaml::Value::String("auto_update".to_string()),
        serde_yaml::Value::Bool(enabled),
    );

    // Write back
    std::fs::create_dir_all(&config_dir)?;
    let yaml = serde_yaml::to_string(&value).map_err(|e| {
        crate::error::BivvyError::Other(anyhow::anyhow!("Failed to serialize config: {}", e))
    })?;
    std::fs::write(&config_path, yaml)?;

    if enabled {
        ui.message("Automatic background updates enabled.");
    } else {
        ui.message("Automatic background updates disabled.");
    }
    ui.message(&format!("Saved to {}", config_path.display()));

    Ok(CommandResult::success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_command_creation() {
        let cmd = UpdateCommand::new(UpdateArgs::default());
        assert!(!cmd.args.check);
        assert!(!cmd.args.enable_auto_update);
        assert!(!cmd.args.disable_auto_update);
    }

    #[test]
    fn update_command_check_flag() {
        let cmd = UpdateCommand::new(UpdateArgs {
            check: true,
            ..Default::default()
        });
        assert!(cmd.args.check);
    }

    #[test]
    fn update_command_enable_auto_update_flag() {
        let cmd = UpdateCommand::new(UpdateArgs {
            enable_auto_update: true,
            ..Default::default()
        });
        assert!(cmd.args.enable_auto_update);
        assert!(!cmd.args.disable_auto_update);
    }

    #[test]
    fn update_command_disable_auto_update_flag() {
        let cmd = UpdateCommand::new(UpdateArgs {
            disable_auto_update: true,
            ..Default::default()
        });
        assert!(cmd.args.disable_auto_update);
        assert!(!cmd.args.enable_auto_update);
    }
}
