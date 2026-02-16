//! Requirement status types for gap detection.
//!
//! Each requirement check produces a `RequirementStatus` that describes
//! whether and how a tool or service is available on the system.

use std::path::PathBuf;

/// The result of checking a single requirement.
#[derive(Debug, Clone)]
pub enum RequirementStatus {
    /// Tool is present and managed properly (version manager, Homebrew, etc.)
    Satisfied,

    /// Tool binary exists but comes from a system/default install that's likely
    /// unsuitable (e.g., /usr/bin/ruby on macOS, /usr/bin/python3 on Ubuntu).
    /// The step can proceed but the user should be warned.
    SystemOnly {
        /// Path to the system binary
        path: PathBuf,
        /// Template to install a proper version
        install_template: Option<String>,
        /// Warning message for the user
        warning: String,
    },

    /// Tool binary exists but comes from a version manager that isn't activated
    /// in this shell context (e.g., nvm-managed Node not on PATH in a non-login shell).
    /// Bivvy can activate it by adding the binary's parent directory to PATH.
    Inactive {
        /// Name of the version manager (e.g., "rbenv", "nvm")
        manager: String,
        /// The resolved binary path from `<manager> which <tool>`.
        binary_path: PathBuf,
        /// Human-readable hint for activation (e.g., 'eval "$(rbenv init -)"')
        activation_hint: String,
    },

    /// For services: binary is present but the service isn't running or reachable.
    ServiceDown {
        /// Whether the service binary is installed
        binary_present: bool,
        /// Template to install the service
        install_template: Option<String>,
        /// Command to start the service (e.g., "brew services start postgresql@16").
        start_command: Option<String>,
        /// Human-readable hint shown if start_command is None or user declines.
        start_hint: String,
    },

    /// Tool is genuinely not installed anywhere.
    Missing {
        /// Template to install the tool
        install_template: Option<String>,
        /// Human-readable install instructions
        install_hint: Option<String>,
    },

    /// Requirement name isn't in the registry. Config error — lint should catch this.
    Unknown,
}

impl RequirementStatus {
    /// Whether the requirement is satisfied (tool available and ready).
    pub fn is_satisfied(&self) -> bool {
        matches!(self, RequirementStatus::Satisfied)
    }

    /// Whether the step can proceed (possibly with warnings).
    pub fn can_proceed(&self) -> bool {
        matches!(
            self,
            RequirementStatus::Satisfied | RequirementStatus::SystemOnly { .. }
        )
    }
}

/// The result of checking a single requirement for a step.
#[derive(Debug, Clone)]
pub struct GapResult {
    /// The requirement name that was checked
    pub requirement: String,
    /// The status of the requirement
    pub status: RequirementStatus,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn satisfied_is_satisfied() {
        let status = RequirementStatus::Satisfied;
        assert!(status.is_satisfied());
        assert!(status.can_proceed());
    }

    #[test]
    fn system_only_can_proceed_but_not_satisfied() {
        let status = RequirementStatus::SystemOnly {
            path: PathBuf::from("/usr/bin/ruby"),
            install_template: Some("ruby-install".to_string()),
            warning: "System Ruby detected".to_string(),
        };
        assert!(!status.is_satisfied());
        assert!(status.can_proceed());
    }

    #[test]
    fn inactive_cannot_proceed() {
        let status = RequirementStatus::Inactive {
            manager: "rbenv".to_string(),
            binary_path: PathBuf::from("/home/user/.rbenv/versions/3.2.2/bin/ruby"),
            activation_hint: r#"eval "$(rbenv init -)""#.to_string(),
        };
        assert!(!status.is_satisfied());
        assert!(!status.can_proceed());
    }

    #[test]
    fn service_down_cannot_proceed() {
        let status = RequirementStatus::ServiceDown {
            binary_present: true,
            install_template: None,
            start_command: Some("brew services start postgresql@16".to_string()),
            start_hint: "Start PostgreSQL with: brew services start postgresql@16".to_string(),
        };
        assert!(!status.is_satisfied());
        assert!(!status.can_proceed());
    }

    #[test]
    fn missing_cannot_proceed() {
        let status = RequirementStatus::Missing {
            install_template: Some("mise-ruby".to_string()),
            install_hint: Some("Install Ruby via mise".to_string()),
        };
        assert!(!status.is_satisfied());
        assert!(!status.can_proceed());
    }

    #[test]
    fn unknown_cannot_proceed() {
        let status = RequirementStatus::Unknown;
        assert!(!status.is_satisfied());
        assert!(!status.can_proceed());
    }

    #[test]
    fn gap_result_holds_requirement_and_status() {
        let result = GapResult {
            requirement: "ruby".to_string(),
            status: RequirementStatus::Satisfied,
        };
        assert_eq!(result.requirement, "ruby");
        assert!(result.status.is_satisfied());
    }

    #[test]
    fn system_only_fields_accessible() {
        let status = RequirementStatus::SystemOnly {
            path: PathBuf::from("/usr/bin/python3"),
            install_template: None,
            warning: "System Python detected".to_string(),
        };
        if let RequirementStatus::SystemOnly {
            path,
            install_template,
            warning,
        } = &status
        {
            assert_eq!(path, &PathBuf::from("/usr/bin/python3"));
            assert!(install_template.is_none());
            assert!(warning.contains("System Python"));
        } else {
            panic!("Expected SystemOnly");
        }
    }

    #[test]
    fn service_down_without_start_command() {
        let status = RequirementStatus::ServiceDown {
            binary_present: false,
            install_template: Some("postgres-install".to_string()),
            start_command: None,
            start_hint: "Install and start PostgreSQL".to_string(),
        };
        if let RequirementStatus::ServiceDown {
            binary_present,
            start_command,
            ..
        } = &status
        {
            assert!(!binary_present);
            assert!(start_command.is_none());
        } else {
            panic!("Expected ServiceDown");
        }
    }

    #[test]
    fn missing_with_no_hints() {
        let status = RequirementStatus::Missing {
            install_template: None,
            install_hint: None,
        };
        if let RequirementStatus::Missing {
            install_template,
            install_hint,
        } = &status
        {
            assert!(install_template.is_none());
            assert!(install_hint.is_none());
        } else {
            panic!("Expected Missing");
        }
    }
}
