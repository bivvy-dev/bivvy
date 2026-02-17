//! Completed check implementations.
//!
//! Completed checks determine if a step has already been executed
//! and can be skipped.

use crate::config::CompletedCheck;
use crate::shell::execute_check;
use std::path::Path;

/// Result of running a completed check.
///
/// The `description` field is user-visible: it appears in skip messages
/// (e.g., "Skipped (rustc --version)") and in the run summary table.
/// Use [`short_description`](CheckResult::short_description) to get a
/// display-friendly form with prefixes like "Command succeeded:" stripped.
#[derive(Debug, Clone)]
pub struct CheckResult {
    /// Whether the check passed (step is complete).
    pub complete: bool,

    /// Description of what was checked.
    pub description: String,

    /// Details about the check result.
    pub details: Option<String>,
}

impl CheckResult {
    /// Create a complete result.
    pub fn complete(description: impl Into<String>) -> Self {
        Self {
            complete: true,
            description: description.into(),
            details: None,
        }
    }

    /// Get a short, display-friendly description with common prefixes stripped.
    ///
    /// Strips prefixes like "Command succeeded: ", "File exists: " etc.
    /// Returns the original description if no prefix is found.
    pub fn short_description(&self) -> &str {
        const PREFIXES: &[&str] = &[
            "Command succeeded: ",
            "Command failed: ",
            "File exists: ",
            "File missing: ",
            "Check passed: ",
        ];
        for prefix in PREFIXES {
            if let Some(rest) = self.description.strip_prefix(prefix) {
                return rest;
            }
        }
        &self.description
    }

    /// Create an incomplete result.
    pub fn incomplete(description: impl Into<String>, details: impl Into<String>) -> Self {
        Self {
            complete: false,
            description: description.into(),
            details: Some(details.into()),
        }
    }
}

/// Run a completed check.
pub fn run_check(check: &CompletedCheck, project_root: &Path) -> CheckResult {
    match check {
        CompletedCheck::FileExists { path } => check_file_exists(path, project_root),
        CompletedCheck::CommandSucceeds { command } => {
            check_command_succeeds(command, project_root)
        }
        CompletedCheck::Marker => check_marker(project_root),
        CompletedCheck::All { checks } => check_all(checks, project_root),
        CompletedCheck::Any { checks } => check_any(checks, project_root),
    }
}

/// Check if a file or directory exists.
fn check_file_exists(path: &str, project_root: &Path) -> CheckResult {
    let full_path = if Path::new(path).is_absolute() {
        Path::new(path).to_path_buf()
    } else {
        project_root.join(path)
    };

    if full_path.exists() {
        CheckResult::complete(format!("File exists: {}", path))
    } else {
        CheckResult::incomplete(
            format!("File missing: {}", path),
            format!("Expected at: {}", full_path.display()),
        )
    }
}

/// Check if a command succeeds (exit code 0).
fn check_command_succeeds(command: &str, project_root: &Path) -> CheckResult {
    if execute_check(command, Some(project_root)) {
        CheckResult::complete(format!("Command succeeded: {}", truncate(command, 50)))
    } else {
        CheckResult::incomplete(
            format!("Command failed: {}", truncate(command, 50)),
            "Exit code was non-zero".to_string(),
        )
    }
}

/// Check marker file.
///
/// Marker checks use state tracking to determine if a step has run.
/// This is a stub that always returns incomplete until the state
/// module is implemented in M6.
///
/// # Future Implementation
///
/// In M6, this will:
/// - Look up the step in the state file
/// - Check if it has a completion marker
/// - Support version tracking for re-runs
fn check_marker(_project_root: &Path) -> CheckResult {
    CheckResult::incomplete(
        "Marker check",
        "Marker-based checks will be implemented with state tracking".to_string(),
    )
}

/// All checks must pass.
fn check_all(checks: &[CompletedCheck], project_root: &Path) -> CheckResult {
    let results: Vec<_> = checks.iter().map(|c| run_check(c, project_root)).collect();

    if results.iter().all(|r| r.complete) {
        CheckResult::complete(format!("All {} checks passed", checks.len()))
    } else {
        let failed: Vec<_> = results
            .iter()
            .filter(|r| !r.complete)
            .map(|r| r.description.clone())
            .collect();

        CheckResult::incomplete(
            format!("{}/{} checks failed", failed.len(), checks.len()),
            failed.join("; "),
        )
    }
}

/// Any check passing is sufficient.
fn check_any(checks: &[CompletedCheck], project_root: &Path) -> CheckResult {
    let results: Vec<_> = checks.iter().map(|c| run_check(c, project_root)).collect();

    if let Some(passed) = results.iter().find(|r| r.complete) {
        CheckResult::complete(format!("Check passed: {}", passed.description))
    } else {
        CheckResult::incomplete(
            format!("None of {} checks passed", checks.len()),
            results
                .iter()
                .map(|r| r.description.clone())
                .collect::<Vec<_>>()
                .join("; "),
        )
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn file_exists_returns_complete_when_exists() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("test.txt"), "content").unwrap();

        let check = CompletedCheck::FileExists {
            path: "test.txt".to_string(),
        };

        let result = run_check(&check, temp.path());
        assert!(result.complete);
    }

    #[test]
    fn file_exists_returns_incomplete_when_missing() {
        let temp = TempDir::new().unwrap();

        let check = CompletedCheck::FileExists {
            path: "missing.txt".to_string(),
        };

        let result = run_check(&check, temp.path());
        assert!(!result.complete);
    }

    #[test]
    fn file_exists_works_with_directories() {
        let temp = TempDir::new().unwrap();
        fs::create_dir(temp.path().join("subdir")).unwrap();

        let check = CompletedCheck::FileExists {
            path: "subdir".to_string(),
        };

        let result = run_check(&check, temp.path());
        assert!(result.complete);
    }

    #[test]
    fn file_exists_handles_absolute_paths() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("abs.txt");
        fs::write(&file_path, "content").unwrap();

        let check = CompletedCheck::FileExists {
            path: file_path.to_string_lossy().to_string(),
        };

        let result = run_check(&check, temp.path());
        assert!(result.complete);
    }

    #[test]
    fn command_succeeds_returns_complete_on_success() {
        let temp = TempDir::new().unwrap();

        let check = CompletedCheck::CommandSucceeds {
            command: "exit 0".to_string(),
        };

        let result = run_check(&check, temp.path());
        assert!(result.complete);
    }

    #[test]
    fn command_succeeds_returns_incomplete_on_failure() {
        let temp = TempDir::new().unwrap();

        let check = CompletedCheck::CommandSucceeds {
            command: "exit 1".to_string(),
        };

        let result = run_check(&check, temp.path());
        assert!(!result.complete);
    }

    #[test]
    fn command_succeeds_runs_in_project_dir() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("marker.txt"), "").unwrap();

        let check = CompletedCheck::CommandSucceeds {
            command: if cfg!(target_os = "windows") {
                "if exist marker.txt exit 0"
            } else {
                "test -f marker.txt"
            }
            .to_string(),
        };

        let result = run_check(&check, temp.path());
        assert!(result.complete);
    }

    #[test]
    fn command_succeeds_truncates_long_commands_in_description() {
        let temp = TempDir::new().unwrap();
        let long_command = "echo ".to_string() + &"a".repeat(100);

        let check = CompletedCheck::CommandSucceeds {
            command: long_command,
        };

        let result = run_check(&check, temp.path());
        assert!(result.description.len() < 100);
    }

    #[test]
    fn marker_check_returns_incomplete_without_state() {
        let temp = TempDir::new().unwrap();

        let check = CompletedCheck::Marker;

        // Without state module, marker always returns incomplete
        let result = run_check(&check, temp.path());
        assert!(!result.complete);
    }
    // Full marker tests will be added in M6 (State Management)

    #[test]
    fn all_check_passes_when_all_pass() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("a.txt"), "").unwrap();
        fs::write(temp.path().join("b.txt"), "").unwrap();

        let check = CompletedCheck::All {
            checks: vec![
                CompletedCheck::FileExists {
                    path: "a.txt".to_string(),
                },
                CompletedCheck::FileExists {
                    path: "b.txt".to_string(),
                },
            ],
        };

        let result = run_check(&check, temp.path());
        assert!(result.complete);
    }

    #[test]
    fn all_check_fails_when_any_fails() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("a.txt"), "").unwrap();
        // b.txt does not exist

        let check = CompletedCheck::All {
            checks: vec![
                CompletedCheck::FileExists {
                    path: "a.txt".to_string(),
                },
                CompletedCheck::FileExists {
                    path: "b.txt".to_string(),
                },
            ],
        };

        let result = run_check(&check, temp.path());
        assert!(!result.complete);
    }

    #[test]
    fn any_check_passes_when_any_passes() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("a.txt"), "").unwrap();
        // b.txt does not exist

        let check = CompletedCheck::Any {
            checks: vec![
                CompletedCheck::FileExists {
                    path: "a.txt".to_string(),
                },
                CompletedCheck::FileExists {
                    path: "b.txt".to_string(),
                },
            ],
        };

        let result = run_check(&check, temp.path());
        assert!(result.complete);
    }

    #[test]
    fn any_check_fails_when_all_fail() {
        let temp = TempDir::new().unwrap();
        // Neither file exists

        let check = CompletedCheck::Any {
            checks: vec![
                CompletedCheck::FileExists {
                    path: "a.txt".to_string(),
                },
                CompletedCheck::FileExists {
                    path: "b.txt".to_string(),
                },
            ],
        };

        let result = run_check(&check, temp.path());
        assert!(!result.complete);
    }

    #[test]
    fn nested_combinators_work() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("required.txt"), "").unwrap();
        fs::write(temp.path().join("option_a.txt"), "").unwrap();

        let check = CompletedCheck::All {
            checks: vec![
                CompletedCheck::FileExists {
                    path: "required.txt".to_string(),
                },
                CompletedCheck::Any {
                    checks: vec![
                        CompletedCheck::FileExists {
                            path: "option_a.txt".to_string(),
                        },
                        CompletedCheck::FileExists {
                            path: "option_b.txt".to_string(),
                        },
                    ],
                },
            ],
        };

        let result = run_check(&check, temp.path());
        assert!(result.complete);
    }

    #[test]
    fn all_check_reports_failed_count() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("a.txt"), "").unwrap();

        let check = CompletedCheck::All {
            checks: vec![
                CompletedCheck::FileExists {
                    path: "a.txt".to_string(),
                },
                CompletedCheck::FileExists {
                    path: "b.txt".to_string(),
                },
                CompletedCheck::FileExists {
                    path: "c.txt".to_string(),
                },
            ],
        };

        let result = run_check(&check, temp.path());
        assert!(!result.complete);
        assert!(result.description.contains("2/3"));
    }

    #[test]
    fn short_description_strips_command_succeeded_prefix() {
        let result = CheckResult::complete("Command succeeded: rustc --version");
        assert_eq!(result.short_description(), "rustc --version");
    }

    #[test]
    fn short_description_strips_file_exists_prefix() {
        let result = CheckResult::complete("File exists: target");
        assert_eq!(result.short_description(), "target");
    }

    #[test]
    fn short_description_passes_through_unknown_prefix() {
        let result = CheckResult::complete("All 3 checks passed");
        assert_eq!(result.short_description(), "All 3 checks passed");
    }

    #[test]
    fn short_description_strips_command_failed_prefix() {
        let result = CheckResult::incomplete("Command failed: cargo test", "nonzero");
        assert_eq!(result.short_description(), "cargo test");
    }
}
