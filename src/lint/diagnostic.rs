//! Lint diagnostic messages.
//!
//! This module provides the [`LintDiagnostic`] type for representing
//! issues found during configuration validation, with optional source
//! location tracking for precise error reporting.

use super::rule::{RuleId, Severity};
use super::span::Span;

/// A diagnostic message produced by a lint rule.
#[derive(Debug, Clone)]
pub struct LintDiagnostic {
    /// The rule that produced this diagnostic.
    pub rule_id: RuleId,
    /// Severity of this diagnostic.
    pub severity: Severity,
    /// Human-readable message.
    pub message: String,
    /// Optional source location.
    pub span: Option<Span>,
    /// Optional suggestion for fixing the issue.
    pub suggestion: Option<String>,
    /// Additional related locations.
    pub related: Vec<RelatedInfo>,
}

/// Additional information related to a diagnostic.
#[derive(Debug, Clone)]
pub struct RelatedInfo {
    /// Location of the related information.
    pub span: Span,
    /// Message explaining the relationship.
    pub message: String,
}

impl LintDiagnostic {
    /// Create a new diagnostic.
    pub fn new(rule_id: RuleId, severity: Severity, message: impl Into<String>) -> Self {
        Self {
            rule_id,
            severity,
            message: message.into(),
            span: None,
            suggestion: None,
            related: vec![],
        }
    }

    /// Add a source span to this diagnostic.
    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }

    /// Add a fix suggestion.
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    /// Add related information.
    pub fn with_related(mut self, span: Span, message: impl Into<String>) -> Self {
        self.related.push(RelatedInfo {
            span,
            message: message.into(),
        });
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_creation() {
        let diag = LintDiagnostic::new(RuleId::new("test-rule"), Severity::Error, "Test message");

        assert_eq!(diag.rule_id, RuleId::new("test-rule"));
        assert_eq!(diag.severity, Severity::Error);
        assert_eq!(diag.message, "Test message");
        assert!(diag.suggestion.is_none());
        assert!(diag.span.is_none());
        assert!(diag.related.is_empty());
    }

    #[test]
    fn diagnostic_with_suggestion() {
        let diag = LintDiagnostic::new(RuleId::new("test-rule"), Severity::Warning, "Test warning")
            .with_suggestion("Fix it like this");

        assert!(diag.suggestion.is_some());
        assert_eq!(diag.suggestion.unwrap(), "Fix it like this");
    }

    #[test]
    fn diagnostic_builder_pattern() {
        let diag = LintDiagnostic::new(RuleId::new("test"), Severity::Error, "Test message")
            .with_span(Span::line("config.yml", 10))
            .with_suggestion("Fix it like this");

        assert_eq!(diag.message, "Test message");
        assert!(diag.span.is_some());
        assert!(diag.suggestion.is_some());
    }

    #[test]
    fn diagnostic_with_related_info() {
        let diag = LintDiagnostic::new(
            RuleId::new("circular-dependency"),
            Severity::Error,
            "Circular dependency detected",
        )
        .with_related(Span::line("config.yml", 5), "step_a defined here")
        .with_related(Span::line("config.yml", 10), "step_b depends on step_a");

        assert_eq!(diag.related.len(), 2);
        assert_eq!(diag.related[0].message, "step_a defined here");
        assert_eq!(diag.related[1].message, "step_b depends on step_a");
    }
}
