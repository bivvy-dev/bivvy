//! Status command implementation.
//!
//! The `bivvy status` command shows current setup status.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::json;

use crate::cli::args::StatusArgs;
use crate::config::load_config;
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
    config_override: Option<PathBuf>,
}

impl StatusCommand {
    /// Create a new status command.
    pub fn new(project_root: &Path, args: StatusArgs) -> Self {
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
    pub fn args(&self) -> &StatusArgs {
        &self.args
    }

    /// Resolve the target environment using the priority chain.
    fn resolve_environment(&self, config: &crate::config::BivvyConfig) -> ResolvedEnvironment {
        ResolvedEnvironment::resolve_from_config(self.args.env.as_deref(), &config.settings)
    }
}

impl StatusCommand {
    /// Produce JSON output for the status command.
    fn execute_json(
        &self,
        ui: &mut dyn UserInterface,
        config: &crate::config::BivvyConfig,
        state: &StateStore,
        resolved_env: &ResolvedEnvironment,
    ) -> Result<CommandResult> {
        let app_name = config.app_name.as_deref().unwrap_or("Bivvy Setup");

        // Build step statuses
        let step_names: Vec<&String> = if let Some(ref step_name) = self.args.step {
            if config.steps.contains_key(step_name) {
                vec![step_name]
            } else {
                let err = json!({ "error": format!("Unknown step: {}", step_name) });
                ui.message(
                    &serde_json::to_string_pretty(&err)
                        .map_err(|e| anyhow::anyhow!("JSON serialization failed: {e}"))?,
                );
                return Ok(CommandResult::failure(1));
            }
        } else {
            config.steps.keys().collect()
        };

        let steps: Vec<serde_json::Value> = step_names
            .iter()
            .map(|name| {
                let step_config = config.steps.get(*name);
                let skipped = step_config
                    .map(|s| {
                        !s.scoping.only_environments.is_empty()
                            && !s
                                .scoping
                                .only_environments
                                .iter()
                                .any(|e| e == &resolved_env.name)
                    })
                    .unwrap_or(false);

                if skipped {
                    return json!({
                        "name": name,
                        "status": "skipped",
                        "reason": format!("skipped in {}", resolved_env.name),
                    });
                }

                let step_state = state.get_step(name);
                let status = step_state.map(|s| s.status).unwrap_or(StepStatus::NeverRun);
                let status_str = match status {
                    StepStatus::Success => "success",
                    StepStatus::Failed => "failed",
                    StepStatus::Skipped => "skipped",
                    StepStatus::NeverRun => "pending",
                };

                let mut obj = json!({
                    "name": name,
                    "status": status_str,
                });

                if let Some(ss) = step_state {
                    if let Some(ts) = ss.last_run {
                        obj["last_run"] = json!(ts.to_rfc3339());
                    }
                    if let Some(ms) = ss.duration_ms {
                        obj["duration_ms"] = json!(ms);
                    }
                }

                obj
            })
            .collect();

        // Build requirements
        let all_reqs: HashSet<String> = config
            .steps
            .values()
            .flat_map(|s| s.requires.iter().cloned())
            .collect();

        let requirements: Vec<serde_json::Value> = if !all_reqs.is_empty() {
            let probe = EnvironmentProbe::run();
            let req_registry = RequirementRegistry::new().with_custom(&config.requirements);
            let mut gap_checker = GapChecker::new(&req_registry, &probe, &self.project_root);

            let mut sorted_reqs: Vec<&str> = all_reqs.iter().map(|s| s.as_str()).collect();
            sorted_reqs.sort();

            sorted_reqs
                .iter()
                .map(|req_name| {
                    let status = gap_checker.check_one(req_name);
                    let (status_str, detail) = json_requirement_status(&status);
                    let mut obj = json!({
                        "name": req_name,
                        "status": status_str,
                    });
                    if let Some(d) = detail {
                        obj["detail"] = json!(d);
                    }
                    obj
                })
                .collect()
        } else {
            Vec::new()
        };

        // Assemble top-level JSON
        let mut output = json!({
            "app_name": app_name,
            "environment": {
                "name": resolved_env.name,
                "source": resolved_env.source.to_string(),
            },
            "steps": steps,
        });

        if !requirements.is_empty() {
            output["requirements"] = json!(requirements);
        }

        let json_str = serde_json::to_string_pretty(&output)
            .map_err(|e| anyhow::anyhow!("JSON serialization failed: {e}"))?;
        ui.message(&json_str);

        Ok(CommandResult::success())
    }
}

impl Command for StatusCommand {
    fn execute(&self, ui: &mut dyn UserInterface) -> Result<CommandResult> {
        // Create event bus for structured logging
        let mut event_bus = crate::logging::EventBus::new();
        if let Ok(logger) = crate::logging::EventLogger::new(
            crate::logging::default_log_dir(),
            &format!("sess_{}_status", chrono::Utc::now().format("%Y%m%d%H%M%S"),),
            crate::logging::RetentionPolicy::default(),
        ) {
            event_bus.add_consumer(Box::new(logger));
        }
        let start = std::time::Instant::now();

        event_bus.emit(&crate::logging::BivvyEvent::SessionStarted {
            command: "status".to_string(),
            args: vec![],
            version: env!("CARGO_PKG_VERSION").to_string(),
            os: Some(std::env::consts::OS.to_string()),
            working_directory: Some(self.project_root.display().to_string()),
        });

        // Load configuration
        let config = match load_config(&self.project_root, self.config_override.as_deref()) {
            Ok(c) => c,
            Err(BivvyError::ConfigNotFound { .. }) => {
                ui.error("No configuration found. Run 'bivvy init' first.");
                event_bus.emit(&crate::logging::BivvyEvent::SessionEnded {
                    exit_code: 2,
                    duration_ms: start.elapsed().as_millis() as u64,
                });
                return Ok(CommandResult::failure(2));
            }
            Err(e) => return Err(e),
        };

        // Apply config default_output when no CLI flag was explicitly set
        if ui.output_mode() == OutputMode::Normal {
            ui.set_output_mode(config.settings.output.default_output.into());
        }

        // Get project identity
        let project_id = ProjectId::from_path(&self.project_root)?;

        // Load state (baseline migrations not needed for status)
        let (state, _) = StateStore::load(&project_id)?;

        // Resolve environment
        let resolved_env = self.resolve_environment(&config);

        // JSON output mode
        if self.args.json {
            let result = self.execute_json(ui, &config, &state, &resolved_env)?;
            event_bus.emit(&crate::logging::BivvyEvent::SessionEnded {
                exit_code: result.exit_code,
                duration_ms: start.elapsed().as_millis() as u64,
            });
            return Ok(result);
        }

        let theme = BivvyTheme::new();

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

        // Show last run info by checking step state timestamps
        let most_recent_step_run = state.steps.values().filter_map(|s| s.last_run).max();
        if let Some(last_ts) = most_recent_step_run {
            ui.message(&format!(
                "  {} {}",
                theme.key.apply_to("Last activity:"),
                theme.dim.apply_to(format_relative_time(last_ts)),
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
                event_bus.emit(&crate::logging::BivvyEvent::SessionEnded {
                    exit_code: 1,
                    duration_ms: start.elapsed().as_millis() as u64,
                });
                return Ok(CommandResult::failure(1));
            }
        } else {
            config.steps.keys().collect()
        };

        for step_name in &step_names {
            // Check if step is skipped by only_environments
            let step_config = config.steps.get(*step_name);
            let skipped = step_config
                .map(|s| {
                    !s.scoping.only_environments.is_empty()
                        && !s
                            .scoping
                            .only_environments
                            .iter()
                            .any(|e| e == &resolved_env.name)
                })
                .unwrap_or(false);

            if skipped {
                ui.message(&format!(
                    "    {} {:<20} {}",
                    theme.dim.apply_to("⊘"),
                    step_name,
                    theme
                        .dim
                        .apply_to(format!("(skipped in {})", resolved_env.name)),
                ));
                continue;
            }

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

        event_bus.emit(&crate::logging::BivvyEvent::SessionEnded {
            exit_code: 0,
            duration_ms: start.elapsed().as_millis() as u64,
        });
        Ok(CommandResult::success())
    }
}

/// Convert a requirement status to JSON-friendly strings.
fn json_requirement_status(status: &RequirementStatus) -> (&'static str, Option<String>) {
    match status {
        RequirementStatus::Satisfied => ("satisfied", None),
        RequirementStatus::SystemOnly { warning, .. } => ("warning", Some(warning.clone())),
        RequirementStatus::Inactive {
            manager,
            activation_hint,
            ..
        } => (
            "warning",
            Some(format!("{} not activated ({})", manager, activation_hint)),
        ),
        RequirementStatus::ServiceDown { start_hint, .. } => ("missing", Some(start_hint.clone())),
        RequirementStatus::Missing { install_hint, .. } => (
            "missing",
            install_hint
                .clone()
                .or_else(|| Some("not installed".to_string())),
        ),
        RequirementStatus::Unknown => ("unknown", Some("unknown requirement".to_string())),
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

        // Should show Environment label
        assert!(ui.messages().iter().any(|m| m.contains("Environment:")));
        // The resolved name depends on where the test runs:
        // "ci" in CI (auto-detected), "development" locally (fallback)
        let has_env_name = ui
            .messages()
            .iter()
            .any(|m| m.contains("development") || m.contains("ci"));
        assert!(has_env_name, "Should show resolved environment name");
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

    #[test]
    fn status_shows_skipped_steps_for_environment() {
        let config = r#"
app_name: Test
steps:
  dev_only:
    command: echo dev
    only_environments:
      - development
  always_run:
    command: echo always
workflows:
  default:
    steps: [dev_only, always_run]
"#;
        let temp = setup_project(config);
        let args = StatusArgs {
            env: Some("ci".to_string()),
            ..Default::default()
        };
        let cmd = StatusCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        // dev_only should show as skipped in ci
        assert!(
            ui.messages()
                .iter()
                .any(|m| m.contains("dev_only") && m.contains("skipped in ci")),
            "Expected 'dev_only' to be shown as 'skipped in ci', messages: {:?}",
            ui.messages()
        );
        // always_run should show normally (no only_environments = runs in all)
        assert!(
            ui.messages()
                .iter()
                .any(|m| m.contains("always_run") && !m.contains("skipped")),
            "Expected 'always_run' to show without skipped, messages: {:?}",
            ui.messages()
        );
    }

    #[test]
    fn status_no_skipped_when_environment_matches() {
        let config = r#"
app_name: Test
steps:
  ci_step:
    command: echo ci
    only_environments:
      - ci
workflows:
  default:
    steps: [ci_step]
"#;
        let temp = setup_project(config);
        let args = StatusArgs {
            env: Some("ci".to_string()),
            ..Default::default()
        };
        let cmd = StatusCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        // ci_step should show normally (pending icon), not skipped
        assert!(
            ui.messages()
                .iter()
                .any(|m| m.contains("ci_step") && m.contains("◌")),
            "Expected 'ci_step' to show as pending, messages: {:?}",
            ui.messages()
        );
        assert!(
            !ui.messages()
                .iter()
                .any(|m| m.contains("ci_step") && m.contains("skipped")),
            "Expected 'ci_step' NOT to be skipped, messages: {:?}",
            ui.messages()
        );
    }
}
