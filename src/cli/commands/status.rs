//! Status command implementation.
//!
//! The `bivvy status` command shows current setup status.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::cli::args::StatusArgs;
use crate::config::load_merged_config;
use crate::environment::resolver::ResolvedEnvironment;
use crate::error::{BivvyError, Result};
use crate::requirements::checker::GapChecker;
use crate::requirements::probe::EnvironmentProbe;
use crate::requirements::registry::RequirementRegistry;
use crate::requirements::status::RequirementStatus;
use crate::state::{ProjectId, StateStore, StepStatus};
use crate::ui::theme::BivvyTheme;
use crate::ui::{format_relative_time, hints, OutputMode, StatusKind, UserInterface};

use super::dispatcher::{Command, CommandResult};

/// The status command implementation.
pub struct StatusCommand {
    project_root: PathBuf,
    args: StatusArgs,
}

impl StatusCommand {
    /// Create a new status command.
    pub fn new(project_root: &Path, args: StatusArgs) -> Self {
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
    pub fn args(&self) -> &StatusArgs {
        &self.args
    }

    /// Resolve the target environment using the priority chain.
    fn resolve_environment(&self, config: &crate::config::BivvyConfig) -> ResolvedEnvironment {
        ResolvedEnvironment::resolve_from_config(self.args.env.as_deref(), &config.settings)
    }
}

impl Command for StatusCommand {
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

        // Get project identity
        let project_id = ProjectId::from_path(&self.project_root)?;

        // Load state
        let state = StateStore::load(&project_id)?;

        let theme = BivvyTheme::new();

        // Resolve environment
        let resolved_env = self.resolve_environment(&config);

        // Show header: ⛺ AppName — Status
        let app_name = config.app_name.as_deref().unwrap_or("Bivvy Setup");
        ui.message(&format!(
            "\n  {} {} {} {}\n",
            theme.header.apply_to("⛺"),
            theme.highlight.apply_to(app_name),
            theme.dim.apply_to("—"),
            theme.dim.apply_to("Status"),
        ));

        // Show environment info
        ui.message(&format!(
            "  {} {} ({})",
            theme.key.apply_to("Environment:"),
            theme.highlight.apply_to(&resolved_env.name),
            theme.dim.apply_to(resolved_env.source.to_string()),
        ));
        ui.message("");

        // Show last run info with relative time
        if let Some(last_run) = state.last_run_record() {
            ui.message(&format!(
                "  {} {} {} {}",
                theme.key.apply_to("Last run:"),
                theme.dim.apply_to(format_relative_time(last_run.timestamp)),
                theme.dim.apply_to("·"),
                theme
                    .dim
                    .apply_to(format!("{} workflow", last_run.workflow)),
            ));
            ui.message("");
        }

        // Show step status
        ui.message(&format!("  {}", theme.key.apply_to("Steps:")));

        let step_names: Vec<&String> = if let Some(ref step_name) = self.args.step {
            if config.steps.contains_key(step_name) {
                vec![step_name]
            } else {
                ui.error(&format!("Unknown step: {}", step_name));
                return Ok(CommandResult::failure(1));
            }
        } else {
            config.steps.keys().collect()
        };

        for step_name in &step_names {
            let step_state = state.get_step(step_name);
            let status = step_state.map(|s| s.status).unwrap_or(StepStatus::NeverRun);
            let kind = StatusKind::from(status);

            // Build the right-side info (duration or relative time)
            let right_side = step_state
                .and_then(|s| {
                    if status == StepStatus::NeverRun {
                        return None;
                    }
                    // Show duration if available, otherwise relative timestamp
                    if let Some(ms) = s.duration_ms {
                        let d = Duration::from_millis(ms);
                        Some(
                            theme
                                .duration
                                .apply_to(crate::ui::format_duration(d))
                                .to_string(),
                        )
                    } else {
                        s.last_run
                            .map(|ts| theme.dim.apply_to(format_relative_time(ts)).to_string())
                    }
                })
                .unwrap_or_default();

            ui.message(&format!(
                "    {} {:<20} {}",
                kind.styled(&theme),
                step_name,
                right_side,
            ));
        }

        // Show requirements section
        let all_reqs: HashSet<String> = config
            .steps
            .values()
            .flat_map(|s| s.requires.iter().cloned())
            .collect();

        if !all_reqs.is_empty() {
            ui.message("");
            ui.message(&format!("  {}", theme.key.apply_to("Requirements:")));

            let probe = EnvironmentProbe::run();
            let req_registry = RequirementRegistry::new().with_custom(&config.requirements);
            let mut gap_checker = GapChecker::new(&req_registry, &probe, &self.project_root);

            let mut sorted_reqs: Vec<&str> = all_reqs.iter().map(|s| s.as_str()).collect();
            sorted_reqs.sort();

            for req_name in sorted_reqs {
                let status = gap_checker.check_one(req_name);
                let (icon, desc) = format_requirement_status(&theme, &status);
                ui.message(&format!("    {} {:<20} {}", icon, req_name, desc));
            }
        }

        // Show recommendations
        let never_run: Vec<_> = config
            .steps
            .keys()
            .filter(|s| {
                state
                    .get_step(s)
                    .map(|st| st.status == StepStatus::NeverRun)
                    .unwrap_or(true)
            })
            .cloned()
            .collect();

        let failed: Vec<_> = config
            .steps
            .keys()
            .filter(|s| {
                state
                    .get_step(s)
                    .map(|st| st.status == StepStatus::Failed)
                    .unwrap_or(false)
            })
            .cloned()
            .collect();

        if !failed.is_empty() {
            ui.message("");
            ui.show_hint(&hints::after_failed_run(&failed));
        } else if !never_run.is_empty() {
            ui.message("");
            if never_run.len() == config.steps.len() {
                ui.show_hint(hints::all_steps_pending());
            } else {
                ui.show_hint(&hints::some_steps_pending(&never_run));
            }
        }

        Ok(CommandResult::success())
    }
}

/// Format a requirement status for display.
fn format_requirement_status(theme: &BivvyTheme, status: &RequirementStatus) -> (String, String) {
    match status {
        RequirementStatus::Satisfied => (
            theme.success.apply_to("✓").to_string(),
            theme.dim.apply_to("available").to_string(),
        ),
        RequirementStatus::SystemOnly { warning, .. } => (
            theme.warning.apply_to("⚠").to_string(),
            theme.warning.apply_to(warning).to_string(),
        ),
        RequirementStatus::Inactive {
            manager,
            activation_hint,
            ..
        } => (
            theme.warning.apply_to("⚠").to_string(),
            theme
                .warning
                .apply_to(format!("{} not activated ({})", manager, activation_hint))
                .to_string(),
        ),
        RequirementStatus::ServiceDown { start_hint, .. } => (
            theme.error.apply_to("✗").to_string(),
            theme.error.apply_to(start_hint).to_string(),
        ),
        RequirementStatus::Missing { install_hint, .. } => {
            let hint = install_hint.as_deref().unwrap_or("not installed");
            (
                theme.error.apply_to("✗").to_string(),
                theme.error.apply_to(hint).to_string(),
            )
        }
        RequirementStatus::Unknown => (
            theme.dim.apply_to("?").to_string(),
            theme.dim.apply_to("unknown requirement").to_string(),
        ),
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
    fn status_command_creation() {
        let temp = TempDir::new().unwrap();
        let args = StatusArgs::default();
        let cmd = StatusCommand::new(temp.path(), args);

        assert_eq!(cmd.project_root(), temp.path());
    }

    #[test]
    fn status_no_config() {
        let temp = TempDir::new().unwrap();
        let args = StatusArgs::default();
        let cmd = StatusCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(!result.success);
        assert_eq!(result.exit_code, 2);
    }

    #[test]
    fn status_with_config() {
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
        let args = StatusArgs::default();
        let cmd = StatusCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
    }

    #[test]
    fn status_applies_config_default_output() {
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
        let args = StatusArgs::default();
        let cmd = StatusCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        assert_eq!(ui.output_mode(), crate::ui::OutputMode::Quiet);
    }

    #[test]
    fn status_shows_pending_for_never_run_steps() {
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
        let args = StatusArgs::default();
        let cmd = StatusCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        // Never-run steps should show ◌ icon (pending)
        assert!(ui
            .messages()
            .iter()
            .any(|m| m.contains("◌") && m.contains("hello")));
        // Should NOT use warning for pending steps
        assert!(!ui.warnings().iter().any(|m| m.contains("hello")));
    }

    #[test]
    fn status_shows_header_with_app_name() {
        let config = r#"
app_name: MyApp
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;
        let temp = setup_project(config);
        let args = StatusArgs::default();
        let cmd = StatusCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        // Should show app name in header
        assert!(ui.messages().iter().any(|m| m.contains("MyApp")));
        // Should show ⛺ tent icon
        assert!(ui.messages().iter().any(|m| m.contains("⛺")));
        // Should show "Status" label
        assert!(ui.messages().iter().any(|m| m.contains("Status")));
    }

    #[test]
    fn status_shows_hint_for_all_pending() {
        let config = r#"
app_name: Test
steps:
  hello:
    command: echo hello
  world:
    command: echo world
workflows:
  default:
    steps: [hello, world]
"#;
        let temp = setup_project(config);
        let args = StatusArgs::default();
        let cmd = StatusCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        // All steps pending → hint to run setup
        assert!(ui.hints().iter().any(|m| m.contains("bivvy run")));
    }

    #[test]
    fn status_shows_steps_label() {
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
        let args = StatusArgs::default();
        let cmd = StatusCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        assert!(ui.messages().iter().any(|m| m.contains("Steps:")));
    }

    #[test]
    fn status_shows_requirements_section_when_requires_present() {
        let config = r#"
app_name: Test
steps:
  install_deps:
    command: bundle install
    requires:
      - ruby
workflows:
  default:
    steps: [install_deps]
"#;
        let temp = setup_project(config);
        let args = StatusArgs::default();
        let cmd = StatusCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        // Should show Requirements label
        assert!(ui.messages().iter().any(|m| m.contains("Requirements:")));
        // Should show ruby requirement with some status
        assert!(ui.messages().iter().any(|m| m.contains("ruby")));
    }

    #[test]
    fn status_no_requirements_section_when_no_requires() {
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
        let args = StatusArgs::default();
        let cmd = StatusCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        // Should NOT show Requirements label
        assert!(!ui.messages().iter().any(|m| m.contains("Requirements:")));
    }

    #[test]
    fn status_requirements_deduplicates() {
        let config = r#"
app_name: Test
steps:
  step_a:
    command: echo a
    requires:
      - node
  step_b:
    command: echo b
    requires:
      - node
workflows:
  default:
    steps: [step_a, step_b]
"#;
        let temp = setup_project(config);
        let args = StatusArgs::default();
        let cmd = StatusCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        // "node" should appear only once in the requirements section
        let node_count = ui
            .messages()
            .iter()
            .filter(|m| m.contains("node") && !m.contains("Steps:") && !m.contains("⛺"))
            .count();
        assert_eq!(node_count, 1, "node should appear exactly once");
    }

    #[test]
    fn status_shows_environment_info() {
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
        let args = StatusArgs::default();
        let cmd = StatusCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        // Should show Environment label with fallback
        assert!(ui.messages().iter().any(|m| m.contains("Environment:")));
        assert!(ui.messages().iter().any(|m| m.contains("development")));
    }

    #[test]
    fn status_shows_environment_from_env_flag() {
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
        let args = StatusArgs {
            env: Some("staging".to_string()),
            ..Default::default()
        };
        let cmd = StatusCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        assert!(ui.messages().iter().any(|m| m.contains("staging")));
    }

    #[test]
    fn format_requirement_status_satisfied() {
        let theme = BivvyTheme::new();
        let (icon, desc) = format_requirement_status(&theme, &RequirementStatus::Satisfied);
        assert!(icon.contains("✓"));
        assert!(desc.contains("available"));
    }

    #[test]
    fn format_requirement_status_missing() {
        let theme = BivvyTheme::new();
        let status = RequirementStatus::Missing {
            install_template: None,
            install_hint: Some("Install via mise".to_string()),
        };
        let (icon, desc) = format_requirement_status(&theme, &status);
        assert!(icon.contains("✗"));
        assert!(desc.contains("Install via mise"));
    }

    #[test]
    fn format_requirement_status_unknown() {
        let theme = BivvyTheme::new();
        let (icon, desc) = format_requirement_status(&theme, &RequirementStatus::Unknown);
        assert!(icon.contains("?"));
        assert!(desc.contains("unknown requirement"));
    }
}
