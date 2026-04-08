//! Comprehensive system tests for global CLI behavior.
//!
//! Tests global flags (--help, --version, --no-color, --debug, --verbose,
//! --quiet, --trust, --config, --project) and the subcommand-level
//! --non-interactive flag, unknown commands, subcommand help, and general
//! CLI ergonomics that aren't specific to any single subcommand.
#![cfg(unix)]

mod system;

use assert_cmd::Command;
use predicates::prelude::*;
use system::helpers::*;

// =====================================================================
// HELP & VERSION
// =====================================================================

/// --help shows all subcommands and usage info.
#[test]
fn global_help() {
    let mut s = spawn_bivvy_global(&["--help"]);

    let text = read_to_eof(&mut s);
    insta::assert_snapshot!("global_help", text);
    assert_exit_code(&s, 0);
}

/// --version shows version string.
#[test]
fn global_version() {
    let mut s = spawn_bivvy_global(&["--version"]);

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("bivvy "),
        "Should show 'bivvy <version>' in version output, got: {}",
        &text[..text.len().min(200)]
    );
    assert_exit_code(&s, 0);
}

/// -V (short version flag).
#[test]
fn global_version_short() {
    let mut s = spawn_bivvy_global(&["-V"]);

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("bivvy "),
        "Should show 'bivvy <version>' in version output, got: {}",
        &text[..text.len().min(200)]
    );
    assert_exit_code(&s, 0);
}

/// -h (short help flag) shows same content as --help (snapshot).
#[test]
fn global_help_short() {
    let mut s = spawn_bivvy_global(&["-h"]);

    let text = read_to_eof(&mut s);
    insta::assert_snapshot!("global_help_short", text);
    assert_exit_code(&s, 0);
}

// =====================================================================
// GLOBAL FLAGS
// =====================================================================

/// --no-color is accepted on any command and suppresses ANSI escape sequences.
#[test]
fn global_no_color_flag() {
    let temp = setup_project("app_name: NoColorApp\nsteps:\n  check_rust:\n    command: \"rustc --version\"\nworkflows:\n  default:\n    steps: [check_rust]\n");
    // spawn_bivvy uses read_to_eof which strips ANSI, so assert on the raw
    // PTY bytes directly for the no-ANSI check.
    let mut s = spawn_bivvy(&["status", "--no-color"], temp.path());
    let raw = s.expect(expectrl::Eof).unwrap();
    let raw_text = String::from_utf8_lossy(raw.as_bytes()).to_string();
    assert!(
        !raw_text.contains('\x1b'),
        "No-color output should not contain ANSI escape sequences, got: {}",
        &raw_text[..raw_text.len().min(300)]
    );
    assert!(
        raw_text.contains("NoColorApp"),
        "No-color status should show the app_name 'NoColorApp', got: {}",
        &raw_text[..raw_text.len().min(300)]
    );
    assert!(
        raw_text.contains("Status"),
        "Status command should print the 'Status' header label, got: {}",
        &raw_text[..raw_text.len().min(300)]
    );
    assert_exit_code(&s, 0);
}

/// --debug enables debug logging with DEBUG-level messages visible.
#[test]
fn global_debug_flag() {
    let temp = setup_project("app_name: DebugTest\nsteps:\n  check_rust:\n    command: \"rustc --version\"\nworkflows:\n  default:\n    steps: [check_rust]\n");
    let mut s = spawn_bivvy(&["status", "--debug"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("DEBUG"),
        "Debug flag should produce DEBUG-level log output, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("DebugTest"),
        "Debug run should still render the status page with app_name 'DebugTest', got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

/// -v (short verbose) is accepted and produces verbose output.
#[test]
fn global_verbose_short() {
    let temp = setup_project("app_name: VerboseTest\nsteps:\n  check_rust:\n    command: \"rustc --version\"\nworkflows:\n  default:\n    steps: [check_rust]\n");
    let mut s = spawn_bivvy(&["status", "-v"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("VerboseTest"),
        "Verbose output should show app_name 'VerboseTest', got: {}",
        &text[..text.len().min(300)]
    );
    assert!(
        text.contains("Steps:"),
        "Verbose status should still render the 'Steps:' section label, got: {}",
        &text[..text.len().min(300)]
    );
    assert_exit_code(&s, 0);
}

/// -q (short quiet) is accepted and produces less output than verbose.
#[test]
fn global_quiet_short() {
    let temp = setup_project("app_name: QuietApp\nsteps:\n  check_rust:\n    command: \"rustc --version\"\nworkflows:\n  default:\n    steps: [check_rust]\n");
    // Get verbose baseline first
    let temp2 = setup_project("app_name: QuietApp\nsteps:\n  check_rust:\n    command: \"rustc --version\"\nworkflows:\n  default:\n    steps: [check_rust]\n");
    let mut s_verbose = spawn_bivvy(&["status", "--verbose"], temp2.path());
    let verbose_text = read_to_eof(&mut s_verbose);
    assert_exit_code(&s_verbose, 0);

    let mut s = spawn_bivvy(&["status", "-q"], temp.path());
    let quiet_text = read_to_eof(&mut s);
    assert_exit_code(&s, 0);

    // Verbose baseline should contain the full status page
    assert!(
        verbose_text.contains("QuietApp") && verbose_text.contains("Steps:"),
        "Verbose baseline should include app_name and 'Steps:' label, got: {}",
        &verbose_text[..verbose_text.len().min(300)]
    );

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
fn global_quiet_flag_run() {
    let temp = setup_project("app_name: QuietRun\nsteps:\n  check_rust:\n    command: \"rustc --version\"\n    skippable: false\nworkflows:\n  default:\n    steps: [check_rust]\n");

    // Run with --quiet: should complete without error
    let mut s = spawn_bivvy(&["run", "--quiet", "--non-interactive"], temp.path());
    let quiet_text = read_to_eof(&mut s);
    let quiet_clean = strip_ansi(&quiet_text);
    assert_exit_code(&s, 0);

    // Run with default (normal) output for comparison
    let temp2 = setup_project("app_name: QuietRun\nsteps:\n  check_rust:\n    command: \"rustc --version\"\n    skippable: false\nworkflows:\n  default:\n    steps: [check_rust]\n");
    let mut s2 = spawn_bivvy(&["run", "--non-interactive"], temp2.path());
    let normal_text = read_to_eof(&mut s2);
    let normal_clean = strip_ansi(&normal_text);
    assert_exit_code(&s2, 0);

    // Normal mode baseline should render the run header and the
    // "Setup complete!" summary for the successful workflow.
    assert!(
        normal_clean.contains("QuietRun") && normal_clean.contains("Setup complete!"),
        "Normal run should show app_name 'QuietRun' and 'Setup complete!' summary, got: {}",
        &normal_clean[..normal_clean.len().min(500)]
    );

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
    let temp = setup_project("app_name: NonInteractiveTest\nsteps:\n  check_rust:\n    command: \"rustc --version\"\nworkflows:\n  default:\n    steps: [check_rust]\n");
    let mut s = spawn_bivvy(&["run", "--non-interactive"], temp.path());

    let text = read_to_eof(&mut s);
    let clean = strip_ansi(&text);
    // Non-interactive should actually run the workflow and emit the
    // "Setup complete!" summary on success — the exact user-facing
    // message from `src/ui/non_interactive.rs`.
    assert!(
        clean.contains("Setup complete!"),
        "Non-interactive run should show 'Setup complete!' summary, got: {}",
        &clean[..clean.len().min(500)]
    );
    assert!(
        clean.contains("NonInteractiveTest"),
        "Non-interactive run should show app_name 'NonInteractiveTest' in the run header, got: {}",
        &clean[..clean.len().min(500)]
    );
    assert!(
        clean.contains("check_rust"),
        "Non-interactive run should list the 'check_rust' step in the summary, got: {}",
        &clean[..clean.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

/// --config flag points to an alternate config file.
#[test]
fn global_config_flag() {
    let temp = tempfile::TempDir::new().unwrap();
    let alt_config = temp.path().join("alt-config.yml");
    std::fs::write(
        &alt_config,
        "app_name: AltConfig\nsteps:\n  check_rust:\n    command: \"rustc --version\"\nworkflows:\n  default:\n    steps: [check_rust]\n",
    )
    .unwrap();

    let mut s = spawn_bivvy(
        &["config", "--config", alt_config.to_str().unwrap()],
        temp.path(),
    );

    let text = read_to_eof(&mut s);
    // `bivvy config` dumps the resolved config as YAML; the override
    // should produce a document containing `app_name: AltConfig` and the
    // `check_rust` step defined in the alt file.
    assert!(
        text.contains("app_name: AltConfig"),
        "Should load alt config and show 'app_name: AltConfig' in YAML output, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("check_rust"),
        "YAML dump from --config should include the 'check_rust' step, got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

/// --project flag sets the project root.
#[test]
fn global_project_flag() {
    let temp = setup_project("app_name: ProjFlag\nsteps:\n  check_rust:\n    command: \"rustc --version\"\nworkflows:\n  default:\n    steps: [check_rust]\n");

    let other = tempfile::TempDir::new().unwrap();
    let mut s = spawn_bivvy(
        &["status", "--project", temp.path().to_str().unwrap()],
        other.path(),
    );

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("ProjFlag"),
        "Should load project from --project path and show app_name 'ProjFlag', got: {}",
        &text[..text.len().min(300)]
    );
    assert!(
        text.contains("check_rust"),
        "Status should list the 'check_rust' step from the --project config, got: {}",
        &text[..text.len().min(300)]
    );
    assert_exit_code(&s, 0);
}

/// --trust flag is accepted and produces normal output.
#[test]
fn global_trust_flag() {
    let temp = setup_project("app_name: TrustTest\nsteps:\n  check_rust:\n    command: \"rustc --version\"\nworkflows:\n  default:\n    steps: [check_rust]\n");
    let mut s = spawn_bivvy(&["status", "--trust"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("TrustTest"),
        "Trust flag should be accepted and show app_name 'TrustTest', got: {}",
        &text[..text.len().min(300)]
    );
    assert!(
        text.contains("Steps:"),
        "Status under --trust should still render the 'Steps:' section, got: {}",
        &text[..text.len().min(300)]
    );
    assert_exit_code(&s, 0);
}

// =====================================================================
// EXIT CODES
// =====================================================================

/// --help exits with code 0 and prints clap's "Usage" banner plus at
/// least one known subcommand name.
#[test]
fn global_help_exit_code_zero() {
    Command::cargo_bin("bivvy")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .code(0)
        .stdout(predicate::str::contains("Usage"))
        .stdout(predicate::str::contains("init"))
        .stdout(predicate::str::contains("run"));
}

/// --version exits with code 0 and prints "bivvy <version>".
#[test]
fn global_version_exit_code_zero() {
    Command::cargo_bin("bivvy")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .code(0)
        .stdout(predicate::str::contains("bivvy "));
}

/// Unknown command exits with clap's parse-error code (2) and shows the
/// error on stderr.
#[test]
fn global_unknown_command_exit_code_nonzero() {
    Command::cargo_bin("bivvy")
        .unwrap()
        .arg("frobnicate")
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("unrecognized subcommand"))
        .stderr(predicate::str::contains("frobnicate"));
}

/// `bivvy status` with valid config exits with code 0 and renders the
/// status page.
#[test]
fn global_status_exit_code_zero() {
    let temp = setup_project("app_name: StatusOK\nsteps:\n  check_rust:\n    command: \"rustc --version\"\nworkflows:\n  default:\n    steps: [check_rust]\n");
    Command::cargo_bin("bivvy")
        .unwrap()
        .arg("status")
        .current_dir(temp.path())
        .assert()
        .success()
        .code(0)
        .stdout(predicate::str::contains("StatusOK"))
        .stdout(predicate::str::contains("Steps:"));
}

/// `bivvy lint` with circular dependency exits non-zero and mentions the
/// cycle in its diagnostics.
#[test]
fn global_lint_invalid_exit_code_nonzero() {
    let temp = setup_project("app_name: \"Invalid\"\nsteps:\n  check_rust:\n    command: \"rustc --version\"\n    depends_on: [check_cargo]\n  check_cargo:\n    command: \"cargo --version\"\n    depends_on: [check_rust]\nworkflows:\n  default:\n    steps: [check_rust, check_cargo]\n");
    Command::cargo_bin("bivvy")
        .unwrap()
        .arg("lint")
        .current_dir(temp.path())
        .assert()
        .failure()
        .stdout(
            predicate::str::contains("circular")
                .or(predicate::str::contains("cycle"))
                .or(predicate::str::contains("check_rust"))
                .or(predicate::str::contains("check_cargo")),
        );
}

// =====================================================================
// SAD PATH
// =====================================================================

/// Unknown command shows error with the unrecognized subcommand name.
#[test]
fn global_unknown_command() {
    let mut s = spawn_bivvy_global(&["frobnicate"]);

    let text = read_to_eof(&mut s);
    // clap emits: "error: unrecognized subcommand 'frobnicate'"
    assert!(
        text.contains("unrecognized subcommand"),
        "Unknown command should show clap's 'unrecognized subcommand' error, got: {}",
        &text[..text.len().min(300)]
    );
    assert!(
        text.contains("frobnicate"),
        "Unknown command error should name the offending subcommand 'frobnicate', got: {}",
        &text[..text.len().min(300)]
    );
    assert_exit_code(&s, 2);
}

/// --verbose and --quiet together: both are accepted by clap; `src/main.rs`
/// gives precedence to `--quiet`, and the command still runs successfully.
#[test]
fn global_conflicting_verbose_quiet() {
    let temp = setup_project("app_name: ConflictTest\nsteps:\n  check_rust:\n    command: \"rustc --version\"\nworkflows:\n  default:\n    steps: [check_rust]\n");
    let mut s = spawn_bivvy(&["status", "--verbose", "--quiet"], temp.path());

    let text = read_to_eof(&mut s);
    // The command should still run and render the status page — the app
    // name from the config must appear regardless of which output mode wins.
    assert!(
        text.contains("ConflictTest"),
        "Passing --verbose --quiet together should still run status and show app_name 'ConflictTest', got: {}",
        &text[..text.len().min(500)]
    );
    // No clap parse error should be emitted, since the flags are not
    // declared as conflicting in `src/cli/args.rs`.
    assert!(
        !text.contains("unrecognized") && !text.contains("cannot be used with"),
        "--verbose --quiet should not produce a clap conflict error, got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

/// --config pointing to nonexistent file should produce an error.
#[test]
fn global_config_nonexistent_file() {
    let temp = tempfile::TempDir::new().unwrap();
    Command::cargo_bin("bivvy")
        .unwrap()
        .args(["config", "--config", "/nonexistent/config.yml"])
        .current_dir(temp.path())
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("/nonexistent/config.yml")
                .or(predicate::str::contains("not found"))
                .or(predicate::str::contains("No such file")),
        );
}

/// --project pointing to nonexistent directory should produce an error.
#[test]
fn global_project_nonexistent_dir() {
    let temp = tempfile::TempDir::new().unwrap();
    Command::cargo_bin("bivvy")
        .unwrap()
        .args(["status", "--project", "/nonexistent/dir"])
        .current_dir(temp.path())
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("/nonexistent/dir")
                .or(predicate::str::contains("not found"))
                .or(predicate::str::contains("No such file"))
                .or(predicate::str::contains("No configuration found")),
        );
}

// =====================================================================
// SUBCOMMAND HELP (snapshot-tested)
// =====================================================================

/// Run `bivvy <subcommand> --help`, assert exit code 0, and return stdout.
fn run_help(subcommand: &str) -> String {
    let output = Command::cargo_bin("bivvy")
        .unwrap()
        .args([subcommand, "--help"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "`bivvy {subcommand} --help` should exit 0, got {:?}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Each subcommand accepts --help and shows relevant content.
#[test]
fn subcommand_help_run() {
    let text = run_help("run");
    assert!(text.contains("Usage"), "run --help should include 'Usage'");
    insta::assert_snapshot!("subcommand_help_run", text);
}

#[test]
fn subcommand_help_init() {
    let text = run_help("init");
    assert!(text.contains("Usage"), "init --help should include 'Usage'");
    insta::assert_snapshot!("subcommand_help_init", text);
}

#[test]
fn subcommand_help_add() {
    let text = run_help("add");
    assert!(text.contains("Usage"), "add --help should include 'Usage'");
    insta::assert_snapshot!("subcommand_help_add", text);
}

#[test]
fn subcommand_help_status() {
    let text = run_help("status");
    assert!(text.contains("Usage"), "status --help should include 'Usage'");
    insta::assert_snapshot!("subcommand_help_status", text);
}

#[test]
fn subcommand_help_list() {
    let text = run_help("list");
    assert!(text.contains("Usage"), "list --help should include 'Usage'");
    insta::assert_snapshot!("subcommand_help_list", text);
}

#[test]
fn subcommand_help_lint() {
    let text = run_help("lint");
    assert!(text.contains("Usage"), "lint --help should include 'Usage'");
    insta::assert_snapshot!("subcommand_help_lint", text);
}

#[test]
fn subcommand_help_last() {
    let text = run_help("last");
    assert!(text.contains("Usage"), "last --help should include 'Usage'");
    insta::assert_snapshot!("subcommand_help_last", text);
}

#[test]
fn subcommand_help_history() {
    let text = run_help("history");
    assert!(text.contains("Usage"), "history --help should include 'Usage'");
    insta::assert_snapshot!("subcommand_help_history", text);
}

#[test]
fn subcommand_help_templates() {
    let text = run_help("templates");
    assert!(text.contains("Usage"), "templates --help should include 'Usage'");
    insta::assert_snapshot!("subcommand_help_templates", text);
}

#[test]
fn subcommand_help_config() {
    let text = run_help("config");
    assert!(text.contains("Usage"), "config --help should include 'Usage'");
    insta::assert_snapshot!("subcommand_help_config", text);
}

#[test]
fn subcommand_help_cache() {
    let text = run_help("cache");
    assert!(text.contains("Usage"), "cache --help should include 'Usage'");
    insta::assert_snapshot!("subcommand_help_cache", text);
}

#[test]
fn subcommand_help_feedback() {
    let text = run_help("feedback");
    assert!(text.contains("Usage"), "feedback --help should include 'Usage'");
    insta::assert_snapshot!("subcommand_help_feedback", text);
}

#[test]
fn subcommand_help_completions() {
    let text = run_help("completions");
    assert!(text.contains("Usage"), "completions --help should include 'Usage'");
    insta::assert_snapshot!("subcommand_help_completions", text);
}

#[test]
fn subcommand_help_update() {
    let text = run_help("update");
    assert!(text.contains("Usage"), "update --help should include 'Usage'");
    insta::assert_snapshot!("subcommand_help_update", text);
}
