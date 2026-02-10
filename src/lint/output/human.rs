//! Human-readable output formatter.
//!
//! Formats lint diagnostics for terminal display with optional color support.

use super::LintFormatter;
use crate::lint::{LintDiagnostic, Severity};
use std::io::Write;

/// Formats lint output for human consumption.
pub struct HumanFormatter {
    /// Whether to use colors (ANSI escape codes).
    pub use_color: bool,
}

impl HumanFormatter {
    /// Create a new human formatter.
    pub fn new(use_color: bool) -> Self {
        Self { use_color }
    }

    fn severity_prefix(&self, severity: Severity) -> &'static str {
        match severity {
            Severity::Hint => "hint",
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }
}

impl LintFormatter for HumanFormatter {
    fn format<W: Write>(
        &self,
        diagnostics: &[LintDiagnostic],
        writer: &mut W,
    ) -> std::io::Result<()> {
        for diag in diagnostics {
            // Header line: error[rule-id]: message
            writeln!(
                writer,
                "{}[{}]: {}",
                self.severity_prefix(diag.severity),
                diag.rule_id.0,
                diag.message
            )?;

            // Location line
            if let Some(ref span) = diag.span {
                writeln!(
                    writer,
                    "  --> {}:{}:{}",
                    span.file.display(),
                    span.start_line,
                    span.start_col
                )?;
            }

            // Suggestion
            if let Some(ref suggestion) = diag.suggestion {
                writeln!(writer, "   = help: {}", suggestion)?;
            }

            // Related info
            for related in &diag.related {
                writeln!(
                    writer,
                    "   = note: {} ({}:{})",
                    related.message,
                    related.span.file.display(),
                    related.span.start_line
                )?;
            }

            writeln!(writer)?;
        }

        // Summary
        let error_count = diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count();
        let warning_count = diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
            .count();

        if error_count > 0 || warning_count > 0 {
            writeln!(
                writer,
                "Found {} error(s) and {} warning(s)",
                error_count, warning_count
            )?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::{RuleId, Span};

    #[test]
    fn formats_error_diagnostic() {
        let formatter = HumanFormatter::new(false);
        let diagnostics = vec![LintDiagnostic::new(
            RuleId::new("test-rule"),
            Severity::Error,
            "Test error message",
        )
        .with_span(Span::line("config.yml", 10))];

        let mut output = Vec::new();
        formatter.format(&diagnostics, &mut output).unwrap();
        let output = String::from_utf8(output).unwrap();

        assert!(output.contains("error[test-rule]"));
        assert!(output.contains("Test error message"));
        assert!(output.contains("config.yml:10"));
    }

    #[test]
    fn formats_warning_diagnostic() {
        let formatter = HumanFormatter::new(false);
        let diagnostics = vec![LintDiagnostic::new(
            RuleId::new("test-rule"),
            Severity::Warning,
            "Test warning message",
        )];

        let mut output = Vec::new();
        formatter.format(&diagnostics, &mut output).unwrap();
        let output = String::from_utf8(output).unwrap();

        assert!(output.contains("warning[test-rule]"));
    }

    #[test]
    fn formats_hint_diagnostic() {
        let formatter = HumanFormatter::new(false);
        let diagnostics = vec![LintDiagnostic::new(
            RuleId::new("test-rule"),
            Severity::Hint,
            "Test hint message",
        )];

        let mut output = Vec::new();
        formatter.format(&diagnostics, &mut output).unwrap();
        let output = String::from_utf8(output).unwrap();

        assert!(output.contains("hint[test-rule]"));
    }

    #[test]
    fn formats_summary_line() {
        let formatter = HumanFormatter::new(false);
        let diagnostics = vec![
            LintDiagnostic::new(RuleId::new("r1"), Severity::Error, "err"),
            LintDiagnostic::new(RuleId::new("r2"), Severity::Warning, "warn"),
            LintDiagnostic::new(RuleId::new("r3"), Severity::Warning, "warn2"),
        ];

        let mut output = Vec::new();
        formatter.format(&diagnostics, &mut output).unwrap();
        let output = String::from_utf8(output).unwrap();

        assert!(output.contains("1 error(s)"));
        assert!(output.contains("2 warning(s)"));
    }

    #[test]
    fn formats_suggestion() {
        let formatter = HumanFormatter::new(false);
        let diagnostics =
            vec![
                LintDiagnostic::new(RuleId::new("test-rule"), Severity::Warning, "Test message")
                    .with_suggestion("Try this instead"),
            ];

        let mut output = Vec::new();
        formatter.format(&diagnostics, &mut output).unwrap();
        let output = String::from_utf8(output).unwrap();

        assert!(output.contains("= help: Try this instead"));
    }

    #[test]
    fn formats_related_info() {
        let formatter = HumanFormatter::new(false);
        let diagnostics = vec![LintDiagnostic::new(
            RuleId::new("circular-dependency"),
            Severity::Error,
            "Circular dependency",
        )
        .with_related(Span::line("config.yml", 5), "step_a defined here")];

        let mut output = Vec::new();
        formatter.format(&diagnostics, &mut output).unwrap();
        let output = String::from_utf8(output).unwrap();

        assert!(output.contains("= note: step_a defined here"));
        assert!(output.contains("config.yml:5"));
    }

    #[test]
    fn no_summary_when_no_issues() {
        let formatter = HumanFormatter::new(false);
        let diagnostics: Vec<LintDiagnostic> = vec![];

        let mut output = Vec::new();
        formatter.format(&diagnostics, &mut output).unwrap();
        let output = String::from_utf8(output).unwrap();

        assert!(!output.contains("Found"));
    }
}
