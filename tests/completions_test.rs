//! Integration tests for shell completions generation.
// The cargo_bin function is marked deprecated in favor of cargo_bin! macro,
// but both work correctly. Suppressing until assert_cmd stabilizes the new API.
#![allow(deprecated)]

use assert_cmd::cargo::cargo_bin;
use assert_cmd::Command;
use predicates::prelude::*;

// --- Shell completions generation ---

#[test]
fn completions_generates_bash_output() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["completions", "bash"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("bivvy"))
        .stdout(predicate::str::contains("complete"));
    Ok(())
}

#[test]
fn completions_generates_zsh_output() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["completions", "zsh"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("bivvy"))
        .stdout(predicate::str::contains("compdef").or(predicate::str::contains("_arguments")));
    Ok(())
}

#[test]
fn completions_generates_fish_output() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["completions", "fish"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("bivvy"))
        .stdout(predicate::str::contains("complete"));
    Ok(())
}

#[test]
fn completions_invalid_shell_fails() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["completions", "invalid-shell"]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("invalid value"));
    Ok(())
}

#[test]
fn completions_no_shell_arg_fails() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.arg("completions");
    cmd.assert().failure();
    Ok(())
}

#[test]
fn completions_bash_includes_subcommands() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["completions", "bash"]);
    let output = cmd.output()?;
    let stdout = String::from_utf8(output.stdout)?;
    // Bash completions should reference known subcommands
    assert!(stdout.contains("run") || stdout.contains("init") || stdout.contains("status"));
    Ok(())
}

#[test]
fn completions_zsh_includes_subcommands() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["completions", "zsh"]);
    let output = cmd.output()?;
    let stdout = String::from_utf8(output.stdout)?;
    // Zsh completions should reference known subcommands
    assert!(stdout.contains("run") || stdout.contains("init") || stdout.contains("status"));
    Ok(())
}

#[test]
fn completions_fish_includes_subcommands() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["completions", "fish"]);
    let output = cmd.output()?;
    let stdout = String::from_utf8(output.stdout)?;
    // Fish completions should reference known subcommands
    assert!(stdout.contains("run") || stdout.contains("init") || stdout.contains("status"));
    Ok(())
}

#[test]
fn completions_outputs_to_stdout_not_stderr() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["completions", "bash"]);
    let output = cmd.output()?;
    // Completions should go to stdout (not empty)
    assert!(!output.stdout.is_empty());
    // stderr should be empty (no errors)
    assert!(output.stderr.is_empty());
    Ok(())
}
