//! Lint rule definitions.
//!
//! This module provides the core traits and types for defining lint rules:
//!
//! - [`LintRule`] - The trait that all lint rules must implement
//! - [`RuleId`] - Unique identifier for a lint rule
//! - [`Severity`] - Severity level for diagnostics (Hint, Warning, Error)

use super::diagnostic::LintDiagnostic;
use crate::config::BivvyConfig;

/// Unique identifier for a lint rule.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RuleId(pub String);

impl RuleId {
    /// Create a new rule ID.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for RuleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Severity level for lint diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Informational hint, does not affect validity.
    Hint,
    /// Warning that should be addressed.
    Warning,
    /// Error that prevents execution.
    Error,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Hint => write!(f, "hint"),
            Severity::Warning => write!(f, "warning"),
            Severity::Error => write!(f, "error"),
        }
    }
}

/// A lint rule that validates configuration.
///
/// Lint rules are the primary mechanism for configuration validation.
/// Each rule checks for a specific issue and produces diagnostics
/// when problems are found.
pub trait LintRule: Send + Sync {
    /// Unique identifier for this rule.
    fn id(&self) -> RuleId;

    /// Human-readable name of the rule.
    fn name(&self) -> &str;

    /// Description of what this rule checks.
    fn description(&self) -> &str;

    /// Default severity for this rule.
    fn default_severity(&self) -> Severity;

    /// Check the configuration and return any diagnostics.
    fn check(&self, config: &BivvyConfig) -> Vec<LintDiagnostic>;

    /// Whether this rule supports auto-fix.
    fn supports_fix(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_id_equality() {
        let id1 = RuleId::new("test-rule");
        let id2 = RuleId::new("test-rule");
        let id3 = RuleId::new("other-rule");

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn rule_id_display() {
        let id = RuleId::new("my-rule");
        assert_eq!(format!("{}", id), "my-rule");
    }

    #[test]
    fn severity_ordering() {
        assert!(Severity::Hint < Severity::Warning);
        assert!(Severity::Warning < Severity::Error);
    }

    #[test]
    fn severity_display() {
        assert_eq!(format!("{}", Severity::Hint), "hint");
        assert_eq!(format!("{}", Severity::Warning), "warning");
        assert_eq!(format!("{}", Severity::Error), "error");
    }
}
