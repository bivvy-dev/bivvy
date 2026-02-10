//! Environment change detection.

use std::collections::HashSet;
use std::path::PathBuf;

/// Detected environment change.
#[derive(Debug, Clone)]
pub struct EnvironmentChange {
    pub kind: EnvironmentChangeKind,
    pub message: String,
    pub action_required: bool,
}

/// Type of environment change.
#[derive(Debug, Clone, PartialEq)]
pub enum EnvironmentChangeKind {
    /// PATH was modified.
    PathChanged {
        added: Vec<String>,
        removed: Vec<String>,
    },
    /// Shell config file was modified.
    ShellConfigModified(String),
    /// New tool requires shell restart.
    ShellRestartRequired,
}

/// Detects changes to the shell environment.
pub struct EnvironmentDetector {
    initial_path: String,
    shell_config_files: Vec<PathBuf>,
}

impl EnvironmentDetector {
    /// Create a new environment detector capturing current state.
    pub fn new() -> Self {
        let shell_config_files = Self::get_shell_config_files();

        Self {
            initial_path: std::env::var("PATH").unwrap_or_default(),
            shell_config_files,
        }
    }

    /// Check for environment changes.
    pub fn check_changes(&self) -> Vec<EnvironmentChange> {
        let mut changes = Vec::new();

        let current_path = std::env::var("PATH").unwrap_or_default();
        if current_path != self.initial_path {
            let initial: HashSet<_> = self.initial_path.split(':').collect();
            let current: HashSet<_> = current_path.split(':').collect();

            let added: Vec<_> = current
                .difference(&initial)
                .map(|s| s.to_string())
                .collect();
            let removed: Vec<_> = initial
                .difference(&current)
                .map(|s| s.to_string())
                .collect();

            if !added.is_empty() || !removed.is_empty() {
                changes.push(EnvironmentChange {
                    kind: EnvironmentChangeKind::PathChanged {
                        added: added.clone(),
                        removed: removed.clone(),
                    },
                    message: format!(
                        "PATH modified: {} added, {} removed",
                        added.len(),
                        removed.len()
                    ),
                    action_required: !added.is_empty(),
                });
            }
        }

        changes
    }

    /// Get the appropriate shell reload command.
    pub fn get_shell_reload_command() -> Option<String> {
        let shell = std::env::var("SHELL").ok()?;

        if shell.contains("zsh") || shell.contains("bash") {
            Some("exec $SHELL".to_string())
        } else if shell.contains("fish") {
            Some("exec fish".to_string())
        } else {
            None
        }
    }

    /// Get common shell config file paths.
    fn get_shell_config_files() -> Vec<PathBuf> {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("~"));

        vec![
            home.join(".zshrc"),
            home.join(".bashrc"),
            home.join(".bash_profile"),
            home.join(".profile"),
            home.join(".config/fish/config.fish"),
        ]
        .into_iter()
        .filter(|p| p.exists())
        .collect()
    }

    /// Check if shell restart is needed after a step.
    pub fn needs_shell_restart(step_template: &str) -> bool {
        matches!(
            step_template,
            "mise" | "asdf" | "volta" | "nvm" | "rbenv" | "pyenv" | "brew"
        )
    }

    /// Get the initial PATH.
    pub fn initial_path(&self) -> &str {
        &self.initial_path
    }

    /// Get the list of shell config files.
    pub fn shell_config_files(&self) -> &[PathBuf] {
        &self.shell_config_files
    }
}

impl Default for EnvironmentDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_detector_creation() {
        let detector = EnvironmentDetector::new();
        assert!(!detector.initial_path.is_empty());
    }

    #[test]
    fn check_changes_no_changes() {
        let detector = EnvironmentDetector::new();
        let changes = detector.check_changes();

        assert!(changes.is_empty());
    }

    #[test]
    fn get_shell_reload_command_returns_value() {
        // May or may not have a value depending on SHELL env var
        let _ = EnvironmentDetector::get_shell_reload_command();
    }

    #[test]
    fn needs_shell_restart_for_version_managers() {
        assert!(EnvironmentDetector::needs_shell_restart("mise"));
        assert!(EnvironmentDetector::needs_shell_restart("asdf"));
        assert!(EnvironmentDetector::needs_shell_restart("volta"));
        assert!(EnvironmentDetector::needs_shell_restart("brew"));
        assert!(!EnvironmentDetector::needs_shell_restart("bundler"));
        assert!(!EnvironmentDetector::needs_shell_restart("npm"));
    }

    #[test]
    fn default_implementation() {
        let detector = EnvironmentDetector::default();
        assert!(!detector.initial_path().is_empty());
    }
}
