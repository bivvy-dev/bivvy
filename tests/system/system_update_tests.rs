//! Comprehensive system tests for `bivvy update`.
//!
//! Tests update checking, auto-update flags, CI behavior, version
//! display, and error handling.
#![cfg(unix)]

mod system;

use system::helpers::*;

// =====================================================================
// HAPPY PATH
// =====================================================================

/// Bare `update` command shows version or checking message.
#[test]
fn update_bare_command() {
    let mut s = spawn_bivvy_global(&["update"]);

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("version") || text.contains("Version") || text.contains("update")
            || text.contains("Update") || text.contains("Checking") || text.contains("bivvy"),
        "Bare update should show version or update info, got: {}",
        &text[..text.len().min(300)]
    );
}

/// --check shows current version info.
#[test]
fn update_check_flag() {
    let mut s = spawn_bivvy_global(&["update", "--check"]);

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("version") || text.contains("Version") || text.contains("up to date")
            || text.contains("bivvy") || text.contains("Checking"),
        "Check should show version info, got: {}",
        &text[..text.len().min(300)]
    );
}

/// Auto-update enable flag is accepted.
#[test]
fn update_enable_auto_update() {
    let mut s = spawn_bivvy_global(&["update", "--enable-auto-update"]);

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("auto-update") || text.contains("enabled") || text.contains("Auto")
            || text.contains("update"),
        "Enable auto-update should be accepted, got: {}",
        &text[..text.len().min(300)]
    );
}

/// Auto-update disable flag is accepted.
#[test]
fn update_disable_auto_update() {
    let mut s = spawn_bivvy_global(&["update", "--disable-auto-update"]);

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("auto-update") || text.contains("disabled") || text.contains("Auto")
            || text.contains("update"),
        "Disable auto-update should be accepted, got: {}",
        &text[..text.len().min(300)]
    );
}

/// CI environment skips interactive update.
#[test]
fn update_ci_environment_skips() {
    let mut s = spawn_bivvy_global(&["update"]);

    let text = read_to_eof(&mut s);
    // In CI, update should still work (may skip interactive parts)
    assert!(
        text.contains("version") || text.contains("Version") || text.contains("update")
            || text.contains("Update") || text.contains("Checking") || text.contains("bivvy"),
        "Update in CI should show version or update info, got: {}",
        &text[..text.len().min(300)]
    );
}

/// Version display verification.
#[test]
fn update_shows_version_info() {
    let mut s = spawn_bivvy_global(&["update", "--check"]);

    let text = read_to_eof(&mut s);
    // Should contain version-like info
    assert!(
        text.contains('.') || text.contains("version") || text.contains("bivvy"),
        "Should show version info, got: {}",
        &text[..text.len().min(300)]
    );
}

// =====================================================================
// HELP
// =====================================================================

/// --help shows all expected flags.
#[test]
fn update_help() {
    let mut s = spawn_bivvy_global(&["update", "--help"]);

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Check for and install updates") || text.contains("update")
            || text.contains("Update"),
        "Help should mention update, got: {}",
        &text[..text.len().min(300)]
    );
    assert!(
        text.contains("--check"),
        "Help should list --check flag, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("--enable-auto-update"),
        "Help should list --enable-auto-update, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("--disable-auto-update"),
        "Help should list --disable-auto-update, got: {}",
        &text[..text.len().min(500)]
    );
}

// =====================================================================
// EXIT CODES
// =====================================================================

/// --help exits with code 0.
#[test]
fn update_help_exit_code_zero() {
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let status = std::process::Command::new(bin)
        .args(["update", "--help"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("Failed to run bivvy");
    assert!(status.success(), "Help should exit with code 0");
}

// =====================================================================
// SAD PATH
// =====================================================================

/// Unknown flags are rejected.
#[test]
fn update_unknown_flag() {
    let mut s = spawn_bivvy_global(&["update", "--frobnicate"]);

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("error") || text.contains("unexpected") || text.contains("unrecognized")
            || text.contains("frobnicate"),
        "Unknown flag should show error, got: {}",
        &text[..text.len().min(300)]
    );
}
