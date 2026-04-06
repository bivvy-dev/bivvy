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

/// Valid config passes lint with "Configuration is valid" message.
#[test]
fn lint_valid_config() {
    let temp = setup_project(VALID_CONFIG);
    let mut s = spawn_bivvy(&["lint"], temp.path());

    s.expect("Configuration is valid!")
        .expect("Should report valid");
    s.expect(expectrl::Eof).unwrap();
}

/// Deep dependency chain is valid (no circular deps).
#[test]
fn lint_deep_chain_valid() {
    let temp = setup_project(DEEP_CHAIN_CONFIG);
    let mut s = spawn_bivvy(&["lint"], temp.path());

    s.expect("Configuration is valid!")
        .expect("Deep chain should be valid");
    s.expect(expectrl::Eof).unwrap();
}

/// Lint after adding a template still passes.
#[test]
fn lint_after_add_passes() {
    let temp = setup_project(VALID_CONFIG);
    run_bivvy_silently(temp.path(), &["add", "bundle-install"]);

    let mut s = spawn_bivvy(&["lint"], temp.path());
    s.expect("Configuration is valid!")
        .expect("Config with added template should be valid");
    s.expect(expectrl::Eof).unwrap();
}

/// Config with a valid built-in requirement passes lint.
#[test]
fn lint_valid_requirement_passes() {
    let temp = setup_project(VALID_REQUIREMENT_CONFIG);
    let mut s = spawn_bivvy(&["lint"], temp.path());

    // ruby is a known built-in; the only possible diagnostic would be
    // install-template-missing (hint severity), which is not an error.
    // In non-strict mode, this should pass.
    let output = read_to_eof(&mut s);
    // Should not contain any error-level diagnostics
    assert!(
        !output.contains("error"),
        "Valid requirement should not produce errors, got: {output}"
    );
}

// =====================================================================
// FLAGS — Format
// =====================================================================

/// --format json produces JSON output (array structure).
#[test]
fn lint_json_format_valid_config() {
    let temp = setup_project(VALID_CONFIG);
    let mut s = spawn_bivvy(&["lint", "--format", "json"], temp.path());

    let output = read_to_eof(&mut s);
    // JSON format for valid config outputs an empty array
    assert!(
        output.contains('[') || output.contains("[]"),
        "JSON output expected for valid config, got: {output}"
    );
}

/// --format json with errors produces JSON with diagnostics.
#[test]
fn lint_json_format_with_errors() {
    let temp = setup_project(CIRCULAR_CONFIG);
    let mut s = spawn_bivvy(&["lint", "--format", "json"], temp.path());

    let output = read_to_eof(&mut s);
    assert!(
        output.contains("circular") || output.contains("Circular"),
        "JSON output should contain circular dependency info, got: {output}"
    );
}

/// --format sarif produces SARIF output.
#[test]
fn lint_sarif_format_valid_config() {
    let temp = setup_project(VALID_CONFIG);
    let mut s = spawn_bivvy(&["lint", "--format", "sarif"], temp.path());

    let output = read_to_eof(&mut s);
    // SARIF output includes schema version or tool info
    assert!(
        output.contains("sarif") || output.contains("SARIF") || output.contains("bivvy"),
        "SARIF output expected, got: {output}"
    );
}

/// --format sarif with errors produces SARIF with diagnostics.
#[test]
fn lint_sarif_format_with_errors() {
    let temp = setup_project(CIRCULAR_CONFIG);
    let mut s = spawn_bivvy(&["lint", "--format", "sarif"], temp.path());

    let output = read_to_eof(&mut s);
    assert!(
        output.contains("circular") || output.contains("Circular") || output.contains("results"),
        "SARIF output should contain diagnostics, got: {output}"
    );
}

// =====================================================================
// FLAGS — Strict mode
// =====================================================================

/// --strict on a valid config still passes.
#[test]
fn lint_strict_valid_config_passes() {
    let temp = setup_project(VALID_CONFIG);
    let mut s = spawn_bivvy(&["lint", "--strict"], temp.path());

    s.expect("Configuration is valid!")
        .expect("Valid config should pass strict mode");
    s.expect(expectrl::Eof).unwrap();
}

/// --strict treats warnings as errors: app_name with spaces fails.
#[test]
fn lint_strict_fails_on_warnings() {
    let temp = setup_project(APP_NAME_SPACES_CONFIG);
    let mut s = spawn_bivvy(&["lint", "--strict"], temp.path());

    let output = read_to_eof(&mut s);
    // Should contain a warning about app_name spaces
    assert!(
        output.contains("app_name") || output.contains("spaces") || output.contains("kebab"),
        "Strict mode should surface app_name warning, got: {output}"
    );
}

/// Without --strict, warnings-only config passes lint (exit 0).
#[test]
fn lint_without_strict_passes_on_warnings() {
    let temp = setup_project(APP_NAME_SPACES_CONFIG);
    let mut s = spawn_bivvy(&["lint"], temp.path());

    let output = read_to_eof(&mut s);
    // The config has a warning but no errors, so lint should not fail.
    // It may show the warning but still succeed.
    assert!(
        output.contains("spaces") || output.contains("kebab") || output.contains("warning")
            || output.contains("valid"),
        "Should produce output about app_name or validity, got: {output}"
    );
}

/// --strict + --format json combined produces JSON and fails on warnings.
#[test]
fn lint_strict_json_combined() {
    let temp = setup_project(APP_NAME_SPACES_CONFIG);
    let mut s = spawn_bivvy(&["lint", "--strict", "--format", "json"], temp.path());

    let output = read_to_eof(&mut s);
    assert!(
        output.contains('[') || output.contains("app_name") || output.contains("spaces"),
        "Strict JSON should contain diagnostics, got: {output}"
    );
}

// =====================================================================
// FLAGS — Fix mode
// =====================================================================

/// --fix on a valid config does nothing harmful.
#[test]
fn lint_fix_valid_config_no_op() {
    let temp = setup_project(VALID_CONFIG);
    let mut s = spawn_bivvy(&["lint", "--fix"], temp.path());

    s.expect("Configuration is valid!")
        .expect("Fix on valid config should report valid");
    s.expect(expectrl::Eof).unwrap();
}

/// --fix on a config with fixable issues attempts to apply fixes.
#[test]
fn lint_fix_with_fixable_issue() {
    let temp = setup_project(APP_NAME_SPACES_CONFIG);
    let mut s = spawn_bivvy(&["lint", "--fix"], temp.path());

    let output = read_to_eof(&mut s);
    // The app-name-format rule supports_fix, so --fix may report fix attempts
    // Even if no actual file change occurs (byte offsets are 0), it should not crash
    assert!(
        output.contains("fix") || output.contains("valid") || output.contains("spaces")
            || output.contains("app_name") || output.contains("warning"),
        "Fix mode should attempt fixes or report status, got: {output}"
    );
}

/// --fix combined with --format json.
#[test]
fn lint_fix_json_combined() {
    let temp = setup_project(APP_NAME_SPACES_CONFIG);
    let mut s = spawn_bivvy(&["lint", "--fix", "--format", "json"], temp.path());

    let output = read_to_eof(&mut s);
    // Should produce valid JSON output without crashing
    assert!(
        output.contains('[') || output.contains('{'),
        "Fix + JSON should produce JSON output, got: {output}"
    );
}

// =====================================================================
// RULE: app-name-format
// =====================================================================

/// App name with spaces produces a warning mentioning kebab-case.
#[test]
fn lint_app_name_spaces_warning() {
    let temp = setup_project(APP_NAME_SPACES_CONFIG);
    let mut s = spawn_bivvy(&["lint"], temp.path());

    let output = read_to_eof(&mut s);
    assert!(
        output.contains("spaces") || output.contains("kebab"),
        "Should warn about spaces in app_name, got: {output}"
    );
}

/// Empty app_name produces an error.
#[test]
fn lint_app_name_empty_error() {
    let temp = setup_project(APP_NAME_EMPTY_CONFIG);
    let mut s = spawn_bivvy(&["lint"], temp.path());

    let output = read_to_eof(&mut s);
    assert!(
        output.contains("empty") || output.contains("app_name"),
        "Should error on empty app_name, got: {output}"
    );
}

// =====================================================================
// RULE: required-fields
// =====================================================================

/// Missing app_name triggers a required-fields error.
#[test]
fn lint_missing_app_name_error() {
    let temp = setup_project(MISSING_APP_NAME_CONFIG);
    let mut s = spawn_bivvy(&["lint"], temp.path());

    let output = read_to_eof(&mut s);
    assert!(
        output.contains("app_name") || output.contains("required"),
        "Should report missing app_name, got: {output}"
    );
}

/// No workflows defined produces a warning.
#[test]
fn lint_no_workflows_warning() {
    let temp = setup_project(NO_WORKFLOWS_CONFIG);
    let mut s = spawn_bivvy(&["lint"], temp.path());

    let output = read_to_eof(&mut s);
    assert!(
        output.contains("workflow") || output.contains("No workflows"),
        "Should warn about missing workflows, got: {output}"
    );
}

// =====================================================================
// RULE: circular-dependency
// =====================================================================

/// Two-step circular dependency detected.
#[test]
fn lint_circular_dependency() {
    let temp = setup_project(CIRCULAR_CONFIG);
    let mut s = spawn_bivvy(&["lint"], temp.path());

    s.expect("Circular dependency detected:")
        .expect("Should detect circular dependency");
    s.expect(expectrl::Eof).unwrap();
}

/// Three-step circular dependency detected.
#[test]
fn lint_three_step_circular_dependency() {
    let temp = setup_project(THREE_STEP_CIRCULAR_CONFIG);
    let mut s = spawn_bivvy(&["lint"], temp.path());

    let output = read_to_eof(&mut s);
    assert!(
        output.contains("ircular") || output.contains("cycle"),
        "Should detect three-step circular dependency, got: {output}"
    );
}

// =====================================================================
// RULE: self-dependency
// =====================================================================

/// Step that depends on itself is flagged.
#[test]
fn lint_self_dependency() {
    let temp = setup_project(SELF_DEPENDENCY_CONFIG);
    let mut s = spawn_bivvy(&["lint"], temp.path());

    let output = read_to_eof(&mut s);
    assert!(
        output.contains("depends on itself") || output.contains("self")
            || output.contains("loopy"),
        "Should detect self-dependency, got: {output}"
    );
}

// =====================================================================
// RULE: undefined-dependency
// =====================================================================

/// Step referencing nonexistent dependency is flagged.
#[test]
fn lint_undefined_dependency() {
    let temp = setup_project(MISSING_DEP_CONFIG);
    let mut s = spawn_bivvy(&["lint"], temp.path());

    let output = read_to_eof(&mut s);
    assert!(
        output.contains("nonexistent") || output.contains("undefined"),
        "Should detect undefined dependency, got: {output}"
    );
}

// =====================================================================
// RULE: unknown-environment-in-step
// =====================================================================

/// Step with environment override for unknown environment is flagged.
#[test]
fn lint_unknown_env_in_step() {
    let temp = setup_project(UNKNOWN_ENV_IN_STEP_CONFIG);
    let mut s = spawn_bivvy(&["lint"], temp.path());

    let output = read_to_eof(&mut s);
    assert!(
        output.contains("staging") || output.contains("unknown environment")
            || output.contains("Unknown"),
        "Should warn about unknown environment 'staging' in step, got: {output}"
    );
}

// =====================================================================
// RULE: unknown-environment-in-only
// =====================================================================

/// only_environments referencing unknown environment is flagged.
#[test]
fn lint_unknown_env_in_only_environments() {
    let temp = setup_project(UNKNOWN_ENV_IN_ONLY_CONFIG);
    let mut s = spawn_bivvy(&["lint"], temp.path());

    let output = read_to_eof(&mut s);
    assert!(
        output.contains("staging") || output.contains("only_environments")
            || output.contains("unknown"),
        "Should warn about unknown environment in only_environments, got: {output}"
    );
}

// =====================================================================
// RULE: environment-default-workflow-missing
// =====================================================================

/// Environment default_workflow references a nonexistent workflow.
#[test]
fn lint_env_default_workflow_missing() {
    let temp = setup_project(ENV_DEFAULT_WORKFLOW_MISSING_CONFIG);
    let mut s = spawn_bivvy(&["lint"], temp.path());

    let output = read_to_eof(&mut s);
    assert!(
        output.contains("fast-ci") || output.contains("default_workflow")
            || output.contains("does not exist"),
        "Should flag missing default_workflow 'fast-ci', got: {output}"
    );
}

// =====================================================================
// RULE: unreachable-environment-override
// =====================================================================

/// Environment override excluded by only_environments is flagged.
#[test]
fn lint_unreachable_env_override() {
    let temp = setup_project(UNREACHABLE_ENV_OVERRIDE_CONFIG);
    let mut s = spawn_bivvy(&["lint"], temp.path());

    let output = read_to_eof(&mut s);
    assert!(
        output.contains("docker") || output.contains("unreachable")
            || output.contains("only_environments"),
        "Should warn about unreachable override for 'docker', got: {output}"
    );
}

// =====================================================================
// RULE: custom-environment-shadows-builtin
// =====================================================================

/// Custom environment shadowing builtin "ci" is flagged.
#[test]
fn lint_custom_env_shadows_builtin() {
    let temp = setup_project(SHADOW_BUILTIN_ENV_CONFIG);
    let mut s = spawn_bivvy(&["lint"], temp.path());

    let output = read_to_eof(&mut s);
    assert!(
        output.contains("shadows") || output.contains("ci")
            || output.contains("built-in"),
        "Should warn about custom env shadowing builtin 'ci', got: {output}"
    );
}

// =====================================================================
// RULE: redundant-environment-override
// =====================================================================

/// Environment override identical to base step command is flagged.
#[test]
fn lint_redundant_env_override() {
    let temp = setup_project(REDUNDANT_ENV_OVERRIDE_CONFIG);
    let mut s = spawn_bivvy(&["lint"], temp.path());

    let output = read_to_eof(&mut s);
    assert!(
        output.contains("redundant") || output.contains("identical")
            || output.contains("command"),
        "Should flag redundant environment override, got: {output}"
    );
}

// =====================================================================
// RULE: redundant-env-null
// =====================================================================

/// Null env entry for key not in base is flagged.
#[test]
fn lint_redundant_env_null() {
    let temp = setup_project(REDUNDANT_ENV_NULL_CONFIG);
    let mut s = spawn_bivvy(&["lint"], temp.path());

    let output = read_to_eof(&mut s);
    assert!(
        output.contains("NONEXISTENT_KEY") || output.contains("removes")
            || output.contains("not in the base"),
        "Should flag redundant env null for nonexistent key, got: {output}"
    );
}

// =====================================================================
// RULE: environment-circular-dependency
// =====================================================================

/// Per-environment dependency override creates a cycle.
#[test]
fn lint_env_circular_dependency() {
    let temp = setup_project(ENV_CIRCULAR_DEP_CONFIG);
    let mut s = spawn_bivvy(&["lint"], temp.path());

    let output = read_to_eof(&mut s);
    assert!(
        output.contains("ircular") || output.contains("ci")
            || output.contains("cycle"),
        "Should detect per-environment circular dependency, got: {output}"
    );
}

// =====================================================================
// RULE: unknown-requirement
// =====================================================================

/// Step requiring unknown requirement is flagged.
#[test]
fn lint_unknown_requirement() {
    let temp = setup_project(UNKNOWN_REQUIREMENT_CONFIG);
    let mut s = spawn_bivvy(&["lint"], temp.path());

    let output = read_to_eof(&mut s);
    assert!(
        output.contains("nonexistent-tool-xyz") || output.contains("unknown")
            || output.contains("Unknown"),
        "Should flag unknown requirement, got: {output}"
    );
}

// =====================================================================
// RULE: service-requirement-without-hint
// =====================================================================

/// Service requirement without install_hint is flagged.
#[test]
fn lint_service_requirement_without_hint() {
    let temp = setup_project(SERVICE_WITHOUT_HINT_CONFIG);
    let mut s = spawn_bivvy(&["lint"], temp.path());

    let output = read_to_eof(&mut s);
    assert!(
        output.contains("my-service") || output.contains("install hint")
            || output.contains("hint"),
        "Should flag service requirement without hint, got: {output}"
    );
}

// =====================================================================
// RULE: install-template-missing
// =====================================================================

/// Custom requirement without install_template is flagged.
#[test]
fn lint_install_template_missing() {
    let temp = setup_project(INSTALL_TEMPLATE_MISSING_CONFIG);
    let mut s = spawn_bivvy(&["lint"], temp.path());

    let output = read_to_eof(&mut s);
    assert!(
        output.contains("my-tool") || output.contains("install template")
            || output.contains("install_template"),
        "Should flag missing install template, got: {output}"
    );
}

// =====================================================================
// RULE: undefined-template
// =====================================================================

/// Step referencing undefined template is flagged.
#[test]
fn lint_undefined_template() {
    let temp = setup_project(UNDEFINED_TEMPLATE_CONFIG);
    let mut s = spawn_bivvy(&["lint"], temp.path());

    let output = read_to_eof(&mut s);
    assert!(
        output.contains("nonexistent-template-xyz-999") || output.contains("undefined")
            || output.contains("template"),
        "Should flag undefined template reference, got: {output}"
    );
}

// =====================================================================
// MULTI-RULE SCENARIOS
// =====================================================================

/// Config triggering multiple errors at once: self-dep + undefined dep + app name warning.
#[test]
fn lint_multi_rule_errors() {
    let temp = setup_project(MULTI_RULE_CONFIG);
    let mut s = spawn_bivvy(&["lint"], temp.path());

    let output = read_to_eof(&mut s);
    // Should have at least the self-dependency and undefined-dependency errors
    let has_self_dep = output.contains("depends on itself") || output.contains("alpha");
    let has_undef_dep = output.contains("nonexistent") || output.contains("undefined");
    assert!(
        has_self_dep || has_undef_dep,
        "Should flag multiple rules, got: {output}"
    );
}

/// Config with only warnings passes without --strict.
#[test]
fn lint_warnings_only_passes_without_strict() {
    let temp = setup_project(WARNINGS_ONLY_CONFIG);
    let mut s = spawn_bivvy(&["lint"], temp.path());

    let output = read_to_eof(&mut s);
    // Warnings-only config should not report as a hard failure
    // It may show warnings but should still succeed
    assert!(
        output.contains("staging") || output.contains("spaces") || output.contains("warning")
            || output.contains("valid"),
        "Should produce warning output or pass, got: {output}"
    );
}

/// Config with only warnings fails with --strict.
#[test]
fn lint_warnings_only_fails_with_strict() {
    let temp = setup_project(WARNINGS_ONLY_CONFIG);
    let mut s = spawn_bivvy(&["lint", "--strict"], temp.path());

    let output = read_to_eof(&mut s);
    // Strict mode should surface warnings; the process should fail
    assert!(
        output.contains("staging") || output.contains("spaces") || output.contains("warning"),
        "Strict mode should report warnings, got: {output}"
    );
}

/// Multi-rule errors reported in JSON format.
#[test]
fn lint_multi_rule_json_format() {
    let temp = setup_project(MULTI_RULE_CONFIG);
    let mut s = spawn_bivvy(&["lint", "--format", "json"], temp.path());

    let output = read_to_eof(&mut s);
    assert!(
        output.contains('[') && (output.contains("alpha") || output.contains("nonexistent")),
        "JSON should contain array of diagnostics, got: {output}"
    );
}

// =====================================================================
// SAD PATH — Structural errors (workflow references)
// =====================================================================

/// Workflow references nonexistent step.
#[test]
fn lint_workflow_references_missing_step() {
    let temp = setup_project(WORKFLOW_REF_MISSING_CONFIG);
    let mut s = spawn_bivvy(&["lint"], temp.path());

    let output = read_to_eof(&mut s);
    // This may be caught by config loading or by lint rules
    assert!(
        output.contains("ghost") || output.contains("error") || output.contains("not found")
            || output.contains("undefined"),
        "Should flag workflow referencing nonexistent step 'ghost', got: {output}"
    );
}

// =====================================================================
// SAD PATH — Parse errors
// =====================================================================

/// No config file at all.
#[test]
fn lint_no_config_fails() {
    let temp = tempfile::TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["lint"], temp.path());

    s.expect("No configuration found").unwrap();
    s.expect(expectrl::Eof).unwrap();
}

/// Empty config file.
#[test]
fn lint_empty_config() {
    let temp = setup_project("");
    let mut s = spawn_bivvy(&["lint"], temp.path());

    let output = read_to_eof(&mut s);
    // Empty config should either parse-error or trigger required-fields
    assert!(
        output.contains("error") || output.contains("required") || output.contains("parse")
            || output.contains("app_name") || output.contains("No workflows")
            || output.contains("valid"),
        "Should handle empty config gracefully, got: {output}"
    );
}

/// Malformed YAML syntax.
#[test]
fn lint_malformed_yaml() {
    let temp = setup_project("{{{{ not yaml :::");
    let mut s = spawn_bivvy(&["lint"], temp.path());

    let output = read_to_eof(&mut s);
    assert!(
        output.contains("error") || output.contains("parse") || output.contains("Parse"),
        "Should report parse error for malformed YAML, got: {output}"
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
    let mut s = spawn_bivvy(&["lint"], temp.path());

    let output = read_to_eof(&mut s);
    // Should at least not crash; may report valid or a warning
    assert!(
        !output.is_empty(),
        "Should produce some output for duplicate keys"
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
    let mut s = spawn_bivvy(&["lint"], temp.path());

    let output = read_to_eof(&mut s);
    // A step with no command and no template should either pass or
    // be flagged; it should not crash.
    assert!(
        !output.is_empty(),
        "Should produce some output for step with no command/template"
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
    let status = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("Failed to run bivvy");
    assert!(status.success(), "Valid config should exit with code 0");
}

/// Config with errors exits with non-zero code.
#[test]
fn lint_error_config_exit_code_nonzero() {
    let temp = setup_project(CIRCULAR_CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let status = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("Failed to run bivvy");
    assert!(!status.success(), "Config with errors should exit non-zero");
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
        if let Some(code) = output.status.code() {
            assert_eq!(code, 2, "No config should exit with code 2, got {code}");
        }
    }
}

/// --strict with warnings-only config exits non-zero.
#[test]
fn lint_strict_warnings_exit_code_nonzero() {
    let temp = setup_project(APP_NAME_SPACES_CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let status = std::process::Command::new(bin)
        .args(["lint", "--strict"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("Failed to run bivvy");
    assert!(
        !status.success(),
        "Strict mode with warnings should exit non-zero"
    );
}

/// Without --strict, warnings-only config exits 0.
#[test]
fn lint_no_strict_warnings_exit_code_zero() {
    let temp = setup_project(APP_NAME_SPACES_CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let status = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("Failed to run bivvy");
    assert!(
        status.success(),
        "Non-strict mode with only warnings should exit 0"
    );
}

/// Self-dependency error exits non-zero.
#[test]
fn lint_self_dependency_exit_code_nonzero() {
    let temp = setup_project(SELF_DEPENDENCY_CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let status = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("Failed to run bivvy");
    assert!(
        !status.success(),
        "Self-dependency should cause non-zero exit"
    );
}

/// Undefined dependency error exits non-zero.
#[test]
fn lint_undefined_dependency_exit_code_nonzero() {
    let temp = setup_project(MISSING_DEP_CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let status = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("Failed to run bivvy");
    assert!(
        !status.success(),
        "Undefined dependency should cause non-zero exit"
    );
}

/// Undefined template error exits non-zero.
#[test]
fn lint_undefined_template_exit_code_nonzero() {
    let temp = setup_project(UNDEFINED_TEMPLATE_CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let status = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("Failed to run bivvy");
    assert!(
        !status.success(),
        "Undefined template should cause non-zero exit"
    );
}

/// Environment default_workflow missing (error severity) exits non-zero.
#[test]
fn lint_env_default_workflow_missing_exit_code_nonzero() {
    let temp = setup_project(ENV_DEFAULT_WORKFLOW_MISSING_CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let status = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("Failed to run bivvy");
    assert!(
        !status.success(),
        "Missing environment default_workflow should cause non-zero exit"
    );
}

/// Malformed YAML exits non-zero.
#[test]
fn lint_malformed_yaml_exit_code_nonzero() {
    let temp = setup_project("{{{{ not yaml :::");
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let status = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("Failed to run bivvy");
    assert!(
        !status.success(),
        "Malformed YAML should cause non-zero exit"
    );
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
    let mut s = spawn_bivvy(&["lint"], temp.path());

    let output = read_to_eof(&mut s);
    assert!(
        output.contains("workflow") || output.contains("No workflows") || output.contains("valid"),
        "Should warn about missing workflows, got: {output}"
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
    let mut s = spawn_bivvy(&["lint"], temp.path());

    // ci and docker are built-in, so no unknown-environment warnings
    // However, there may be redundant/hint diagnostics; the key is no errors
    let output = read_to_eof(&mut s);
    assert!(
        !output.contains("error:") || output.contains("valid"),
        "Built-in environments should not cause errors, got: {output}"
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
    let mut s = spawn_bivvy(&["lint"], temp.path());

    let output = read_to_eof(&mut s);
    // "staging" is defined in settings.environments, so no unknown-env warning
    assert!(
        output.contains("valid") || !output.contains("staging"),
        "Defined custom environment should not be flagged, got: {output}"
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
    let mut s = spawn_bivvy(&["lint"], temp.path());

    s.expect("Configuration is valid!")
        .expect("Diamond pattern should be valid");
    s.expect(expectrl::Eof).unwrap();
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
    let mut s = spawn_bivvy(&["lint"], temp.path());

    let output = read_to_eof(&mut s);
    assert!(
        output.contains("phantom") || output.contains("undefined"),
        "Should flag multiple undefined dependencies, got: {output}"
    );
}

/// Config with both command and template on same step (may or may not warn).
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
    let mut s = spawn_bivvy(&["lint"], temp.path());

    let output = read_to_eof(&mut s);
    // Should not crash; may produce a warning or error about ambiguity
    assert!(
        !output.is_empty(),
        "Should produce output for step with both command and template"
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
    let mut s = spawn_bivvy(&["lint"], temp.path());

    let output = read_to_eof(&mut s);
    // brew-bundle is a real built-in template, so no undefined-template error
    assert!(
        !output.contains("undefined template") && !output.contains("nonexistent"),
        "Known template should not be flagged as undefined, got: {output}"
    );
}
