//! Shell refresh detection.
//!
//! This module provides functionality for detecting when PATH changes
//! require a shell refresh.

use std::collections::HashSet;

/// Tracks PATH changes that require shell refresh.
///
/// # Example
///
/// ```
/// use bivvy::shell::PathChangeDetector;
///
/// let mut detector = PathChangeDetector::new();
/// detector.expect_additions(vec!["/nonexistent/path"]);
///
/// let result = detector.check();
/// assert!(result.needs_refresh);
/// assert!(!result.missing_paths.is_empty());
/// ```
pub struct PathChangeDetector {
    /// PATH value at start of execution.
    initial_path: String,
    /// Expected path additions from steps.
    expected_additions: HashSet<String>,
}

/// Result of checking for PATH changes.
#[derive(Debug)]
pub struct PathChangeResult {
    /// Whether a shell refresh is needed.
    pub needs_refresh: bool,
    /// Paths that were expected but not found in current PATH.
    pub missing_paths: Vec<String>,
    /// Human-readable message explaining the change.
    pub message: Option<String>,
}

impl PathChangeDetector {
    /// Create a detector capturing the current PATH.
    pub fn new() -> Self {
        Self {
            initial_path: std::env::var("PATH").unwrap_or_default(),
            expected_additions: HashSet::new(),
        }
    }

    /// Record that a step is expected to add paths.
    pub fn expect_additions(&mut self, paths: impl IntoIterator<Item = impl Into<String>>) {
        for path in paths {
            self.expected_additions.insert(path.into());
        }
    }

    /// Check if expected path additions are present.
    pub fn check(&self) -> PathChangeResult {
        let current_path = std::env::var("PATH").unwrap_or_default();
        let separator = if cfg!(windows) { ';' } else { ':' };
        let current_parts: HashSet<&str> = current_path.split(separator).collect();

        let missing: Vec<String> = self
            .expected_additions
            .iter()
            .filter(|p| !current_parts.contains(p.as_str()))
            .cloned()
            .collect();

        let needs_refresh = !missing.is_empty();
        let message = if needs_refresh {
            Some(format!(
                "Shell refresh needed to add {} path(s) to PATH",
                missing.len()
            ))
        } else {
            None
        };

        PathChangeResult {
            needs_refresh,
            missing_paths: missing,
            message,
        }
    }

    /// Get the initial PATH value.
    pub fn initial_path(&self) -> &str {
        &self.initial_path
    }

    /// Get the number of expected additions.
    pub fn expected_count(&self) -> usize {
        self.expected_additions.len()
    }
}

impl Default for PathChangeDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// Information about what shell reload command to use.
///
/// # Example
///
/// ```no_run
/// use bivvy::shell::ShellReloadInfo;
///
/// // Detection depends on SHELL environment variable (Unix only)
/// // std::env::set_var("SHELL", "/bin/bash");
/// if let Some(info) = ShellReloadInfo::detect() {
///     assert_eq!(info.shell, "bash");
///     assert!(info.reload_command.contains("source"));
/// }
/// ```
#[derive(Debug, Clone)]
pub struct ShellReloadInfo {
    /// Shell name (bash, zsh, fish).
    pub shell: String,
    /// Command to reload the shell.
    pub reload_command: String,
    /// Config file that was modified.
    pub config_file: String,
}

impl ShellReloadInfo {
    /// Detect the current shell and provide reload info.
    pub fn detect() -> Option<Self> {
        let shell_path = std::env::var("SHELL").ok()?;
        let shell_name = std::path::Path::new(&shell_path).file_name()?.to_str()?;

        let (reload_command, config_file) = match shell_name {
            "bash" => ("source ~/.bashrc", "~/.bashrc"),
            "zsh" => ("source ~/.zshrc", "~/.zshrc"),
            "fish" => (
                "source ~/.config/fish/config.fish",
                "~/.config/fish/config.fish",
            ),
            _ => return None,
        };

        Some(Self {
            shell: shell_name.to_string(),
            reload_command: reload_command.to_string(),
            config_file: config_file.to_string(),
        })
    }

    /// Get an alternative reload method (exec the shell).
    pub fn exec_command(&self) -> String {
        format!("exec {}", self.shell)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mutex to serialize tests that modify SHELL env var (unix only)
    #[cfg(unix)]
    use std::sync::Mutex;
    #[cfg(unix)]
    static SHELL_MUTEX: Mutex<()> = Mutex::new(());

    #[cfg(unix)]
    fn with_shell_env<F, R>(shell_path: &str, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let _guard = SHELL_MUTEX.lock().unwrap();
        let old_shell = std::env::var("SHELL").ok();
        std::env::set_var("SHELL", shell_path);
        let result = f();
        match old_shell {
            Some(v) => std::env::set_var("SHELL", v),
            None => std::env::remove_var("SHELL"),
        }
        result
    }

    #[test]
    fn detects_missing_path_additions() {
        let mut detector = PathChangeDetector::new();
        detector.expect_additions(vec!["/nonexistent/path/here"]);

        let result = detector.check();

        assert!(result.needs_refresh);
        assert!(result
            .missing_paths
            .contains(&"/nonexistent/path/here".to_string()));
    }

    #[test]
    fn no_refresh_when_paths_present() {
        // Get a path that we know is in the current PATH
        let current_path = std::env::var("PATH").unwrap_or_default();
        let separator = if cfg!(windows) { ';' } else { ':' };
        let existing_path = current_path
            .split(separator)
            .next()
            .unwrap_or("")
            .to_string();

        // Only run if we actually have a path to test
        if existing_path.is_empty() {
            return;
        }

        let mut detector = PathChangeDetector::new();
        detector.expect_additions(vec![existing_path]);

        let result = detector.check();

        assert!(!result.needs_refresh);
        assert!(result.missing_paths.is_empty());
    }

    #[test]
    fn no_refresh_when_no_expectations() {
        let detector = PathChangeDetector::new();
        let result = detector.check();

        assert!(!result.needs_refresh);
        assert!(result.missing_paths.is_empty());
        assert!(result.message.is_none());
    }

    #[test]
    fn message_indicates_count() {
        let mut detector = PathChangeDetector::new();
        detector.expect_additions(vec!["/path1", "/path2", "/path3"]);

        let result = detector.check();

        assert!(result.needs_refresh);
        let message = result.message.unwrap();
        assert!(message.contains("3"));
    }

    #[test]
    #[cfg(unix)]
    fn shell_reload_info_detects_bash() {
        with_shell_env("/bin/bash", || {
            let info = ShellReloadInfo::detect().unwrap();
            assert_eq!(info.shell, "bash");
            assert!(info.reload_command.contains("source"));
            assert!(info.reload_command.contains(".bashrc"));
        });
    }

    #[test]
    #[cfg(unix)]
    fn shell_reload_info_detects_zsh() {
        with_shell_env("/bin/zsh", || {
            let info = ShellReloadInfo::detect().unwrap();
            assert_eq!(info.shell, "zsh");
            assert!(info.reload_command.contains(".zshrc"));
        });
    }

    #[test]
    #[cfg(unix)]
    fn shell_reload_info_detects_fish() {
        with_shell_env("/usr/bin/fish", || {
            let info = ShellReloadInfo::detect().unwrap();
            assert_eq!(info.shell, "fish");
            assert!(info.reload_command.contains("config.fish"));
        });
    }

    #[test]
    #[cfg(unix)]
    fn shell_reload_info_unknown_shell() {
        with_shell_env("/bin/unknown", || {
            let info = ShellReloadInfo::detect();
            assert!(info.is_none());
        });
    }

    #[test]
    #[cfg(unix)]
    fn exec_command_uses_shell_name() {
        with_shell_env("/bin/bash", || {
            let info = ShellReloadInfo::detect().unwrap();
            assert_eq!(info.exec_command(), "exec bash");
        });
    }

    #[test]
    fn expected_count_tracks_additions() {
        let mut detector = PathChangeDetector::new();
        assert_eq!(detector.expected_count(), 0);

        detector.expect_additions(vec!["/path1", "/path2"]);
        assert_eq!(detector.expected_count(), 2);
    }

    #[test]
    fn default_creates_new_detector() {
        let detector = PathChangeDetector::default();
        assert_eq!(detector.expected_count(), 0);
    }
}
