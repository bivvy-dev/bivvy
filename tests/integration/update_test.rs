//! Integration tests for the update checking functionality.
//!
//! Tests cover version comparison, install method detection, update info
//! construction, and the update notification flow. Network-dependent
//! tests (e.g., fetching from GitHub) are avoided to keep tests reliable.
#![allow(deprecated)]

use assert_cmd::cargo::cargo_bin;
use assert_cmd::Command;
use bivvy::updates::{
    clear_cache, detect_install_method, get_install_path, InstallMethod, UpdateInfo, VERSION,
};
use predicates::prelude::*;

// --- Version constant ---

#[test]
fn version_matches_cargo_pkg_version() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
    Ok(())
}

#[test]
fn version_is_not_empty() -> Result<(), Box<dyn std::error::Error>> {
    assert!(!VERSION.is_empty());
    Ok(())
}

// --- CLI version output uses the same constant ---

#[test]
fn cli_version_output_contains_version_constant() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.arg("--version");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains(VERSION));
    Ok(())
}

// --- Install method detection ---

#[test]
fn detect_install_method_does_not_panic() -> Result<(), Box<dyn std::error::Error>> {
    let method = detect_install_method();
    // Should return a valid variant with a name
    assert!(!method.name().is_empty());
    Ok(())
}

#[test]
fn get_install_path_returns_some() -> Result<(), Box<dyn std::error::Error>> {
    let path = get_install_path();
    // In a test context, the current executable should be resolvable
    assert!(path.is_some());
    Ok(())
}

// --- InstallMethod behavior ---

#[test]
fn cargo_install_method_supports_auto_update() -> Result<(), Box<dyn std::error::Error>> {
    assert!(InstallMethod::Cargo.supports_auto_update());
    assert!(InstallMethod::Cargo.update_command().is_some());
    assert_eq!(InstallMethod::Cargo.name(), "cargo");
    Ok(())
}

#[test]
fn homebrew_install_method_supports_auto_update() -> Result<(), Box<dyn std::error::Error>> {
    assert!(InstallMethod::Homebrew.supports_auto_update());
    assert!(InstallMethod::Homebrew.update_command().is_some());
    assert_eq!(InstallMethod::Homebrew.name(), "homebrew");
    Ok(())
}

#[test]
fn manual_install_method_does_not_support_auto_update() -> Result<(), Box<dyn std::error::Error>> {
    let method = InstallMethod::Manual {
        path: std::path::PathBuf::from("/usr/local/bin/bivvy"),
    };
    assert!(!method.supports_auto_update());
    assert!(method.update_command().is_none());
    assert_eq!(method.name(), "manual");
    Ok(())
}

#[test]
fn unknown_install_method_does_not_support_auto_update() -> Result<(), Box<dyn std::error::Error>> {
    assert!(!InstallMethod::Unknown.supports_auto_update());
    assert!(InstallMethod::Unknown.update_command().is_none());
    assert_eq!(InstallMethod::Unknown.name(), "unknown");
    Ok(())
}

// --- UpdateInfo construction ---

#[test]
fn update_info_with_newer_version() -> Result<(), Box<dyn std::error::Error>> {
    let info = UpdateInfo {
        current: "0.1.0".to_string(),
        latest: "0.2.0".to_string(),
        update_available: true,
        release_url: Some("https://github.com/bivvy-dev/bivvy/releases/v0.2.0".to_string()),
        checked_at: chrono::Utc::now(),
    };

    assert!(info.update_available);
    assert_eq!(info.current, "0.1.0");
    assert_eq!(info.latest, "0.2.0");
    assert!(info.release_url.is_some());
    Ok(())
}

#[test]
fn update_info_when_up_to_date() -> Result<(), Box<dyn std::error::Error>> {
    let info = UpdateInfo {
        current: VERSION.to_string(),
        latest: VERSION.to_string(),
        update_available: false,
        release_url: None,
        checked_at: chrono::Utc::now(),
    };

    assert!(!info.update_available);
    assert_eq!(info.current, info.latest);
    Ok(())
}

#[test]
fn update_info_serialization_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let info = UpdateInfo {
        current: "1.0.0".to_string(),
        latest: "1.1.0".to_string(),
        update_available: true,
        release_url: Some("https://example.com/release".to_string()),
        checked_at: chrono::Utc::now(),
    };

    let json = serde_json::to_string(&info)?;
    let parsed: UpdateInfo = serde_json::from_str(&json)?;

    assert_eq!(parsed.current, info.current);
    assert_eq!(parsed.latest, info.latest);
    assert_eq!(parsed.update_available, info.update_available);
    assert_eq!(parsed.release_url, info.release_url);
    Ok(())
}

#[test]
fn update_info_without_release_url() -> Result<(), Box<dyn std::error::Error>> {
    let info = UpdateInfo {
        current: "0.1.0".to_string(),
        latest: "0.2.0".to_string(),
        update_available: true,
        release_url: None,
        checked_at: chrono::Utc::now(),
    };

    assert!(info.release_url.is_none());
    Ok(())
}

// --- Cache management ---

#[test]
fn clear_cache_does_not_error() -> Result<(), Box<dyn std::error::Error>> {
    // Clearing cache should not fail even if the cache file does not exist
    clear_cache()?;
    Ok(())
}

// --- Update notification suppression ---

#[test]
fn suppress_and_check_notification() -> Result<(), Box<dyn std::error::Error>> {
    use bivvy::updates::{is_notification_suppressed, suppress_notification};

    suppress_notification();
    assert!(is_notification_suppressed());
    Ok(())
}

// --- Update commands in install method ---

#[test]
fn cargo_update_command_uses_force() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = InstallMethod::Cargo.update_command().unwrap();
    assert!(cmd.contains("cargo install"));
    assert!(cmd.contains("--force"));
    Ok(())
}

#[test]
fn homebrew_update_command_uses_upgrade() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = InstallMethod::Homebrew.update_command().unwrap();
    assert!(cmd.contains("brew upgrade"));
    Ok(())
}
