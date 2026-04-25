//! Init command implementation.
//!
//! The `bivvy init` command initializes project configuration.

use std::fs;
use std::path::{Path, PathBuf};

use crate::cli::args::{InitArgs, RunArgs};
use crate::config::CompletedCheck;
use crate::detection::DetectionRunner;
use crate::error::Result;
use crate::registry::builtin::BuiltinLoader;
use crate::registry::template::Template;
use crate::ui::{
    hints, OutputWriter, Prompt, PromptOption, PromptResult, PromptType, UserInterface,
};

use super::dispatcher::{Command, CommandResult};
use super::run::RunCommand;

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
             #       template: bundle-install\n\
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
             #       steps: [bundle-install, yarn-install]\n\
             #       settings:\n\
             #         default_output: quiet\n\
             \n\
             app_name: \"{project_name}\"\n\
             \n\
             settings:\n\
             \x20 default_output: verbose  # verbose | quiet | silent\n"
        );

        if !steps.is_empty() {
            config.push_str("\nsteps:\n");

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
                        let watches: Vec<&str> =
                            tmpl.step.watches.iter().map(|s| s.as_str()).collect();
                        config.push_str(&format!("    # watches: [{}]\n", watches.join(", ")));
                    }
                }

                config.push('\n');
            }

            config.push_str("workflows:\n  default:\n    steps: ");
            config.push_str(&format!("[{}]\n", step_names.join(", ")));
        }

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

    /// Execute `--from`: copy config from another project.
    ///
    /// Only requires `OutputWriter` — displays messages but does not prompt.
    fn execute_from(&self, ui: &mut dyn OutputWriter, from_path: &str) -> Result<CommandResult> {
        let source = Path::new(from_path).join(".bivvy/config.yml");
        if !source.exists() {
            ui.error(&format!(
                "No .bivvy/config.yml found at {}",
                source.display()
            ));
            return Ok(CommandResult::failure(1));
        }

        let bivvy_dir = self.project_root.join(".bivvy");
        fs::create_dir_all(&bivvy_dir)?;
        fs::copy(&source, bivvy_dir.join("config.yml"))?;

        self.update_gitignore(ui)?;

        ui.message("");
        ui.success(&format!("Copied configuration from {}", source.display()));

        ui.show_hint(hints::after_init());
        Ok(CommandResult::success())
    }

    /// Execute `--template`: generate config from a specific template or category.
    fn execute_template(
        &self,
        ui: &mut dyn UserInterface,
        template_name: &str,
    ) -> Result<CommandResult> {
        let loader = match BuiltinLoader::new() {
            Ok(l) => l,
            Err(e) => {
                ui.error(&format!("Failed to load templates: {}", e));
                return Ok(CommandResult::failure(1));
            }
        };

        // Collect matching templates: first try as a direct template name,
        // then try as a category name to find all templates in that category.
        let mut steps: Vec<(String, Option<&Template>)> = Vec::new();

        if let Some(tmpl) = loader.get(template_name) {
            // Direct template match
            steps.push((tmpl.name.clone(), Some(tmpl)));
        } else {
            // Try as a category — find all templates in this category
            for qualified_name in loader.template_names() {
                if let Some(tmpl) = loader.get(qualified_name) {
                    if tmpl.category == template_name {
                        steps.push((tmpl.name.clone(), Some(tmpl)));
                    }
                }
            }
        }

        if steps.is_empty() {
            ui.error(&format!(
                "No template or category found matching '{}'",
                template_name
            ));
            ui.message("Run 'bivvy templates' to see available templates.");
            return Ok(CommandResult::failure(1));
        }

        ui.message(&format!("Using template: {}", template_name));

        let steps_with_templates: Vec<(&str, Option<&Template>)> = steps
            .iter()
            .map(|(name, tmpl)| (name.as_str(), *tmpl))
            .collect();

        let config = self.create_config(&steps_with_templates);

        let bivvy_dir = self.project_root.join(".bivvy");
        fs::create_dir_all(&bivvy_dir)?;
        fs::write(bivvy_dir.join("config.yml"), &config)?;

        self.update_gitignore(ui)?;

        ui.message("");
        ui.success("Created .bivvy/config.yml");

        let step_names: Vec<&str> = steps.iter().map(|(n, _)| n.as_str()).collect();
        ui.message("Workflow: default");
        ui.message(&format!(
            "Steps: {} ({})",
            steps.len(),
            step_names.join(", ")
        ));
        ui.message("");

        ui.show_hint(hints::after_init());
        Ok(CommandResult::success())
    }

    /// Update gitignore to exclude local overrides.
    ///
    /// Only requires `OutputWriter` — displays a message but does not prompt.
    fn update_gitignore(&self, ui: &mut dyn OutputWriter) -> Result<()> {
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

        // Show init header with version
        let version = crate::updates::version::VERSION;
        ui.message(&format!(
            "\n{} {} {}\n",
            console::style("⛺").bold().magenta(),
            console::style(format!("bivvy v{}", version)).dim(),
            console::style("· init").dim(),
        ));

        // Handle --from: copy config from another project
        if let Some(ref from_path) = self.args.from {
            return self.execute_from(ui, from_path);
        }

        // Handle --template: use a specific template instead of detection
        if let Some(ref template_name) = self.args.template {
            return self.execute_template(ui, template_name);
        }

        // Run detection
        let detection = DetectionRunner::run(&self.project_root);

        // Show detected technologies
        if !detection.project.details.is_empty() {
            ui.message("Detected technologies:");
            for detail in &detection.project.details {
                ui.success(&format!("{} - {}", detail.name, detail.details.join(", ")));
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
                steps.push(suggestion.name.to_string());
            }
        } else if !detection.suggested_templates.is_empty() {
            // Interactive multi-select checklist
            let options: Vec<PromptOption> = detection
                .suggested_templates
                .iter()
                .map(|s| PromptOption {
                    label: format!("{} — {}", s.name, s.reason),
                    value: s.name.to_string(),
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

        ui.message("");
        ui.success("Created .bivvy/config.yml");

        // Show init summary
        let step_names: Vec<&str> = steps.iter().map(|s| s.as_str()).collect();
        ui.message("Workflow: default");
        ui.message(&format!(
            "Steps: {} ({})",
            steps.len(),
            step_names.join(", ")
        ));
        ui.message("");

        // Offer to run setup immediately (interactive only)
        if ui.is_interactive() {
            let prompt = Prompt {
                key: "run_after_init".to_string(),
                question: "Run setup now?".to_string(),
                prompt_type: PromptType::Select {
                    options: vec![
                        PromptOption {
                            label: "No  (n)".to_string(),
                            value: "no".to_string(),
                        },
                        PromptOption {
                            label: "Yes (y)".to_string(),
                            value: "yes".to_string(),
                        },
                    ],
                },
                default: Some("no".to_string()),
            };

            if let Ok(PromptResult::String(answer)) = ui.prompt(&prompt) {
                if answer == "yes" {
                    ui.message("");
                    let run_args = RunArgs {
                        force: steps.iter().map(|s| s.to_string()).collect(),
                        suppress_header: true,
                        ..RunArgs::default()
                    };
                    let run_cmd = RunCommand::new(&self.project_root, run_args);
                    return run_cmd.execute(ui);
                }
            }
        }

        ui.show_hint(hints::after_init());

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

        let steps: Vec<(&str, Option<&Template>)> =
            vec![("bundle-install", None), ("yarn-install", None)];
        let config = cmd.create_config(&steps);

        assert!(config.contains("bundle-install"));
        assert!(config.contains("yarn-install"));
        assert!(config.contains("template: bundle-install"));
        assert!(config.contains("template: yarn-install"));
        assert!(config.contains("workflows:"));
        assert!(config.contains("default:"));
        assert!(config.contains("steps: [bundle-install, yarn-install]"));
    }

    #[test]
    fn create_config_with_templates() {
        let temp = TempDir::new().unwrap();
        let args = InitArgs::default();
        let cmd = InitCommand::new(temp.path(), args);

        let loader = BuiltinLoader::new().unwrap();
        let bundler = loader.get("bundle-install");
        let yarn = loader.get("yarn-install");

        let steps: Vec<(&str, Option<&Template>)> =
            vec![("bundle-install", bundler), ("yarn-install", yarn)];
        let config = cmd.create_config(&steps);

        // Should contain template references
        assert!(config.contains("template: bundle-install"));
        assert!(config.contains("template: yarn-install"));

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
        let npm = loader.get("npm-install");

        let steps: Vec<(&str, Option<&Template>)> = vec![("npm-install", npm)];
        let config = cmd.create_config(&steps);

        assert!(config.contains("#   type: file_exists"));
        assert!(config.contains("#   path: \"node_modules\""));
    }

    #[test]
    fn create_config_header_and_settings() {
        let temp = TempDir::new().unwrap();
        let args = InitArgs::default();
        let cmd = InitCommand::new(temp.path(), args);

        let steps: Vec<(&str, Option<&Template>)> = vec![("bundle-install", None)];
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
    fn create_config_no_steps_omits_steps_and_workflows() {
        let temp = TempDir::new().unwrap();
        let args = InitArgs::default();
        let cmd = InitCommand::new(temp.path(), args);

        let steps: Vec<(&str, Option<&Template>)> = vec![];
        let config = cmd.create_config(&steps);

        assert!(config.contains("app_name:"));
        assert!(config.contains("settings:"));
        // Should not have actual steps/workflows sections (only in comments)
        assert!(!config.contains("\nsteps:\n"));
        assert!(!config.contains("\nworkflows:\n"));
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
        assert!(config.contains("bundle-install"));
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
        ui.set_prompt_response("run_after_init", "no");

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
        ui.set_prompt_response("run_after_init", "no");

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
        let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
        assert!(config.contains("bundle-install"));
        assert!(config.contains("cargo-build"));
    }

    #[test]
    fn init_interactive_shows_summary() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("Gemfile"), "source 'https://rubygems.org'").unwrap();

        let args = InitArgs::default();
        let cmd = InitCommand::new(temp.path(), args);
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        ui.set_prompt_response("run_after_init", "no");

        cmd.execute(&mut ui).unwrap();

        assert!(ui.has_message("Workflow: default"));
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
        // Only select cargo-build, not bundle-install
        ui.set_prompt_response("init_steps", "cargo-build");
        ui.set_prompt_response("run_after_init", "no");

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
        let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
        // Should have cargo-build as a step definition
        assert!(config.contains("  cargo-build:\n    template: cargo-build\n"));
        // Should NOT have bundle-install as a step definition
        assert!(!config.contains("  bundle-install:\n    template: bundle-install\n"));
        // Workflow should only list cargo-build
        assert!(config.contains("steps: [cargo-build]"));
    }

    #[test]
    fn init_interactive_prompts_run_after_init() {
        let temp = TempDir::new().unwrap();

        let args = InitArgs {
            minimal: true,
            ..Default::default()
        };
        let cmd = InitCommand::new(temp.path(), args);
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        ui.set_prompt_response("run_after_init", "no");

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
        assert!(ui.prompts_shown().contains(&"run_after_init".to_string()));
        // Should show hint when user declines
        assert!(ui.has_hint("bivvy run"));
    }

    #[test]
    fn init_non_interactive_skips_run_prompt() {
        let temp = TempDir::new().unwrap();

        let args = InitArgs {
            minimal: true,
            ..Default::default()
        };
        let cmd = InitCommand::new(temp.path(), args);
        let mut ui = MockUI::new();
        // Non-interactive (default for MockUI)

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
        // Should NOT prompt for run_after_init
        assert!(!ui.prompts_shown().contains(&"run_after_init".to_string()));
        // Should show hint instead
        assert!(ui.has_hint("bivvy run"));
    }

    #[test]
    fn init_interactive_yes_runs_workflow() {
        let temp = TempDir::new().unwrap();

        let args = InitArgs {
            minimal: true,
            ..Default::default()
        };
        let cmd = InitCommand::new(temp.path(), args);
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        ui.set_prompt_response("run_after_init", "yes");
        // If a step fails, the recovery menu prompts with a key like "recovery_brew".
        // Step names depend on template detection, so use a default response to abort.
        ui.set_default_prompt_response("abort");

        // The run may succeed or fail depending on template availability,
        // but the prompt should have been shown and the run attempted.
        let _result = cmd.execute(&mut ui);
        assert!(ui.prompts_shown().contains(&"run_after_init".to_string()));
    }
}
