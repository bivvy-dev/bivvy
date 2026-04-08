//! System tests for `bivvy lint` — all interactive, PTY-based.
#![cfg(unix)]

mod system;

use system::helpers::*;

/// Valid config used as the baseline for passing tests.
///
/// Uses real development-environment commands (`cargo`, `rustc`) so the
/// config exercises realistic step definitions.
const VALID_CONFIG: &str = r#"
app_name: "LintTest"
steps:
  deps:
    title: "Install dependencies"
    command: "cargo --version"
  build:
    title: "Build project"
    command: "rustc --version"
    depends_on: [deps]
workflows:
  default:
    steps: [deps, build]
"#;

/// Config containing a two-step circular dependency (a -> b -> a).
///
/// Triggers the `circular-dependency` lint rule.
const CIRCULAR_CONFIG: &str = r#"
app_name: "BadApp"
steps:
  a:
    command: "git --version"
    depends_on: [b]
  b:
    command: "cargo --version"
    depends_on: [a]
workflows:
  default:
    steps: [a, b]
"#;

/// Config with spaces in app_name — triggers the `app-name-format` warning.
const WARNING_CONFIG: &str = r#"
app_name: "My App With Spaces"
steps:
  deps:
    title: "Install dependencies"
    command: "cargo --version"
workflows:
  default:
    steps: [deps]
"#;

// ── Happy paths ───────────────────────────────────────────────────────

#[test]
fn lint_valid_config() {
    let temp = setup_project(VALID_CONFIG);
    let mut s = spawn_bivvy(&["lint"], temp.path());

    s.expect("Configuration is valid!").unwrap();
    s.expect(expectrl::Eof).unwrap();

    assert_exit_code(&s, 0);
}

#[test]
fn lint_strict_flag_valid_config() {
    let temp = setup_project(VALID_CONFIG);
    let mut s = spawn_bivvy(&["lint", "--strict"], temp.path());

    s.expect("Configuration is valid!").unwrap();
    s.expect(expectrl::Eof).unwrap();

    assert_exit_code(&s, 0);
}

#[test]
fn lint_fix_flag_valid_config() {
    // --fix is documented; on a valid config it should be a no-op and still
    // report the configuration as valid with exit code 0.
    let temp = setup_project(VALID_CONFIG);
    let mut s = spawn_bivvy(&["lint", "--fix"], temp.path());

    s.expect("Configuration is valid!").unwrap();
    s.expect(expectrl::Eof).unwrap();

    assert_exit_code(&s, 0);
}

// ── Warning handling ──────────────────────────────────────────────────

#[test]
fn lint_warnings_without_strict_passes() {
    let temp = setup_project(WARNING_CONFIG);
    let mut s = spawn_bivvy(&["lint"], temp.path());

    // Full user-facing message from src/lint/rules/app_name.rs.
    s.expect("app_name contains spaces; consider using kebab-case")
        .unwrap();
    // Human formatter emits "warning[<rule-id>]:" prefix.
    // (Already consumed above — verify the summary line next.)
    // Summary line comes from src/lint/output/human.rs.
    s.expect("Found 0 error(s) and 1 warning(s)").unwrap();
    s.expect(expectrl::Eof).unwrap();

    assert_exit_code(&s, 0);
}

#[test]
fn lint_strict_flag_fails_on_warnings() {
    let temp = setup_project(WARNING_CONFIG);
    let mut s = spawn_bivvy(&["lint", "--strict"], temp.path());

    s.expect("app_name contains spaces; consider using kebab-case")
        .unwrap();
    // Under --strict, the summary is still printed and the process must
    // exit with code 1.
    s.expect("Found 0 error(s) and 1 warning(s)").unwrap();
    s.expect(expectrl::Eof).unwrap();

    assert_exit_code(&s, 1);
}

// ── Error paths ───────────────────────────────────────────────────────

#[test]
fn lint_circular_dependency_detected() {
    let temp = setup_project(CIRCULAR_CONFIG);
    let mut s = spawn_bivvy(&["lint"], temp.path());

    // Human formatter prefixes error lines with "error[<rule-id>]:".
    s.expect("error[circular-dependency]").unwrap();
    // The full cycle message from the rule (cycle start is HashMap-order
    // dependent, so we match on the stable prefix).
    s.expect("Circular dependency detected:").unwrap();
    // Summary line from src/lint/output/human.rs.
    s.expect("Found 1 error(s) and 0 warning(s)").unwrap();
    s.expect(expectrl::Eof).unwrap();

    assert_exit_code(&s, 1);
}

#[test]
fn lint_no_config_fails() {
    let temp = tempfile::TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["lint"], temp.path());

    // Full user-facing message from src/cli/commands/lint.rs.
    s.expect("No configuration found. Run 'bivvy init' first.")
        .unwrap();
    s.expect(expectrl::Eof).unwrap();

    assert_exit_code(&s, 2);
}

// ── Structured output formats ─────────────────────────────────────────

#[test]
fn lint_json_format_flag() {
    let temp = setup_project(VALID_CONFIG);
    let mut s = spawn_bivvy(&["lint", "--format", "json"], temp.path());

    let output = s.expect(expectrl::Eof).unwrap();
    let text = String::from_utf8_lossy(output.as_bytes());
    let clean = strip_ansi(&text);

    // Extract the JSON object from the output (skip any non-JSON preamble)
    let json_start = clean.find('{').expect("JSON output should contain '{'");
    let json_str = &clean[json_start..];
    let parsed: serde_json::Value =
        serde_json::from_str(json_str).expect("lint --format json should produce valid JSON");

    assert!(
        parsed["diagnostics"].is_array(),
        "JSON output should contain a 'diagnostics' array"
    );
    assert_eq!(
        parsed["summary"]["total"].as_u64().unwrap(),
        0,
        "Valid config should have zero diagnostics"
    );
    assert_eq!(
        parsed["summary"]["errors"].as_u64().unwrap(),
        0,
        "Valid config should have zero errors"
    );
    assert_eq!(
        parsed["summary"]["warnings"].as_u64().unwrap(),
        0,
        "Valid config should have zero warnings"
    );

    assert_exit_code(&s, 0);
}

#[test]
fn lint_json_format_reports_warnings() {
    let temp = setup_project(WARNING_CONFIG);
    let mut s = spawn_bivvy(&["lint", "--format", "json"], temp.path());

    let output = s.expect(expectrl::Eof).unwrap();
    let text = String::from_utf8_lossy(output.as_bytes());
    let clean = strip_ansi(&text);

    let json_start = clean.find('{').expect("JSON output should contain '{'");
    let json_str = &clean[json_start..];
    let parsed: serde_json::Value = serde_json::from_str(json_str)
        .expect("lint --format json with warnings should produce valid JSON");

    assert_eq!(
        parsed["summary"]["errors"].as_u64().unwrap(),
        0,
        "Warning-only config should have zero errors"
    );
    assert!(
        parsed["summary"]["warnings"].as_u64().unwrap() >= 1,
        "Warning-only config should report at least one warning"
    );
    let diagnostics = parsed["diagnostics"].as_array().unwrap();
    assert!(
        diagnostics.iter().any(|d| {
            d["message"]
                .as_str()
                .is_some_and(|m| m.contains("app_name contains spaces"))
        }),
        "JSON diagnostics should contain the app-name-format warning"
    );

    // Warnings alone do not cause a failing exit code (see docs/commands/lint.md).
    assert_exit_code(&s, 0);
}

#[test]
fn lint_sarif_format_flag() {
    let temp = setup_project(VALID_CONFIG);
    let mut s = spawn_bivvy(&["lint", "--format", "sarif"], temp.path());

    let output = s.expect(expectrl::Eof).unwrap();
    let text = String::from_utf8_lossy(output.as_bytes());
    let clean = strip_ansi(&text);

    // Extract the JSON object from the output
    let json_start = clean.find('{').expect("SARIF output should contain '{'");
    let json_str = &clean[json_start..];
    let parsed: serde_json::Value = serde_json::from_str(json_str)
        .expect("lint --format sarif should produce valid SARIF JSON");

    assert_eq!(
        parsed["version"].as_str().unwrap(),
        "2.1.0",
        "SARIF output should have version 2.1.0"
    );
    assert!(
        parsed["runs"].is_array(),
        "SARIF output should contain a 'runs' array"
    );
    assert_eq!(
        parsed["runs"][0]["tool"]["driver"]["name"].as_str().unwrap(),
        "bivvy",
        "SARIF tool name should be 'bivvy'"
    );

    assert_exit_code(&s, 0);
}

#[test]
fn lint_json_format_with_errors() {
    let temp = setup_project(CIRCULAR_CONFIG);
    let mut s = spawn_bivvy(&["lint", "--format", "json"], temp.path());

    let output = s.expect(expectrl::Eof).unwrap();
    let text = String::from_utf8_lossy(output.as_bytes());
    let clean = strip_ansi(&text);

    let json_start = clean.find('{').expect("JSON output should contain '{'");
    let json_str = &clean[json_start..];
    let parsed: serde_json::Value = serde_json::from_str(json_str)
        .expect("lint --format json with errors should produce valid JSON");

    assert!(
        parsed["summary"]["errors"].as_u64().unwrap() > 0,
        "JSON output should report at least one error"
    );
    assert!(
        parsed["summary"]["total"].as_u64().unwrap() > 0,
        "JSON output should report at least one total diagnostic"
    );
    let diagnostics = parsed["diagnostics"].as_array().unwrap();
    assert!(
        diagnostics.iter().any(|d| {
            d["message"]
                .as_str()
                .is_some_and(|m| m.contains("Circular dependency detected"))
        }),
        "JSON diagnostics should contain a circular dependency error"
    );
    // The diagnostic should also be tagged with the rule id.
    assert!(
        diagnostics
            .iter()
            .any(|d| d["rule_id"].as_str() == Some("circular-dependency")),
        "JSON diagnostics should include the circular-dependency rule id"
    );

    assert_exit_code(&s, 1);
}
