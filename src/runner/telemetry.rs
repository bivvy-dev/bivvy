//! Failure telemetry: scrubbing, consent, and local report storage.
//!
//! After a successful recovery (step eventually passes after retry, fix,
//! or shell), optionally prompts the user to share a scrubbed failure report.
//! Reports are stored locally as YAML files under `~/.bivvy/failure-reports/`.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::secrets::mask::OutputMasker;
use crate::secrets::pattern::SecretMatcher;
use crate::ui::{Prompt, PromptOption, PromptType, UserInterface};

/// Regex for scrubbing filesystem paths.
static PATH_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:/Users/[^\s:]+|/home/[^\s:]+|/tmp/[^\s:]+|C:\\Users\\[^\s:]+)")
        .expect("PATH_REGEX must compile")
});

/// Regex for scrubbing token-like strings (hex > 16 chars, base64 blobs, known prefixes).
static TOKEN_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:ghp_[A-Za-z0-9]{36,}|gho_[A-Za-z0-9]{36,}|sk-[A-Za-z0-9]{20,}|xoxb-[A-Za-z0-9\-]+|xoxp-[A-Za-z0-9\-]+|[A-Fa-f0-9]{32,}|[A-Za-z0-9+/]{40,}={0,2})")
        .expect("TOKEN_REGEX must compile")
});

/// Maximum length of scrubbed error output in a report.
const MAX_ERROR_LENGTH: usize = 500;

/// User's telemetry consent preference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TelemetryConsent {
    /// Send this one report.
    Send,
    /// Don't send this report.
    NoThanks,
    /// Always send (save preference).
    Always,
    /// Never ask again (save preference).
    Never,
}

impl fmt::Display for TelemetryConsent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TelemetryConsent::Send => write!(f, "send"),
            TelemetryConsent::NoThanks => write!(f, "no_thanks"),
            TelemetryConsent::Always => write!(f, "always"),
            TelemetryConsent::Never => write!(f, "never"),
        }
    }
}

/// A scrubbed failure report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureReport {
    /// Step name (e.g., "bundler").
    pub step_name: String,
    /// Scrubbed command (e.g., "bundle install").
    pub command_scrubbed: String,
    /// Process exit code.
    pub exit_code: Option<i32>,
    /// Scrubbed error output (first 500 chars).
    pub error_scrubbed: String,
    /// Recovery action taken (e.g., "retry", "shell → retry").
    pub recovery_action: String,
    /// Bivvy version.
    pub bivvy_version: String,
    /// Platform string (e.g., "macos-aarch64").
    pub platform: String,
}

/// Scrub sensitive data from text for telemetry.
///
/// 1. Masks known secret env var values using `OutputMasker`
/// 2. Replaces filesystem paths with `[PATH]`
/// 3. Replaces token-like strings with `[REDACTED]`
/// 4. Truncates to `MAX_ERROR_LENGTH` chars
pub fn scrub_for_telemetry(input: &str, step_env: &HashMap<String, String>) -> String {
    let matcher = SecretMatcher::with_builtins();
    let mut masker = OutputMasker::new();

    // Mask known secret env var values
    for (key, value) in step_env {
        if matcher.is_secret(key) && !value.is_empty() {
            masker.add_secret(value);
        }
    }

    let mut result = masker.mask(input);

    // Scrub filesystem paths
    result = PATH_REGEX.replace_all(&result, "[PATH]").to_string();

    // Scrub token-like strings
    result = TOKEN_REGEX.replace_all(&result, "[REDACTED]").to_string();

    // Truncate
    if result.len() > MAX_ERROR_LENGTH {
        result.truncate(MAX_ERROR_LENGTH);
        result.push_str("...[truncated]");
    }

    result
}

/// Build a `FailureReport` from step failure context.
pub fn build_report(
    step_name: &str,
    command: &str,
    exit_code: Option<i32>,
    error_output: &str,
    recovery_action: &str,
    step_env: &HashMap<String, String>,
) -> FailureReport {
    let platform = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);

    FailureReport {
        step_name: step_name.to_string(),
        command_scrubbed: scrub_for_telemetry(command, step_env),
        exit_code,
        error_scrubbed: scrub_for_telemetry(error_output, step_env),
        recovery_action: recovery_action.to_string(),
        bivvy_version: env!("CARGO_PKG_VERSION").to_string(),
        platform,
    }
}

/// Prompt the user for telemetry consent after a successful recovery.
///
/// Returns `None` if the preference is already saved (always/never).
/// Checks `preferences_other` for the `telemetry_failures` key.
pub fn prompt_telemetry(
    ui: &mut dyn UserInterface,
    report: &FailureReport,
    saved_preference: Option<&str>,
) -> Result<TelemetryConsent> {
    // Check saved preference
    match saved_preference {
        Some("always") => return Ok(TelemetryConsent::Always),
        Some("never") => return Ok(TelemetryConsent::Never),
        _ => {}
    }

    // Show the scrubbed report to the user
    ui.message("");
    ui.message("  Help bivvy get smarter?");
    ui.message("  We'd save this failure report to improve auto-recovery:");
    ui.message("");
    ui.message(&format!("    Step:    {}", report.step_name));
    ui.message(&format!("    Command: {}", report.command_scrubbed));
    if let Some(code) = report.exit_code {
        ui.message(&format!("    Exit:    {}", code));
    }
    if !report.error_scrubbed.is_empty() {
        ui.message(&format!("    Error:   {}", report.error_scrubbed));
    }
    ui.message(&format!("    Fix:     {}", report.recovery_action));
    ui.message("");
    ui.message("    NO env vars, paths, secrets, or project code — just the above.");
    ui.message("");

    let prompt = Prompt {
        key: "telemetry_failure".to_string(),
        question: "Save this failure report?".to_string(),
        prompt_type: PromptType::Select {
            options: vec![
                PromptOption {
                    label: "Send".to_string(),
                    value: "send".to_string(),
                },
                PromptOption {
                    label: "No thanks".to_string(),
                    value: "no_thanks".to_string(),
                },
                PromptOption {
                    label: "Always send".to_string(),
                    value: "always".to_string(),
                },
                PromptOption {
                    label: "Never ask".to_string(),
                    value: "never".to_string(),
                },
            ],
        },
        default: Some("no_thanks".to_string()),
    };

    let answer = ui.prompt(&prompt)?;
    match answer.as_string().as_str() {
        "send" => Ok(TelemetryConsent::Send),
        "always" => Ok(TelemetryConsent::Always),
        "never" => Ok(TelemetryConsent::Never),
        _ => Ok(TelemetryConsent::NoThanks),
    }
}

/// Get the failure reports directory path.
pub fn reports_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("~"))
        .join(".bivvy")
        .join("failure-reports")
}

/// Save a failure report to disk.
///
/// Reports are saved as timestamped YAML files in `~/.bivvy/failure-reports/`.
pub fn save_report(report: &FailureReport) -> Result<PathBuf> {
    save_report_to(report, &reports_dir())
}

/// Save a failure report to a specific directory (for testing).
pub fn save_report_to(report: &FailureReport, dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)?;

    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let filename = format!("{}-{}.yml", timestamp, report.step_name);
    let path = dir.join(filename);

    let content = serde_yaml::to_string(report).map_err(|e| {
        crate::error::BivvyError::ConfigValidationError {
            message: format!("Failed to serialize failure report: {}", e),
        }
    })?;

    std::fs::write(&path, content)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrub_removes_secret_env_values() {
        let mut env = HashMap::new();
        env.insert("API_KEY".to_string(), "super-secret-key-123".to_string());

        let input = "Error: failed with key super-secret-key-123";
        let scrubbed = scrub_for_telemetry(input, &env);
        assert!(
            !scrubbed.contains("super-secret-key-123"),
            "Secret should be masked: {}",
            scrubbed
        );
    }

    #[test]
    fn scrub_removes_filesystem_paths() {
        let input = "Error: file not found at /Users/brenna/project/src/main.rs";
        let scrubbed = scrub_for_telemetry(input, &HashMap::new());
        assert!(
            scrubbed.contains("[PATH]"),
            "Path should be replaced: {}",
            scrubbed
        );
        assert!(!scrubbed.contains("/Users/brenna"));
    }

    #[test]
    fn scrub_removes_tokens() {
        let input = "Error: auth failed with token ghp_abcdefghijklmnopqrstuvwxyz1234567890";
        let scrubbed = scrub_for_telemetry(input, &HashMap::new());
        assert!(
            scrubbed.contains("[REDACTED]"),
            "Token should be redacted: {}",
            scrubbed
        );
        assert!(!scrubbed.contains("ghp_"));
    }

    #[test]
    fn scrub_truncates_long_output() {
        // Use a word that won't be matched by the hex/token regex
        let long_input = "error ".repeat(200);
        let scrubbed = scrub_for_telemetry(&long_input, &HashMap::new());
        assert!(scrubbed.len() <= MAX_ERROR_LENGTH + 20); // +20 for the truncation message
        assert!(scrubbed.ends_with("...[truncated]"));
    }

    #[test]
    fn scrub_preserves_safe_text() {
        let input = "Error: bundle install failed with exit code 1";
        let scrubbed = scrub_for_telemetry(input, &HashMap::new());
        assert_eq!(scrubbed, input);
    }

    #[test]
    fn build_report_creates_correct_structure() {
        let report = build_report(
            "bundler",
            "bundle install",
            Some(1),
            "gem build error",
            "retry",
            &HashMap::new(),
        );

        assert_eq!(report.step_name, "bundler");
        assert_eq!(report.command_scrubbed, "bundle install");
        assert_eq!(report.exit_code, Some(1));
        assert_eq!(report.error_scrubbed, "gem build error");
        assert_eq!(report.recovery_action, "retry");
        assert!(!report.bivvy_version.is_empty());
        assert!(!report.platform.is_empty());
    }

    #[test]
    fn telemetry_prompt_not_shown_when_never() {
        let report = build_report("test", "echo test", Some(0), "", "retry", &HashMap::new());
        let mut ui = crate::ui::MockUI::new();

        let result = prompt_telemetry(&mut ui, &report, Some("never")).unwrap();
        assert_eq!(result, TelemetryConsent::Never);
        // No prompt should have been shown
        assert!(ui.prompts_shown().is_empty());
    }

    #[test]
    fn telemetry_auto_sends_when_always() {
        let report = build_report("test", "echo test", Some(0), "", "retry", &HashMap::new());
        let mut ui = crate::ui::MockUI::new();

        let result = prompt_telemetry(&mut ui, &report, Some("always")).unwrap();
        assert_eq!(result, TelemetryConsent::Always);
        assert!(ui.prompts_shown().is_empty());
    }

    #[test]
    fn telemetry_saves_report_to_disk() {
        let temp = tempfile::TempDir::new().unwrap();
        let report = build_report(
            "bundler",
            "bundle install",
            Some(1),
            "build error",
            "retry",
            &HashMap::new(),
        );

        let path = save_report_to(&report, temp.path()).unwrap();
        assert!(path.exists());

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("bundler"));
        assert!(content.contains("bundle install"));
    }

    #[test]
    fn failure_report_contains_no_paths() {
        let mut env = HashMap::new();
        env.insert("HOME".to_string(), "/Users/brenna".to_string());

        let report = build_report(
            "test",
            "make build",
            Some(1),
            "Error at /Users/brenna/project/main.c:42",
            "retry",
            &env,
        );

        assert!(!report.error_scrubbed.contains("/Users/brenna"));
        assert!(report.error_scrubbed.contains("[PATH]"));
    }

    #[test]
    fn telemetry_prompt_returns_send() {
        let report = build_report("test", "echo test", Some(0), "", "retry", &HashMap::new());
        let mut ui = crate::ui::MockUI::new();
        ui.set_interactive(true);
        ui.set_prompt_response("telemetry_failure", "send");

        let result = prompt_telemetry(&mut ui, &report, None).unwrap();
        assert_eq!(result, TelemetryConsent::Send);
    }

    #[test]
    fn telemetry_prompt_returns_no_thanks() {
        let report = build_report("test", "echo test", Some(0), "", "retry", &HashMap::new());
        let mut ui = crate::ui::MockUI::new();
        ui.set_interactive(true);
        ui.set_prompt_response("telemetry_failure", "no_thanks");

        let result = prompt_telemetry(&mut ui, &report, None).unwrap();
        assert_eq!(result, TelemetryConsent::NoThanks);
    }

    #[test]
    fn scrub_removes_sk_tokens() {
        let input = "key: sk-abcdefghij1234567890abcdef";
        let scrubbed = scrub_for_telemetry(input, &HashMap::new());
        assert!(scrubbed.contains("[REDACTED]"));
        assert!(!scrubbed.contains("sk-"));
    }

    #[test]
    fn scrub_removes_home_paths() {
        let input = "Error at /home/user/project/file.rs:42";
        let scrubbed = scrub_for_telemetry(input, &HashMap::new());
        assert!(scrubbed.contains("[PATH]"));
        assert!(!scrubbed.contains("/home/user"));
    }
}
