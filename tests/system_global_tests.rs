//! Comprehensive system tests for global CLI behavior.
//!
//! Tests global flags (--help, --version, --no-color, --debug, --silent,
//! --non-interactive), unknown commands, subcommand help, and general
//! CLI ergonomics that aren't specific to any single subcommand.
#![cfg(unix)]

mod system;

use system::helpers::*;

// =====================================================================
// HELP & VERSION
// =====================================================================

/// --help shows all subcommands.
#[test]
fn global_help() {
    let mut s = spawn_bivvy_global(&["--help"]);

    let text = read_to_eof(&mut s);
    assert!(text.contains("run"), "Help should list run command");
    assert!(text.contains("init"), "Help should list init command");
    assert!(text.contains("add"), "Help should list add command");
    assert!(text.contains("status"), "Help should list status command");
    assert!(text.contains("list"), "Help should list list command");
    assert!(text.contains("lint"), "Help should list lint command");
    assert!(text.contains("last"), "Help should list last command");
    assert!(text.contains("history"), "Help should list history command");
    assert!(text.contains("templates"), "Help should list templates command");
    assert!(text.contains("config"), "Help should list config command");
    assert!(text.contains("cache"), "Help should list cache command");
    assert!(text.contains("feedback"), "Help should list feedback command");
    assert!(text.contains("completions"), "Help should list completions command");
    assert!(text.contains("update"), "Help should list update command");
}

/// --version shows version string.
#[test]
fn global_version() {
    let mut s = spawn_bivvy_global(&["--version"]);

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("bivvy"),
        "Should show 'bivvy' in version, got: {}",
        &text[..text.len().min(200)]
    );
}

/// -V (short version flag).
#[test]
fn global_version_short() {
    let mut s = spawn_bivvy_global(&["-V"]);

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("bivvy"),
        "Should show version, got: {}",
        &text[..text.len().min(200)]
    );
}

/// -h (short help flag).
#[test]
fn global_help_short() {
    let mut s = spawn_bivvy_global(&["-h"]);

    let text = read_to_eof(&mut s);
    assert!(text.contains("run"), "Short help should list commands");
}

// =====================================================================
// GLOBAL FLAGS
// =====================================================================

/// --no-color is accepted on any command.
#[test]
fn global_no_color_flag() {
    let temp = setup_project("app_name: Test\nsteps:\n  a:\n    command: \"rustc --version\"\nworkflows:\n  default:\n    steps: [a]\n");
    let mut s = spawn_bivvy(&["status", "--no-color"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        !text.contains('\x1b'),
        "No-color output should not contain ANSI escape sequences"
    );
    assert!(
        text.contains("Test") || text.contains("status") || text.contains("a"),
        "No-color status should still show project info, got: {}",
        &text[..text.len().min(300)]
    );
}

/// --debug enables debug logging.
#[test]
fn global_debug_flag() {
    let temp = setup_project("app_name: Test\nsteps:\n  a:\n    command: \"rustc --version\"\nworkflows:\n  default:\n    steps: [a]\n");
    let mut s = spawn_bivvy(&["status", "--debug"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("DEBUG") || text.contains("debug"),
        "Debug flag should produce debug-level output, got: {}",
        &text[..text.len().min(500)]
    );
}

/// -v (short verbose) is accepted.
#[test]
fn global_verbose_short() {
    let temp = setup_project("app_name: Test\nsteps:\n  a:\n    command: \"rustc --version\"\nworkflows:\n  default:\n    steps: [a]\n");
    let mut s = spawn_bivvy(&["status", "-v"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Test") || text.contains("Steps"),
        "Verbose should show output, got: {}",
        &text[..text.len().min(300)]
    );
}

/// -q (short quiet) is accepted and produces minimal output.
#[test]
fn global_quiet_short() {
    let temp = setup_project("app_name: VerboseApp\nsteps:\n  a:\n    command: \"rustc --version\"\nworkflows:\n  default:\n    steps: [a]\n");
    // Get verbose baseline first
    let temp2 = setup_project("app_name: VerboseApp\nsteps:\n  a:\n    command: \"rustc --version\"\nworkflows:\n  default:\n    steps: [a]\n");
    let mut s_verbose = spawn_bivvy(&["status", "--verbose"], temp2.path());
    let verbose_text = read_to_eof(&mut s_verbose);

    let mut s = spawn_bivvy(&["status", "-q"], temp.path());
    let quiet_text = read_to_eof(&mut s);

    // Quiet mode should produce less output than verbose
    assert!(
        quiet_text.len() <= verbose_text.len(),
        "Quiet mode ({} bytes) should produce no more output than verbose ({} bytes)",
        quiet_text.len(),
        verbose_text.len()
    );
}

/// --quiet flag suppresses verbose output but still shows progress and status.
#[test]
fn global_silent_flag() {
    let temp = setup_project("app_name: Test\nsteps:\n  a:\n    command: \"rustc --version\"\n    skippable: false\nworkflows:\n  default:\n    steps: [a]\n");

    // Run with --quiet: should complete without error
    let mut s = spawn_bivvy(&["run", "--quiet", "--non-interactive"], temp.path());
    let quiet_text = read_to_eof(&mut s);
    let quiet_clean = strip_ansi(&quiet_text);

    // Run with default (normal) output for comparison
    let mut s = spawn_bivvy(&["run", "--non-interactive"], temp.path());
    let normal_text = read_to_eof(&mut s);
    let normal_clean = strip_ansi(&normal_text);

    // Quiet mode should produce less output than normal mode
    assert!(
        quiet_clean.len() <= normal_clean.len(),
        "Quiet mode ({} bytes) should produce no more output than normal mode ({} bytes)",
        quiet_clean.len(),
        normal_clean.len()
    );
}

/// --non-interactive flag skips all prompts and completes without hanging.
#[test]
fn global_non_interactive_flag() {
    let temp = setup_project("app_name: Test\nsteps:\n  a:\n    command: \"rustc --version\"\nworkflows:\n  default:\n    steps: [a]\n");
    let mut s = spawn_bivvy(&["run", "--non-interactive"], temp.path());

    let text = read_to_eof(&mut s);
    // Non-interactive should actually run and produce step output
    assert!(
        text.contains("a") || text.contains("rustc") || text.contains("completed")
            || text.contains("passed") || text.contains("Test"),
        "Non-interactive run should produce step output, got: {}",
        &text[..text.len().min(300)]
    );
}

/// --config flag points to an alternate config file.
#[test]
fn global_config_flag() {
    let temp = tempfile::TempDir::new().unwrap();
    let alt_config = temp.path().join("alt-config.yml");
    std::fs::write(
        &alt_config,
        "app_name: AltConfig\nsteps:\n  a:\n    command: \"rustc --version\"\nworkflows:\n  default:\n    steps: [a]\n",
    )
    .unwrap();

    let mut s = spawn_bivvy(
        &["config", "--config", alt_config.to_str().unwrap()],
        temp.path(),
    );

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("AltConfig"),
        "Should load alt config, got: {}",
        &text[..text.len().min(300)]
    );
}

/// --project flag sets the project root.
#[test]
fn global_project_flag() {
    let temp = setup_project("app_name: ProjFlag\nsteps:\n  a:\n    command: \"rustc --version\"\nworkflows:\n  default:\n    steps: [a]\n");

    let other = tempfile::TempDir::new().unwrap();
    let mut s = spawn_bivvy(
        &["status", "--project", temp.path().to_str().unwrap()],
        other.path(),
    );

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("ProjFlag"),
        "Should load project from --project path, got: {}",
        &text[..text.len().min(300)]
    );
}

/// --trust flag is accepted and produces normal output.
#[test]
fn global_trust_flag() {
    let temp = setup_project("app_name: Trust\nsteps:\n  a:\n    command: \"rustc --version\"\nworkflows:\n  default:\n    steps: [a]\n");
    let mut s = spawn_bivvy(&["status", "--trust"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Trust") || text.contains("a"),
        "Trust flag should be accepted and show status output, got: {}",
        &text[..text.len().min(300)]
    );
}

// =====================================================================
// EXIT CODES
// =====================================================================

/// --help exits with code 0.
#[test]
fn global_help_exit_code_zero() {
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["--help"])
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert!(output.status.success(), "Help should exit with code 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Usage"), "Help output should contain Usage");
}

/// --version exits with code 0.
#[test]
fn global_version_exit_code_zero() {
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["--version"])
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert!(output.status.success(), "Version should exit with code 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("bivvy"), "Version output should contain bivvy");
}

/// Unknown command exits with non-zero code.
#[test]
fn global_unknown_command_exit_code_nonzero() {
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["frobnicate"])
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert!(!output.status.success(), "Unknown command should exit non-zero");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("error") || stderr.contains("unrecognized") || stderr.contains("invalid"),
        "Unknown command should produce error on stderr, got: {}",
        &stderr[..stderr.len().min(300)]
    );
}

/// `bivvy status` with valid config exits with code 0.
#[test]
fn global_status_exit_code_zero() {
    let temp = setup_project("app_name: Test\nsteps:\n  a:\n    command: \"rustc --version\"\nworkflows:\n  default:\n    steps: [a]\n");
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["status"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert!(output.status.success(), "Status with valid config should exit 0");
}

/// `bivvy lint` with invalid config exits with non-zero code.
#[test]
fn global_lint_invalid_exit_code_nonzero() {
    let temp = setup_project("app_name: \"Invalid\"\nsteps:\n  a:\n    command: \"rustc --version\"\n    depends_on: [b]\n  b:\n    command: \"cargo --version\"\n    depends_on: [a]\nworkflows:\n  default:\n    steps: [a, b]\n");
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    // Circular dependency should cause lint to fail
    assert!(
        !output.status.success(),
        "Lint with circular deps should exit non-zero"
    );
}

// =====================================================================
// SAD PATH
// =====================================================================

/// Unknown command shows error.
#[test]
fn global_unknown_command() {
    let mut s = spawn_bivvy_global(&["frobnicate"]);

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("error") || text.contains("unrecognized") || text.contains("invalid")
            || text.contains("frobnicate"),
        "Unknown command should show error, got: {}",
        &text[..text.len().min(300)]
    );
}

/// Conflicting flags: --verbose and --quiet together.
#[test]
fn global_conflicting_verbose_quiet() {
    let temp = setup_project("app_name: Test\nsteps:\n  a:\n    command: \"rustc --version\"\nworkflows:\n  default:\n    steps: [a]\n");
    let mut s = spawn_bivvy(&["status", "--verbose", "--quiet"], temp.path());

    let text = read_to_eof(&mut s);
    // Should either pick one, error, or handle gracefully -- verify it produces output
    assert!(
        text.contains("error") || text.contains("conflict") || text.contains("Test")
            || text.contains("status"),
        "Conflicting flags should either error or pick one and produce output, got: {}",
        &text[..text.len().min(300)]
    );
}

/// --config pointing to nonexistent file.
#[test]
fn global_config_nonexistent_file() {
    let temp = tempfile::TempDir::new().unwrap();
    let mut s = spawn_bivvy(
        &["config", "--config", "/nonexistent/config.yml"],
        temp.path(),
    );

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("error") || text.contains("not found") || text.contains("No")
            || text.contains("configuration"),
        "Nonexistent config should show error, got: {}",
        &text[..text.len().min(300)]
    );
}

/// --project pointing to nonexistent directory.
#[test]
fn global_project_nonexistent_dir() {
    let temp = tempfile::TempDir::new().unwrap();
    let mut s = spawn_bivvy(
        &["status", "--project", "/nonexistent/dir"],
        temp.path(),
    );

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("error") || text.contains("not found") || text.contains("No")
            || text.contains("does not exist") || text.contains("configuration"),
        "Nonexistent project should show error, got: {}",
        &text[..text.len().min(300)]
    );
}

// =====================================================================
// SUBCOMMAND HELP
// =====================================================================

/// Each subcommand accepts --help and shows relevant content.
#[test]
fn subcommand_help_run() {
    let mut s = spawn_bivvy_global(&["run", "--help"]);
    let text = read_to_eof(&mut s);
    assert!(text.contains("run") || text.contains("Run"), "Run help should mention run");
    assert!(text.contains("Usage"), "Run help should show Usage section");
}

#[test]
fn subcommand_help_init() {
    let mut s = spawn_bivvy_global(&["init", "--help"]);
    let text = read_to_eof(&mut s);
    assert!(text.contains("init") || text.contains("Init"), "Init help should mention init");
    assert!(text.contains("Usage"), "Init help should show Usage section");
}

#[test]
fn subcommand_help_add() {
    let mut s = spawn_bivvy_global(&["add", "--help"]);
    let text = read_to_eof(&mut s);
    assert!(text.contains("add") || text.contains("Add"), "Add help should mention add");
    assert!(text.contains("Usage"), "Add help should show Usage section");
}

#[test]
fn subcommand_help_status() {
    let mut s = spawn_bivvy_global(&["status", "--help"]);
    let text = read_to_eof(&mut s);
    assert!(text.contains("status") || text.contains("Status"), "Status help should mention status");
    assert!(text.contains("Usage"), "Status help should show Usage section");
}

#[test]
fn subcommand_help_list() {
    let mut s = spawn_bivvy_global(&["list", "--help"]);
    let text = read_to_eof(&mut s);
    assert!(text.contains("list") || text.contains("List"), "List help should mention list");
    assert!(text.contains("Usage"), "List help should show Usage section");
}

#[test]
fn subcommand_help_lint() {
    let mut s = spawn_bivvy_global(&["lint", "--help"]);
    let text = read_to_eof(&mut s);
    assert!(text.contains("lint") || text.contains("Lint"), "Lint help should mention lint");
    assert!(text.contains("Usage"), "Lint help should show Usage section");
}

#[test]
fn subcommand_help_last() {
    let mut s = spawn_bivvy_global(&["last", "--help"]);
    let text = read_to_eof(&mut s);
    assert!(text.contains("last") || text.contains("Last"), "Last help should mention last");
    assert!(text.contains("Usage"), "Last help should show Usage section");
}

#[test]
fn subcommand_help_history() {
    let mut s = spawn_bivvy_global(&["history", "--help"]);
    let text = read_to_eof(&mut s);
    assert!(text.contains("history") || text.contains("History"), "History help should mention history");
    assert!(text.contains("Usage"), "History help should show Usage section");
}

#[test]
fn subcommand_help_templates() {
    let mut s = spawn_bivvy_global(&["templates", "--help"]);
    let text = read_to_eof(&mut s);
    assert!(text.contains("template") || text.contains("Template"), "Templates help should mention templates");
    assert!(text.contains("Usage"), "Templates help should show Usage section");
}

#[test]
fn subcommand_help_config() {
    let mut s = spawn_bivvy_global(&["config", "--help"]);
    let text = read_to_eof(&mut s);
    assert!(text.contains("config") || text.contains("Config"), "Config help should mention config");
    assert!(text.contains("Usage"), "Config help should show Usage section");
}

#[test]
fn subcommand_help_cache() {
    let mut s = spawn_bivvy_global(&["cache", "--help"]);
    let text = read_to_eof(&mut s);
    assert!(text.contains("cache") || text.contains("Cache"), "Cache help should mention cache");
    assert!(text.contains("Usage"), "Cache help should show Usage section");
}

#[test]
fn subcommand_help_feedback() {
    let mut s = spawn_bivvy_global(&["feedback", "--help"]);
    let text = read_to_eof(&mut s);
    assert!(text.contains("feedback") || text.contains("Feedback"), "Feedback help should mention feedback");
    assert!(text.contains("Usage"), "Feedback help should show Usage section");
}

#[test]
fn subcommand_help_completions() {
    let mut s = spawn_bivvy_global(&["completions", "--help"]);
    let text = read_to_eof(&mut s);
    assert!(text.contains("completions") || text.contains("Completions"), "Completions help should mention completions");
    assert!(text.contains("Usage"), "Completions help should show Usage section");
}

#[test]
fn subcommand_help_update() {
    let mut s = spawn_bivvy_global(&["update", "--help"]);
    let text = read_to_eof(&mut s);
    assert!(text.contains("update") || text.contains("Update"), "Update help should mention update");
    assert!(text.contains("Usage"), "Update help should show Usage section");
}
