//! Comprehensive system tests for `bivvy lint`.
//!
//! Tests configuration validation including valid configs, various
//! error types (circular deps, missing refs, parse errors), output
//! formats, the --strict / --fix flags, environment rules, requirement
//! rules, and multi-rule scenarios.
#![cfg(unix)]

mod system;

use system::helpers::*;

// ─────────────────────────────────────────────────────────────────────
// Configs — Happy path
// ─────────────────────────────────────────────────────────────────────

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

const DEEP_CHAIN_CONFIG: &str = r#"
app_name: "DeepChain"
steps:
  a:
    command: "git --version"
  b:
    command: "cargo --version"
    depends_on: [a]
  c:
    command: "rustc --version"
    depends_on: [b]
  d:
    command: "cargo fmt --version"
    depends_on: [c]
  e:
    command: "cargo clippy --version"
    depends_on: [d]
workflows:
  default:
    steps: [a, b, c, d, e]
"#;

// ─────────────────────────────────────────────────────────────────────
// Configs — Step dependency errors
// ─────────────────────────────────────────────────────────────────────

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

const SELF_DEPENDENCY_CONFIG: &str = r#"
app_name: "SelfDep"
steps:
  loopy:
    command: "git --version"
    depends_on: [loopy]
workflows:
  default:
    steps: [loopy]
"#;

const MISSING_DEP_CONFIG: &str = r#"
app_name: "MissingDep"
steps:
  orphan:
    command: "rustc --version"
    depends_on: [nonexistent]
workflows:
  default:
    steps: [orphan]
"#;

const WORKFLOW_REF_MISSING_CONFIG: &str = r#"
app_name: "BadWorkflow"
steps:
  real:
    command: "git --version"
workflows:
  default:
    steps: [real, ghost]
"#;

// ─────────────────────────────────────────────────────────────────────
// Configs — App name rule
// ─────────────────────────────────────────────────────────────────────

const APP_NAME_SPACES_CONFIG: &str = r#"
app_name: "My Cool App"
steps:
  hello:
    command: "git --version"
workflows:
  default:
    steps: [hello]
"#;

const APP_NAME_EMPTY_CONFIG: &str = r#"
app_name: ""
steps:
  hello:
    command: "git --version"
workflows:
  default:
    steps: [hello]
"#;

// ─────────────────────────────────────────────────────────────────────
// Configs — Required fields rule
// ─────────────────────────────────────────────────────────────────────

const MISSING_APP_NAME_CONFIG: &str = r#"
steps:
  hello:
    command: "git --version"
workflows:
  default:
    steps: [hello]
"#;

const NO_WORKFLOWS_CONFIG: &str = r#"
app_name: "NoWorkflows"
steps:
  hello:
    command: "git --version"
"#;

// ─────────────────────────────────────────────────────────────────────
// Configs — Environment rules
// ─────────────────────────────────────────────────────────────────────

/// Step has environment override for unknown environment "staging"
const UNKNOWN_ENV_IN_STEP_CONFIG: &str = r#"
app_name: "EnvTest"
steps:
  build:
    command: "rustc --version"
    environments:
      staging:
        command: "git log --oneline -1"
workflows:
  default:
    steps: [build]
"#;

/// Step has only_environments referencing unknown environment "staging"
const UNKNOWN_ENV_IN_ONLY_CONFIG: &str = r#"
app_name: "EnvOnlyTest"
steps:
  build:
    command: "rustc --version"
    only_environments: [staging]
workflows:
  default:
    steps: [build]
"#;

/// Environment default_workflow references nonexistent workflow
const ENV_DEFAULT_WORKFLOW_MISSING_CONFIG: &str = r#"
app_name: "EnvWorkflow"
settings:
  environments:
    ci:
      detect:
        - env: CI
      default_workflow: fast-ci
steps:
  build:
    command: "rustc --version"
workflows:
  default:
    steps: [build]
"#;

/// Step has environment override for env excluded by only_environments
const UNREACHABLE_ENV_OVERRIDE_CONFIG: &str = r#"
app_name: "Unreachable"
steps:
  build:
    command: "rustc --version"
    only_environments: [ci]
    environments:
      docker:
        command: "git status --short"
workflows:
  default:
    steps: [build]
"#;

/// Custom environment shadows builtin "ci"
const SHADOW_BUILTIN_ENV_CONFIG: &str = r#"
app_name: "ShadowEnv"
settings:
  environments:
    ci:
      detect:
        - env: CI
steps:
  build:
    command: "rustc --version"
workflows:
  default:
    steps: [build]
"#;

/// Redundant environment override: same command as base step
const REDUNDANT_ENV_OVERRIDE_CONFIG: &str = r#"
app_name: "Redundant"
steps:
  build:
    command: "rustc --version"
    environments:
      ci:
        command: "rustc --version"
workflows:
  default:
    steps: [build]
"#;

/// Redundant env null: nulling a key not present in base
const REDUNDANT_ENV_NULL_CONFIG: &str = r#"
app_name: "RedundantNull"
steps:
  build:
    command: "rustc --version"
    environments:
      ci:
        env:
          NONEXISTENT_KEY: null
workflows:
  default:
    steps: [build]
"#;

/// Per-environment circular dependency: base is fine but CI deps create a cycle
const ENV_CIRCULAR_DEP_CONFIG: &str = r#"
app_name: "EnvCircular"
steps:
  a:
    command: "git --version"
    environments:
      ci:
        depends_on: [b]
  b:
    command: "cargo --version"
    depends_on: [a]
workflows:
  default:
    steps: [a, b]
"#;

// ─────────────────────────────────────────────────────────────────────
// Configs — Requirement rules
// ─────────────────────────────────────────────────────────────────────

/// Step requires an unknown requirement
const UNKNOWN_REQUIREMENT_CONFIG: &str = r#"
app_name: "UnknownReq"
steps:
  build:
    command: "rustc --version"
    requires: [nonexistent-tool-xyz]
workflows:
  default:
    steps: [build]
"#;

/// Step requires a valid built-in requirement (should pass)
const VALID_REQUIREMENT_CONFIG: &str = r#"
app_name: "ValidReq"
steps:
  build:
    command: "rustc --version"
    requires: [ruby]
workflows:
  default:
    steps: [build]
"#;

/// Custom requirement with ServiceReachable but no install_hint
const SERVICE_WITHOUT_HINT_CONFIG: &str = r#"
app_name: "ServiceNoHint"
requirements:
  my-service:
    check:
      type: service_reachable
      command: "curl -s http://localhost:5432"
steps:
  build:
    command: "rustc --version"
    requires: [my-service]
workflows:
  default:
    steps: [build]
"#;

/// Custom requirement with CommandSucceeds but no install_template
const INSTALL_TEMPLATE_MISSING_CONFIG: &str = r#"
app_name: "NoInstallTemplate"
requirements:
  my-tool:
    check:
      type: command_succeeds
      command: "my-tool --version"
steps:
  build:
    command: "rustc --version"
    requires: [my-tool]
workflows:
  default:
    steps: [build]
"#;

// ─────────────────────────────────────────────────────────────────────
// Configs — Template rules
// ─────────────────────────────────────────────────────────────────────

/// Step references a template that does not exist in the registry
const UNDEFINED_TEMPLATE_CONFIG: &str = r#"
app_name: "UndefinedTpl"
steps:
  build:
    template: "nonexistent-template-xyz-999"
workflows:
  default:
    steps: [build]
"#;

// ─────────────────────────────────────────────────────────────────────
// Configs — Multi-rule scenarios
// ─────────────────────────────────────────────────────────────────────

/// Config that triggers multiple lint diagnostics at once:
/// - app_name with spaces (warning)
/// - undefined dependency (error)
/// - self dependency (error)
const MULTI_RULE_CONFIG: &str = r#"
app_name: "My Bad App"
steps:
  alpha:
    command: "git --version"
    depends_on: [alpha]
  beta:
    command: "cargo --version"
    depends_on: [nonexistent]
workflows:
  default:
    steps: [alpha, beta]
"#;

/// Config that triggers only warnings, not errors:
/// - app_name with spaces (warning)
/// - unknown environment in step (warning)
const WARNINGS_ONLY_CONFIG: &str = r#"
app_name: "My Warning App"
steps:
  build:
    command: "rustc --version"
    environments:
      staging:
        command: "git log --oneline -1"
workflows:
  default:
    steps: [build]
"#;

/// Config with three-step circular dependency
const THREE_STEP_CIRCULAR_CONFIG: &str = r#"
app_name: "ThreeCycle"
steps:
  a:
    command: "git --version"
    depends_on: [c]
  b:
    command: "cargo --version"
    depends_on: [a]
  c:
    command: "rustc --version"
    depends_on: [b]
workflows:
  default:
    steps: [a, b, c]
"#;

// =====================================================================
// HAPPY PATH
// =====================================================================

/// Valid config passes lint with "Configuration is valid!" message and exit code 0.
#[test]
fn lint_valid_config() {
    let temp = setup_project(VALID_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains("Configuration is valid!"),
        "Should report valid config, got: {combined}"
    );
    assert!(output.status.success(), "Valid config should exit 0");
    assert_eq!(
        output.status.code(),
        Some(0),
        "Valid config should exit with code 0"
    );
}

/// Deep dependency chain is valid (no circular deps).
#[test]
fn lint_deep_chain_valid() {
    let temp = setup_project(DEEP_CHAIN_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains("Configuration is valid!"),
        "Deep chain should be valid, got: {combined}"
    );
    assert!(output.status.success(), "Deep chain should exit 0");
    assert_eq!(
        output.status.code(),
        Some(0),
        "Deep chain should exit with code 0"
    );
}

/// Lint after adding a template still passes.
#[test]
fn lint_after_add_passes() {
    let temp = setup_project(VALID_CONFIG);
    run_bivvy_silently(temp.path(), &["add", "bundle-install"]);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains("Configuration is valid!"),
        "Config with added template should be valid, got: {combined}"
    );
    assert!(output.status.success(), "Lint after add should exit 0");
    assert_eq!(
        output.status.code(),
        Some(0),
        "Lint after add should exit with code 0"
    );
}

/// Config with a valid built-in requirement passes lint.
#[test]
fn lint_valid_requirement_passes() {
    let temp = setup_project(VALID_REQUIREMENT_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // ruby is a known built-in; the only possible diagnostic would be
    // install-template-missing (hint severity), which is not an error.
    // In non-strict mode, this should pass.
    assert!(
        !combined.contains("error["),
        "Valid requirement should not produce error-level diagnostics, got: {combined}"
    );
    assert!(
        !combined.contains("warning[unknown-requirement]"),
        "Valid requirement 'ruby' should not be flagged as unknown, got: {combined}"
    );
    assert!(
        output.status.success(),
        "Valid requirement config should exit 0"
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "Valid requirement config should exit with code 0"
    );
}

// =====================================================================
// FLAGS — Format
// =====================================================================

/// --format json produces JSON output (empty diagnostics array for valid config).
#[test]
fn lint_json_format_valid_config() {
    let temp = setup_project(VALID_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint", "--format", "json"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);

    // JSON output is { "diagnostics": [...], "summary": {...} }
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|_| panic!("Should be valid JSON, got: {stdout}"));
    let diagnostics = parsed
        .get("diagnostics")
        .and_then(|d| d.as_array())
        .expect("JSON output should contain diagnostics array");
    assert!(
        diagnostics.is_empty(),
        "Valid config should produce empty diagnostics array, got: {stdout}"
    );
    let summary = parsed
        .get("summary")
        .expect("JSON output should contain summary object");
    assert_eq!(
        summary.get("total").and_then(|v| v.as_u64()),
        Some(0),
        "Summary total should be 0 for valid config, got: {stdout}"
    );
    assert!(output.status.success(), "Valid config JSON should exit 0");
    assert_eq!(
        output.status.code(),
        Some(0),
        "Valid config JSON should exit with code 0"
    );
}

/// --format json with errors produces JSON array with diagnostic objects.
#[test]
fn lint_json_format_with_errors() {
    let temp = setup_project(CIRCULAR_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint", "--format", "json"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);

    let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|_| panic!("Should be valid JSON, got: {stdout}"));
    let arr = parsed
        .get("diagnostics")
        .and_then(|d| d.as_array())
        .expect("JSON output should contain diagnostics array");
    assert!(!arr.is_empty(), "Should have at least one diagnostic");
    // Verify diagnostics contain circular dependency info
    let has_circular = arr.iter().any(|d| {
        d.get("message")
            .and_then(|m| m.as_str())
            .map(|m| m.starts_with("Circular dependency detected"))
            .unwrap_or(false)
            && d.get("rule_id").and_then(|r| r.as_str()) == Some("circular-dependency")
            && d.get("severity").and_then(|s| s.as_str()) == Some("error")
    });
    assert!(
        has_circular,
        "JSON diagnostics should contain circular-dependency error, got: {stdout}"
    );
    // Summary should report at least one error
    let errors_count = parsed
        .pointer("/summary/errors")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert!(
        errors_count >= 1,
        "JSON summary.errors should be >= 1, got: {stdout}"
    );
    assert!(!output.status.success(), "Error config JSON should exit non-zero");
    assert_eq!(
        output.status.code(),
        Some(1),
        "Error config JSON should exit with code 1"
    );
}

/// --format sarif produces valid SARIF JSON output.
#[test]
fn lint_sarif_format_valid_config() {
    let temp = setup_project(VALID_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint", "--format", "sarif"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);

    let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|_| panic!("Should be valid SARIF JSON, got: {stdout}"));
    // SARIF has a $schema or version field and a runs array
    assert!(
        parsed.get("$schema").is_some() || parsed.get("version").is_some(),
        "SARIF output should have schema or version field, got: {stdout}"
    );
    // For a valid config, the runs[0].results array should be empty
    let results = parsed
        .pointer("/runs/0/results")
        .and_then(|r| r.as_array())
        .expect("SARIF output should contain runs[0].results array");
    assert!(
        results.is_empty(),
        "Valid config SARIF should have empty results, got: {stdout}"
    );
    assert!(output.status.success(), "Valid config SARIF should exit 0");
    assert_eq!(
        output.status.code(),
        Some(0),
        "Valid config SARIF should exit with code 0"
    );
}

/// --format sarif with errors produces SARIF with results.
#[test]
fn lint_sarif_format_with_errors() {
    let temp = setup_project(CIRCULAR_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint", "--format", "sarif"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);

    let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|_| panic!("Should be valid SARIF JSON, got: {stdout}"));
    // SARIF runs[0].results should have entries
    let results = parsed
        .pointer("/runs/0/results")
        .and_then(|r| r.as_array())
        .expect("SARIF output should contain runs[0].results array");
    assert!(
        !results.is_empty(),
        "SARIF should contain diagnostic results, got: {stdout}"
    );
    // Verify the circular-dependency result is present
    let has_circular = results.iter().any(|r| {
        r.pointer("/ruleId").and_then(|v| v.as_str()) == Some("circular-dependency")
    });
    assert!(
        has_circular,
        "SARIF results should include circular-dependency ruleId, got: {stdout}"
    );
    assert!(
        !output.status.success(),
        "Error config SARIF should exit non-zero"
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "Error config SARIF should exit with code 1"
    );
}

// =====================================================================
// FLAGS — Strict mode
// =====================================================================

/// --strict on a valid config still passes with exit code 0.
#[test]
fn lint_strict_valid_config_passes() {
    let temp = setup_project(VALID_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint", "--strict"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains("Configuration is valid!"),
        "Valid config should pass strict mode, got: {combined}"
    );
    assert!(output.status.success(), "Strict valid config should exit 0");
    assert_eq!(
        output.status.code(),
        Some(0),
        "Strict valid config should exit with code 0"
    );
}

/// --strict treats warnings as errors: app_name with spaces fails.
#[test]
fn lint_strict_fails_on_warnings() {
    let temp = setup_project(APP_NAME_SPACES_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint", "--strict"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains("warning[app-name-format]: app_name contains spaces; consider using kebab-case"),
        "Strict mode should surface app_name warning with full message, got: {combined}"
    );
    assert!(
        !output.status.success(),
        "Strict mode with warnings should exit non-zero"
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "Strict mode with warnings should exit with code 1"
    );
}

/// Without --strict, warnings-only config passes lint (exit 0).
#[test]
fn lint_without_strict_passes_on_warnings() {
    let temp = setup_project(APP_NAME_SPACES_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // The config has a warning but no errors, so lint should still succeed.
    assert!(
        combined.contains("warning[app-name-format]: app_name contains spaces; consider using kebab-case"),
        "Should show app_name warning with full message, got: {combined}"
    );
    assert!(
        output.status.success(),
        "Non-strict mode with only warnings should exit 0"
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "Non-strict mode with only warnings should exit with code 0"
    );
}

/// --strict + --format json combined produces JSON with diagnostics and fails.
#[test]
fn lint_strict_json_combined() {
    let temp = setup_project(APP_NAME_SPACES_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint", "--strict", "--format", "json"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);

    let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|_| panic!("Should be valid JSON, got: {stdout}"));
    let arr = parsed
        .get("diagnostics")
        .and_then(|d| d.as_array())
        .expect("JSON output should contain diagnostics array");
    assert!(!arr.is_empty(), "Strict JSON should contain diagnostics");
    // Verify the full structured diagnostic (message, rule_id, severity)
    let has_app_name = arr.iter().any(|d| {
        d.get("message").and_then(|m| m.as_str())
            == Some("app_name contains spaces; consider using kebab-case")
            && d.get("rule_id").and_then(|r| r.as_str()) == Some("app-name-format")
            && d.get("severity").and_then(|s| s.as_str()) == Some("warning")
    });
    assert!(
        has_app_name,
        "JSON should contain structured app_name warning (rule_id=app-name-format, severity=warning, full message), got: {stdout}"
    );
    assert!(
        !output.status.success(),
        "Strict mode with warnings should exit non-zero"
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "Strict mode with warnings should exit with code 1"
    );
}

// =====================================================================
// FLAGS — Fix mode
// =====================================================================

/// --fix on a valid config does nothing harmful and reports valid.
#[test]
fn lint_fix_valid_config_no_op() {
    let temp = setup_project(VALID_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint", "--fix"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains("Configuration is valid!"),
        "Fix on valid config should report valid, got: {combined}"
    );
    assert!(output.status.success(), "Fix on valid config should exit 0");
    assert_eq!(
        output.status.code(),
        Some(0),
        "Fix on valid config should exit with code 0"
    );
}

/// --fix on a config with fixable issues attempts to apply fixes.
#[test]
fn lint_fix_with_fixable_issue() {
    let temp = setup_project(APP_NAME_SPACES_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint", "--fix"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // The app-name-format rule supports_fix, so --fix may report fix attempts
    // Even if no actual file change occurs (byte offsets are 0), it should not crash
    // The diagnostic message should still appear
    assert!(
        combined.contains("warning[app-name-format]: app_name contains spaces; consider using kebab-case"),
        "Fix mode should show the app_name warning with full message, got: {combined}"
    );
    // --fix should still exit 0 on warning-only config
    assert!(
        output.status.success(),
        "Fix on warning-only config should exit 0"
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "Fix on warning-only config should exit with code 0"
    );
}

/// --fix combined with --format json produces valid JSON.
#[test]
fn lint_fix_json_combined() {
    let temp = setup_project(APP_NAME_SPACES_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint", "--fix", "--format", "json"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should produce valid JSON output without crashing
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|_| panic!("Should be valid JSON, got: {stdout}"));
    let arr = parsed
        .get("diagnostics")
        .and_then(|d| d.as_array())
        .expect("Fix + JSON should produce diagnostics array");
    // The diagnostics should contain the app_name warning with full message
    let has_app_name = arr.iter().any(|d| {
        d.get("message").and_then(|m| m.as_str())
            == Some("app_name contains spaces; consider using kebab-case")
            && d.get("rule_id").and_then(|r| r.as_str()) == Some("app-name-format")
            && d.get("severity").and_then(|s| s.as_str()) == Some("warning")
    });
    assert!(
        has_app_name,
        "Fix + JSON should include structured app_name warning, got: {stdout}"
    );
    // Non-strict warning-only → exit 0
    assert!(
        output.status.success(),
        "Fix + JSON on warning-only config should exit 0"
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "Fix + JSON on warning-only config should exit with code 0"
    );
}

// =====================================================================
// RULE: app-name-format
// =====================================================================

/// App name with spaces produces a warning mentioning kebab-case.
#[test]
fn lint_app_name_spaces_warning() {
    let temp = setup_project(APP_NAME_SPACES_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains("warning[app-name-format]: app_name contains spaces; consider using kebab-case"),
        "Should warn about spaces in app_name with full message, got: {combined}"
    );
    assert!(
        output.status.success(),
        "Warning-only config should still exit 0"
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "Warning-only config should exit with code 0"
    );
}

/// Empty app_name produces an error.
#[test]
fn lint_app_name_empty_error() {
    let temp = setup_project(APP_NAME_EMPTY_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains("error[app-name-format]: app_name cannot be empty"),
        "Should error on empty app_name with full message, got: {combined}"
    );
    assert!(
        !output.status.success(),
        "Empty app_name error should exit non-zero"
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "Empty app_name error should exit with code 1"
    );
}

// =====================================================================
// RULE: required-fields
// =====================================================================

/// Missing app_name triggers a required-fields error.
#[test]
fn lint_missing_app_name_error() {
    let temp = setup_project(MISSING_APP_NAME_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains("error[required-fields]: Missing required field: app_name"),
        "Should report missing app_name with full message, got: {combined}"
    );
    assert!(
        !output.status.success(),
        "Missing required field should exit non-zero"
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "Missing required field should exit with code 1"
    );
}

/// No workflows defined produces a warning.
#[test]
fn lint_no_workflows_warning() {
    let temp = setup_project(NO_WORKFLOWS_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains("warning[required-fields]: No workflows defined"),
        "Should warn about missing workflows with full message, got: {combined}"
    );
    assert!(
        output.status.success(),
        "Warning-only config should exit 0"
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "Warning-only config should exit with code 0"
    );
}

// =====================================================================
// RULE: circular-dependency
// =====================================================================

/// Two-step circular dependency detected with exit code 1.
#[test]
fn lint_circular_dependency() {
    let temp = setup_project(CIRCULAR_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains("error[circular-dependency]: Circular dependency detected:"),
        "Should detect circular dependency with full message, got: {combined}"
    );
    assert!(!output.status.success(), "Circular dependency should exit non-zero");
    assert_eq!(
        output.status.code(),
        Some(1),
        "Circular dependency should exit with code 1"
    );
}

/// Three-step circular dependency detected.
#[test]
fn lint_three_step_circular_dependency() {
    let temp = setup_project(THREE_STEP_CIRCULAR_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains("error[circular-dependency]: Circular dependency detected:"),
        "Should detect three-step circular dependency, got: {combined}"
    );
    assert!(!output.status.success(), "Circular dependency should exit non-zero");
    assert_eq!(
        output.status.code(),
        Some(1),
        "Three-step circular dependency should exit with code 1"
    );
}

// =====================================================================
// RULE: self-dependency
// =====================================================================

/// Step that depends on itself is flagged.
#[test]
fn lint_self_dependency() {
    let temp = setup_project(SELF_DEPENDENCY_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains("error[self-dependency]: Step 'loopy' depends on itself"),
        "Should detect self-dependency with full message, got: {combined}"
    );
    assert!(!output.status.success(), "Self-dependency should exit non-zero");
    assert_eq!(
        output.status.code(),
        Some(1),
        "Self-dependency should exit with code 1"
    );
}

// =====================================================================
// RULE: undefined-dependency
// =====================================================================

/// Step referencing nonexistent dependency is flagged.
#[test]
fn lint_undefined_dependency() {
    let temp = setup_project(MISSING_DEP_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains("error[undefined-dependency]: Step 'orphan' depends on undefined step 'nonexistent'"),
        "Should detect undefined dependency with full message, got: {combined}"
    );
    assert!(!output.status.success(), "Undefined dependency should exit non-zero");
    assert_eq!(
        output.status.code(),
        Some(1),
        "Undefined dependency should exit with code 1"
    );
}

// =====================================================================
// RULE: unknown-environment-in-step
// =====================================================================

/// Step with environment override for unknown environment is flagged.
#[test]
fn lint_unknown_env_in_step() {
    let temp = setup_project(UNKNOWN_ENV_IN_STEP_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains(
            "warning[unknown-environment-in-step]: Step 'build' has override for unknown environment 'staging'"
        ),
        "Should warn about unknown environment 'staging' in step with full message, got: {combined}"
    );
    assert!(
        output.status.success(),
        "Warning-only config should exit 0"
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "Warning-only config should exit with code 0"
    );
}

// =====================================================================
// RULE: unknown-environment-in-only
// =====================================================================

/// only_environments referencing unknown environment is flagged.
#[test]
fn lint_unknown_env_in_only_environments() {
    let temp = setup_project(UNKNOWN_ENV_IN_ONLY_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains(
            "warning[unknown-environment-in-only]: Step 'build' only_environments references unknown environment 'staging'"
        ),
        "Should warn about unknown environment in only_environments with full message, got: {combined}"
    );
    assert!(
        output.status.success(),
        "Warning-only config should exit 0"
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "Warning-only config should exit with code 0"
    );
}

// =====================================================================
// RULE: environment-default-workflow-missing
// =====================================================================

/// Environment default_workflow references a nonexistent workflow.
#[test]
fn lint_env_default_workflow_missing() {
    let temp = setup_project(ENV_DEFAULT_WORKFLOW_MISSING_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains(
            "error[environment-default-workflow-missing]: Environment 'ci' default_workflow 'fast-ci' does not exist"
        ),
        "Should flag missing default_workflow 'fast-ci' with full message, got: {combined}"
    );
    assert!(!output.status.success(), "Missing default_workflow should exit non-zero");
    assert_eq!(
        output.status.code(),
        Some(1),
        "Missing default_workflow should exit with code 1"
    );
}

// =====================================================================
// RULE: unreachable-environment-override
// =====================================================================

/// Environment override excluded by only_environments is flagged.
#[test]
fn lint_unreachable_env_override() {
    let temp = setup_project(UNREACHABLE_ENV_OVERRIDE_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains(
            "warning[unreachable-environment-override]: Step 'build' has override for 'docker' but only_environments excludes it"
        ),
        "Should warn about unreachable override for 'docker' with full message, got: {combined}"
    );
    assert!(
        output.status.success(),
        "Warning-only config should exit 0"
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "Warning-only config should exit with code 0"
    );
}

// =====================================================================
// RULE: custom-environment-shadows-builtin
// =====================================================================

/// Custom environment shadowing builtin "ci" is flagged.
#[test]
fn lint_custom_env_shadows_builtin() {
    let temp = setup_project(SHADOW_BUILTIN_ENV_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains(
            "warning[custom-environment-shadows-builtin]: Custom environment 'ci' shadows built-in environment"
        ),
        "Should warn about custom env shadowing builtin 'ci' with full message, got: {combined}"
    );
    assert!(
        output.status.success(),
        "Warning-only config should exit 0"
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "Warning-only config should exit with code 0"
    );
}

// =====================================================================
// RULE: redundant-environment-override
// =====================================================================

/// Environment override identical to base step command is flagged.
#[test]
fn lint_redundant_env_override() {
    let temp = setup_project(REDUNDANT_ENV_OVERRIDE_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains(
            "hint[redundant-environment-override]: Step 'build' environment 'ci' override has redundant fields: command"
        ),
        "Should flag redundant environment override with full message, got: {combined}"
    );
    assert!(
        output.status.success(),
        "Hint-only config should exit 0"
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "Hint-only config should exit with code 0"
    );
}

// =====================================================================
// RULE: redundant-env-null
// =====================================================================

/// Null env entry for key not in base is flagged.
#[test]
fn lint_redundant_env_null() {
    let temp = setup_project(REDUNDANT_ENV_NULL_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains(
            "hint[redundant-env-null]: Step 'build' environment 'ci' removes 'NONEXISTENT_KEY' but it's not in the base env"
        ),
        "Should flag redundant env null for nonexistent key with full message, got: {combined}"
    );
    assert!(
        output.status.success(),
        "Hint-only config should exit 0"
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "Hint-only config should exit with code 0"
    );
}

// =====================================================================
// RULE: environment-circular-dependency
// =====================================================================

/// Per-environment dependency override creates a cycle.
#[test]
fn lint_env_circular_dependency() {
    let temp = setup_project(ENV_CIRCULAR_DEP_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains("error[environment-circular-dependency]: Circular dependency in 'ci' environment:"),
        "Should detect per-environment circular dependency with full message, got: {combined}"
    );
    assert!(!output.status.success(), "Env circular dependency should exit non-zero");
    assert_eq!(
        output.status.code(),
        Some(1),
        "Env circular dependency should exit with code 1"
    );
}

// =====================================================================
// RULE: unknown-requirement
// =====================================================================

/// Step requiring unknown requirement is flagged.
#[test]
fn lint_unknown_requirement() {
    let temp = setup_project(UNKNOWN_REQUIREMENT_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains(
            "warning[unknown-requirement]: Step 'build' requires unknown requirement 'nonexistent-tool-xyz'"
        ),
        "Should flag unknown requirement with full message, got: {combined}"
    );
    assert!(
        output.status.success(),
        "Warning-only config should exit 0"
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "Warning-only config should exit with code 0"
    );
}

// =====================================================================
// RULE: service-requirement-without-hint
// =====================================================================

/// Service requirement without install_hint is flagged.
#[test]
fn lint_service_requirement_without_hint() {
    let temp = setup_project(SERVICE_WITHOUT_HINT_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains(
            "warning[service-requirement-without-hint]: Service requirement 'my-service' has no install hint"
        ),
        "Should flag service requirement without hint with full message, got: {combined}"
    );
    assert!(
        output.status.success(),
        "Warning-only config should exit 0"
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "Warning-only config should exit with code 0"
    );
}

// =====================================================================
// RULE: install-template-missing
// =====================================================================

/// Custom requirement without install_template is flagged.
#[test]
fn lint_install_template_missing() {
    let temp = setup_project(INSTALL_TEMPLATE_MISSING_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains(
            "hint[install-template-missing]: Requirement 'my-tool' has no install template"
        ),
        "Should flag missing install template with full message, got: {combined}"
    );
    assert!(
        output.status.success(),
        "Hint-only config should exit 0"
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "Hint-only config should exit with code 0"
    );
}

// =====================================================================
// RULE: undefined-template
// =====================================================================

/// Step referencing undefined template is flagged.
#[test]
fn lint_undefined_template() {
    let temp = setup_project(UNDEFINED_TEMPLATE_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains(
            "error[undefined-template]: Step 'build' references undefined template 'nonexistent-template-xyz-999'"
        ),
        "Should flag undefined template reference with full message, got: {combined}"
    );
    assert!(!output.status.success(), "Undefined template should exit non-zero");
    assert_eq!(
        output.status.code(),
        Some(1),
        "Undefined template should exit with code 1"
    );
}

// =====================================================================
// MULTI-RULE SCENARIOS
// =====================================================================

/// Config triggering multiple errors at once: self-dep + undefined dep + app name warning.
#[test]
fn lint_multi_rule_errors() {
    let temp = setup_project(MULTI_RULE_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // Must verify BOTH diagnostics are present with full messages
    assert!(
        combined.contains("error[self-dependency]: Step 'alpha' depends on itself"),
        "Should flag self-dependency on alpha with full message, got: {combined}"
    );
    assert!(
        combined.contains("error[undefined-dependency]: Step 'beta' depends on undefined step 'nonexistent'"),
        "Should flag undefined dependency on nonexistent with full message, got: {combined}"
    );
    assert!(
        combined.contains("warning[app-name-format]: app_name contains spaces; consider using kebab-case"),
        "Should warn about app_name with spaces with full message, got: {combined}"
    );
    assert!(!output.status.success(), "Multi-rule errors should exit non-zero");
    assert_eq!(
        output.status.code(),
        Some(1),
        "Multi-rule errors should exit with code 1"
    );
}

/// Config with only warnings passes without --strict.
#[test]
fn lint_warnings_only_passes_without_strict() {
    let temp = setup_project(WARNINGS_ONLY_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // Should show both warnings with full messages
    assert!(
        combined.contains(
            "warning[unknown-environment-in-step]: Step 'build' has override for unknown environment 'staging'"
        ),
        "Should warn about unknown environment with full message, got: {combined}"
    );
    assert!(
        combined.contains("warning[app-name-format]: app_name contains spaces; consider using kebab-case"),
        "Should warn about app_name with full message, got: {combined}"
    );
    assert!(
        output.status.success(),
        "Warnings-only config without strict should exit 0"
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "Warnings-only config without strict should exit with code 0"
    );
}

/// Config with only warnings fails with --strict.
#[test]
fn lint_warnings_only_fails_with_strict() {
    let temp = setup_project(WARNINGS_ONLY_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint", "--strict"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains(
            "warning[unknown-environment-in-step]: Step 'build' has override for unknown environment 'staging'"
        ),
        "Strict mode should report warnings with full message, got: {combined}"
    );
    assert!(
        !output.status.success(),
        "Strict mode with warnings should exit non-zero"
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "Strict mode with warnings should exit with code 1"
    );
}

/// Multi-rule errors reported in JSON format.
#[test]
fn lint_multi_rule_json_format() {
    let temp = setup_project(MULTI_RULE_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint", "--format", "json"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);

    let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|_| panic!("Should be valid JSON, got: {stdout}"));
    let arr = parsed
        .get("diagnostics")
        .and_then(|d| d.as_array())
        .expect("JSON output should contain diagnostics array");
    // Should have at least 3 diagnostics: self-dep, undefined-dep, app-name
    assert!(
        arr.len() >= 3,
        "Should have at least 3 diagnostics, got {} in: {stdout}",
        arr.len()
    );
    // Verify each expected diagnostic is present with correct rule_id
    let has_self_dep = arr.iter().any(|d| {
        d.get("rule_id").and_then(|r| r.as_str()) == Some("self-dependency")
            && d.get("message").and_then(|m| m.as_str())
                == Some("Step 'alpha' depends on itself")
    });
    let has_undefined_dep = arr.iter().any(|d| {
        d.get("rule_id").and_then(|r| r.as_str()) == Some("undefined-dependency")
            && d.get("message").and_then(|m| m.as_str())
                == Some("Step 'beta' depends on undefined step 'nonexistent'")
    });
    let has_app_name = arr.iter().any(|d| {
        d.get("rule_id").and_then(|r| r.as_str()) == Some("app-name-format")
            && d.get("message").and_then(|m| m.as_str())
                == Some("app_name contains spaces; consider using kebab-case")
    });
    assert!(
        has_self_dep,
        "JSON should include self-dependency diagnostic, got: {stdout}"
    );
    assert!(
        has_undefined_dep,
        "JSON should include undefined-dependency diagnostic, got: {stdout}"
    );
    assert!(
        has_app_name,
        "JSON should include app-name-format diagnostic, got: {stdout}"
    );
    assert!(!output.status.success(), "Multi-rule errors should exit non-zero");
    assert_eq!(
        output.status.code(),
        Some(1),
        "Multi-rule errors should exit with code 1"
    );
}

// =====================================================================
// SAD PATH — Structural errors (workflow references)
// =====================================================================

/// Workflow references nonexistent step: lint currently does not flag this
/// (no lint rule covers workflow→step references), so the config is reported
/// as valid. If a lint rule is added later, this test should be updated to
/// assert on the new diagnostic.
#[test]
fn lint_workflow_references_missing_step() {
    let temp = setup_project(WORKFLOW_REF_MISSING_CONFIG);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // Lint rules do not cover workflow→step references. The config should
    // therefore be reported as valid and exit with code 0.
    assert!(
        combined.contains("Configuration is valid!"),
        "Workflow step ref should not be flagged by lint (no rule covers it), got: {combined}"
    );
    assert!(
        output.status.success(),
        "Config without lint-rule coverage should exit 0"
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "Config without lint-rule coverage should exit with code 0"
    );
}

// =====================================================================
// SAD PATH — Parse errors
// =====================================================================

/// No config file at all exits with code 2.
#[test]
fn lint_no_config_fails() {
    let temp = tempfile::TempDir::new().unwrap();

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stderr.contains("No configuration found. Run 'bivvy init' first."),
        "Should report no configuration found with full message, got: {stderr}"
    );
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(
            output.status.code(),
            Some(2),
            "No config should exit with code 2"
        );
    }
}

/// Empty config file triggers required-fields diagnostics.
#[test]
fn lint_empty_config() {
    let temp = setup_project("");

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // Empty config should trigger required-fields for missing app_name
    assert!(
        combined.contains("error[required-fields]: Missing required field: app_name"),
        "Should report missing app_name for empty config with full message, got: {combined}"
    );
    assert!(
        !output.status.success(),
        "Empty config with errors should exit non-zero"
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "Empty config with errors should exit with code 1"
    );
}

/// Malformed YAML syntax exits non-zero with parse error.
#[test]
fn lint_malformed_yaml() {
    let temp = setup_project("{{{{ not yaml :::");

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stderr.contains("Parse error in"),
        "Should report parse error for malformed YAML, got: {stderr}"
    );
    assert!(
        !output.status.success(),
        "Malformed YAML should exit non-zero"
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "Malformed YAML should exit with code 1"
    );
}

/// Config with duplicate step names (YAML allows duplicate keys, last wins).
#[test]
fn lint_duplicate_step_names() {
    let config = r#"
app_name: "Dupe"
steps:
  same:
    command: "git --version"
  same:
    command: "cargo --version"
workflows:
  default:
    steps: [same]
"#;
    let temp = setup_project(config);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // YAML last-wins on duplicate keys means only one "same" step exists.
    // With a single step and valid config, lint should pass.
    assert!(
        combined.contains("Configuration is valid!"),
        "Duplicate YAML keys should resolve via last-wins and pass lint, got: {combined}"
    );
    assert!(
        output.status.success(),
        "Duplicate keys config should exit 0"
    );
}

/// Config with step that has no command and no template.
#[test]
fn lint_step_no_command_no_template() {
    let config = r#"
app_name: "NoCmd"
steps:
  empty:
    title: "Empty step"
workflows:
  default:
    steps: [empty]
"#;
    let temp = setup_project(config);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // The lint rule system does not check for steps without command/template
    // (that check is in the config validator, not lint rules). Since lint
    // only runs lint rules, this config currently reports as valid.
    // If a step-no-action lint rule is added later, update this test to
    // assert on the new diagnostic.
    assert!(
        combined.contains("Configuration is valid!"),
        "Step with no command or template should pass lint (no lint rule checks this), got: {combined}"
    );
    assert!(
        output.status.success(),
        "Config without lint-rule coverage should exit 0"
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "Config without lint-rule coverage should exit with code 0"
    );
}

// =====================================================================
// EXIT CODE VERIFICATION
// =====================================================================

/// Valid config exits with code 0.
#[test]
fn lint_valid_config_exit_code_zero() {
    let temp = setup_project(VALID_CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(
            output.status.code(),
            Some(0),
            "Valid config should exit with code 0"
        );
    }
}

/// Config with errors exits with code 1.
#[test]
fn lint_error_config_exit_code_nonzero() {
    let temp = setup_project(CIRCULAR_CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(
            output.status.code(),
            Some(1),
            "Config with errors should exit with code 1"
        );
    }
}

/// No config file exits with code 2.
#[test]
fn lint_no_config_exit_code_two() {
    let temp = tempfile::TempDir::new().unwrap();
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert!(!output.status.success(), "No config should exit non-zero");
    // CommandResult::failure(2) maps to exit code 2
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(
            output.status.code(),
            Some(2),
            "No config should exit with code 2"
        );
    }
}

/// --strict with warnings-only config exits with code 1.
#[test]
fn lint_strict_warnings_exit_code_nonzero() {
    let temp = setup_project(APP_NAME_SPACES_CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint", "--strict"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(
            output.status.code(),
            Some(1),
            "Strict mode with warnings should exit with code 1"
        );
    }
}

/// Without --strict, warnings-only config exits with code 0.
#[test]
fn lint_no_strict_warnings_exit_code_zero() {
    let temp = setup_project(APP_NAME_SPACES_CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(
            output.status.code(),
            Some(0),
            "Non-strict mode with only warnings should exit with code 0"
        );
    }
}

/// Self-dependency error exits with code 1.
#[test]
fn lint_self_dependency_exit_code_nonzero() {
    let temp = setup_project(SELF_DEPENDENCY_CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(
            output.status.code(),
            Some(1),
            "Self-dependency should exit with code 1"
        );
    }
}

/// Undefined dependency error exits with code 1.
#[test]
fn lint_undefined_dependency_exit_code_nonzero() {
    let temp = setup_project(MISSING_DEP_CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(
            output.status.code(),
            Some(1),
            "Undefined dependency should exit with code 1"
        );
    }
}

/// Undefined template error exits with code 1.
#[test]
fn lint_undefined_template_exit_code_nonzero() {
    let temp = setup_project(UNDEFINED_TEMPLATE_CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(
            output.status.code(),
            Some(1),
            "Undefined template should exit with code 1"
        );
    }
}

/// Environment default_workflow missing (error severity) exits with code 1.
#[test]
fn lint_env_default_workflow_missing_exit_code_nonzero() {
    let temp = setup_project(ENV_DEFAULT_WORKFLOW_MISSING_CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(
            output.status.code(),
            Some(1),
            "Missing environment default_workflow should exit with code 1"
        );
    }
}

/// Malformed YAML exits with code 1.
#[test]
fn lint_malformed_yaml_exit_code_nonzero() {
    let temp = setup_project("{{{{ not yaml :::");
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(
            output.status.code(),
            Some(1),
            "Malformed YAML should exit with code 1"
        );
    }
}

// =====================================================================
// EDGE CASES
// =====================================================================

/// Config with only steps and no workflows triggers required-fields warning.
#[test]
fn lint_steps_without_workflows() {
    let config = r#"
app_name: "NoWorkflow"
steps:
  hello:
    command: "git --version"
"#;
    let temp = setup_project(config);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains("warning[required-fields]: No workflows defined"),
        "Should warn about missing workflows with full message, got: {combined}"
    );
    assert!(
        output.status.success(),
        "Warning-only config should exit 0"
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "Warning-only config should exit with code 0"
    );
}

/// Config with built-in environment names in step overrides passes.
#[test]
fn lint_builtin_env_in_step_passes() {
    let config = r#"
app_name: "BuiltinEnv"
steps:
  build:
    command: "rustc --version"
    environments:
      ci:
        command: "cargo --version"
      docker:
        command: "git status --short"
workflows:
  default:
    steps: [build]
"#;
    let temp = setup_project(config);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // ci and docker are built-in, so no unknown-environment warnings
    assert!(
        !combined.contains("unknown environment"),
        "Built-in environments should not trigger unknown-environment warnings, got: {combined}"
    );
    // Positive assertion: config should be reported as valid
    assert!(
        combined.contains("Configuration is valid!"),
        "Config with built-in environments should be valid, got: {combined}"
    );
    assert!(
        output.status.success(),
        "Built-in environments should not cause errors"
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "Built-in environments config should exit with code 0"
    );
}

/// Config with defined custom environment in step overrides passes.
#[test]
fn lint_defined_custom_env_passes() {
    let config = r#"
app_name: "CustomEnv"
settings:
  environments:
    staging:
      detect:
        - env: STAGING
steps:
  build:
    command: "rustc --version"
    environments:
      staging:
        command: "git log --oneline -1"
workflows:
  default:
    steps: [build]
"#;
    let temp = setup_project(config);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // "staging" is defined in settings.environments, so no unknown-env warning
    assert!(
        !combined.contains("unknown environment"),
        "Defined custom environment should not be flagged as unknown, got: {combined}"
    );
    // Positive assertion: config should be reported as valid and exit 0
    assert!(
        combined.contains("Configuration is valid!"),
        "Config with defined custom environment should be valid, got: {combined}"
    );
    assert!(
        output.status.success(),
        "Defined custom env config should exit 0"
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "Defined custom env config should exit with code 0"
    );
}

/// Diamond dependency pattern (no cycle) passes lint.
#[test]
fn lint_diamond_dependency_pattern() {
    let config = r#"
app_name: "Diamond"
steps:
  a:
    command: "git --version"
    depends_on: [b, c]
  b:
    command: "cargo --version"
    depends_on: [d]
  c:
    command: "rustc --version"
    depends_on: [d]
  d:
    command: "cargo fmt --version"
workflows:
  default:
    steps: [d, b, c, a]
"#;
    let temp = setup_project(config);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains("Configuration is valid!"),
        "Diamond pattern should be valid, got: {combined}"
    );
    assert!(output.status.success(), "Diamond pattern should exit 0");
    assert_eq!(
        output.status.code(),
        Some(0),
        "Diamond pattern should exit with code 0"
    );
}

/// Config with multiple undefined dependencies at once.
#[test]
fn lint_multiple_undefined_dependencies() {
    let config = r#"
app_name: "MultiMissing"
steps:
  build:
    command: "rustc --version"
    depends_on: [phantom1, phantom2, phantom3]
workflows:
  default:
    steps: [build]
"#;
    let temp = setup_project(config);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // All three undefined dependencies should be flagged with full messages
    assert!(
        combined.contains("error[undefined-dependency]: Step 'build' depends on undefined step 'phantom1'"),
        "Should flag phantom1 with full message, got: {combined}"
    );
    assert!(
        combined.contains("error[undefined-dependency]: Step 'build' depends on undefined step 'phantom2'"),
        "Should flag phantom2 with full message, got: {combined}"
    );
    assert!(
        combined.contains("error[undefined-dependency]: Step 'build' depends on undefined step 'phantom3'"),
        "Should flag phantom3 with full message, got: {combined}"
    );
    assert!(!output.status.success(), "Undefined dependencies should exit non-zero");
    assert_eq!(
        output.status.code(),
        Some(1),
        "Undefined dependencies should exit with code 1"
    );
}

/// Config with both command and template on same step: no lint rule currently
/// flags this, so the config reports as valid with exit code 0. If a rule is
/// added later (e.g., command-and-template-ambiguous), update this test to
/// assert on the new diagnostic.
#[test]
fn lint_step_with_command_and_template() {
    let config = r#"
app_name: "BothCmdTpl"
steps:
  build:
    command: "rustc --version"
    template: "brew-bundle"
workflows:
  default:
    steps: [build]
"#;
    let temp = setup_project(config);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // No lint rule currently checks for the command+template combo; the config
    // should report as valid and exit 0.
    assert!(
        combined.contains("Configuration is valid!"),
        "Step with both command and template should pass lint (no rule covers it), got: {combined}"
    );
    assert!(
        output.status.success(),
        "Config without lint-rule coverage should exit 0"
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "Config without lint-rule coverage should exit with code 0"
    );
}

/// Config with known template reference (brew-bundle) passes the template check.
#[test]
fn lint_known_template_passes() {
    let config = r#"
app_name: "KnownTpl"
steps:
  setup:
    template: "brew-bundle"
workflows:
  default:
    steps: [setup]
"#;
    let temp = setup_project(config);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // brew-bundle is a real built-in template, so no undefined-template error
    assert!(
        !combined.contains("error[undefined-template]"),
        "Known template should not be flagged as undefined, got: {combined}"
    );
    // Positive assertion: config should be reported as valid
    assert!(
        combined.contains("Configuration is valid!"),
        "Config with known template should be valid, got: {combined}"
    );
    assert!(
        output.status.success(),
        "Known template should pass lint with exit 0"
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "Known template config should exit with code 0"
    );
}
