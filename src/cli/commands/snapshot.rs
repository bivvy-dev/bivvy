//! Snapshot command implementation.
//!
//! The `bivvy snapshot` command manages named snapshots for change check baselines.
//!
//! Usage:
//!   bivvy snapshot <slug>                    # Capture snapshot for all change checks
//!   bivvy snapshot <slug> --step <name>      # Capture for a specific step
//!   bivvy snapshot <slug> --workflow <name>  # Capture for a specific workflow
//!   bivvy snapshot list                      # List all named snapshots
//!   bivvy snapshot delete <slug>             # Delete a named snapshot

use std::path::{Path, PathBuf};

use crate::checks::{change, Check};
use crate::config::{load_config, load_for_run, load_project_config};
use crate::error::{BivvyError, Result};
use crate::snapshots::{SnapshotKey, SnapshotStore};
use crate::state::ProjectId;
use crate::ui::UserInterface;

use super::dispatcher::{Command, CommandResult};

/// Arguments for the `snapshot` command.
///
/// Supports two modes:
/// - Subcommand mode: `bivvy snapshot list`, `bivvy snapshot delete <slug>`
/// - Direct capture mode: `bivvy snapshot <slug> [--step <name>] [--workflow <name>]`
#[derive(Debug, Clone, clap::Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct SnapshotArgs {
    #[command(subcommand)]
    pub action: Option<SnapshotAction>,

    /// Snapshot name (slug) — captures a snapshot when provided directly
    pub slug: Option<String>,

    /// Capture for a specific step only
    #[arg(long)]
    pub step: Option<String>,

    /// Capture for a specific workflow's steps
    #[arg(long)]
    pub workflow: Option<String>,
}

/// Snapshot subcommands.
#[derive(Debug, Clone, clap::Subcommand)]
pub enum SnapshotAction {
    /// List all named snapshots
    List,

    /// Delete a named snapshot
    Delete(SnapshotDeleteArgs),
}

/// Arguments for `bivvy snapshot delete`.
#[derive(Debug, Clone, clap::Args)]
pub struct SnapshotDeleteArgs {
    /// Snapshot name to delete
    pub slug: String,
}

/// The snapshot command implementation.
pub struct SnapshotCommand {
    project_root: PathBuf,
    args: SnapshotArgs,
    config_override: Option<PathBuf>,
}

impl SnapshotCommand {
    /// Create a new snapshot command.
    pub fn new(project_root: &Path, args: SnapshotArgs) -> Self {
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
}

impl Command for SnapshotCommand {
    fn execute(&self, ui: &mut dyn UserInterface) -> Result<CommandResult> {
        let mut event_bus = crate::logging::EventBus::new();
        if let Ok(logger) = crate::logging::EventLogger::new(
            crate::logging::default_log_dir(),
            &format!(
                "sess_{}_snapshot",
                chrono::Utc::now().format("%Y%m%d%H%M%S"),
            ),
            crate::logging::RetentionPolicy::default(),
        ) {
            event_bus.add_consumer(Box::new(logger));
        }
        let start = std::time::Instant::now();

        event_bus.emit(&crate::logging::BivvyEvent::SessionStarted {
            command: "snapshot".to_string(),
            args: vec![],
            version: env!("CARGO_PKG_VERSION").to_string(),
            os: Some(std::env::consts::OS.to_string()),
            working_directory: Some(self.project_root.display().to_string()),
        });

        let result = if let Some(ref action) = self.args.action {
            match action {
                SnapshotAction::List => self.execute_list(ui, &mut event_bus),
                SnapshotAction::Delete(delete_args) => {
                    self.execute_delete(delete_args, ui, &mut event_bus)
                }
            }
        } else if let Some(ref slug) = self.args.slug {
            self.execute_capture(
                slug,
                &self.args.step,
                &self.args.workflow,
                ui,
                &mut event_bus,
            )
        } else {
            // No subcommand and no slug — show help equivalent
            ui.error(
                "Missing snapshot name. Usage: bivvy snapshot <slug>, bivvy snapshot list, or bivvy snapshot delete <slug>",
            );
            return Ok(CommandResult::failure(2));
        };

        result?;

        event_bus.emit(&crate::logging::BivvyEvent::SessionEnded {
            exit_code: 0,
            duration_ms: start.elapsed().as_millis() as u64,
        });

        Ok(CommandResult::success())
    }
}

impl SnapshotCommand {
    fn execute_capture(
        &self,
        slug: &str,
        step_filter: &Option<String>,
        workflow_filter: &Option<String>,
        ui: &mut dyn UserInterface,
        event_bus: &mut crate::logging::EventBus,
    ) -> Result<()> {
        let project_id = ProjectId::from_path(&self.project_root)?;
        let mut store = SnapshotStore::load_for_project(&project_id);

        // Load only what's needed for this snapshot. With --workflow we use
        // the run-style loader so steps bundled in the named workflow file
        // are visible. Otherwise the cheap project-only loader is enough.
        let config = if let Some(ref override_path) = self.config_override {
            match load_config(&self.project_root, Some(override_path)) {
                Ok(c) => c,
                Err(BivvyError::ConfigNotFound { .. }) => {
                    ui.error("No configuration found. Run 'bivvy init' first.");
                    return Ok(());
                }
                Err(e) => return Err(e),
            }
        } else if let Some(ref workflow_name) = workflow_filter {
            match load_for_run(&self.project_root, workflow_name) {
                Ok(c) => c,
                Err(BivvyError::ConfigNotFound { .. }) => {
                    ui.error("No configuration found. Run 'bivvy init' first.");
                    return Ok(());
                }
                Err(e) => return Err(e),
            }
        } else {
            match load_project_config(&self.project_root) {
                Ok(c) => c,
                Err(BivvyError::ConfigNotFound { .. }) => {
                    ui.error("No configuration found. Run 'bivvy init' first.");
                    return Ok(());
                }
                Err(e) => return Err(e),
            }
        };

        // Determine which steps to capture
        let step_names: Vec<String> = if let Some(ref step_name) = step_filter {
            if !config.steps.contains_key(step_name) {
                ui.error(&format!("Unknown step: {}", step_name));
                return Ok(());
            }
            vec![step_name.clone()]
        } else if let Some(ref workflow_name) = workflow_filter {
            let Some(workflow) = config.workflows.get(workflow_name) else {
                ui.error(&format!("Unknown workflow: {}", workflow_name));
                return Ok(());
            };
            workflow.steps.clone()
        } else {
            // Default: all steps in the default workflow, or all steps if no default
            config
                .workflows
                .get("default")
                .map(|w| w.steps.clone())
                .unwrap_or_else(|| config.steps.keys().cloned().collect())
        };

        let mut captured = 0;
        for step_name in &step_names {
            let Some(step_config) = config.steps.get(step_name) else {
                continue;
            };

            // Find change checks on this step
            let checks = collect_change_checks(step_config);
            for check in &checks {
                let Check::Change { target, kind, .. } = check else {
                    continue;
                };

                let config_hash = check.config_hash();
                let key = SnapshotKey::project(step_name.as_str(), &config_hash);

                // Hash the target
                match change::hash_target(target, kind, &self.project_root) {
                    Ok(hash) => {
                        store.capture_named(&key, slug, hash.clone(), target.clone());

                        event_bus.emit(&crate::logging::BivvyEvent::SnapshotCaptured {
                            slug: slug.to_string(),
                            step: step_name.clone(),
                            target: target.clone(),
                            hash: hash.clone(),
                        });

                        ui.success(&format!(
                            "Captured snapshot '{}' for {}: {} ({})",
                            slug,
                            step_name,
                            target,
                            &hash[..16.min(hash.len())],
                        ));
                        captured += 1;
                    }
                    Err(e) => {
                        ui.warning(&format!(
                            "Could not hash target '{}' for step '{}': {}",
                            target, step_name, e,
                        ));
                    }
                }
            }
        }

        if captured == 0 {
            ui.warning("No change checks found to snapshot.");
        } else {
            store.save()?;
        }

        Ok(())
    }

    fn execute_list(
        &self,
        ui: &mut dyn UserInterface,
        _event_bus: &mut crate::logging::EventBus,
    ) -> Result<()> {
        let project_id = ProjectId::from_path(&self.project_root)?;
        let mut store = SnapshotStore::load_for_project(&project_id);

        let snapshots = store.list_named();
        if snapshots.is_empty() {
            ui.message("No named snapshots.");
            return Ok(());
        }

        ui.message(&format!("{} snapshot(s):", snapshots.len()));
        for snap in &snapshots {
            ui.message(&format!(
                "  {} — step: {}, target: {}, captured: {}",
                snap.slug, snap.step, snap.target, snap.captured_at,
            ));
        }

        Ok(())
    }

    fn execute_delete(
        &self,
        args: &SnapshotDeleteArgs,
        ui: &mut dyn UserInterface,
        _event_bus: &mut crate::logging::EventBus,
    ) -> Result<()> {
        let project_id = ProjectId::from_path(&self.project_root)?;
        let mut store = SnapshotStore::load_for_project(&project_id);

        if store.delete_named(&args.slug) {
            store.save()?;
            ui.success(&format!("Deleted snapshot '{}'.", args.slug));
        } else {
            ui.error(&format!("Snapshot '{}' not found.", args.slug));
        }

        Ok(())
    }
}

/// Collect all change checks from a step config (from both new and legacy fields).
fn collect_change_checks(step: &crate::config::StepConfig) -> Vec<Check> {
    let mut checks = Vec::new();

    // New check fields
    if let Some(ref check) = step.execution.check {
        collect_change_checks_recursive(check, &mut checks);
    }
    for check in &step.execution.checks {
        collect_change_checks_recursive(check, &mut checks);
    }

    checks
}

fn collect_change_checks_recursive(check: &Check, out: &mut Vec<Check>) {
    match check {
        Check::Change { .. } => out.push(check.clone()),
        Check::All { checks, .. } | Check::Any { checks, .. } => {
            for c in checks {
                collect_change_checks_recursive(c, out);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::ChangeKind;

    #[test]
    fn collect_change_checks_finds_nested() {
        let step = crate::config::StepConfig {
            execution: crate::config::ExecutionConfig {
                checks: vec![
                    Check::Presence {
                        name: None,
                        target: Some("node_modules".to_string()),
                        kind: None,
                        command: None,
                    },
                    Check::Change {
                        name: None,
                        target: "Gemfile.lock".to_string(),
                        kind: ChangeKind::File,
                        on_change: Default::default(),
                        require_step: None,
                        baseline: Default::default(),
                        baseline_snapshot: None,
                        baseline_git: None,
                        size_limit: Default::default(),
                        scope: Default::default(),
                    },
                ],
                ..Default::default()
            },
            ..Default::default()
        };

        let checks = collect_change_checks(&step);
        assert_eq!(checks.len(), 1);
        assert!(matches!(checks[0], Check::Change { .. }));
    }

    #[test]
    fn collect_change_checks_empty_when_no_changes() {
        let step = crate::config::StepConfig::default();
        assert!(collect_change_checks(&step).is_empty());
    }
}
