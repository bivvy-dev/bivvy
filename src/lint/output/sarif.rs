//! SARIF output formatter.
//!
//! SARIF (Static Analysis Results Interchange Format) is an OASIS standard
//! for static analysis tools, supported by GitHub, VS Code, and other tools.

use super::LintFormatter;
use crate::lint::{LintDiagnostic, Severity};
use serde::Serialize;
use std::collections::HashSet;
use std::io::Write;

/// SARIF version we generate.
const SARIF_VERSION: &str = "2.1.0";
const SARIF_SCHEMA: &str = "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json";

/// Formats lint output as SARIF.
pub struct SarifFormatter {
    /// Tool name to report.
    pub tool_name: String,
    /// Tool version to report.
    pub tool_version: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifLog {
    #[serde(rename = "$schema")]
    schema: &'static str,
    version: &'static str,
    runs: Vec<SarifRun>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifRun {
    tool: SarifTool,
    results: Vec<SarifResult>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifTool {
    driver: SarifDriver,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifDriver {
    name: String,
    version: String,
    rules: Vec<SarifRule>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifRule {
    id: String,
    short_description: SarifMessage,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifResult {
    rule_id: String,
    level: &'static str,
    message: SarifMessage,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    locations: Vec<SarifLocation>,
}

#[derive(Serialize)]
struct SarifMessage {
    text: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifLocation {
    physical_location: SarifPhysicalLocation,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifPhysicalLocation {
    artifact_location: SarifArtifactLocation,
    #[serde(skip_serializing_if = "Option::is_none")]
    region: Option<SarifRegion>,
}

#[derive(Serialize)]
struct SarifArtifactLocation {
    uri: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifRegion {
    start_line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_column: Option<usize>,
}

impl SarifFormatter {
    /// Create a new SARIF formatter.
    pub fn new(tool_name: impl Into<String>, tool_version: impl Into<String>) -> Self {
        Self {
            tool_name: tool_name.into(),
            tool_version: tool_version.into(),
        }
    }

    fn severity_to_level(severity: Severity) -> &'static str {
        match severity {
            Severity::Hint => "note",
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }
}

impl LintFormatter for SarifFormatter {
    fn format<W: Write>(
        &self,
        diagnostics: &[LintDiagnostic],
        writer: &mut W,
    ) -> std::io::Result<()> {
        // Collect unique rule IDs
        let rule_ids: HashSet<_> = diagnostics.iter().map(|d| &d.rule_id).collect();

        let rules: Vec<_> = rule_ids
            .iter()
            .map(|id| SarifRule {
                id: id.0.clone(),
                short_description: SarifMessage {
                    text: format!("Rule {}", id.0),
                },
            })
            .collect();

        let results: Vec<_> = diagnostics
            .iter()
            .map(|d| {
                let locations = d
                    .span
                    .as_ref()
                    .map(|span| {
                        vec![SarifLocation {
                            physical_location: SarifPhysicalLocation {
                                artifact_location: SarifArtifactLocation {
                                    uri: span.file.display().to_string(),
                                },
                                region: Some(SarifRegion {
                                    start_line: span.start_line,
                                    start_column: if span.start_col > 1 {
                                        Some(span.start_col)
                                    } else {
                                        None
                                    },
                                }),
                            },
                        }]
                    })
                    .unwrap_or_default();

                SarifResult {
                    rule_id: d.rule_id.0.clone(),
                    level: Self::severity_to_level(d.severity),
                    message: SarifMessage {
                        text: d.message.clone(),
                    },
                    locations,
                }
            })
            .collect();

        let log = SarifLog {
            schema: SARIF_SCHEMA,
            version: SARIF_VERSION,
            runs: vec![SarifRun {
                tool: SarifTool {
                    driver: SarifDriver {
                        name: self.tool_name.clone(),
                        version: self.tool_version.clone(),
                        rules,
                    },
                },
                results,
            }],
        };

        serde_json::to_writer_pretty(writer, &log).map_err(std::io::Error::other)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::{RuleId, Span};

    #[test]
    fn produces_valid_sarif() {
        let formatter = SarifFormatter::new("bivvy", "1.0.0");
        let diagnostics = vec![LintDiagnostic::new(
            RuleId::new("circular-dependency"),
            Severity::Error,
            "Circular dependency detected",
        )
        .with_span(Span::line("config.yml", 15))];

        let mut output = Vec::new();
        formatter.format(&diagnostics, &mut output).unwrap();

        let parsed: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(parsed["version"], "2.1.0");
        assert!(parsed["runs"].is_array());
        assert_eq!(parsed["runs"][0]["tool"]["driver"]["name"], "bivvy");
    }

    #[test]
    fn maps_severity_to_sarif_level() {
        assert_eq!(SarifFormatter::severity_to_level(Severity::Error), "error");
        assert_eq!(
            SarifFormatter::severity_to_level(Severity::Warning),
            "warning"
        );
        assert_eq!(SarifFormatter::severity_to_level(Severity::Hint), "note");
    }

    #[test]
    fn includes_rule_definitions() {
        let formatter = SarifFormatter::new("bivvy", "1.0.0");
        let diagnostics = vec![
            LintDiagnostic::new(RuleId::new("rule1"), Severity::Error, "msg1"),
            LintDiagnostic::new(RuleId::new("rule2"), Severity::Warning, "msg2"),
        ];

        let mut output = Vec::new();
        formatter.format(&diagnostics, &mut output).unwrap();

        let parsed: serde_json::Value = serde_json::from_slice(&output).unwrap();
        let rules = &parsed["runs"][0]["tool"]["driver"]["rules"];
        assert!(rules.as_array().unwrap().len() >= 2);
    }

    #[test]
    fn includes_location_information() {
        let formatter = SarifFormatter::new("bivvy", "1.0.0");
        let diagnostics =
            vec![
                LintDiagnostic::new(RuleId::new("test"), Severity::Error, "Test message")
                    .with_span(Span::new("config.yml", 10, 5, 10, 20)),
            ];

        let mut output = Vec::new();
        formatter.format(&diagnostics, &mut output).unwrap();

        let parsed: serde_json::Value = serde_json::from_slice(&output).unwrap();
        let location = &parsed["runs"][0]["results"][0]["locations"][0];
        assert_eq!(
            location["physicalLocation"]["artifactLocation"]["uri"],
            "config.yml"
        );
        assert_eq!(location["physicalLocation"]["region"]["startLine"], 10);
        assert_eq!(location["physicalLocation"]["region"]["startColumn"], 5);
    }

    #[test]
    fn omits_column_one() {
        let formatter = SarifFormatter::new("bivvy", "1.0.0");
        let diagnostics = vec![
            LintDiagnostic::new(RuleId::new("test"), Severity::Error, "msg")
                .with_span(Span::line("config.yml", 10)),
        ];

        let mut output = Vec::new();
        formatter.format(&diagnostics, &mut output).unwrap();

        let parsed: serde_json::Value = serde_json::from_slice(&output).unwrap();
        let region = &parsed["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["region"];
        assert!(region["startColumn"].is_null());
    }
}
