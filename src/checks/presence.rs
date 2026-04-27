//! Presence check evaluation.
//!
//! Confirms the existence of a file, binary, or custom resource.

use super::{truncate_display, Check, CheckResult, PresenceKind};
use crate::shell::execute_check;
use std::path::Path;

/// Evaluate a presence check.
///
/// Infers `kind` when not explicitly set:
/// - If `command` is specified -> `Custom`
/// - If `kind: binary` is specified -> `Binary`
/// - If `kind: file` is specified or omitted and `target` looks like a path -> `File`
/// - Otherwise -> `Binary`
pub fn evaluate_presence(
    target: Option<&str>,
    kind: Option<PresenceKind>,
    command: Option<&str>,
    project_root: &Path,
) -> CheckResult {
    let effective_kind = infer_kind(target, kind, command);

    match effective_kind {
        PresenceKind::File => {
            let target = match target {
                Some(t) => t,
                None => {
                    return CheckResult::failed(
                        "Presence check misconfigured",
                        "File presence check requires a target",
                    );
                }
            };
            check_file(target, project_root)
        }
        PresenceKind::Binary => {
            let target = match target {
                Some(t) => t,
                None => {
                    return CheckResult::failed(
                        "Presence check misconfigured",
                        "Binary presence check requires a target",
                    );
                }
            };
            check_binary(target)
        }
        PresenceKind::Custom => {
            let cmd = match command {
                Some(c) => c,
                None => {
                    return CheckResult::failed(
                        "Presence check misconfigured",
                        "Custom presence check requires a command",
                    );
                }
            };
            check_custom(cmd, project_root)
        }
    }
}

/// Infer the presence kind from the fields provided.
fn infer_kind(
    target: Option<&str>,
    kind: Option<PresenceKind>,
    command: Option<&str>,
) -> PresenceKind {
    // Explicit kind takes precedence
    if let Some(k) = kind {
        return k;
    }

    // If command is specified, it's custom
    if command.is_some() {
        return PresenceKind::Custom;
    }

    // Infer from target
    if let Some(t) = target {
        if looks_like_path(t) {
            return PresenceKind::File;
        }
        return PresenceKind::Binary;
    }

    // Fallback
    PresenceKind::File
}

/// Heuristic: does this string look like a file path?
fn looks_like_path(s: &str) -> bool {
    s.contains('/') || s.contains('.') || s.starts_with('~')
}

fn check_file(target: &str, project_root: &Path) -> CheckResult {
    let full_path = if Path::new(target).is_absolute() {
        Path::new(target).to_path_buf()
    } else {
        project_root.join(target)
    };

    if full_path.exists() {
        CheckResult::passed(format!("\u{2713} {} exists", target))
    } else {
        CheckResult::failed(
            format!("\u{2717} {} not found", target),
            format!("Expected at: {}", full_path.display()),
        )
    }
}

fn check_binary(name: &str) -> CheckResult {
    let check_cmd = if cfg!(windows) {
        format!("where {}", name)
    } else {
        format!("which {}", name)
    };

    if execute_check(&check_cmd, None) {
        CheckResult::passed(format!("\u{2713} {} found", name))
    } else {
        CheckResult::failed(
            format!("\u{2717} {} not found on PATH", name),
            "Binary not available",
        )
    }
}

fn check_custom(command: &str, project_root: &Path) -> CheckResult {
    if execute_check(command, Some(project_root)) {
        CheckResult::passed(format!(
            "\u{2713} {} succeeded",
            truncate_display(command, 50)
        ))
    } else {
        CheckResult::failed(
            format!("\u{2717} {} failed", truncate_display(command, 50)),
            "Command exited with non-zero status",
        )
    }
}

/// Extract presence check fields from a [`Check::Presence`] variant.
///
/// Returns `None` if the check is not a presence check.
pub fn extract_presence_fields(
    check: &Check,
) -> Option<(Option<&str>, Option<PresenceKind>, Option<&str>)> {
    match check {
        Check::Presence {
            target,
            kind,
            command,
            ..
        } => Some((target.as_deref(), *kind, command.as_deref())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn file_exists_returns_passed() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("test.txt"), "content").unwrap();

        let result = evaluate_presence(
            Some("test.txt"),
            Some(PresenceKind::File),
            None,
            temp.path(),
        );
        assert!(result.passed_check());
        assert_eq!(result.description, "\u{2713} test.txt exists");
    }

    #[test]
    fn file_missing_returns_failed() {
        let temp = TempDir::new().unwrap();

        let result = evaluate_presence(
            Some("missing.txt"),
            Some(PresenceKind::File),
            None,
            temp.path(),
        );
        assert!(!result.passed_check());
        assert_eq!(result.description, "\u{2717} missing.txt not found");
    }

    #[test]
    fn directory_exists_returns_passed() {
        let temp = TempDir::new().unwrap();
        fs::create_dir(temp.path().join("subdir")).unwrap();

        let result = evaluate_presence(Some("subdir"), Some(PresenceKind::File), None, temp.path());
        assert!(result.passed_check());
    }

    #[test]
    fn directory_inferred_as_file_when_path_like() {
        let temp = TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join("node_modules/.cache")).unwrap();

        // "node_modules/.cache" contains both / and ., so inferred as file
        let result = evaluate_presence(Some("node_modules/.cache"), None, None, temp.path());
        assert!(result.passed_check());
    }

    #[test]
    fn absolute_path_works() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("abs.txt");
        fs::write(&file_path, "content").unwrap();

        let result = evaluate_presence(
            Some(&file_path.to_string_lossy()),
            Some(PresenceKind::File),
            None,
            temp.path(),
        );
        assert!(result.passed_check());
    }

    #[test]
    fn binary_found_returns_passed() {
        // `sh` should exist on all Unix systems
        if cfg!(unix) {
            let result =
                evaluate_presence(Some("sh"), Some(PresenceKind::Binary), None, Path::new("."));
            assert!(result.passed_check());
            assert_eq!(result.description, "\u{2713} sh found");
        }
    }

    #[test]
    fn binary_missing_returns_failed() {
        let result = evaluate_presence(
            Some("nonexistent_binary_xyz_12345"),
            Some(PresenceKind::Binary),
            None,
            Path::new("."),
        );
        assert!(!result.passed_check());
        assert!(result.description.contains("not found on PATH"));
    }

    #[test]
    fn custom_command_success() {
        let temp = TempDir::new().unwrap();
        let result = evaluate_presence(
            None,
            Some(PresenceKind::Custom),
            Some("exit 0"),
            temp.path(),
        );
        assert!(result.passed_check());
    }

    #[test]
    fn custom_command_failure() {
        let temp = TempDir::new().unwrap();
        let result = evaluate_presence(
            None,
            Some(PresenceKind::Custom),
            Some("exit 1"),
            temp.path(),
        );
        assert!(!result.passed_check());
    }

    #[test]
    fn infer_kind_from_command() {
        assert_eq!(
            infer_kind(None, None, Some("pg_isready")),
            PresenceKind::Custom
        );
    }

    #[test]
    fn infer_kind_from_path_like_target() {
        assert_eq!(
            infer_kind(Some("node_modules/foo"), None, None),
            PresenceKind::File
        );
        assert_eq!(
            infer_kind(Some("config.yml"), None, None),
            PresenceKind::File
        );
    }

    #[test]
    fn infer_kind_binary_for_simple_name() {
        assert_eq!(infer_kind(Some("rustc"), None, None), PresenceKind::Binary);
    }

    #[test]
    fn explicit_kind_overrides_inference() {
        assert_eq!(
            infer_kind(Some("rustc"), Some(PresenceKind::File), None),
            PresenceKind::File
        );
    }

    #[test]
    fn file_check_no_target_returns_error() {
        let temp = TempDir::new().unwrap();
        let result = evaluate_presence(None, Some(PresenceKind::File), None, temp.path());
        assert!(!result.passed_check());
        assert!(result.description.contains("misconfigured"));
    }

    #[test]
    fn binary_check_no_target_returns_error() {
        let result = evaluate_presence(None, Some(PresenceKind::Binary), None, Path::new("."));
        assert!(!result.passed_check());
        assert!(result.description.contains("misconfigured"));
    }

    #[test]
    fn custom_check_no_command_returns_error() {
        let temp = TempDir::new().unwrap();
        let result = evaluate_presence(
            Some("target"),
            Some(PresenceKind::Custom),
            None,
            temp.path(),
        );
        assert!(!result.passed_check());
        assert!(result.description.contains("misconfigured"));
    }

    #[test]
    fn looks_like_path_with_slash() {
        assert!(looks_like_path("src/main.rs"));
    }

    #[test]
    fn looks_like_path_with_dot() {
        assert!(looks_like_path("config.yml"));
    }

    #[test]
    fn looks_like_path_with_tilde() {
        assert!(looks_like_path("~/.bashrc"));
    }

    #[test]
    fn does_not_look_like_path() {
        assert!(!looks_like_path("rustc"));
        assert!(!looks_like_path("node"));
    }
}
