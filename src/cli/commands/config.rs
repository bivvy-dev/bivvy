//! Config command implementation.
//!
//! The `bivvy config` command shows resolved configuration.

use std::path::{Path, PathBuf};

use crate::cli::args::ConfigArgs;
use crate::config::{load_merged_config, ConfigPaths};
use crate::error::{BivvyError, Result};
use crate::ui::{OutputMode, UserInterface};

use super::dispatcher::{Command, CommandResult};

/// The config command implementation.
pub struct ConfigCommand {
    project_root: PathBuf,
    args: ConfigArgs,
}

impl ConfigCommand {
    /// Create a new config command.
    pub fn new(project_root: &Path, args: ConfigArgs) -> Self {
        Self {
            project_root: project_root.to_path_buf(),
            args,
        }
    }

    /// Get the project root path.
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    /// Get the command arguments.
    pub fn args(&self) -> &ConfigArgs {
        &self.args
    }
}

impl Command for ConfigCommand {
    fn execute(&self, ui: &mut dyn UserInterface) -> Result<CommandResult> {
        // Load configuration
        let config = match load_merged_config(&self.project_root) {
            Ok(c) => c,
            Err(BivvyError::ConfigNotFound { .. }) => {
                ui.error("No configuration found. Run 'bivvy init' first.");
                return Ok(CommandResult::failure(2));
            }
            Err(e) => return Err(e),
        };

        // Apply config default_output when no CLI flag was explicitly set
        if ui.output_mode() == OutputMode::Normal {
            ui.set_output_mode(config.settings.default_output.into());
        }

        // Show config file path(s)
        let paths = ConfigPaths::discover(&self.project_root);
        let existing: Vec<_> = paths.all_existing();
        if !existing.is_empty() {
            for path in &existing {
                ui.message(&format!("# {}", path.display()));
            }
            ui.message("");
        }

        // Output format
        if self.args.json {
            let json =
                serde_json::to_string_pretty(&config).map_err(|e| BivvyError::Other(e.into()))?;
            ui.message(&json);
        } else {
            let yaml = serde_yaml::to_string(&config).map_err(|e| BivvyError::Other(e.into()))?;
            ui.message(&yaml);
        }

        Ok(CommandResult::success())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::MockUI;
    use std::fs;
    use tempfile::TempDir;

    fn setup_project(config: &str) -> TempDir {
        let temp = TempDir::new().unwrap();
        let bivvy_dir = temp.path().join(".bivvy");
        fs::create_dir_all(&bivvy_dir).unwrap();
        fs::write(bivvy_dir.join("config.yml"), config).unwrap();
        temp
    }

    #[test]
    fn config_command_creation() {
        let temp = TempDir::new().unwrap();
        let args = ConfigArgs::default();
        let cmd = ConfigCommand::new(temp.path(), args);

        assert_eq!(cmd.project_root(), temp.path());
    }

    #[test]
    fn config_no_config() {
        let temp = TempDir::new().unwrap();
        let args = ConfigArgs::default();
        let cmd = ConfigCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(!result.success);
        assert_eq!(result.exit_code, 2);
    }

    #[test]
    fn config_shows_config_path() {
        let config = r#"
app_name: Test
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;
        let temp = setup_project(config);
        let args = ConfigArgs::default();
        let cmd = ConfigCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        assert!(ui.messages().iter().any(|m| m.contains("config.yml")));
    }

    #[test]
    fn config_applies_config_default_output() {
        let config = r#"
app_name: Test
settings:
  default_output: quiet
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;
        let temp = setup_project(config);
        let args = ConfigArgs::default();
        let cmd = ConfigCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        assert_eq!(ui.output_mode(), crate::ui::OutputMode::Quiet);
    }

    #[test]
    fn config_yaml_output() {
        let config = r#"
app_name: Test
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;
        let temp = setup_project(config);
        let args = ConfigArgs::default();
        let cmd = ConfigCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
    }

    #[test]
    fn config_json_output() {
        let config = r#"
app_name: Test
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;
        let temp = setup_project(config);
        let args = ConfigArgs {
            json: true,
            ..Default::default()
        };
        let cmd = ConfigCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
    }
}
