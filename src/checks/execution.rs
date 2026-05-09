//! Execution check evaluation.
//!
//! Runs a command and validates the result.

use super::{truncate_display, CheckResult, ValidationMode};
use std::path::Path;
use std::process::Command;

/// Evaluate an execution check.
///
/// Runs the command in a subprocess and validates the result according
/// to the validation mode:
/// - `Success`: command exits with code 0
/// - `Truthy`: command exits 0 AND produces non-empty stdout
/// - `Falsy`: command exits 0 AND produces empty stdout (or exits non-zero)
pub fn evaluate_execution(
    command: &str,
    validation: ValidationMode,
    project_root: &Path,
) -> CheckResult {
    let output = Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(project_root)
        .output();

    let output = match output {
        Ok(o) => o,
        Err(e) => {
            return CheckResult::failed(
                format!(
                    "\u{2717} {} failed to execute",
                    truncate_display(command, 50)
                ),
                format!("Error: {}", e),
            );
        }
    };

    let exit_success = output.status.success();
    // Validation depends on whether stdout is empty; lossy conversion would
    // silently substitute U+FFFD for invalid bytes and flip Truthy/Falsy
    // results. Surface invalid UTF-8 explicitly instead.
    let stdout = match std::str::from_utf8(&output.stdout) {
        Ok(s) => s,
        Err(_) => {
            return CheckResult::failed(
                format!(
                    "\u{2717} {} produced invalid UTF-8 output",
                    truncate_display(command, 50)
                ),
                "Command stdout was not valid UTF-8",
            );
        }
    };
    let stdout_trimmed = stdout.trim();

    match validation {
        ValidationMode::Success => {
            if exit_success {
                CheckResult::passed(format!(
                    "\u{2713} {} succeeded",
                    truncate_display(command, 50)
                ))
            } else {
                let code = output.status.code().unwrap_or(-1);
                CheckResult::failed(
                    format!(
                        "\u{2717} {} failed (exit code {})",
                        truncate_display(command, 50),
                        code
                    ),
                    "Exit code was non-zero",
                )
            }
        }
        ValidationMode::Truthy => {
            if exit_success && !stdout_trimmed.is_empty() {
                CheckResult::passed(format!(
                    "\u{2713} {} returned truthy output",
                    truncate_display(command, 50)
                ))
            } else if !exit_success {
                let code = output.status.code().unwrap_or(-1);
                CheckResult::failed(
                    format!(
                        "\u{2717} {} failed (exit code {})",
                        truncate_display(command, 50),
                        code
                    ),
                    "Command did not succeed",
                )
            } else {
                CheckResult::failed(
                    format!(
                        "\u{2717} {} returned empty output",
                        truncate_display(command, 50)
                    ),
                    "Command succeeded but produced no stdout",
                )
            }
        }
        ValidationMode::Falsy => {
            // Passes when: exits 0 with empty stdout, OR exits non-zero
            if !exit_success || stdout_trimmed.is_empty() {
                CheckResult::passed(format!(
                    "\u{2713} {} returned falsy",
                    truncate_display(command, 50)
                ))
            } else {
                CheckResult::failed(
                    format!(
                        "\u{2717} {} returned truthy output",
                        truncate_display(command, 50)
                    ),
                    format!(
                        "Expected empty output, got: {}",
                        truncate_display(stdout_trimmed, 100)
                    ),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp() -> TempDir {
        TempDir::new().unwrap()
    }

    // --- Success validation ---

    #[test]
    fn success_passes_on_exit_zero() {
        let result = evaluate_execution("exit 0", ValidationMode::Success, temp().path());
        assert!(result.passed_check());
        assert!(result.description.contains("succeeded"));
    }

    #[test]
    fn success_fails_on_exit_nonzero() {
        let result = evaluate_execution("exit 1", ValidationMode::Success, temp().path());
        assert!(!result.passed_check());
        assert!(result.description.contains("failed"));
        assert!(result.description.contains("exit code 1"));
    }

    #[test]
    fn success_passes_with_real_command() {
        let tmp = temp();
        std::fs::write(tmp.path().join("test.txt"), "content").unwrap();
        let result = evaluate_execution(
            &format!("test -f {}/test.txt", tmp.path().display()),
            ValidationMode::Success,
            tmp.path(),
        );
        assert!(result.passed_check());
    }

    // --- Truthy validation ---

    #[test]
    fn truthy_passes_when_stdout_nonempty() {
        let result = evaluate_execution("echo hello", ValidationMode::Truthy, temp().path());
        assert!(result.passed_check());
        assert!(result.description.contains("truthy"));
    }

    #[test]
    fn truthy_fails_when_stdout_empty() {
        // `true` produces no output
        let result = evaluate_execution("true", ValidationMode::Truthy, temp().path());
        assert!(!result.passed_check());
        assert!(result.description.contains("empty output"));
    }

    #[test]
    fn truthy_fails_when_command_fails() {
        let result = evaluate_execution("exit 1", ValidationMode::Truthy, temp().path());
        assert!(!result.passed_check());
        assert!(result.description.contains("failed"));
    }

    // --- Falsy validation ---

    #[test]
    fn falsy_passes_when_stdout_empty() {
        let result = evaluate_execution("true", ValidationMode::Falsy, temp().path());
        assert!(result.passed_check());
        assert!(result.description.contains("falsy"));
    }

    #[test]
    fn falsy_passes_when_command_fails() {
        let result = evaluate_execution("exit 1", ValidationMode::Falsy, temp().path());
        assert!(result.passed_check());
    }

    #[test]
    fn falsy_fails_when_stdout_nonempty() {
        let result = evaluate_execution("echo hello", ValidationMode::Falsy, temp().path());
        assert!(!result.passed_check());
        assert!(result.description.contains("truthy output"));
    }

    // --- Edge cases ---

    #[test]
    fn handles_nonexistent_command() {
        let result = evaluate_execution(
            "nonexistent_cmd_xyz_12345",
            ValidationMode::Success,
            temp().path(),
        );
        assert!(!result.passed_check());
    }

    #[test]
    fn runs_in_project_root_directory() {
        let tmp = temp();
        std::fs::write(tmp.path().join("marker.txt"), "present").unwrap();
        let result = evaluate_execution("test -f marker.txt", ValidationMode::Success, tmp.path());
        assert!(result.passed_check());
    }

    #[test]
    fn long_command_truncated_in_description() {
        let long_cmd = format!("echo {}", "a".repeat(100));
        let result = evaluate_execution(&long_cmd, ValidationMode::Success, temp().path());
        assert!(result.description.len() < long_cmd.len() + 30);
    }

    /// Regression test for M7: a check command that emits invalid UTF-8
    /// must mark the check as failed with a clear "not valid UTF-8"
    /// message instead of letting `from_utf8_lossy` silently substitute
    /// `U+FFFD` and flip Truthy/Falsy validation.
    #[test]
    #[cfg(unix)]
    fn invalid_utf8_stdout_marks_check_failed() {
        // printf '\377' produces a single non-UTF-8 byte (0xFF) on stdout.
        // The octal escape is portable across POSIX shells, whereas the `\xHH`
        // hex escape is unsupported by dash (the default `/bin/sh` on many
        // Linux distributions) and would emit the literal, valid-UTF-8 text.
        let result = evaluate_execution("printf '\\377'", ValidationMode::Truthy, temp().path());
        assert!(
            !result.passed_check(),
            "invalid UTF-8 must fail the check, got: {:?}",
            result.description
        );
        let combined = format!(
            "{}\n{}",
            result.description,
            result.details.as_deref().unwrap_or("")
        );
        assert!(
            combined.contains("UTF-8"),
            "expected message to mention UTF-8, got: {combined}"
        );
    }
}
