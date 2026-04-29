//! Config command implementation.
//!
//! The `bivvy config` command shows resolved configuration.

use std::path::{Path, PathBuf};

use crate::cli::args::ConfigArgs;
use crate::config::{load_config_file, load_merged_config, ConfigPaths};
use crate::error::{BivvyError, Result};
use crate::ui::{OutputMode, UserInterface};

use super::dispatcher::{Command, CommandResult};

/// The config command implementation.
pub struct ConfigCommand {
    project_root: PathBuf,
    args: ConfigArgs,
    config_override: Option<PathBuf>,
}

impl ConfigCommand {
    /// Create a new config command.
    pub fn new(project_root: &Path, args: ConfigArgs) -> Self {
        Self {
            project_root: project_root.to_path_buf(),
            args,
            config_override: None,
        }
    }

    /// Set an override config path.
    pub fn with_config_override(mut self, config_override: Option<PathBuf>) -> Self {
        self.config_override = config_override;
        self
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
        let paths = ConfigPaths::discover(&self.project_root);

        // Load configuration: override path, merged (all sources), or project-only
        let config = if let Some(ref override_path) = self.config_override {
            load_config_file(override_path)?
        } else if self.args.merged {
            match load_merged_config(&self.project_root) {
                Ok(c) => c,
                Err(BivvyError::ConfigNotFound { .. }) => {
                    ui.error("No configuration found. Run 'bivvy init' first.");
                    return Ok(CommandResult::failure(2));
                }
                Err(e) => return Err(e),
            }
        } else {
            // Load only the project config (.bivvy/config.yml) without merging
            match &paths.project {
                Some(project_path) => load_config_file(project_path)?,
                None => {
                    ui.error("No configuration found. Run 'bivvy init' first.");
                    return Ok(CommandResult::failure(2));
                }
            }
        };

        // Apply config default_output when no CLI flag was explicitly set
        if ui.output_mode() == OutputMode::Normal {
            ui.set_output_mode(config.settings.defaults.output.into());
        }

        // Show config file path(s)
        if self.args.merged {
            let existing: Vec<_> = paths.all_existing();
            if !existing.is_empty() {
                for path in &existing {
                    ui.message(&format!("# {}", path.display()));
                }
                ui.message("");
            }
        } else if let Some(project_path) = &paths.project {
            ui.message(&format!("# {}", project_path.display()));
            ui.message("");
        }

        // Output format
        if self.args.json {
            let json =
                serde_json::to_string_pretty(&config).map_err(|e| BivvyError::Other(e.into()))?;
            ui.message(&json);
        } else {
            // --yaml or default: output as YAML
            let yaml = serde_yaml::to_string(&config).map_err(|e| BivvyError::Other(e.into()))?;
            ui.message(&yaml);
        }

        Ok(CommandResult::success())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{MockUI, UiState};
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
  defaults:
    output: quiet
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
