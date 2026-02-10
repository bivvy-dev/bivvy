//! Init command implementation.
//!
//! The `bivvy init` command initializes project configuration.

use std::fs;
use std::path::{Path, PathBuf};

use crate::cli::args::InitArgs;
use crate::config::CompletedCheck;
use crate::detection::DetectionRunner;
use crate::error::Result;
use crate::registry::builtin::BuiltinLoader;
use crate::registry::template::Template;
use crate::ui::{Prompt, PromptOption, PromptResult, PromptType, UserInterface};

use super::dispatcher::{Command, CommandResult};

/// The init command implementation.
pub struct InitCommand {
    project_root: PathBuf,
    args: InitArgs,
}

impl InitCommand {
    /// Create a new init command.
    pub fn new(project_root: &Path, args: InitArgs) -> Self {
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
    pub fn args(&self) -> &InitArgs {
        &self.args
    }

    /// Check if config already exists.
    fn config_exists(&self) -> bool {
        self.project_root.join(".bivvy/config.yml").exists()
    }

    /// Create configuration content from selected steps with template info.
    fn create_config(&self, steps: &[(&str, Option<&Template>)]) -> String {
        let project_name = self
            .project_root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("MyApp");

        let mut config = format!(
            "# Bivvy configuration for {project_name}\n\
             # Docs: https://bivvy.dev/configuration\n\
             #\n\
             # Override any template field per-step:\n\
             #   steps:\n\
             #     example:\n\
             #       template: bundler\n\
             #       env:\n\
             #         BUNDLE_WITHOUT: \"production\"\n\
             #\n\
             # Add custom steps:\n\
             #   steps:\n\
             #     setup_db:\n\
             #       title: \"Set up database\"\n\
             #       command: \"bin/rails db:setup\"\n\
             #       completed_check:\n\
             #         type: command_succeeds\n\
             #         command: \"bin/rails db:version\"\n\
             #\n\
             # Create named workflows:\n\
             #   workflows:\n\
             #     ci:\n\
             #       steps: [bundler, yarn]\n\
             #       settings:\n\
             #         default_output: quiet\n\
             \n\
             app_name: \"{project_name}\"\n\
             \n\
             settings:\n\
             \x20 default_output: verbose  # verbose | quiet | silent\n\
             \n\
             steps:\n"
        );

        let step_names: Vec<&str> = steps.iter().map(|(name, _)| *name).collect();

        for (name, template) in steps {
            config.push_str(&format!("  {}:\n    template: {}\n", name, name));

            if let Some(tmpl) = template {
                // Show command
                if let Some(ref cmd) = tmpl.step.command {
                    config.push_str(&format!("    # command: {}\n", cmd));
                }

                // Show completed_check
                if let Some(ref check) = tmpl.step.completed_check {
                    Self::format_completed_check(&mut config, check);
                }

                // Show watches
                if !tmpl.step.watches.is_empty() {
                    let watches: Vec<&str> = tmpl.step.watches.iter().map(|s| s.as_str()).collect();
                    config.push_str(&format!("    # watches: [{}]\n", watches.join(", ")));
                }
            }

            config.push('\n');
        }

        config.push_str("workflows:\n  default:\n    steps: ");
        config.push_str(&format!("[{}]\n", step_names.join(", ")));

        config
    }

    /// Format a completed_check as YAML comments.
    fn format_completed_check(config: &mut String, check: &CompletedCheck) {
        match check {
            CompletedCheck::FileExists { path } => {
                config.push_str("    # completed_check:\n");
                config.push_str("    #   type: file_exists\n");
                config.push_str(&format!("    #   path: \"{}\"\n", path));
            }
            CompletedCheck::CommandSucceeds { command } => {
                config.push_str("    # completed_check:\n");
                config.push_str("    #   type: command_succeeds\n");
                config.push_str(&format!("    #   command: \"{}\"\n", command));
            }
            _ => {}
        }
    }

    /// Update gitignore to exclude local overrides.
    fn update_gitignore(&self, ui: &mut dyn UserInterface) -> Result<()> {
        let gitignore_entry = ".bivvy/config.local.yml";
        let gitignore_path = self.project_root.join(".gitignore");

        if gitignore_path.exists() {
            let content = fs::read_to_string(&gitignore_path)?;
            if !content.contains(gitignore_entry) {
                let new_content = if content.ends_with('\n') {
                    format!("{}{}\n", content, gitignore_entry)
                } else {
                    format!("{}\n{}\n", content, gitignore_entry)
                };
                fs::write(&gitignore_path, new_content)?;
                ui.message("Added .bivvy/config.local.yml to .gitignore");
            }
        }

        Ok(())
    }
}

impl Command for InitCommand {
    fn execute(&self, ui: &mut dyn UserInterface) -> Result<CommandResult> {
        // Check if config already exists
        if self.config_exists() && !self.args.force {
            ui.warning("Configuration already exists. Use --force to overwrite.");
            return Ok(CommandResult::failure(1));
        }

        ui.show_header("Project Setup");
        ui.message("Scanning project...\n");

        // Run detection
        let detection = DetectionRunner::run(&self.project_root);

        // Show detected technologies
        if !detection.project.details.is_empty() {
            ui.message("Detected technologies:");
            for detail in &detection.project.details {
                ui.success(&format!(
                    "  {} - {}",
                    detail.name,
                    detail.details.join(", ")
                ));
            }
            ui.message("");
        }

        // Show conflicts
        for conflict in &detection.conflicts {
            ui.warning(&conflict.message);
            ui.message(&format!("  Suggestion: {}", conflict.suggestion));
            ui.message("");
        }

        // Collect steps to include
        let mut steps = Vec::new();

        if self.args.minimal || !ui.is_interactive() {
            // Just use detected templates
            for suggestion in &detection.suggested_templates {
                steps.push(suggestion.name.clone());
            }
        } else if !detection.suggested_templates.is_empty() {
            // Interactive multi-select checklist
            ui.message("Use [space] to toggle, [a] to toggle all, [enter] to confirm\n");

            let options: Vec<PromptOption> = detection
                .suggested_templates
                .iter()
                .map(|s| PromptOption {
                    label: format!("{} — {}", s.name, s.reason),
                    value: s.name.clone(),
                })
                .collect();

            let all_values = detection
                .suggested_templates
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>()
                .join(",");

            let prompt = Prompt {
                key: "init_steps".to_string(),
                question: "Select steps to include".to_string(),
                prompt_type: PromptType::MultiSelect { options },
                default: Some(all_values),
            };

            if let Ok(PromptResult::Strings(selected)) = ui.prompt(&prompt) {
                steps = selected;
            }
        }

        // If no steps detected, create a minimal example
        if steps.is_empty() {
            steps.push("setup".to_string());
        }

        // Load templates to enrich config output
        let loader = BuiltinLoader::new().ok();
        let steps_with_templates: Vec<(&str, Option<&Template>)> = steps
            .iter()
            .map(|name| {
                let template = loader.as_ref().and_then(|l| l.get(name));
                (name.as_str(), template)
            })
            .collect();

        // Generate config
        let config = self.create_config(&steps_with_templates);

        // Write config
        let bivvy_dir = self.project_root.join(".bivvy");
        fs::create_dir_all(&bivvy_dir)?;
        fs::write(bivvy_dir.join("config.yml"), &config)?;

        // Update gitignore
        self.update_gitignore(ui)?;

        ui.success("Created .bivvy/config.yml");
        ui.message("\nNext steps:");
        ui.message("  1. Review .bivvy/config.yml and adjust as needed");
        ui.message("  2. Run `bivvy` to set up your environment");

        Ok(CommandResult::success())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::MockUI;
    use tempfile::TempDir;

    #[test]
    fn init_command_creation() {
        let temp = TempDir::new().unwrap();
        let args = InitArgs::default();
        let cmd = InitCommand::new(temp.path(), args);

        assert_eq!(cmd.project_root(), temp.path());
        assert!(!cmd.config_exists());
    }

    #[test]
    fn create_config_basic() {
        let temp = TempDir::new().unwrap();
        let args = InitArgs::default();
        let cmd = InitCommand::new(temp.path(), args);

        let steps: Vec<(&str, Option<&Template>)> = vec![("bundler", None), ("yarn", None)];
        let config = cmd.create_config(&steps);

        assert!(config.contains("bundler"));
        assert!(config.contains("yarn"));
        assert!(config.contains("template: bundler"));
        assert!(config.contains("template: yarn"));
        assert!(config.contains("workflows:"));
        assert!(config.contains("default:"));
        assert!(config.contains("steps: [bundler, yarn]"));
    }

    #[test]
    fn create_config_with_templates() {
        let temp = TempDir::new().unwrap();
        let args = InitArgs::default();
        let cmd = InitCommand::new(temp.path(), args);

        let loader = BuiltinLoader::new().unwrap();
        let bundler = loader.get("bundler");
        let yarn = loader.get("yarn");

        let steps: Vec<(&str, Option<&Template>)> = vec![("bundler", bundler), ("yarn", yarn)];
        let config = cmd.create_config(&steps);

        // Should contain template references
        assert!(config.contains("template: bundler"));
        assert!(config.contains("template: yarn"));

        // Should contain commented-out command info from templates
        assert!(config.contains("# command: bundle install"));
        assert!(config.contains("# command: yarn install"));

        // Should contain completed_check comments
        assert!(config.contains("# completed_check:"));
        assert!(config.contains("#   command: \"bundle check\""));

        // Should contain watches
        assert!(config.contains("# watches: [Gemfile, Gemfile.lock]"));

        // Should contain customization guide in the header
        assert!(config.contains("# Override any template field per-step:"));
    }

    #[test]
    fn create_config_with_file_exists_check() {
        let temp = TempDir::new().unwrap();
        let args = InitArgs::default();
        let cmd = InitCommand::new(temp.path(), args);

        let loader = BuiltinLoader::new().unwrap();
        let npm = loader.get("npm");

        let steps: Vec<(&str, Option<&Template>)> = vec![("npm", npm)];
        let config = cmd.create_config(&steps);

        assert!(config.contains("#   type: file_exists"));
        assert!(config.contains("#   path: \"node_modules\""));
    }

    #[test]
    fn create_config_header_and_settings() {
        let temp = TempDir::new().unwrap();
        let args = InitArgs::default();
        let cmd = InitCommand::new(temp.path(), args);

        let steps: Vec<(&str, Option<&Template>)> = vec![("setup", None)];
        let config = cmd.create_config(&steps);

        assert!(config.contains("# Bivvy configuration for"));
        assert!(config.contains("# Docs: https://bivvy.dev/configuration"));
        assert!(config.contains("default_output: verbose  # verbose | quiet | silent"));

        // Customization guide should appear before app_name (in the header)
        let guide_pos = config.find("# Override any template field").unwrap();
        let app_name_pos = config.find("app_name:").unwrap();
        assert!(guide_pos < app_name_pos);
    }

    #[test]
    fn config_exists_check() {
        let temp = TempDir::new().unwrap();
        let bivvy_dir = temp.path().join(".bivvy");
        fs::create_dir_all(&bivvy_dir).unwrap();

        let args = InitArgs::default();
        let cmd = InitCommand::new(temp.path(), args);

        assert!(!cmd.config_exists());

        fs::write(bivvy_dir.join("config.yml"), "app_name: test").unwrap();
        assert!(cmd.config_exists());
    }

    #[test]
    fn init_fails_if_config_exists() {
        let temp = TempDir::new().unwrap();
        let bivvy_dir = temp.path().join(".bivvy");
        fs::create_dir_all(&bivvy_dir).unwrap();
        fs::write(bivvy_dir.join("config.yml"), "app_name: test").unwrap();

        let args = InitArgs::default();
        let cmd = InitCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(!result.success);
        assert_eq!(result.exit_code, 1);
    }

    #[test]
    fn init_with_force_overwrites() {
        let temp = TempDir::new().unwrap();
        let bivvy_dir = temp.path().join(".bivvy");
        fs::create_dir_all(&bivvy_dir).unwrap();
        fs::write(bivvy_dir.join("config.yml"), "app_name: old").unwrap();

        let args = InitArgs {
            force: true,
            minimal: true,
            ..Default::default()
        };
        let cmd = InitCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);

        let config = fs::read_to_string(bivvy_dir.join("config.yml")).unwrap();
        assert!(!config.contains("app_name: old"));
    }

    #[test]
    fn init_minimal_creates_config() {
        let temp = TempDir::new().unwrap();

        let args = InitArgs {
            minimal: true,
            ..Default::default()
        };
        let cmd = InitCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
        assert!(temp.path().join(".bivvy/config.yml").exists());
    }

    #[test]
    fn init_with_ruby_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("Gemfile"), "source 'https://rubygems.org'").unwrap();

        let args = InitArgs {
            minimal: true,
            ..Default::default()
        };
        let cmd = InitCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);

        let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
        assert!(config.contains("bundler"));
        // Enriched output should include template info
        assert!(config.contains("# command: bundle install"));
    }

    #[test]
    fn init_updates_gitignore() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join(".gitignore"), "node_modules\n").unwrap();

        let args = InitArgs {
            minimal: true,
            ..Default::default()
        };
        let cmd = InitCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        let gitignore = fs::read_to_string(temp.path().join(".gitignore")).unwrap();
        assert!(gitignore.contains(".bivvy/config.local.yml"));
    }

    #[test]
    fn init_does_not_duplicate_gitignore_entry() {
        let temp = TempDir::new().unwrap();
        fs::write(
            temp.path().join(".gitignore"),
            "node_modules\n.bivvy/config.local.yml\n",
        )
        .unwrap();

        let args = InitArgs {
            minimal: true,
            ..Default::default()
        };
        let cmd = InitCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        let gitignore = fs::read_to_string(temp.path().join(".gitignore")).unwrap();
        let count = gitignore.matches(".bivvy/config.local.yml").count();
        assert_eq!(count, 1);
    }

    #[test]
    fn init_interactive_uses_multiselect() {
        let temp = TempDir::new().unwrap();
        // Create a Ruby project so detection suggests bundler
        fs::write(temp.path().join("Gemfile"), "source 'https://rubygems.org'").unwrap();

        let args = InitArgs::default();
        let cmd = InitCommand::new(temp.path(), args);
        let mut ui = MockUI::new();
        ui.set_interactive(true);

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
        // Should prompt with single multiselect key, not per-step include_X
        assert!(ui.prompts_shown().contains(&"init_steps".to_string()));
        assert!(!ui.prompts_shown().iter().any(|k| k.starts_with("include_")));
    }

    #[test]
    fn init_interactive_all_detected_by_default() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("Gemfile"), "source 'https://rubygems.org'").unwrap();
        fs::write(temp.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();

        let args = InitArgs::default();
        let cmd = InitCommand::new(temp.path(), args);
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        // Don't set a response — MockUI falls back to default, which has all values

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
        let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
        assert!(config.contains("bundler"));
        assert!(config.contains("cargo"));
    }

    #[test]
    fn init_interactive_shows_hints() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("Gemfile"), "source 'https://rubygems.org'").unwrap();

        let args = InitArgs::default();
        let cmd = InitCommand::new(temp.path(), args);
        let mut ui = MockUI::new();
        ui.set_interactive(true);

        cmd.execute(&mut ui).unwrap();

        assert!(ui.has_message("[space] to toggle"));
    }

    #[test]
    fn init_interactive_subset_selection() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("Gemfile"), "source 'https://rubygems.org'").unwrap();
        fs::write(temp.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();

        let args = InitArgs::default();
        let cmd = InitCommand::new(temp.path(), args);
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        // Only select cargo, not bundler
        ui.set_prompt_response("init_steps", "cargo");

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
        let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
        // Should have cargo as a step definition
        assert!(config.contains("  cargo:\n    template: cargo\n"));
        // Should NOT have bundler as a step definition
        assert!(!config.contains("  bundler:\n    template: bundler\n"));
        // Workflow should only list cargo
        assert!(config.contains("steps: [cargo]"));
    }
}
