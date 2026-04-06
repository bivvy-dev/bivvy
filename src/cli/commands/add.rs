//! Add command implementation.
//!
//! The `bivvy add` command adds a template step to an existing
//! `.bivvy/config.yml` file. It appends the step to the end of the
//! steps section and optionally adds it to a workflow.

use std::fs;
use std::path::{Path, PathBuf};

use crate::cli::args::AddArgs;
use crate::config::{load_config, CompletedCheck};
use crate::error::{BivvyError, Result};
use crate::registry::resolver::Registry;
use crate::registry::template::Template;
use crate::ui::{hints, UserInterface};

use super::dispatcher::{Command, CommandResult};

/// The add command implementation.
pub struct AddCommand {
    project_root: PathBuf,
    args: AddArgs,
    config_override: Option<PathBuf>,
}

impl AddCommand {
    /// Create a new add command.
    pub fn new(project_root: &Path, args: AddArgs) -> Self {
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
    pub fn args(&self) -> &AddArgs {
        &self.args
    }

    /// Format a step block for insertion into the config file.
    ///
    /// Produces the same format as `bivvy init` — a template reference
    /// with commented-out details showing what the template provides.
    fn format_step_block(step_name: &str, template_name: &str, template: &Template) -> String {
        let mut block = format!("  {}:\n    template: {}\n", step_name, template_name);

        // Show command as comment
        if let Some(ref cmd) = template.step.command {
            block.push_str(&format!("    # command: {}\n", cmd));
        }

        // Show completed_check as comment
        if let Some(ref check) = template.step.completed_check {
            match check {
                CompletedCheck::FileExists { path } => {
                    block.push_str("    # completed_check:\n");
                    block.push_str("    #   type: file_exists\n");
                    block.push_str(&format!("    #   path: \"{}\"\n", path));
                }
                CompletedCheck::CommandSucceeds { command } => {
                    block.push_str("    # completed_check:\n");
                    block.push_str("    #   type: command_succeeds\n");
                    block.push_str(&format!("    #   command: \"{}\"\n", command));
                }
                _ => {}
            }
        }

        // Show watches as comment
        if !template.step.watches.is_empty() {
            let watches: Vec<&str> = template.step.watches.iter().map(|s| s.as_str()).collect();
            block.push_str(&format!("    # watches: [{}]\n", watches.join(", ")));
        }

        block
    }

    /// Insert a step into the config file content (text-level editing).
    ///
    /// Appends the step block after the last step entry in the `steps:` section.
    fn insert_step_into_config(content: &str, step_block: &str) -> Result<String> {
        let lines: Vec<&str> = content.lines().collect();
        let mut result_lines: Vec<String> = Vec::new();
        let mut found_steps = false;
        let mut insert_index = None;
        let mut in_steps_section = false;

        for line in lines.iter() {
            let trimmed = line.trim();

            // Detect the `steps:` top-level key
            if trimmed == "steps:" && !line.starts_with(' ') && !line.starts_with('#') {
                found_steps = true;
                in_steps_section = true;
                result_lines.push(line.to_string());
                continue;
            }

            if in_steps_section {
                // We're inside the steps section. A new top-level key
                // (non-indented, non-empty, non-comment) ends it.
                let is_top_level_key = !trimmed.is_empty()
                    && !trimmed.starts_with('#')
                    && !line.starts_with(' ')
                    && !line.starts_with('\t');

                if is_top_level_key {
                    // Insert right before this line
                    insert_index = Some(result_lines.len());
                    in_steps_section = false;
                }
            }

            result_lines.push(line.to_string());
        }

        if !found_steps {
            return Err(BivvyError::ConfigValidationError {
                message: "No 'steps:' section found in config file".to_string(),
            });
        }

        // If we never left the steps section, append at the end
        let idx = insert_index.unwrap_or(result_lines.len());

        // Ensure there's a blank line before the new step block
        // when inserting at the boundary
        let step_block_with_newline = if idx < result_lines.len() {
            // Inserting before a top-level key — add trailing newline
            format!("{}\n", step_block)
        } else {
            // Appending at end
            format!("\n{}", step_block)
        };

        result_lines.insert(idx, step_block_with_newline.trim_end().to_string());

        let mut output = result_lines.join("\n");
        // Ensure file ends with newline
        if !output.ends_with('\n') {
            output.push('\n');
        }
        Ok(output)
    }

    /// Add a step name to a workflow's steps list in the config file content.
    fn add_to_workflow(
        content: &str,
        workflow_name: &str,
        step_name: &str,
        after: Option<&str>,
    ) -> Result<String> {
        let lines: Vec<&str> = content.lines().collect();

        // Find the target steps line
        let target = Self::find_workflow_steps_line(&lines, workflow_name);

        if let Some((line_idx, prefix, current_steps)) = target {
            let new_steps = Self::insert_step_name(&current_steps, step_name, after);
            let mut result_lines: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
            result_lines[line_idx] = format!("{}steps: [{}]", prefix, new_steps);
            let mut output = result_lines.join("\n");
            if !output.ends_with('\n') {
                output.push('\n');
            }
            Ok(output)
        } else {
            // Workflow not found — return content unchanged
            Ok(content.to_string())
        }
    }

    /// Find the `steps: [...]` line for a given workflow name.
    ///
    /// Returns `(line_index, indentation_prefix, steps_content)` if found.
    fn find_workflow_steps_line(
        lines: &[&str],
        workflow_name: &str,
    ) -> Option<(usize, String, String)> {
        // Find `workflows:` top-level key
        let workflows_idx = lines.iter().enumerate().find_map(|(i, line)| {
            let trimmed = line.trim();
            if trimmed == "workflows:" && !line.starts_with(' ') && !line.starts_with('#') {
                Some(i)
            } else {
                None
            }
        })?;

        // Find the specific workflow by name
        let workflow_pattern = format!("{}:", workflow_name);
        let mut workflow_idx = None;
        for (i, line) in lines.iter().enumerate().skip(workflows_idx + 1) {
            let trimmed = line.trim();
            if trimmed == workflow_pattern || trimmed.starts_with(&format!("{}: ", workflow_name)) {
                workflow_idx = Some(i);
                break;
            }
            // Stop at next top-level key
            if !trimmed.is_empty()
                && !trimmed.starts_with('#')
                && !line.starts_with(' ')
                && !line.starts_with('\t')
            {
                break;
            }
        }

        let workflow_idx = workflow_idx?;
        let workflow_indent = lines[workflow_idx].len() - lines[workflow_idx].trim_start().len();

        // Find the `steps:` line under this workflow
        for (i, line) in lines.iter().enumerate().skip(workflow_idx + 1) {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            let current_indent = line.len() - trimmed.len();

            // Stop if we hit a sibling or parent-level key
            if current_indent <= workflow_indent {
                break;
            }

            if trimmed.starts_with("steps:") {
                let prefix = line[..line.len() - trimmed.len()].to_string();
                if let (Some(start), Some(end)) = (trimmed.find('['), trimmed.find(']')) {
                    let list_content = trimmed[start + 1..end].to_string();
                    return Some((i, prefix, list_content));
                }
            }
        }

        None
    }

    /// Insert a step name into a comma-separated list string.
    fn insert_step_name(current: &str, new_step: &str, after: Option<&str>) -> String {
        let steps: Vec<&str> = current
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        let mut new_steps: Vec<String> = steps.iter().map(|s| s.to_string()).collect();

        if let Some(after_step) = after {
            if let Some(pos) = new_steps.iter().position(|s| s == after_step) {
                new_steps.insert(pos + 1, new_step.to_string());
            } else {
                // --after target not found, append
                new_steps.push(new_step.to_string());
            }
        } else {
            new_steps.push(new_step.to_string());
        }

        new_steps.join(", ")
    }
}

impl Command for AddCommand {
    fn execute(&self, ui: &mut dyn UserInterface) -> Result<CommandResult> {
        let config_path = self.project_root.join(".bivvy/config.yml");

        // Validate config exists
        if !config_path.exists() {
            ui.error("No configuration found. Run 'bivvy init' first.");
            return Ok(CommandResult::failure(2));
        }

        let template_name = &self.args.template;

        // Validate template exists
        let registry = Registry::new(Some(&self.project_root))?;
        let template = registry
            .get(template_name)
            .ok_or_else(|| BivvyError::UnknownTemplate {
                name: template_name.clone(),
            })?;

        // Determine step name
        let step_name = self
            .args
            .step_name
            .as_deref()
            .unwrap_or(template_name.as_str());

        // Validate step doesn't already exist
        let config = load_config(&self.project_root, self.config_override.as_deref())?;
        if config.steps.contains_key(step_name) {
            ui.error(&format!(
                "Step '{}' already exists in configuration. Use a different name with --as.",
                step_name
            ));
            return Ok(CommandResult::failure(1));
        }

        // Read the raw config file
        let content = fs::read_to_string(&config_path)?;

        // Format the new step block
        let step_block = Self::format_step_block(step_name, template_name, template);

        // Insert the step into the config
        let mut new_content = Self::insert_step_into_config(&content, &step_block)?;

        // Add to workflow unless --no-workflow
        if !self.args.no_workflow {
            let workflow = self.args.workflow.as_deref().unwrap_or("default");

            new_content = Self::add_to_workflow(
                &new_content,
                workflow,
                step_name,
                self.args.after.as_deref(),
            )?;
        }

        // Write the updated config
        fs::write(&config_path, &new_content)?;

        ui.success(&format!(
            "Added '{}' step using template '{}'",
            step_name, template_name
        ));

        if !self.args.no_workflow {
            let workflow = self.args.workflow.as_deref().unwrap_or("default");
            ui.message(&format!("  Added to '{}' workflow", workflow));
        }

        ui.show_hint(&hints::after_add(step_name));

        Ok(CommandResult::success())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::builtin::BuiltinLoader;
    use crate::ui::MockUI;
    use tempfile::TempDir;

    fn setup_project(config: &str) -> TempDir {
        let temp = TempDir::new().unwrap();
        let bivvy_dir = temp.path().join(".bivvy");
        fs::create_dir_all(&bivvy_dir).unwrap();
        fs::write(bivvy_dir.join("config.yml"), config).unwrap();
        temp
    }

    #[test]
    fn add_command_creation() {
        let temp = TempDir::new().unwrap();
        let args = AddArgs {
            template: "bundle-install".to_string(),
            ..Default::default()
        };
        let cmd = AddCommand::new(temp.path(), args);

        assert_eq!(cmd.project_root(), temp.path());
        assert_eq!(cmd.args().template, "bundle-install");
    }

    #[test]
    fn add_fails_without_config() {
        let temp = TempDir::new().unwrap();
        let args = AddArgs {
            template: "bundle-install".to_string(),
            ..Default::default()
        };
        let cmd = AddCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(!result.success);
        assert_eq!(result.exit_code, 2);
    }

    #[test]
    fn add_fails_for_unknown_template() {
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
        let args = AddArgs {
            template: "nonexistent".to_string(),
            ..Default::default()
        };
        let cmd = AddCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui);

        assert!(result.is_err());
    }

    #[test]
    fn add_fails_for_duplicate_step() {
        let config = r#"
app_name: Test
steps:
  bundle-install:
    command: echo hello
workflows:
  default:
    steps: [bundle-install]
"#;
        let temp = setup_project(config);
        let args = AddArgs {
            template: "bundle-install".to_string(),
            ..Default::default()
        };
        let cmd = AddCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(!result.success);
        assert_eq!(result.exit_code, 1);
    }

    #[test]
    fn add_succeeds_with_valid_template() {
        let config = "app_name: Test\n\nsteps:\n  hello:\n    command: echo hello\n\nworkflows:\n  default:\n    steps: [hello]\n";
        let temp = setup_project(config);
        let args = AddArgs {
            template: "bundle-install".to_string(),
            ..Default::default()
        };
        let cmd = AddCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);

        let new_config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
        assert!(new_config.contains("  bundle-install:\n    template: bundle-install\n"));
        assert!(new_config.contains("# command: bundle install"));
    }

    #[test]
    fn add_updates_workflow() {
        let config = "app_name: Test\n\nsteps:\n  hello:\n    command: echo hello\n\nworkflows:\n  default:\n    steps: [hello]\n";
        let temp = setup_project(config);
        let args = AddArgs {
            template: "bundle-install".to_string(),
            ..Default::default()
        };
        let cmd = AddCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        let new_config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
        assert!(new_config.contains("steps: [hello, bundle-install]"));
    }

    #[test]
    fn add_with_custom_step_name() {
        let config = "app_name: Test\n\nsteps:\n  hello:\n    command: echo hello\n\nworkflows:\n  default:\n    steps: [hello]\n";
        let temp = setup_project(config);
        let args = AddArgs {
            template: "bundle-install".to_string(),
            step_name: Some("ruby_deps".to_string()),
            ..Default::default()
        };
        let cmd = AddCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
        let new_config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
        assert!(new_config.contains("  ruby_deps:\n    template: bundle-install\n"));
        assert!(new_config.contains("steps: [hello, ruby_deps]"));
    }

    #[test]
    fn add_with_no_workflow() {
        let config = "app_name: Test\n\nsteps:\n  hello:\n    command: echo hello\n\nworkflows:\n  default:\n    steps: [hello]\n";
        let temp = setup_project(config);
        let args = AddArgs {
            template: "bundle-install".to_string(),
            no_workflow: true,
            ..Default::default()
        };
        let cmd = AddCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
        let new_config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
        assert!(new_config.contains("  bundle-install:\n    template: bundle-install\n"));
        // Workflow should NOT be updated
        assert!(new_config.contains("steps: [hello]"));
    }

    #[test]
    fn add_with_after() {
        let config = "app_name: Test\n\nsteps:\n  install:\n    command: npm install\n  build:\n    command: npm build\n\nworkflows:\n  default:\n    steps: [install, build]\n";
        let temp = setup_project(config);
        let args = AddArgs {
            template: "bundle-install".to_string(),
            after: Some("install".to_string()),
            ..Default::default()
        };
        let cmd = AddCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
        let new_config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
        assert!(new_config.contains("steps: [install, bundle-install, build]"));
    }

    #[test]
    fn add_shows_success_message() {
        let config = "app_name: Test\n\nsteps:\n  hello:\n    command: echo hello\n\nworkflows:\n  default:\n    steps: [hello]\n";
        let temp = setup_project(config);
        let args = AddArgs {
            template: "bundle-install".to_string(),
            ..Default::default()
        };
        let cmd = AddCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        assert!(ui.successes().iter().any(|m| m.contains("bundle-install")));
        assert!(ui.messages().iter().any(|m| m.contains("default")));
    }

    #[test]
    fn add_preserves_existing_comments() {
        let config = "# My project config\napp_name: Test\n\n# Steps section\nsteps:\n  hello:\n    command: echo hello\n    # A custom comment\n\nworkflows:\n  default:\n    steps: [hello]\n";
        let temp = setup_project(config);
        let args = AddArgs {
            template: "bundle-install".to_string(),
            ..Default::default()
        };
        let cmd = AddCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        let new_config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
        assert!(new_config.contains("# My project config"));
        assert!(new_config.contains("# Steps section"));
        assert!(new_config.contains("# A custom comment"));
    }

    // --- Unit tests for helper functions ---

    #[test]
    fn format_step_block_basic() {
        let loader = BuiltinLoader::new().unwrap();
        let template = loader.get("bundle-install").unwrap();

        let block = AddCommand::format_step_block("bundle-install", "bundle-install", template);

        assert!(block.contains("  bundle-install:\n    template: bundle-install\n"));
        assert!(block.contains("# command: bundle install"));
        assert!(block.contains("# completed_check:"));
        assert!(block.contains("# watches:"));
    }

    #[test]
    fn format_step_block_custom_name() {
        let loader = BuiltinLoader::new().unwrap();
        let template = loader.get("bundle-install").unwrap();

        let block = AddCommand::format_step_block("ruby_deps", "bundle-install", template);

        assert!(block.starts_with("  ruby_deps:\n    template: bundle-install\n"));
    }

    #[test]
    fn insert_step_into_config_appends() {
        let config = "app_name: Test\n\nsteps:\n  hello:\n    command: echo hello\n\nworkflows:\n  default:\n    steps: [hello]\n";
        let step_block = "  world:\n    template: world\n";

        let result = AddCommand::insert_step_into_config(config, step_block).unwrap();

        // Step should appear before workflows
        let steps_pos = result.find("  world:").unwrap();
        let workflows_pos = result.find("workflows:").unwrap();
        assert!(steps_pos < workflows_pos);
    }

    #[test]
    fn insert_step_into_config_no_steps_section() {
        let config = "app_name: Test\n";
        let step_block = "  hello:\n    command: echo hello\n";

        let result = AddCommand::insert_step_into_config(config, step_block);

        assert!(result.is_err());
    }

    #[test]
    fn insert_step_at_end_when_no_following_section() {
        let config = "app_name: Test\n\nsteps:\n  hello:\n    command: echo hello\n";
        let step_block = "  world:\n    template: world\n";

        let result = AddCommand::insert_step_into_config(config, step_block).unwrap();

        assert!(result.contains("  world:\n    template: world\n"));
    }

    #[test]
    fn insert_step_name_appends() {
        let result = AddCommand::insert_step_name("hello, world", "new", None);
        assert_eq!(result, "hello, world, new");
    }

    #[test]
    fn insert_step_name_after() {
        let result = AddCommand::insert_step_name("hello, world", "new", Some("hello"));
        assert_eq!(result, "hello, new, world");
    }

    #[test]
    fn insert_step_name_after_nonexistent_appends() {
        let result = AddCommand::insert_step_name("hello, world", "new", Some("missing"));
        assert_eq!(result, "hello, world, new");
    }

    #[test]
    fn insert_step_name_empty_list() {
        let result = AddCommand::insert_step_name("", "new", None);
        assert_eq!(result, "new");
    }

    #[test]
    fn add_to_workflow_updates_default() {
        let config = "workflows:\n  default:\n    steps: [hello]\n";

        let result = AddCommand::add_to_workflow(config, "default", "world", None).unwrap();

        assert!(result.contains("steps: [hello, world]"));
    }

    #[test]
    fn add_to_workflow_with_after() {
        let config = "workflows:\n  default:\n    steps: [hello, goodbye]\n";

        let result =
            AddCommand::add_to_workflow(config, "default", "world", Some("hello")).unwrap();

        assert!(result.contains("steps: [hello, world, goodbye]"));
    }
}
