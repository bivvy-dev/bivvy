//! Change check evaluation.
//!
//! Detects whether a specific target has changed from a known baseline
//! by computing a SHA-256 hash and comparing it to a stored value.

use super::{ChangeKind, CheckResult, OnChange, SizeLimit};
use sha2::{Digest, Sha256};
use std::path::Path;

/// Result of hashing a change check target.
#[derive(Debug)]
pub enum HashResult {
    /// Successfully computed the hash.
    Ok(String),
    /// Target exceeds the size limit.
    SizeLimitExceeded { actual: u64, limit: u64 },
    /// Target not found or could not be read.
    NotFound(String),
    /// An error occurred during hashing.
    Error(String),
}

/// Hash a file's contents.
pub fn hash_file(path: &Path) -> HashResult {
    match std::fs::read(path) {
        Ok(contents) => {
            let hash = Sha256::digest(&contents);
            HashResult::Ok(format!("sha256:{}", hex::encode(hash)))
        }
        Err(e) => HashResult::NotFound(format!("Cannot read {}: {}", path.display(), e)),
    }
}

/// Hash all files matching a glob pattern.
///
/// The hash is computed over the sorted concatenation of
/// (relative_path + file_contents) for all matched files.
/// If a `size_limit` is provided, the total size of all matched files
/// is checked before hashing.
pub fn hash_glob(pattern: &str, project_root: &Path, size_limit: &SizeLimit) -> HashResult {
    let full_pattern = if Path::new(pattern).is_absolute() {
        pattern.to_string()
    } else {
        project_root.join(pattern).to_string_lossy().to_string()
    };

    let all_paths = match crate::sys::glob(&full_pattern) {
        Ok(paths) => paths,
        Err(e) => return HashResult::Error(format!("Invalid glob pattern '{}': {}", pattern, e)),
    };

    let file_paths: Vec<std::path::PathBuf> =
        all_paths.into_iter().filter(|p| p.is_file()).collect();

    if file_paths.is_empty() {
        return HashResult::NotFound(format!("No files matched pattern '{}'", pattern));
    }

    // Check total size against limit
    if let SizeLimit {
        max_bytes: Some(limit),
    } = size_limit
    {
        let total_size: u64 = file_paths
            .iter()
            .filter_map(|p| std::fs::metadata(p).ok())
            .map(|m| m.len())
            .sum();
        if total_size > *limit {
            return HashResult::SizeLimitExceeded {
                actual: total_size,
                limit: *limit,
            };
        }
    }

    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    for path in &file_paths {
        let rel = path
            .strip_prefix(project_root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();
        match std::fs::read(path) {
            Ok(contents) => entries.push((rel, contents)),
            Err(e) => return HashResult::Error(format!("Cannot read {}: {}", path.display(), e)),
        }
    }

    // Sort for deterministic hashing
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut hasher = Sha256::new();
    for (path, contents) in &entries {
        hasher.update(path.as_bytes());
        hasher.update(contents);
    }
    HashResult::Ok(format!("sha256:{}", hex::encode(hasher.finalize())))
}

/// Hash the stdout of a command.
pub fn hash_command(command: &str, project_root: &Path) -> HashResult {
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(project_root)
        .output();

    match output {
        Ok(o) if o.status.success() => {
            let hash = Sha256::digest(&o.stdout);
            HashResult::Ok(format!("sha256:{}", hex::encode(hash)))
        }
        Ok(o) => {
            let code = o.status.code().unwrap_or(-1);
            HashResult::Error(format!(
                "Command '{}' failed with exit code {}",
                command, code
            ))
        }
        Err(e) => HashResult::Error(format!("Failed to execute '{}': {}", command, e)),
    }
}

/// Hash a change check target based on its kind.
///
/// Dispatches to [`hash_file`], [`hash_glob`], or [`hash_command`] depending on
/// the `ChangeKind`. Returns the hash as a `Result<String, String>` for convenience.
pub fn hash_target(target: &str, kind: &ChangeKind, project_root: &Path) -> Result<String, String> {
    let result = match kind {
        ChangeKind::File => {
            let path = if Path::new(target).is_absolute() {
                std::path::PathBuf::from(target)
            } else {
                project_root.join(target)
            };
            hash_file(&path)
        }
        ChangeKind::Glob => hash_glob(target, project_root, &SizeLimit::default()),
        ChangeKind::Command => hash_command(target, project_root),
    };

    match result {
        HashResult::Ok(hash) => Ok(hash),
        HashResult::NotFound(msg) => Err(msg),
        HashResult::SizeLimitExceeded { actual, limit } => {
            Err(format!("Size limit exceeded: {} > {}", actual, limit))
        }
        HashResult::Error(msg) => Err(msg),
    }
}

/// Check total file size against the limit.
pub fn check_size_limit(path: &Path, size_limit: &SizeLimit) -> Option<(u64, u64)> {
    let limit = match size_limit {
        SizeLimit {
            max_bytes: Some(limit),
        } => *limit,
        SizeLimit { max_bytes: None } => return None,
    };

    let metadata = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return None, // File doesn't exist, will be caught by hash
    };

    let size = metadata.len();
    if size > limit {
        Some((size, limit))
    } else {
        None
    }
}

/// Evaluate a change check given a current hash and baseline hash.
///
/// This is the core logic that interprets the hash comparison result
/// according to the `on_change` semantics.
///
/// A check answers "is this step's work already done?", so `Passed` means
/// satisfied (the runner skips the step) and `Failed` means not satisfied
/// (the runner runs it). The two `on_change` modes read that question
/// differently:
///
/// - [`OnChange::Proceed`] treats the target as an input the step consumes,
///   so an unchanged target means the work is current (`Passed`) and a changed
///   target means it is stale (`Failed`).
/// - [`OnChange::Fail`] treats the target as something the step keeps stable,
///   so an unchanged target means the invariant holds (`Passed`) and a changed
///   target is unexpected drift (`Failed`).
/// - [`OnChange::Require`] reports on the step named by `require_step`, not on
///   the step hosting the check, so it is always `Indeterminate`.
///
/// With no baseline the comparison never happens, so the outcome is
/// `Indeterminate` and satisfaction falls through to execution history.
///
/// `require_step` is the step name reported when `on_change` is `Require`.
/// It's passed separately because the `OnChange` enum is a unit enum - the
/// step name lives on the Change check's `require_step` field.
pub fn evaluate_change_result(
    target: &str,
    current_hash: &str,
    baseline_hash: Option<&str>,
    on_change: &OnChange,
    require_step: Option<&str>,
) -> CheckResult {
    let Some(baseline) = baseline_hash else {
        return CheckResult::indeterminate(
            format!(
                "\u{25C7} No baseline for {} - this run will establish it",
                target
            ),
            format!(
                "No baseline exists for {}. This run will establish the baseline.",
                target
            ),
        );
    };

    let changed = current_hash != baseline;
    match (changed, on_change) {
        (true, OnChange::Proceed) => CheckResult::failed(
            format!("\u{25B3} {} changed - step will run", target),
            format!("{} changed since the last baseline", target),
        ),
        (false, OnChange::Proceed) => CheckResult::passed(format!("\u{2713} {} unchanged", target)),
        (true, OnChange::Fail) => CheckResult::failed(
            format!("\u{2717} {} changed unexpectedly", target),
            "File was expected to remain stable",
        ),
        (false, OnChange::Fail) => CheckResult::passed(format!("\u{2713} {} unchanged", target)),
        (_, OnChange::Require) => {
            let state = if changed { "changed" } else { "unchanged" };
            match require_step {
                Some(step_name) => CheckResult::indeterminate_with_details(
                    format!(
                        "\u{25C7} {} {} - {} is the required step",
                        target, state, step_name
                    ),
                    format!(
                        "on_change: require reports on '{}' and gives no verdict for this step",
                        step_name
                    ),
                    format!("Change in {} is tracked for step {}", target, step_name),
                ),
                None => CheckResult::indeterminate(
                    format!("\u{25C7} {} {} - no require_step set", target, state),
                    format!(
                        "on_change: require needs a require_step to name; {} gives no verdict for this step",
                        target
                    ),
                ),
            }
        }
    }
}

/// Compute the hash for a change check target.
pub fn compute_target_hash(
    target: &str,
    kind: ChangeKind,
    project_root: &Path,
    size_limit: &SizeLimit,
) -> HashResult {
    match kind {
        ChangeKind::File => {
            let path = if Path::new(target).is_absolute() {
                Path::new(target).to_path_buf()
            } else {
                project_root.join(target)
            };

            // Check size limit for files
            if let Some((actual, limit)) = check_size_limit(&path, size_limit) {
                return HashResult::SizeLimitExceeded { actual, limit };
            }

            hash_file(&path)
        }
        ChangeKind::Glob => hash_glob(target, project_root, size_limit),
        ChangeKind::Command => hash_command(target, project_root),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::CheckOutcome;
    use std::fs;
    use tempfile::TempDir;

    // --- File hashing tests ---

    #[test]
    fn hash_file_produces_consistent_hash() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("test.txt");
        fs::write(&path, "hello world").unwrap();

        let hash1 = hash_file(&path);
        let hash2 = hash_file(&path);
        match (hash1, hash2) {
            (HashResult::Ok(h1), HashResult::Ok(h2)) => assert_eq!(h1, h2),
            _ => panic!("Expected Ok results"),
        }
    }

    #[test]
    fn hash_file_differs_for_different_content() {
        let temp = TempDir::new().unwrap();
        let path1 = temp.path().join("a.txt");
        let path2 = temp.path().join("b.txt");
        fs::write(&path1, "hello").unwrap();
        fs::write(&path2, "world").unwrap();

        match (hash_file(&path1), hash_file(&path2)) {
            (HashResult::Ok(h1), HashResult::Ok(h2)) => assert_ne!(h1, h2),
            _ => panic!("Expected Ok results"),
        }
    }

    #[test]
    fn hash_file_not_found() {
        let result = hash_file(Path::new("/nonexistent/file.txt"));
        assert!(matches!(result, HashResult::NotFound(_)));
    }

    // --- Glob hashing tests ---

    #[test]
    fn hash_glob_hashes_matching_files() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("a.rb"), "class A; end").unwrap();
        fs::write(temp.path().join("b.rb"), "class B; end").unwrap();
        fs::write(temp.path().join("c.txt"), "not ruby").unwrap();

        let result = hash_glob("*.rb", temp.path(), &SizeLimit::default());
        assert!(matches!(result, HashResult::Ok(_)));
    }

    #[test]
    fn hash_glob_is_deterministic() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("a.rb"), "class A; end").unwrap();
        fs::write(temp.path().join("b.rb"), "class B; end").unwrap();

        match (
            hash_glob("*.rb", temp.path(), &SizeLimit::default()),
            hash_glob("*.rb", temp.path(), &SizeLimit::default()),
        ) {
            (HashResult::Ok(h1), HashResult::Ok(h2)) => assert_eq!(h1, h2),
            _ => panic!("Expected Ok results"),
        }
    }

    #[test]
    fn hash_glob_no_matches() {
        let temp = TempDir::new().unwrap();
        let result = hash_glob("*.nonexistent", temp.path(), &SizeLimit::default());
        assert!(matches!(result, HashResult::NotFound(_)));
    }

    #[test]
    fn hash_glob_changes_when_file_added() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("a.rb"), "class A; end").unwrap();

        let hash1 = match hash_glob("*.rb", temp.path(), &SizeLimit::default()) {
            HashResult::Ok(h) => h,
            _ => panic!("Expected Ok"),
        };

        fs::write(temp.path().join("b.rb"), "class B; end").unwrap();

        let hash2 = match hash_glob("*.rb", temp.path(), &SizeLimit::default()) {
            HashResult::Ok(h) => h,
            _ => panic!("Expected Ok"),
        };

        assert_ne!(hash1, hash2);
    }

    // --- Command hashing tests ---

    #[test]
    fn hash_command_success() {
        let temp = TempDir::new().unwrap();
        let result = hash_command("echo hello", temp.path());
        assert!(matches!(result, HashResult::Ok(_)));
    }

    #[test]
    fn hash_command_consistent() {
        let temp = TempDir::new().unwrap();
        match (
            hash_command("echo deterministic", temp.path()),
            hash_command("echo deterministic", temp.path()),
        ) {
            (HashResult::Ok(h1), HashResult::Ok(h2)) => assert_eq!(h1, h2),
            _ => panic!("Expected Ok results"),
        }
    }

    #[test]
    fn hash_command_failure() {
        let temp = TempDir::new().unwrap();
        let result = hash_command("exit 1", temp.path());
        assert!(matches!(result, HashResult::Error(_)));
    }

    // --- Size limit tests ---

    #[test]
    fn size_limit_not_exceeded() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("small.txt");
        fs::write(&path, "small").unwrap();

        assert!(check_size_limit(&path, &SizeLimit::bytes(1024)).is_none());
    }

    #[test]
    fn size_limit_exceeded() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("big.txt");
        fs::write(&path, "x".repeat(100)).unwrap();

        let result = check_size_limit(&path, &SizeLimit::bytes(10));
        assert!(result.is_some());
        let (actual, limit) = result.unwrap();
        assert_eq!(actual, 100);
        assert_eq!(limit, 10);
    }

    #[test]
    fn size_limit_none_always_passes() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("big.txt");
        fs::write(&path, "x".repeat(1000)).unwrap();

        assert!(check_size_limit(&path, &SizeLimit::none()).is_none());
    }

    // --- Change result evaluation tests ---

    #[test]
    fn proceed_fails_when_changed() {
        let result = evaluate_change_result(
            "Gemfile.lock",
            "hash_new",
            Some("hash_old"),
            &OnChange::Proceed,
            None,
        );
        assert!(!result.passed_check());
        assert!(result.description.contains("changed"));
    }

    #[test]
    fn proceed_passes_when_unchanged() {
        let result = evaluate_change_result(
            "Gemfile.lock",
            "hash_same",
            Some("hash_same"),
            &OnChange::Proceed,
            None,
        );
        assert!(result.passed_check());
        assert!(result.description.contains("unchanged"));
    }

    #[test]
    fn fail_passes_when_unchanged() {
        let result = evaluate_change_result(
            ".env.example",
            "hash_same",
            Some("hash_same"),
            &OnChange::Fail,
            None,
        );
        assert!(result.passed_check());
    }

    #[test]
    fn fail_fails_when_changed() {
        let result = evaluate_change_result(
            ".env.example",
            "hash_new",
            Some("hash_old"),
            &OnChange::Fail,
            None,
        );
        assert!(!result.passed_check());
        assert!(result.description.contains("changed unexpectedly"));
    }

    #[test]
    fn require_is_indeterminate_when_changed() {
        let result = evaluate_change_result(
            "Gemfile.lock",
            "hash_new",
            Some("hash_old"),
            &OnChange::Require,
            Some("bundle_install"),
        );
        assert!(matches!(result.outcome, CheckOutcome::Indeterminate(_)));
        assert!(result.description.contains("bundle_install"));
    }

    #[test]
    fn require_is_indeterminate_when_unchanged() {
        let result = evaluate_change_result(
            "Gemfile.lock",
            "hash_same",
            Some("hash_same"),
            &OnChange::Require,
            Some("bundle_install"),
        );
        assert!(matches!(result.outcome, CheckOutcome::Indeterminate(_)));
    }

    #[test]
    fn no_baseline_returns_indeterminate() {
        let result = evaluate_change_result(
            "Gemfile.lock",
            "hash_current",
            None,
            &OnChange::Proceed,
            None,
        );
        assert!(!result.passed_check());
        assert!(matches!(result.outcome, CheckOutcome::Indeterminate(_)));
        assert!(result.description.contains("No baseline"));
    }

    // --- compute_target_hash tests ---

    #[test]
    fn compute_file_hash() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("test.txt"), "content").unwrap();

        let result = compute_target_hash(
            "test.txt",
            ChangeKind::File,
            temp.path(),
            &SizeLimit::default(),
        );
        assert!(matches!(result, HashResult::Ok(_)));
    }

    #[test]
    fn compute_glob_hash() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("a.rb"), "class A").unwrap();

        let result =
            compute_target_hash("*.rb", ChangeKind::Glob, temp.path(), &SizeLimit::default());
        assert!(matches!(result, HashResult::Ok(_)));
    }

    #[test]
    fn compute_command_hash() {
        let temp = TempDir::new().unwrap();

        let result = compute_target_hash(
            "echo hello",
            ChangeKind::Command,
            temp.path(),
            &SizeLimit::default(),
        );
        assert!(matches!(result, HashResult::Ok(_)));
    }

    #[test]
    fn compute_file_hash_exceeds_size_limit() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("big.txt");
        fs::write(&path, "x".repeat(100)).unwrap();

        let result = compute_target_hash(
            "big.txt",
            ChangeKind::File,
            temp.path(),
            &SizeLimit::bytes(10),
        );
        assert!(matches!(result, HashResult::SizeLimitExceeded { .. }));
    }

    // --- Glob size limit tests ---

    #[test]
    fn glob_size_limit_exceeded() {
        let temp = TempDir::new().unwrap();
        // Create two files that together exceed the limit
        fs::write(temp.path().join("a.rb"), "x".repeat(60)).unwrap();
        fs::write(temp.path().join("b.rb"), "x".repeat(60)).unwrap();

        let result = compute_target_hash(
            "*.rb",
            ChangeKind::Glob,
            temp.path(),
            &SizeLimit::bytes(100),
        );
        assert!(matches!(result, HashResult::SizeLimitExceeded { .. }));
        if let HashResult::SizeLimitExceeded { actual, limit } = result {
            assert_eq!(actual, 120);
            assert_eq!(limit, 100);
        }
    }

    #[test]
    fn glob_size_limit_not_exceeded() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("a.rb"), "small").unwrap();
        fs::write(temp.path().join("b.rb"), "also small").unwrap();

        let result = compute_target_hash(
            "*.rb",
            ChangeKind::Glob,
            temp.path(),
            &SizeLimit::bytes(1024),
        );
        assert!(matches!(result, HashResult::Ok(_)));
    }

    #[test]
    fn glob_no_size_limit_allows_any_size() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("a.rb"), "x".repeat(1000)).unwrap();

        let result = compute_target_hash("*.rb", ChangeKind::Glob, temp.path(), &SizeLimit::none());
        assert!(matches!(result, HashResult::Ok(_)));
    }
}
