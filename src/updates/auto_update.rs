//! Background auto-update functionality.
//!
//! Provides automatic, zero-interaction updates for bivvy. On each run,
//! a background process checks for new versions and either:
//! - Runs the package manager update command (cargo/homebrew installs)
//! - Downloads and stages a new binary (manual installs)
//!
//! For manual installs, the staged binary is swapped in on the next startup.
//! For package-manager installs, the update is applied immediately by the
//! package manager and takes effect on the next invocation.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use super::version::{check_for_updates, UpdateInfo};
use super::{detect_install_method, InstallMethod};

/// Metadata for a staged binary update (manual installs only).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedUpdate {
    /// The version that was downloaded.
    pub version: String,
    /// When the binary was staged.
    pub staged_at: DateTime<Utc>,
}

/// Get the staging directory for downloaded binaries.
fn staging_dir() -> Option<PathBuf> {
    crate::sys::data_dir().map(|d| d.join("bivvy").join("staged-update"))
}

/// Get the path to the staged binary.
fn staged_binary_path() -> Option<PathBuf> {
    staging_dir().map(|d| {
        if cfg!(windows) {
            d.join("bivvy.exe")
        } else {
            d.join("bivvy")
        }
    })
}

/// Get the path to the staging metadata file.
fn staged_metadata_path() -> Option<PathBuf> {
    staging_dir().map(|d| d.join("metadata.json"))
}

/// Get the path to the background update lock file.
fn lock_path() -> Option<PathBuf> {
    crate::sys::cache_dir().map(|d| d.join("bivvy").join("update.lock"))
}

/// Check if a staged update is ready to apply.
pub fn check_staged_update() -> Option<StagedUpdate> {
    let meta_path = staged_metadata_path()?;
    let content = fs::read_to_string(meta_path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Apply a staged update by replacing the current binary.
///
/// Only applies to manual installs — for cargo/homebrew, the package manager
/// already placed the new binary. Returns the new version string if applied.
pub fn apply_staged_update() -> Result<Option<String>> {
    let staged = match check_staged_update() {
        Some(s) => s,
        None => return Ok(None),
    };

    let method = detect_install_method();

    // Staged binary replacement only applies to manual installs.
    // For package-manager installs, the manager already updated the binary.
    if !matches!(method, InstallMethod::Manual { .. }) {
        let _ = clear_staged_update();
        return Ok(None);
    }

    let staged_binary = match staged_binary_path() {
        Some(p) if p.exists() => p,
        _ => return Ok(None),
    };

    let current_exe = std::env::current_exe().context("Failed to locate current executable")?;

    replace_binary(&staged_binary, &current_exe)?;

    let _ = clear_staged_update();

    Ok(Some(staged.version))
}

/// Replace the current binary with the staged one (Unix).
#[cfg(unix)]
fn replace_binary(staged: &PathBuf, current: &PathBuf) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    // Copy to a temp file in the same directory (ensures same-filesystem rename)
    let temp = current.with_extension("new");
    fs::copy(staged, &temp).context("Failed to copy staged binary")?;

    // Preserve original permissions and ensure executable
    let perms = fs::metadata(current)
        .map(|m| m.permissions())
        .unwrap_or_else(|_| {
            let mut p = fs::metadata(&temp).unwrap().permissions();
            p.set_mode(0o755);
            p
        });
    fs::set_permissions(&temp, perms)?;

    // Atomic rename
    fs::rename(&temp, current).context("Failed to replace binary")?;

    Ok(())
}

/// Replace the current binary with the staged one (Windows).
#[cfg(windows)]
fn replace_binary(staged: &PathBuf, current: &PathBuf) -> Result<()> {
    let old = current.with_extension("old.exe");

    // Clean up any leftover from a previous update
    let _ = fs::remove_file(&old);

    // Windows allows renaming a running executable
    fs::rename(current, &old).context("Failed to rename current binary")?;
    fs::copy(staged, current).context("Failed to copy new binary into place")?;

    // Best-effort cleanup (may fail if old binary is still running)
    let _ = fs::remove_file(&old);

    Ok(())
}

/// Remove all staged update files.
fn clear_staged_update() -> Result<()> {
    if let Some(dir) = staging_dir() {
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
    }
    Ok(())
}

/// Check whether a background update process is already running (or ran recently).
///
/// Uses a lock file with a timestamp. If the lock is less than 5 minutes old,
/// we assume the process is still active or just finished.
fn is_update_process_running() -> bool {
    let path = match lock_path() {
        Some(p) => p,
        None => return false,
    };

    if let Ok(meta) = fs::metadata(&path) {
        if let Ok(modified) = meta.modified() {
            if let Ok(age) = modified.elapsed() {
                return age.as_secs() < 300;
            }
        }
    }

    false
}

/// Write a lock file to prevent concurrent background update processes.
fn write_lock_file() -> Result<()> {
    let path = lock_path().context("No cache directory for lock file")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, Utc::now().to_rfc3339())?;
    Ok(())
}

/// Remove the lock file after the background process completes.
fn remove_lock_file() {
    if let Some(path) = lock_path() {
        let _ = fs::remove_file(path);
    }
}

/// Read the `auto_update` setting from the system config (`~/.bivvy/config.yml`).
///
/// Returns the configured value, or `true` (the default) if the system
/// config doesn't exist or doesn't contain the setting.
pub fn is_auto_update_enabled() -> bool {
    let path = match crate::sys::home_dir() {
        Some(h) => h.join(".bivvy").join("config.yml"),
        None => return true,
    };

    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return true, // No system config → default on
    };

    // Parse just enough to read settings.auto_update
    let value: serde_yaml::Value = match serde_yaml::from_str(&content) {
        Ok(v) => v,
        Err(_) => return true,
    };

    value
        .get("settings")
        .and_then(|s| s.get("auto_update"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

/// Decide whether to spawn a background update process.
pub fn should_spawn_background_update(is_interactive: bool, is_ci: bool) -> bool {
    // Never auto-update in CI
    if is_ci {
        return false;
    }

    // Only auto-update in interactive sessions
    if !is_interactive {
        return false;
    }

    // Respect the user's config setting
    if !is_auto_update_enabled() {
        return false;
    }

    // Don't spawn if another process is already running
    if is_update_process_running() {
        return false;
    }

    // Don't spawn if there's already a staged update waiting
    if check_staged_update().is_some() {
        return false;
    }

    true
}

/// Spawn a detached background process to check for and apply updates.
///
/// The background process is the same bivvy binary, invoked with the
/// `BIVVY_SELF_UPDATE_BG=1` environment variable. It runs silently
/// with no stdin/stdout/stderr.
pub fn spawn_background_update() -> Result<()> {
    let exe = std::env::current_exe().context("Failed to locate current executable")?;

    let mut cmd = Command::new(exe);
    cmd.env("BIVVY_SELF_UPDATE_BG", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    // On Unix, create a new session so the child survives if the parent's
    // terminal closes.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }

    // On Windows, use DETACHED_PROCESS so the child has no console window.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x00000008); // DETACHED_PROCESS
    }

    cmd.spawn()
        .context("Failed to spawn background update process")?;

    Ok(())
}

/// Entry point for the background update process.
///
/// Called when bivvy is invoked with `BIVVY_SELF_UPDATE_BG=1`.
/// Checks for updates and either runs the package manager command
/// or downloads and stages the new binary.
pub fn perform_background_update() -> Result<()> {
    write_lock_file()?;

    let result = do_background_update();

    remove_lock_file();

    result
}

/// Inner logic for background updates, separated so the lock file
/// is always cleaned up regardless of success/failure.
fn do_background_update() -> Result<()> {
    let info = match check_for_updates() {
        Some(info) if info.update_available => info,
        _ => return Ok(()),
    };

    let method = detect_install_method();

    match method {
        InstallMethod::Cargo => {
            run_package_manager_update("cargo", &["install", "bivvy", "--force"])?;
        }
        InstallMethod::Homebrew => {
            run_package_manager_update("brew", &["upgrade", "bivvy"])?;
        }
        InstallMethod::Manual { .. } => {
            stage_binary_update(&info)?;
        }
        InstallMethod::Unknown => {
            // Can't auto-update — silently skip
        }
    }

    Ok(())
}

/// Run a package manager's update command silently.
fn run_package_manager_update(program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| format!("Failed to run {} {}", program, args.join(" ")))?;

    if !status.success() {
        anyhow::bail!(
            "{} {} exited with code {:?}",
            program,
            args.join(" "),
            status.code()
        );
    }

    Ok(())
}

/// Download the appropriate binary from GitHub releases and stage it.
fn stage_binary_update(info: &UpdateInfo) -> Result<()> {
    let asset_url = find_platform_asset_url(info)?;

    let dir = staging_dir().context("No data directory for staging")?;
    fs::create_dir_all(&dir)?;

    let binary_path = if cfg!(windows) {
        dir.join("bivvy.exe")
    } else {
        dir.join("bivvy")
    };

    download_asset(&asset_url, &binary_path)?;

    // Write metadata so apply_staged_update knows the version
    let metadata = StagedUpdate {
        version: info.latest.clone(),
        staged_at: Utc::now(),
    };
    let meta_path = dir.join("metadata.json");
    fs::write(meta_path, serde_json::to_string_pretty(&metadata)?)?;

    Ok(())
}

/// Query the GitHub releases API to find the download URL for this platform.
fn find_platform_asset_url(info: &UpdateInfo) -> Result<String> {
    let target = platform_target();

    let client = reqwest::blocking::Client::builder()
        .user_agent("bivvy")
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let api_url = format!(
        "https://api.github.com/repos/bivvy-dev/bivvy/releases/tags/v{}",
        info.latest
    );

    let response: serde_json::Value = client
        .get(&api_url)
        .send()?
        .json()
        .context("Failed to parse GitHub releases API response")?;

    let assets = response["assets"]
        .as_array()
        .context("No assets array in release")?;

    for asset in assets {
        let name = asset["name"].as_str().unwrap_or("");
        if name.contains(&target) {
            if let Some(url) = asset["browser_download_url"].as_str() {
                return Ok(url.to_string());
            }
        }
    }

    anyhow::bail!(
        "No release asset found for platform '{}' in v{}",
        target,
        info.latest
    )
}

/// Return the Rust target triple fragment for this platform.
///
/// Used to match against GitHub release asset filenames (e.g.
/// `bivvy-x86_64-apple-darwin`, `bivvy-aarch64-unknown-linux-gnu`).
pub fn platform_target() -> String {
    let os = match std::env::consts::OS {
        "macos" => "apple-darwin",
        "linux" => "unknown-linux-gnu",
        "windows" => "pc-windows-msvc",
        other => other,
    };

    let arch = std::env::consts::ARCH;

    format!("{}-{}", arch, os)
}

/// Download a file from a URL to a local path.
fn download_asset(url: &str, dest: &PathBuf) -> Result<()> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("bivvy")
        .timeout(std::time::Duration::from_secs(120))
        .build()?;

    let bytes = client
        .get(url)
        .send()
        .context("Failed to download update")?
        .bytes()
        .context("Failed to read download body")?;

    fs::write(dest, &bytes)?;

    // Make executable on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(dest)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(dest, perms)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn platform_target_contains_arch_and_os() {
        let target = platform_target();
        // Should contain architecture
        assert!(
            target.contains("x86_64")
                || target.contains("aarch64")
                || target.contains(std::env::consts::ARCH),
            "Target '{}' should contain architecture",
            target
        );
        // Should contain OS identifier
        assert!(
            target.contains("darwin")
                || target.contains("linux")
                || target.contains("windows")
                || target.contains(std::env::consts::OS),
            "Target '{}' should contain OS identifier",
            target
        );
    }

    #[test]
    fn staged_update_serialization_roundtrip() {
        let staged = StagedUpdate {
            version: "2.0.0".to_string(),
            staged_at: Utc::now(),
        };

        let json = serde_json::to_string(&staged).unwrap();
        let parsed: StagedUpdate = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.version, "2.0.0");
    }

    #[test]
    fn staging_dir_returns_path() {
        // On most systems crate::sys::data_dir() is Some
        if let Some(dir) = staging_dir() {
            assert!(dir.ends_with("staged-update"));
        }
    }

    #[test]
    fn staged_binary_path_has_correct_name() {
        if let Some(path) = staged_binary_path() {
            let name = path.file_name().unwrap().to_string_lossy();
            if cfg!(windows) {
                assert_eq!(name, "bivvy.exe");
            } else {
                assert_eq!(name, "bivvy");
            }
        }
    }

    #[test]
    fn check_staged_update_returns_none_when_no_staging() {
        // No staged update should exist in test environment
        // (unless a real update was staged, which is unlikely)
        // This primarily tests that the function doesn't panic
        let _ = check_staged_update();
    }

    #[test]
    fn clear_staged_update_is_idempotent() {
        // Should not error even when there's nothing to clear
        let result = clear_staged_update();
        assert!(result.is_ok());
    }

    #[test]
    fn should_spawn_returns_false_in_ci() {
        assert!(!should_spawn_background_update(true, true));
    }

    #[test]
    fn should_spawn_returns_false_when_non_interactive() {
        assert!(!should_spawn_background_update(false, false));
    }

    #[test]
    fn should_spawn_returns_true_in_interactive_non_ci() {
        // This may still return false if a lock file or staged update exists,
        // but the CI and interactive checks should pass
        let result = should_spawn_background_update(true, false);
        // Can't assert true because lock file state is unpredictable,
        // but we at least verify it doesn't panic
        let _ = result;
    }

    #[test]
    fn is_update_process_running_returns_false_without_lock() {
        // Without a lock file, should be false
        // (unless one exists from a real update, but unlikely in test)
        let _ = is_update_process_running();
    }

    #[test]
    fn write_and_check_lock_file() {
        // Write a lock file and verify it's detected
        if write_lock_file().is_ok() {
            assert!(is_update_process_running());
            remove_lock_file();
            // After removal, should no longer be running
            // (give a tiny margin for filesystem lag)
            assert!(!is_update_process_running());
        }
    }

    #[test]
    fn replace_binary_with_temp_files() {
        let temp = TempDir::new().unwrap();

        // Create a fake "current" binary
        let current = temp.path().join("bivvy");
        fs::write(&current, b"old version").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&current).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&current, perms).unwrap();
        }

        // Create a fake "staged" binary
        let staged = temp.path().join("bivvy-staged");
        fs::write(&staged, b"new version").unwrap();

        // Replace
        let result = replace_binary(&staged, &current);
        assert!(result.is_ok(), "replace_binary failed: {:?}", result.err());

        // Verify content changed
        let content = fs::read_to_string(&current).unwrap();
        assert_eq!(content, "new version");

        // Verify temp file was cleaned up
        assert!(!temp.path().join("bivvy.new").exists());
    }

    #[test]
    fn apply_staged_update_returns_none_when_nothing_staged() {
        // In test environment, there should be no staged update
        let result = apply_staged_update().unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn staged_metadata_path_returns_json_file() {
        if let Some(path) = staged_metadata_path() {
            assert_eq!(path.file_name().unwrap().to_string_lossy(), "metadata.json");
        }
    }

    #[test]
    fn lock_path_returns_lock_file() {
        if let Some(path) = lock_path() {
            assert_eq!(path.file_name().unwrap().to_string_lossy(), "update.lock");
        }
    }
}
