//! Comprehensive system tests for `bivvy completions`.
//!
//! Tests shell completion generation for all supported shells,
//! shell-specific syntax validation, output substance, subcommand
//! coverage, and error handling for invalid inputs.
#![cfg(unix)]

mod system;

use system::helpers::*;

// =====================================================================
// HAPPY PATH — Shell-specific syntax validation
// =====================================================================

/// Bash completions contain bash-specific `complete` builtin.
#[test]
fn completions_bash() {
    let mut s = spawn_bivvy_global(&["completions", "bash"]);
    let text = read_to_eof(&mut s);

    assert!(
        text.contains("bivvy"),
        "Bash completions should reference bivvy, got: {}",
        &text[..text.len().min(200)]
    );
    assert!(
        text.contains("complete"),
        "Bash completions should contain 'complete' builtin, got: {}",
        &text[..text.len().min(200)]
    );
}

/// Zsh completions contain zsh-specific `compdef` or `_bivvy`.
#[test]
fn completions_zsh() {
    let mut s = spawn_bivvy_global(&["completions", "zsh"]);
    let text = read_to_eof(&mut s);

    assert!(
        text.contains("bivvy"),
        "Zsh completions should reference bivvy"
    );
    assert!(
        text.contains("compdef") || text.contains("_bivvy"),
        "Zsh completions should contain 'compdef' or '_bivvy', got: {}",
        &text[..text.len().min(200)]
    );
}

/// Fish completions contain fish-specific `complete` command.
#[test]
fn completions_fish() {
    let mut s = spawn_bivvy_global(&["completions", "fish"]);
    let text = read_to_eof(&mut s);

    assert!(
        text.contains("bivvy"),
        "Fish completions should reference bivvy"
    );
    assert!(
        text.contains("complete"),
        "Fish completions should contain 'complete' command, got: {}",
        &text[..text.len().min(200)]
    );
}

/// PowerShell completions contain Register-ArgumentCompleter or similar.
#[test]
fn completions_powershell() {
    let mut s = spawn_bivvy_global(&["completions", "powershell"]);
    let text = read_to_eof(&mut s);

    assert!(
        text.contains("bivvy"),
        "PowerShell completions should reference bivvy"
    );
    assert!(
        text.contains("Register-ArgumentCompleter") || text.contains("CommandAst"),
        "PowerShell completions should contain PS-specific keywords, got: {}",
        &text[..text.len().min(200)]
    );
}

/// Elvish completions contain elvish-specific syntax.
#[test]
fn completions_elvish() {
    let mut s = spawn_bivvy_global(&["completions", "elvish"]);
    let text = read_to_eof(&mut s);

    assert!(
        text.contains("bivvy"),
        "Elvish completions should reference bivvy"
    );
    assert!(
        text.contains("edit:completion") || text.contains("set-env") || text.contains("elvish"),
        "Elvish completions should contain elvish-specific syntax, got: {}",
        &text[..text.len().min(200)]
    );
}

// =====================================================================
// HAPPY PATH — Substantial output for all shells
// =====================================================================

/// Bash completions output is non-trivial.
#[test]
fn completions_bash_substantial_output() {
    let mut s = spawn_bivvy_global(&["completions", "bash"]);
    let text = read_to_eof(&mut s);
    assert!(
        text.len() > 100,
        "Bash completions should be substantial, got {} bytes",
        text.len()
    );
}

/// Zsh completions output is non-trivial.
#[test]
fn completions_zsh_substantial_output() {
    let mut s = spawn_bivvy_global(&["completions", "zsh"]);
    let text = read_to_eof(&mut s);
    assert!(
        text.len() > 100,
        "Zsh completions should be substantial, got {} bytes",
        text.len()
    );
}

/// Fish completions output is non-trivial.
#[test]
fn completions_fish_substantial_output() {
    let mut s = spawn_bivvy_global(&["completions", "fish"]);
    let text = read_to_eof(&mut s);
    assert!(
        text.len() > 100,
        "Fish completions should be substantial, got {} bytes",
        text.len()
    );
}

/// PowerShell completions output is non-trivial.
#[test]
fn completions_powershell_substantial_output() {
    let mut s = spawn_bivvy_global(&["completions", "powershell"]);
    let text = read_to_eof(&mut s);
    assert!(
        text.len() > 100,
        "PowerShell completions should be substantial, got {} bytes",
        text.len()
    );
}

/// Elvish completions output is non-trivial.
#[test]
fn completions_elvish_substantial_output() {
    let mut s = spawn_bivvy_global(&["completions", "elvish"]);
    let text = read_to_eof(&mut s);
    assert!(
        text.len() > 100,
        "Elvish completions should be substantial, got {} bytes",
        text.len()
    );
}

// =====================================================================
// HAPPY PATH — Subcommand coverage in completions
// =====================================================================

/// Bash completions include all bivvy subcommands.
#[test]
fn completions_contain_all_subcommands() {
    let mut s = spawn_bivvy_global(&["completions", "bash"]);
    let text = read_to_eof(&mut s);

    let expected_subcommands = [
        "run", "init", "add", "templates", "status", "list", "lint",
        "last", "history", "feedback", "completions", "config", "cache",
        "update",
    ];

    for subcmd in &expected_subcommands {
        assert!(
            text.contains(subcmd),
            "Bash completions should contain subcommand '{}', output length: {}",
            subcmd,
            text.len()
        );
    }
}

// =====================================================================
// HELP
// =====================================================================

/// --help shows the expected description.
#[test]
fn completions_help() {
    let mut s = spawn_bivvy_global(&["completions", "--help"]);
    let text = read_to_eof(&mut s);

    assert!(
        text.contains("completions") || text.contains("Completions") || text.contains("shell"),
        "Help should mention completions or shell, got: {}",
        &text[..text.len().min(300)]
    );
}

// =====================================================================
// SAD PATH
// =====================================================================

/// No shell argument shows error about missing required argument.
#[test]
fn completions_no_argument() {
    let mut s = spawn_bivvy_global(&["completions"]);
    let text = read_to_eof(&mut s);

    assert!(
        text.contains("required") || text.contains("Usage") || text.contains("error"),
        "Missing shell arg should produce an error message, got: {}",
        &text[..text.len().min(300)]
    );
}

/// Unknown shell name shows error about invalid value.
#[test]
fn completions_unknown_shell() {
    let mut s = spawn_bivvy_global(&["completions", "fakeshell"]);
    let text = read_to_eof(&mut s);

    assert!(
        text.contains("invalid") || text.contains("fakeshell") || text.contains("error"),
        "Unknown shell should produce an error message, got: {}",
        &text[..text.len().min(300)]
    );
}
