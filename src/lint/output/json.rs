//! JSON output formatter.
//!
//! Formats lint diagnostics as machine-readable JSON for tooling integration.

use super::LintFormatter;
use crate::lint::{LintDiagnostic, Severity};
use serde::Serialize;
use std::io::Write;

/// Formats lint output as JSON.
pub struct JsonFormatter;

#[derive(Serialize)]
struct JsonOutput {
    diagnostics: Vec<JsonDiagnostic>,
    summary: JsonSummary,
}

#[derive(Serialize)]
struct JsonDiagnostic {
    rule_id: String,
    severity: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    column: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    suggestion: Option<String>,
}

#[derive(Serialize)]
struct JsonSummary {
    total: usize,
    errors: usize,
    warnings: usize,
    hints: usize,
}

impl JsonFormatter {
    /// Create a new JSON formatter.
    pub fn new() -> Self {
        Self
    }

    fn severity_to_string(severity: Severity) -> &'static str {
        match severity {
            Severity::Hint => "hint",
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }
}

impl Default for JsonFormatter {
    fn default() -> Self {
        Self::new()
    }
}

impl LintFormatter for JsonFormatter {
    fn format<W: Write>(
        &self,
        diagnostics: &[LintDiagnostic],
        writer: &mut W,
    ) -> std::io::Result<()> {
        let json_diagnostics: Vec<_> = diagnostics
            .iter()
            .map(|d| JsonDiagnostic {
                rule_id: d.rule_id.0.clone(),
                severity: Self::severity_to_string(d.severity).to_string(),
                message: d.message.clone(),
                file: d.span.as_ref().map(|s| s.file.display().to_string()),
                line: d.span.as_ref().map(|s| s.start_line),
                column: d.span.as_ref().map(|s| s.start_col),
                suggestion: d.suggestion.clone(),
            })
            .collect();

        let summary = JsonSummary {
            total: diagnostics.len(),
            errors: diagnostics
                .iter()
                .filter(|d| d.severity == Severity::Error)
                .count(),
            warnings: diagnostics
                .iter()
                .filter(|d| d.severity == Severity::Warning)
                .count(),
            hints: diagnostics
                .iter()
                .filter(|d| d.severity == Severity::Hint)
                .count(),
        };

        let output = JsonOutput {
            diagnostics: json_diagnostics,
            summary,
        };

        serde_json::to_writer_pretty(writer, &output).map_err(std::io::Error::other)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::{RuleId, Span};

    #[test]
    fn produces_valid_json() {
        let formatter = JsonFormatter::new();
        let diagnostics = vec![LintDiagnostic::new(
            RuleId::new("test"),
            Severity::Error,
            "Error message",
        )];

        let mut output = Vec::new();
        formatter.format(&diagnostics, &mut output).unwrap();

        let parsed: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert!(parsed["diagnostics"].is_array());
        assert_eq!(parsed["summary"]["total"].as_u64().unwrap(), 1);
    }

    #[test]
    fn includes_location_when_present() {
        let formatter = JsonFormatter::new();
        let diagnostics =
            vec![
                LintDiagnostic::new(RuleId::new("test"), Severity::Error, "msg")
                    .with_span(Span::new("config.yml", 10, 5, 10, 20)),
            ];

        let mut output = Vec::new();
        formatter.format(&diagnostics, &mut output).unwrap();

        let parsed: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(parsed["diagnostics"][0]["line"], 10);
        assert_eq!(parsed["diagnostics"][0]["column"], 5);
    }

    #[test]
    fn omits_location_when_absent() {
        let formatter = JsonFormatter::new();
        let diagnostics = vec![LintDiagnostic::new(
            RuleId::new("test"),
            Severity::Error,
            "msg",
        )];

        let mut output = Vec::new();
        formatter.format(&diagnostics, &mut output).unwrap();

        let parsed: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert!(parsed["diagnostics"][0]["line"].is_null());
    }

    #[test]
    fn summary_counts_by_severity() {
        let formatter = JsonFormatter::new();
        let diagnostics = vec![
            LintDiagnostic::new(RuleId::new("r1"), Severity::Error, "e1"),
            LintDiagnostic::new(RuleId::new("r2"), Severity::Error, "e2"),
            LintDiagnostic::new(RuleId::new("r3"), Severity::Warning, "w1"),
            LintDiagnostic::new(RuleId::new("r4"), Severity::Hint, "h1"),
        ];

        let mut output = Vec::new();
        formatter.format(&diagnostics, &mut output).unwrap();

        let parsed: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(parsed["summary"]["total"], 4);
        assert_eq!(parsed["summary"]["errors"], 2);
        assert_eq!(parsed["summary"]["warnings"], 1);
        assert_eq!(parsed["summary"]["hints"], 1);
    }

    #[test]
    fn default_impl_works() {
        let formatter = JsonFormatter;
        let diagnostics: Vec<LintDiagnostic> = vec![];

        let mut output = Vec::new();
        formatter.format(&diagnostics, &mut output).unwrap();

        let parsed: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(parsed["summary"]["total"], 0);
    }
}
