//! Command-based detection.

use std::path::Path;
use std::process::Command;

use super::types::{Detection, DetectionKind, DetectionResult};

/// Detects based on command success.
pub struct CommandDetector {
    name: String,
    command: String,
    args: Vec<String>,
}

impl CommandDetector {
    /// Create a new command detector.
    pub fn new(name: &str, command: &str) -> Self {
        Self {
            name: name.to_string(),
            command: command.to_string(),
            args: Vec::new(),
        }
    }

    /// Create from a full command string.
    pub fn from_string(name: &str, cmd: &str) -> Self {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        let (command, args) = if parts.is_empty() {
            (cmd.to_string(), Vec::new())
        } else {
            (
                parts[0].to_string(),
                parts[1..].iter().map(|s| s.to_string()).collect(),
            )
        };

        Self {
            name: name.to_string(),
            command,
            args,
        }
    }

    /// Add arguments to the command.
    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }
}

impl Detection for CommandDetector {
    fn name(&self) -> &str {
        &self.name
    }

    fn detect(&self, _project_root: &Path) -> DetectionResult {
        match Command::new(&self.command).args(&self.args).output() {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let version = extract_version(&stdout);

                let mut result =
                    DetectionResult::found(&self.name).with_kind(DetectionKind::CommandSucceeds(
                        format!("{} {}", self.command, self.args.join(" ")),
                    ));

                if let Some(v) = version {
                    result = result.with_detail(&format!("Version: {}", v));
                }

                result
            }
            _ => DetectionResult::not_found(&self.name),
        }
    }
}

/// Check if a command succeeds.
pub fn command_succeeds(command: &str) -> bool {
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        return false;
    }

    Command::new(parts[0])
        .args(&parts[1..])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Extract version from command output.
fn extract_version(output: &str) -> Option<String> {
    let patterns = [r"(\d+\.\d+\.\d+)", r"version\s+(\d+\.\d+)", r"v(\d+\.\d+)"];

    for pattern in &patterns {
        if let Ok(re) = regex::Regex::new(pattern) {
            if let Some(caps) = re.captures(output) {
                if let Some(m) = caps.get(1) {
                    return Some(m.as_str().to_string());
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn command_detector_not_found() {
        let temp = TempDir::new().unwrap();

        let detector = CommandDetector::new("nonexistent", "this-command-does-not-exist-12345");

        let result = detector.detect(temp.path());
        assert!(!result.detected);
    }

    #[test]
    fn command_detector_from_string() {
        let detector = CommandDetector::from_string("ruby", "ruby --version");

        assert_eq!(detector.command, "ruby");
        assert_eq!(detector.args, vec!["--version"]);
    }

    #[test]
    fn command_succeeds_helper_false() {
        assert!(!command_succeeds("this-command-does-not-exist-12345"));
    }

    #[test]
    fn extract_version_semver() {
        let output = "ruby 3.2.1 (2023-02-08 revision 31819e82c8)";
        let version = extract_version(output);
        assert_eq!(version, Some("3.2.1".to_string()));
    }

    #[test]
    fn extract_version_with_v() {
        let output = "v18.17.0";
        let version = extract_version(output);
        assert_eq!(version, Some("18.17.0".to_string()));
    }

    #[test]
    fn extract_version_no_match() {
        let output = "no version here";
        let version = extract_version(output);
        assert!(version.is_none());
    }
}
