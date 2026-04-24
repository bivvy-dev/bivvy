//! System tests for `bivvy completions` — all interactive, PTY-based.
//!
//! Verifies that `bivvy completions <shell>` produces well-formed completion
//! scripts for every supported shell, and that the command's help and error
//! messages match documented behavior via snapshots.
#![cfg(unix)]

mod system;

use expectrl::WaitStatus;
use system::helpers::*;

// ---------------------------------------------------------------------------
// Helper: verify clean exit with a specific code after EOF is reached
// ---------------------------------------------------------------------------

fn assert_exit_code_is(s: &mut expectrl::Session, expected: i32) {
    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(
        status,
        WaitStatus::Exited(pid, expected),
        "Expected exit code {expected}"
    );
}

/// Every `bivvy` subcommand that should appear in a generated completion
/// script. Used to ensure clap_complete emitted a full command tree.
const EXPECTED_SUBCOMMANDS: &[&str] = &[
    "run",
    "init",
    "add",
    "templates",
    "status",
    "list",
    "lint",
    "last",
    "history",
    "feedback",
    "completions",
    "config",
    "cache",
    "update",
];

// ===========================================================================
// HAPPY PATH — per-shell content verification
// ===========================================================================

/// Bash completions define the `_bivvy` function, use the bash-specific
/// `COMPREPLY` array, and reference every bivvy subcommand.
#[test]
fn completions_bash() {
    let mut s = spawn_bivvy_global(&["completions", "bash"]);
    let text = read_to_eof(&mut s);

    assert!(
        text.contains("_bivvy()"),
        "Bash completions should define the _bivvy() function, got: {}",
        &text[..text.len().min(400)]
    );
    assert!(
        text.contains("COMPREPLY"),
        "Bash completions should populate the COMPREPLY array, got: {}",
        &text[..text.len().min(400)]
    );
    assert!(
        text.len() > 500,
        "Bash completions should be substantial, got {} bytes",
        text.len()
    );
    for sub in EXPECTED_SUBCOMMANDS {
        assert!(
            text.contains(sub),
            "Bash completions should reference the '{sub}' subcommand"
        );
    }

    assert_exit_code_is(&mut s, 0);
}

/// Zsh completions are headed by `#compdef bivvy`, define the `_bivvy`
/// function via `_arguments`, and cover every bivvy subcommand.
#[test]
fn completions_zsh() {
    let mut s = spawn_bivvy_global(&["completions", "zsh"]);
    let text = read_to_eof(&mut s);

    assert!(
        text.contains("#compdef bivvy"),
        "Zsh completions should begin with '#compdef bivvy', got: {}",
        &text[..text.len().min(400)]
    );
    assert!(
        text.contains("_bivvy"),
        "Zsh completions should define a _bivvy completion function, got: {}",
        &text[..text.len().min(400)]
    );
    assert!(
        text.contains("_arguments"),
        "Zsh completions should use the _arguments helper, got: {}",
        &text[..text.len().min(400)]
    );
    for sub in EXPECTED_SUBCOMMANDS {
        assert!(
            text.contains(sub),
            "Zsh completions should reference the '{sub}' subcommand"
        );
    }

    assert_exit_code_is(&mut s, 0);
}

/// Fish completions use the `complete -c bivvy` builtin and define the
/// `__fish_bivvy_global_optspecs` helper for option parsing.
#[test]
fn completions_fish() {
    let mut s = spawn_bivvy_global(&["completions", "fish"]);
    let text = read_to_eof(&mut s);

    assert!(
        text.contains("__fish_bivvy_global_optspecs"),
        "Fish completions should define __fish_bivvy_global_optspecs, got: {}",
        &text[..text.len().min(400)]
    );
    assert!(
        text.contains("complete -c bivvy"),
        "Fish completions should use 'complete -c bivvy', got: {}",
        &text[..text.len().min(400)]
    );
    for sub in EXPECTED_SUBCOMMANDS {
        assert!(
            text.contains(sub),
            "Fish completions should reference the '{sub}' subcommand"
        );
    }

    assert_exit_code_is(&mut s, 0);
}

/// PowerShell completions register a native argument completer and cover
/// every bivvy subcommand.
#[test]
fn completions_powershell() {
    let mut s = spawn_bivvy_global(&["completions", "powershell"]);
    let text = read_to_eof(&mut s);

    assert!(
        text.contains("Register-ArgumentCompleter"),
        "PowerShell completions should call Register-ArgumentCompleter, got: {}",
        &text[..text.len().min(400)]
    );
    assert!(
        text.contains("'bivvy'"),
        "PowerShell completions should target the 'bivvy' command, got: {}",
        &text[..text.len().min(400)]
    );
    for sub in EXPECTED_SUBCOMMANDS {
        assert!(
            text.contains(sub),
            "PowerShell completions should reference the '{sub}' subcommand"
        );
    }

    assert_exit_code_is(&mut s, 0);
}

/// Elvish completions use `edit:completion:arg-completer` and cover every
/// bivvy subcommand.
#[test]
fn completions_elvish() {
    let mut s = spawn_bivvy_global(&["completions", "elvish"]);
    let text = read_to_eof(&mut s);

    assert!(
        text.contains("edit:completion:arg-completer"),
        "Elvish completions should set edit:completion:arg-completer, got: {}",
        &text[..text.len().min(400)]
    );
    assert!(
        text.contains("bivvy"),
        "Elvish completions should reference bivvy, got: {}",
        &text[..text.len().min(400)]
    );
    for sub in EXPECTED_SUBCOMMANDS {
        assert!(
            text.contains(sub),
            "Elvish completions should reference the '{sub}' subcommand"
        );
    }

    assert_exit_code_is(&mut s, 0);
}

// ===========================================================================
// HELP — verified via snapshot to catch description / flag regressions
// ===========================================================================

/// `bivvy completions --help` matches the approved snapshot and exits 0.
#[test]
fn completions_help() {
    let mut s = spawn_bivvy_global(&["completions", "--help"]);
    let text = read_to_eof(&mut s);

    // Sanity-check the user-visible description before snapshotting — this
    // guarantees the snapshot content is load-bearing.
    assert!(
        text.contains("Generate shell completions"),
        "Help output should contain the full description 'Generate shell completions', got: {}",
        &text[..text.len().min(400)]
    );
    assert!(
        text.contains("[possible values: bash, elvish, fish, powershell, zsh]"),
        "Help output should list every supported shell as possible values, got: {}",
        &text[..text.len().min(400)]
    );

    insta::assert_snapshot!("completions_help", text);

    assert_exit_code_is(&mut s, 0);
}

// ===========================================================================
// SAD PATH — invalid and missing arguments exit with documented code 2
// ===========================================================================

/// Passing an unknown shell exits with code 2 and prints the clap-standard
/// "invalid value" error that names the offending input.
#[test]
fn completions_invalid_shell_exits_2() {
    let mut s = spawn_bivvy_global(&["completions", "invalid"]);
    let text = read_to_eof(&mut s);

    assert!(
        text.contains("error: invalid value 'invalid' for '<SHELL>'"),
        "Should show the exact clap error 'error: invalid value 'invalid' for '<SHELL>'', got: {}",
        &text[..text.len().min(400)]
    );
    assert!(
        text.contains("[possible values: bash, elvish, fish, powershell, zsh]"),
        "Error should list every supported shell as possible values, got: {}",
        &text[..text.len().min(400)]
    );

    insta::assert_snapshot!("completions_invalid_shell_error", text);

    assert_exit_code_is(&mut s, 2);
}

/// Omitting the shell argument exits with code 2 and prints the clap-standard
/// "the following required arguments were not provided" error.
#[test]
fn completions_missing_shell_exits_2() {
    let mut s = spawn_bivvy_global(&["completions"]);
    let text = read_to_eof(&mut s);

    assert!(
        text.contains("error: the following required arguments were not provided:"),
        "Should show the exact clap error 'error: the following required arguments were not provided:', got: {}",
        &text[..text.len().min(400)]
    );
    assert!(
        text.contains("<SHELL>"),
        "Error should name the missing <SHELL> argument, got: {}",
        &text[..text.len().min(400)]
    );
    assert!(
        text.contains("Usage: bivvy completions <SHELL>"),
        "Error should display the full usage line 'Usage: bivvy completions <SHELL>', got: {}",
        &text[..text.len().min(400)]
    );

    insta::assert_snapshot!("completions_missing_shell_error", text);

    assert_exit_code_is(&mut s, 2);
}
