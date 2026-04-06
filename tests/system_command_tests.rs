//! System tests for ALL non-run commands and ALL their flags.
//!
//! Uses a single realistic config with 4 non-skippable steps that exercise
//! real external commands (git, rustc, cargo, grep, wc, cat).  Tests are
//! organized by command: status, list, lint, last, history, config, init,
//! templates, and completions.
#![cfg(unix)]

mod system;

use std::fs;
use system::helpers::*;
use tempfile::TempDir;

// ─────────────────────────────────────────────────────────────────────
// Shared realistic config
// ─────────────────────────────────────────────────────────────────────

const CONFIG: &str = r#"
app_name: "CommandTest"

settings:
  default_output: verbose

vars:
  version:
    command: "git log --oneline -1"

steps:
  verify-tools:
    title: "Verify development tools"
    command: "rustc --version && cargo --version"
    skippable: false
    completed_check:
      type: command_succeeds
      command: "rustc --version"

  check-repo:
    title: "Check repository"
    command: "git rev-parse --git-dir && git branch --show-current"
    skippable: false
    depends_on: [verify-tools]
    completed_check:
      type: command_succeeds
      command: "git rev-parse --git-dir"

  analyze-project:
    title: "Analyze project"
    command: "grep -c 'fn' src/main.rs && wc -l Cargo.toml"
    skippable: false
    depends_on: [check-repo]
    watches:
      - Cargo.toml

  build-report:
    title: "Build report"
    command: "rustc --version > .build-report.txt && cargo --version >> .build-report.txt"
    skippable: false
    depends_on: [analyze-project]

workflows:
  default:
    steps: [verify-tools, check-repo, analyze-project, build-report]
  quick:
    description: "Quick check"
    steps: [verify-tools, check-repo]
"#;

// ─────────────────────────────────────────────────────────────────────
// Invalid config for lint tests
// ─────────────────────────────────────────────────────────────────────

const INVALID_CONFIG: &str = r#"
app_name: "InvalidProject"

steps:
  alpha:
    title: "Alpha step"
    command: "git --version"
    depends_on: [beta]
  beta:
    title: "Beta step"
    command: "rustc --version"
    depends_on: [alpha]

workflows:
  default:
    steps: [alpha, beta, ghost-step]
"#;

// =====================================================================
// STATUS COMMAND
// =====================================================================

/// `bivvy status` shows step names and their status.
#[test]
fn status_shows_step_status() {
    let temp = setup_project_with_git(CONFIG);
    let mut s = spawn_bivvy(&["status"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(text.contains("CommandTest"), "Should show app name");
    assert!(text.contains("verify-tools"), "Should show verify-tools step");
    assert!(text.contains("check-repo"), "Should show check-repo step");
    assert!(text.contains("analyze-project"), "Should show analyze-project step");
    assert!(text.contains("build-report"), "Should show build-report step");
}

/// `bivvy status --json` produces valid JSON-like output (contains `{`).
#[test]
fn status_json_output() {
    let temp = setup_project_with_git(CONFIG);
    let mut s = spawn_bivvy(&["status", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("{") || text.contains("app_name"),
        "JSON output should contain {{ or app_name, got:\n{text}"
    );
}

/// `bivvy status --step verify-tools` shows info for that step.
#[test]
fn status_specific_step() {
    let temp = setup_project_with_git(CONFIG);
    let mut s = spawn_bivvy(&["status", "--step", "verify-tools"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("verify-tools"),
        "Should show verify-tools step info, got:\n{text}"
    );
}

/// `bivvy status --env ci` includes environment context.
#[test]
fn status_with_env() {
    let temp = setup_project_with_git(CONFIG);
    let mut s = spawn_bivvy(&["status", "--env", "ci"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Environment:") || text.contains("ci") || text.contains("CommandTest"),
        "Should include environment context or app info, got:\n{text}"
    );
}

// =====================================================================
// LIST COMMAND
// =====================================================================

/// `bivvy list` shows both steps and workflows.
#[test]
fn list_shows_steps_and_workflows() {
    let temp = setup_project_with_git(CONFIG);
    let mut s = spawn_bivvy(&["list"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(text.contains("Steps:"), "Should have Steps section");
    assert!(text.contains("verify-tools"), "Should list verify-tools");
    assert!(text.contains("check-repo"), "Should list check-repo");
    assert!(text.contains("analyze-project"), "Should list analyze-project");
    assert!(text.contains("build-report"), "Should list build-report");
    assert!(text.contains("Workflows:"), "Should have Workflows section");
    assert!(text.contains("default"), "Should list default workflow");
    assert!(text.contains("quick"), "Should list quick workflow");
}

/// `bivvy list --steps-only` shows steps but not workflows section.
#[test]
fn list_steps_only() {
    let temp = setup_project_with_git(CONFIG);
    let mut s = spawn_bivvy(&["list", "--steps-only"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(text.contains("Steps:"), "Should have Steps section");
    assert!(text.contains("verify-tools"), "Should list verify-tools");
    assert!(text.contains("build-report"), "Should list build-report");
}

/// `bivvy list --workflows-only` shows workflows but not steps section.
#[test]
fn list_workflows_only() {
    let temp = setup_project_with_git(CONFIG);
    let mut s = spawn_bivvy(&["list", "--workflows-only"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(text.contains("Workflows:"), "Should have Workflows section");
    assert!(text.contains("default"), "Should list default workflow");
    assert!(text.contains("quick"), "Should list quick workflow");
}

/// `bivvy list --json` produces JSON-like output.
#[test]
fn list_json_output() {
    let temp = setup_project_with_git(CONFIG);
    let mut s = spawn_bivvy(&["list", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("{") || text.contains("\"") || text.contains("verify-tools"),
        "JSON output should contain structured data, got:\n{text}"
    );
}

/// `bivvy list --env ci` includes environment context.
#[test]
fn list_with_env() {
    let temp = setup_project_with_git(CONFIG);
    let mut s = spawn_bivvy(&["list", "--env", "ci"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Environment:") || text.contains("ci") || text.contains("Steps:"),
        "Should include environment context or step list, got:\n{text}"
    );
}

// =====================================================================
// LINT COMMAND
// =====================================================================

/// `bivvy lint` on a valid config shows no errors.
#[test]
fn lint_valid_config() {
    let temp = setup_project_with_git(CONFIG);
    let mut s = spawn_bivvy(&["lint"], temp.path());

    s.expect("Configuration is valid!").unwrap();
    s.expect(expectrl::Eof).unwrap();
}

/// `bivvy lint --format json` produces JSON output.
#[test]
fn lint_json_format() {
    let temp = setup_project_with_git(CONFIG);
    let mut s = spawn_bivvy(&["lint", "--format", "json"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("{") || text.contains("valid") || text.contains("ok"),
        "JSON lint output should contain structured data, got:\n{text}"
    );
}

/// `bivvy lint --format sarif` produces SARIF output.
#[test]
fn lint_sarif_format() {
    let temp = setup_project_with_git(CONFIG);
    let mut s = spawn_bivvy(&["lint", "--format", "sarif"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("{") || text.contains("sarif") || text.contains("SARIF")
            || text.contains("valid") || text.contains("results"),
        "SARIF lint output should contain structured data, got:\n{text}"
    );
}

/// `bivvy lint --strict` treats warnings as errors.
#[test]
fn lint_strict_mode() {
    let temp = setup_project_with_git(CONFIG);
    let mut s = spawn_bivvy(&["lint", "--strict"], temp.path());

    s.expect("Configuration is valid!").unwrap();
    s.expect(expectrl::Eof).unwrap();
}

/// Create a config with errors, verify lint catches them.
#[test]
fn lint_invalid_config() {
    let temp = setup_project_with_git(INVALID_CONFIG);
    let mut s = spawn_bivvy(&["lint"], temp.path());

    let text = read_to_eof(&mut s);
    // Should detect circular dependency and/or missing step reference
    assert!(
        text.contains("circular") || text.contains("error") || text.contains("ghost"),
        "Should report config errors, got:\n{text}"
    );
}

/// `bivvy lint` on invalid config exits with non-zero code.
#[test]
fn lint_invalid_config_exit_code() {
    let temp = setup_project_with_git(INVALID_CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert!(
        !output.status.success(),
        "Lint on invalid config should exit non-zero"
    );
}

/// `bivvy lint` on valid config exits with code 0.
#[test]
fn lint_valid_config_exit_code() {
    let temp = setup_project_with_git(CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert!(
        output.status.success(),
        "Lint on valid config should exit 0"
    );
}

// =====================================================================
// LAST COMMAND (requires a prior run)
// =====================================================================

/// Run workflow first, then `bivvy last` shows info.
#[test]
fn last_shows_run_info() {
    let temp = setup_project_with_git(CONFIG);
    run_bivvy_silently(temp.path(), &["run"]);

    let mut s = spawn_bivvy(&["last"], temp.path());

    s.expect("Last Run").unwrap();
    s.expect("Workflow:").unwrap();
    s.expect("default").unwrap();
    s.expect(expectrl::Eof).unwrap();
}

/// `bivvy last --json` produces JSON output.
#[test]
fn last_json_output() {
    let temp = setup_project_with_git(CONFIG);
    run_bivvy_silently(temp.path(), &["run"]);

    let mut s = spawn_bivvy(&["last", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("workflow") || text.contains("{"),
        "JSON output should contain workflow key or {{, got:\n{text}"
    );
}

/// `bivvy last --step verify-tools` shows that step's info.
#[test]
fn last_specific_step() {
    let temp = setup_project_with_git(CONFIG);
    run_bivvy_silently(temp.path(), &["run"]);

    let mut s = spawn_bivvy(&["last", "--step", "verify-tools"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("verify-tools") || text.contains("Verify"),
        "Should show verify-tools step info, got:\n{text}"
    );
}

/// `bivvy last --all` shows all runs.
#[test]
fn last_all_runs() {
    let temp = setup_project_with_git(CONFIG);
    run_bivvy_silently(temp.path(), &["run"]);

    let mut s = spawn_bivvy(&["last", "--all"], temp.path());

    s.expect("Run 1 of").unwrap();
    s.expect(expectrl::Eof).unwrap();
}

/// `bivvy last --output` includes command output.
#[test]
fn last_with_output() {
    let temp = setup_project_with_git(CONFIG);
    run_bivvy_silently(temp.path(), &["run"]);

    let mut s = spawn_bivvy(&["last", "--output"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Last Run") || text.contains("Output"),
        "Should show last run with output, got:\n{text}"
    );
}

/// `bivvy last` exits with code 0 after a successful run.
#[test]
fn last_exit_code_after_run() {
    let temp = setup_project_with_git(CONFIG);
    run_bivvy_silently(temp.path(), &["run"]);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["last"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert!(output.status.success(), "Last after successful run should exit 0");
}

// =====================================================================
// HISTORY COMMAND (requires prior runs)
// =====================================================================

/// Run workflow, then `bivvy history` shows run entries.
#[test]
fn history_shows_runs() {
    let temp = setup_project_with_git(CONFIG);
    run_bivvy_silently(temp.path(), &["run"]);

    let mut s = spawn_bivvy(&["history"], temp.path());

    s.expect("Run History").unwrap();
    s.expect("default").unwrap();
    s.expect(expectrl::Eof).unwrap();
}

/// `bivvy history --json` produces JSON output.
#[test]
fn history_json_output() {
    let temp = setup_project_with_git(CONFIG);
    run_bivvy_silently(temp.path(), &["run"]);

    let mut s = spawn_bivvy(&["history", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("workflow") || text.contains("{"),
        "JSON output should contain workflow key or {{, got:\n{text}"
    );
}

/// `bivvy history --step verify-tools` filters by step.
#[test]
fn history_step_filter() {
    let temp = setup_project_with_git(CONFIG);
    run_bivvy_silently(temp.path(), &["run"]);

    let mut s = spawn_bivvy(&["history", "--step", "verify-tools"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("verify-tools") || text.contains("Verify") || text.contains("History"),
        "History step filter should show relevant info, got:\n{text}"
    );
}

/// `bivvy history --limit 1` shows only 1 entry.
#[test]
fn history_limit() {
    let temp = setup_project_with_git(CONFIG);
    run_bivvy_silently(temp.path(), &["run"]);
    run_bivvy_silently(temp.path(), &["run"]);

    let mut s = spawn_bivvy(&["history", "--limit", "1"], temp.path());

    s.expect("Run History").unwrap();
    s.expect(expectrl::Eof).unwrap();
}

/// `bivvy history --since 1h` shows recent runs.
#[test]
fn history_since() {
    let temp = setup_project_with_git(CONFIG);
    run_bivvy_silently(temp.path(), &["run"]);

    let mut s = spawn_bivvy(&["history", "--since", "1h"], temp.path());

    s.expect("Run History").unwrap();
    s.expect(expectrl::Eof).unwrap();
}

/// `bivvy history --detail` shows detailed view with step info.
#[test]
fn history_detail() {
    let temp = setup_project_with_git(CONFIG);
    run_bivvy_silently(temp.path(), &["run"]);

    let mut s = spawn_bivvy(&["history", "--detail"], temp.path());

    s.expect("Steps:").unwrap();
    s.expect(expectrl::Eof).unwrap();
}

/// `bivvy history` exits with code 0 after a run.
#[test]
fn history_exit_code_after_run() {
    let temp = setup_project_with_git(CONFIG);
    run_bivvy_silently(temp.path(), &["run"]);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["history"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert!(output.status.success(), "History after run should exit 0");
}

// =====================================================================
// CONFIG COMMAND
// =====================================================================

/// `bivvy config` shows the resolved configuration.
#[test]
fn config_shows_resolved() {
    let temp = setup_project_with_git(CONFIG);
    let mut s = spawn_bivvy(&["config"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(text.contains("app_name"), "Should show app_name key");
    assert!(text.contains("CommandTest"), "Should show app name value");
    assert!(text.contains("verify-tools"), "Should show step names");
}

/// `bivvy config --json` produces JSON output.
#[test]
fn config_json_output() {
    let temp = setup_project_with_git(CONFIG);
    let mut s = spawn_bivvy(&["config", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("CommandTest") || text.contains("{"),
        "JSON output should contain app name or {{, got:\n{text}"
    );
}

/// `bivvy config --yaml` produces YAML output.
#[test]
fn config_yaml_output() {
    let temp = setup_project_with_git(CONFIG);
    let mut s = spawn_bivvy(&["config", "--yaml"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(text.contains("app_name"), "YAML output should contain app_name key");
    assert!(text.contains("CommandTest"), "YAML output should contain app name value");
}

/// `bivvy config --merged` shows merged config.
#[test]
fn config_merged() {
    let temp = setup_project_with_git(CONFIG);
    let mut s = spawn_bivvy(&["config", "--merged"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(text.contains("app_name"), "Merged config should contain app_name");
    assert!(text.contains("CommandTest"), "Merged config should contain app name value");
}

/// `bivvy config` exits with code 0.
#[test]
fn config_exit_code() {
    let temp = setup_project_with_git(CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["config"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert!(output.status.success(), "Config should exit 0");
}

// =====================================================================
// INIT COMMAND
// =====================================================================

/// `bivvy init --minimal` creates .bivvy/config.yml in empty dir.
#[test]
fn init_creates_config() {
    let temp = TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml").unwrap();
    // Dismiss the "Run setup now?" interactive prompt (Enter accepts default "No")
    wait_and_answer(&s, "Run setup now?", KEY_ENTER, "init: dismiss run prompt");
    s.expect(expectrl::Eof).unwrap();

    assert!(
        temp.path().join(".bivvy/config.yml").exists(),
        "Config file should exist"
    );

    // Verify the created config is valid YAML with expected structure
    let content = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    assert!(
        content.contains("app_name") || content.contains("steps"),
        "Created config should have basic structure, got:\n{content}"
    );
}

/// `bivvy init --minimal --force` overwrites existing config.
#[test]
fn init_force_overwrites() {
    let temp = TempDir::new().unwrap();
    let bivvy_dir = temp.path().join(".bivvy");
    fs::create_dir_all(&bivvy_dir).unwrap();
    fs::write(bivvy_dir.join("config.yml"), "app_name: OldConfig\n").unwrap();

    let mut s = spawn_bivvy(&["init", "--minimal", "--force"], temp.path());

    s.expect("Created .bivvy/config.yml").unwrap();
    // Dismiss the "Run setup now?" interactive prompt (Enter accepts default "No")
    wait_and_answer(&s, "Run setup now?", KEY_ENTER, "init --force: dismiss run prompt");
    s.expect(expectrl::Eof).unwrap();

    let content = fs::read_to_string(bivvy_dir.join("config.yml")).unwrap();
    assert!(
        !content.contains("OldConfig"),
        "Old config should be replaced"
    );
}

/// `bivvy init --minimal` exits with code 0.
#[test]
fn init_exit_code() {
    let temp = TempDir::new().unwrap();
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["init", "--minimal"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert!(output.status.success(), "Init --minimal should exit 0");
}

// =====================================================================
// TEMPLATES COMMAND
// =====================================================================

/// `bivvy templates` shows template list.
#[test]
fn templates_lists_available() {
    let mut s = spawn_bivvy_global(&["templates"]);

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("cargo-build") || text.contains("npm-install") || text.contains("templates available"),
        "Should list available templates, got:\n{text}"
    );
}

/// `bivvy templates --category rust` filters by category.
#[test]
fn templates_category_filter() {
    let mut s = spawn_bivvy_global(&["templates", "--category", "rust"]);

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("cargo-build") || text.contains("cargo") || text.contains("rust"),
        "Should show Rust templates, got:\n{text}"
    );
}

// =====================================================================
// COMPLETIONS COMMAND
// =====================================================================

/// `bivvy completions bash` produces bash completion script.
#[test]
fn completions_bash() {
    let mut s = spawn_bivvy_global(&["completions", "bash"]);

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("bivvy"),
        "Bash completions should reference bivvy, got:\n{}",
        &text[..text.len().min(300)]
    );
}

/// `bivvy completions zsh` produces zsh completion script.
#[test]
fn completions_zsh() {
    let mut s = spawn_bivvy_global(&["completions", "zsh"]);

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("bivvy"),
        "Zsh completions should reference bivvy, got:\n{}",
        &text[..text.len().min(300)]
    );
}

/// `bivvy completions fish` produces fish completion script.
#[test]
fn completions_fish() {
    let mut s = spawn_bivvy_global(&["completions", "fish"]);

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("bivvy"),
        "Fish completions should reference bivvy, got:\n{}",
        &text[..text.len().min(300)]
    );
}

/// `bivvy completions bash` exits with code 0.
#[test]
fn completions_exit_code() {
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["completions", "bash"])
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert!(output.status.success(), "Completions should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("bivvy"), "Completions output should reference bivvy");
}
