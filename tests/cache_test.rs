//! Integration tests for the `bivvy cache` CLI command.
// The cargo_bin function is marked deprecated in favor of cargo_bin! macro,
// but both work correctly. Suppressing until assert_cmd stabilizes the new API.
#![allow(deprecated)]

use assert_cmd::cargo::cargo_bin;
use assert_cmd::Command;
use predicates::prelude::*;

// --- Help and argument parsing ---

#[test]
fn cache_help_shows_subcommands() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["cache", "--help"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("clear"))
        .stdout(predicate::str::contains("stats"));
    Ok(())
}

#[test]
fn cache_no_subcommand_shows_usage() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.arg("cache");
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Usage"));
    Ok(())
}

// --- cache list ---

#[test]
fn cache_list_empty_shows_message() -> Result<(), Box<dyn std::error::Error>> {
    // Use a temporary HOME to get an empty cache dir
    let temp = tempfile::TempDir::new()?;
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["cache", "list"]);
    // Override cache location by pointing HOME to temp dir
    cmd.env("HOME", temp.path());
    cmd.assert().success().stdout(
        predicate::str::contains("Cache is empty").or(predicate::str::contains("cached entries")),
    );
    Ok(())
}

#[test]
fn cache_list_json_flag_accepted() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::TempDir::new()?;
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["cache", "list", "--json"]);
    cmd.env("HOME", temp.path());
    cmd.assert().success();
    Ok(())
}

#[test]
fn cache_list_verbose_flag_accepted() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::TempDir::new()?;
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["cache", "list", "--verbose"]);
    cmd.env("HOME", temp.path());
    cmd.assert().success();
    Ok(())
}

#[test]
fn cache_list_help() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["cache", "list", "--help"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("--verbose"))
        .stdout(predicate::str::contains("--json"));
    Ok(())
}

// --- cache clear ---

#[test]
fn cache_clear_force_on_empty_cache() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::TempDir::new()?;
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["cache", "clear", "--force"]);
    cmd.env("HOME", temp.path());
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("already empty").or(predicate::str::contains("Cleared")));
    Ok(())
}

#[test]
fn cache_clear_expired_on_empty_cache() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::TempDir::new()?;
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["cache", "clear", "--expired"]);
    cmd.env("HOME", temp.path());
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("expired entries"));
    Ok(())
}

#[test]
fn cache_clear_help() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["cache", "clear", "--help"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("--expired"))
        .stdout(predicate::str::contains("--force"));
    Ok(())
}

// --- cache stats ---

#[test]
fn cache_stats_shows_statistics() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::TempDir::new()?;
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["cache", "stats"]);
    cmd.env("HOME", temp.path());
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Cache Statistics"))
        .stdout(predicate::str::contains("Total entries"))
        .stdout(predicate::str::contains("Fresh"))
        .stdout(predicate::str::contains("Expired"))
        .stdout(predicate::str::contains("Total size"))
        .stdout(predicate::str::contains("Location"));
    Ok(())
}

#[test]
fn cache_stats_empty_shows_zero_counts() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::TempDir::new()?;
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["cache", "stats"]);
    cmd.env("HOME", temp.path());
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Total entries: 0"))
        .stdout(predicate::str::contains("Fresh: 0"))
        .stdout(predicate::str::contains("Expired: 0"))
        .stdout(predicate::str::contains("Total size: 0 bytes"));
    Ok(())
}

// --- Global flags with cache ---

#[test]
fn cache_accepts_debug_flag() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::TempDir::new()?;
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["--debug", "cache", "stats"]);
    cmd.env("HOME", temp.path());
    cmd.assert().success();
    Ok(())
}

#[test]
fn cache_accepts_quiet_flag() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::TempDir::new()?;
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["--quiet", "cache", "stats"]);
    cmd.env("HOME", temp.path());
    cmd.assert().success();
    Ok(())
}

// --- Invalid cache subcommands ---

#[test]
fn cache_invalid_subcommand_fails() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["cache", "invalid"]);
    cmd.assert().failure();
    Ok(())
}
