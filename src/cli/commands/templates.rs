//! Templates command implementation.
//!
//! The `bivvy templates` command lists all available templates from
//! all sources (built-in, local, remote), organized by category.

use std::path::{Path, PathBuf};

use crate::cli::args::TemplatesArgs;
use crate::error::Result;
use crate::registry::resolver::Registry;
use crate::ui::theme::BivvyTheme;
use crate::ui::UserInterface;

use super::dispatcher::{Command, CommandResult};

/// The templates command implementation.
pub struct TemplatesCommand {
    project_root: PathBuf,
    args: TemplatesArgs,
}

impl TemplatesCommand {
    /// Create a new templates command.
    pub fn new(project_root: &Path, args: TemplatesArgs) -> Self {
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
    pub fn args(&self) -> &TemplatesArgs {
        &self.args
    }
}

impl Command for TemplatesCommand {
    fn execute(&self, ui: &mut dyn UserInterface) -> Result<CommandResult> {
        let registry = Registry::new(Some(&self.project_root))?;
        let manifest = registry.builtin().manifest();
        let theme = BivvyTheme::new();

        ui.show_header("Available Templates");
        ui.message("");

        let mut shown_count = 0;

        for category in &manifest.categories {
            // Filter by category if specified
            if let Some(ref filter) = self.args.category {
                if category.name != *filter {
                    continue;
                }
            }

            // Collect templates in this category that actually exist
            let mut category_templates: Vec<_> = category
                .templates
                .iter()
                .filter_map(|name| registry.get(name).map(|t| (name.as_str(), t)))
                .collect();

            if category_templates.is_empty() {
                continue;
            }

            // Filter by platform
            category_templates.retain(|(_, t)| t.platforms.iter().any(|p| p.is_current()));

            if category_templates.is_empty() {
                continue;
            }

            // Category header
            ui.message(&format!(
                "  {} {}",
                theme.key.apply_to(&category.name),
                theme.dim.apply_to(format!("— {}", category.description)),
            ));

            for (name, template) in &category_templates {
                ui.message(&format!(
                    "    {}  {}",
                    theme.highlight.apply_to(name),
                    theme.dim.apply_to(&template.description),
                ));
                shown_count += 1;
            }

            ui.message("");
        }

        // Show local/remote templates that aren't in the manifest
        let all_names = registry.all_template_names();
        let manifest_names: Vec<&str> = manifest.all_template_names().into_iter().collect();
        let extra: Vec<_> = all_names
            .iter()
            .filter(|n| !manifest_names.contains(&n.as_str()))
            .collect();

        if !extra.is_empty() && self.args.category.is_none() {
            ui.message(&format!(
                "  {} {}",
                theme.key.apply_to("custom"),
                theme.dim.apply_to("— Project and user templates"),
            ));
            for name in &extra {
                if let Some(template) = registry.get(name) {
                    ui.message(&format!(
                        "    {}  {}",
                        theme.highlight.apply_to(name.as_str()),
                        theme.dim.apply_to(&template.description),
                    ));
                    shown_count += 1;
                }
            }
            ui.message("");
        }

        ui.message(&format!(
            "  {} templates available. Use `bivvy add <template>` to add one.",
            shown_count
        ));

        Ok(CommandResult::success())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::MockUI;
    use tempfile::TempDir;

    #[test]
    fn templates_command_creation() {
        let temp = TempDir::new().unwrap();
        let args = TemplatesArgs::default();
        let cmd = TemplatesCommand::new(temp.path(), args);

        assert_eq!(cmd.project_root(), temp.path());
    }

    #[test]
    fn templates_lists_all_categories() {
        let temp = TempDir::new().unwrap();
        let args = TemplatesArgs::default();
        let cmd = TemplatesCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
        // Should show at least some well-known categories
        assert!(ui.messages().iter().any(|m| m.contains("ruby")));
        assert!(ui.messages().iter().any(|m| m.contains("node")));
        assert!(ui.messages().iter().any(|m| m.contains("rust")));
    }

    #[test]
    fn templates_shows_template_names_and_descriptions() {
        let temp = TempDir::new().unwrap();
        let args = TemplatesArgs::default();
        let cmd = TemplatesCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        // Should show well-known template names
        assert!(ui.messages().iter().any(|m| m.contains("bundler")));
        assert!(ui.messages().iter().any(|m| m.contains("yarn")));
        assert!(ui.messages().iter().any(|m| m.contains("cargo")));
    }

    #[test]
    fn templates_filter_by_category() {
        let temp = TempDir::new().unwrap();
        let args = TemplatesArgs {
            category: Some("ruby".to_string()),
        };
        let cmd = TemplatesCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
        // Should show ruby category
        assert!(ui.messages().iter().any(|m| m.contains("bundler")));
        // Should NOT show non-ruby templates
        assert!(!ui.messages().iter().any(|m| m.contains("yarn")));
        assert!(!ui.messages().iter().any(|m| m.contains("cargo")));
    }

    #[test]
    fn templates_shows_count() {
        let temp = TempDir::new().unwrap();
        let args = TemplatesArgs::default();
        let cmd = TemplatesCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        assert!(ui
            .messages()
            .iter()
            .any(|m| m.contains("templates available")));
    }

    #[test]
    fn templates_shows_add_hint() {
        let temp = TempDir::new().unwrap();
        let args = TemplatesArgs::default();
        let cmd = TemplatesCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        assert!(ui.messages().iter().any(|m| m.contains("bivvy add")));
    }

    #[test]
    fn templates_shows_custom_local_templates() {
        let temp = TempDir::new().unwrap();
        let templates_dir = temp.path().join(".bivvy").join("templates").join("steps");
        std::fs::create_dir_all(&templates_dir).unwrap();

        let custom = r#"
name: my-custom
description: "My custom setup step"
category: custom
step:
  command: "echo custom"
"#;
        std::fs::write(templates_dir.join("my-custom.yml"), custom).unwrap();

        let args = TemplatesArgs::default();
        let cmd = TemplatesCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
        assert!(ui.messages().iter().any(|m| m.contains("my-custom")));
        assert!(ui
            .messages()
            .iter()
            .any(|m| m.contains("My custom setup step")));
    }

    #[test]
    fn templates_filter_nonexistent_category_shows_nothing() {
        let temp = TempDir::new().unwrap();
        let args = TemplatesArgs {
            category: Some("nonexistent".to_string()),
        };
        let cmd = TemplatesCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
        // Should show 0 templates
        assert!(ui
            .messages()
            .iter()
            .any(|m| m.contains("0 templates available")));
    }
}
