//! Lint diagnostic messages.
//!
//! This module provides the [`LintDiagnostic`] type for representing
//! issues found during configuration validation, with optional source
//! location tracking for precise error reporting.

use std::path::Path;

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

/// Parse a `BivvyError::ConfigParseError` message into a structured
/// [`LintDiagnostic`].
///
/// `serde_yaml` returns single-line messages like:
///
/// ```text
/// unknown field `my-settings`, expected one of `app_name`, `settings`, ... at line 31 column 1
/// ```
///
/// This function teases the message apart into a typed diagnostic with a
/// proper `rule_id`, source span, and (when applicable) a "did you mean?"
/// suggestion based on Levenshtein distance against the listed valid fields.
pub fn parse_error_to_diagnostic(file: &Path, raw_message: &str) -> LintDiagnostic {
    let (line, col, head) = extract_location(raw_message);

    if let Some(parsed) = parse_unknown_field(head) {
        let valid_list = parsed.valid.join(", ");
        let mut diag = LintDiagnostic::new(
            RuleId::new("parse-error/unknown-field"),
            Severity::Error,
            format!(
                "unrecognized top-level key `{}`\nexpected one of: {}",
                parsed.field, valid_list
            ),
        );
        let len = parsed.field.chars().count().max(1);
        let span = Span::new(file, line, col, line, col + len);
        diag = diag.with_span(span);
        if let Some(suggestion) = closest_match(&parsed.field, &parsed.valid) {
            diag = diag.with_suggestion(format!("did you mean `{suggestion}`?"));
        }
        return diag;
    }

    if let Some(parsed) = parse_invalid_type(head) {
        let mut diag = LintDiagnostic::new(
            RuleId::new("parse-error/invalid-type"),
            Severity::Error,
            format!(
                "invalid type: {} (expected {})",
                parsed.found, parsed.expected
            ),
        );
        diag = diag.with_span(Span::new(file, line, col, line, col + 1));
        return diag;
    }

    if let Some(field) = parse_missing_field(head) {
        let mut diag = LintDiagnostic::new(
            RuleId::new("parse-error/missing-field"),
            Severity::Error,
            format!("missing required field `{field}`"),
        );
        diag = diag.with_span(Span::new(file, line, col, line, col + 1));
        return diag;
    }

    if let Some(field) = parse_duplicate_key(head) {
        let mut diag = LintDiagnostic::new(
            RuleId::new("parse-error/duplicate-key"),
            Severity::Error,
            format!("duplicate key `{field}`"),
        );
        diag = diag.with_span(Span::new(file, line, col, line, col + 1));
        return diag;
    }

    // Generic fallback — still classified, with a span at file start at minimum.
    let mut diag = LintDiagnostic::new(
        RuleId::new("parse-error"),
        Severity::Error,
        head.trim().to_string(),
    );
    diag = diag.with_span(Span::new(file, line, col, line, col + 1));
    diag
}

#[derive(Debug)]
struct UnknownField {
    field: String,
    valid: Vec<String>,
}

#[derive(Debug)]
struct InvalidType {
    found: String,
    expected: String,
}

/// Pull `line N column M` out of the tail of a serde error message.
///
/// Returns the prefix (with trailing whitespace trimmed) plus the line/column
/// it parsed (defaulting to `1:1` when serde didn't include location data).
fn extract_location(raw: &str) -> (usize, usize, &str) {
    if let Some(idx) = raw.rfind(" at line ") {
        let head = &raw[..idx];
        let rest = &raw[idx + " at line ".len()..];
        // rest looks like "N column M" or "N column M, ..."
        let mut parts = rest.split_whitespace();
        let line = parts
            .next()
            .and_then(|s| s.trim_end_matches(',').parse::<usize>().ok())
            .unwrap_or(1);
        let _ = parts.next(); // "column"
        let col = parts
            .next()
            .and_then(|s| s.trim_end_matches(',').parse::<usize>().ok())
            .unwrap_or(1);
        return (line, col, head);
    }
    (1, 1, raw)
}

/// Match `unknown field \`X\`, expected one of \`A\`, \`B\`, ...` (or
/// `expected \`A\`` / `unknown field \`X\``).
fn parse_unknown_field(head: &str) -> Option<UnknownField> {
    let prefix = "unknown field `";
    let rest = head.strip_prefix(prefix)?;
    let end = rest.find('`')?;
    let field = rest[..end].to_string();
    let after = &rest[end + 1..];

    // After the field, serde may emit ", expected one of `a`, `b`, ..." or
    // ", expected `a`" or nothing. Parse all backtick-quoted tokens after the
    // first comma as candidates.
    let valid = if let Some(stripped) = after.strip_prefix(", expected one of ") {
        collect_backticked(stripped)
    } else if let Some(stripped) = after.strip_prefix(", expected ") {
        collect_backticked(stripped)
    } else {
        Vec::new()
    };
    Some(UnknownField { field, valid })
}

fn collect_backticked(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'`' {
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() && bytes[j] != b'`' {
                j += 1;
            }
            if j > start && j < bytes.len() {
                out.push(s[start..j].to_string());
            }
            i = j + 1;
        } else {
            i += 1;
        }
    }
    out
}

fn parse_invalid_type(head: &str) -> Option<InvalidType> {
    // serde_yaml: "invalid type: integer `1`, expected a string"
    let rest = head.strip_prefix("invalid type: ")?;
    let comma = rest.find(", expected ")?;
    let found = rest[..comma].to_string();
    let expected = rest[comma + ", expected ".len()..].to_string();
    Some(InvalidType { found, expected })
}

fn parse_missing_field(head: &str) -> Option<String> {
    let prefix = "missing field `";
    let rest = head.strip_prefix(prefix)?;
    let end = rest.find('`')?;
    Some(rest[..end].to_string())
}

fn parse_duplicate_key(head: &str) -> Option<String> {
    // marked_yaml / serde_yaml report duplicate keys with various phrasings.
    if let Some(rest) = head.strip_prefix("duplicate key `") {
        let end = rest.find('`')?;
        return Some(rest[..end].to_string());
    }
    if let Some(rest) = head.strip_prefix("duplicate entry with key `") {
        let end = rest.find('`')?;
        return Some(rest[..end].to_string());
    }
    None
}

/// Levenshtein distance between two strings, capped for early exit.
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr: Vec<usize> = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            curr[j + 1] = (curr[j] + 1).min(prev[j + 1] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

/// Pick the closest valid candidate within edit distance 3.
pub(crate) fn closest_match(needle: &str, haystack: &[String]) -> Option<String> {
    let mut best: Option<(usize, &String)> = None;
    for cand in haystack {
        let d = levenshtein(needle, cand);
        match best {
            Some((bd, _)) if d >= bd => {}
            _ => best = Some((d, cand)),
        }
    }
    best.and_then(|(d, c)| if d <= 3 { Some(c.clone()) } else { None })
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

    #[test]
    fn parse_error_unknown_field_with_suggestion() {
        let raw = "unknown field `setting`, expected one of `app_name`, `settings`, `steps`, `workflows` at line 3 column 1";
        let diag = parse_error_to_diagnostic(Path::new("config.yml"), raw);

        assert_eq!(diag.rule_id, RuleId::new("parse-error/unknown-field"));
        assert_eq!(diag.severity, Severity::Error);
        assert!(diag.message.contains("`setting`"));
        let span = diag.span.as_ref().unwrap();
        assert_eq!(span.start_line, 3);
        assert_eq!(span.start_col, 1);
        // `setting` is one edit from `settings`.
        assert_eq!(diag.suggestion.unwrap(), "did you mean `settings`?");
    }

    #[test]
    fn parse_error_unknown_field_no_suggestion_when_distance_high() {
        let raw =
            "unknown field `xyzqq`, expected one of `app_name`, `settings` at line 1 column 1";
        let diag = parse_error_to_diagnostic(Path::new("config.yml"), raw);

        assert_eq!(diag.rule_id, RuleId::new("parse-error/unknown-field"));
        assert!(diag.suggestion.is_none());
    }

    #[test]
    fn parse_error_invalid_type() {
        let raw = "invalid type: integer `1`, expected a string at line 5 column 7";
        let diag = parse_error_to_diagnostic(Path::new("config.yml"), raw);

        assert_eq!(diag.rule_id, RuleId::new("parse-error/invalid-type"));
        assert!(diag.message.contains("expected a string"));
        let span = diag.span.as_ref().unwrap();
        assert_eq!(span.start_line, 5);
        assert_eq!(span.start_col, 7);
    }

    #[test]
    fn parse_error_missing_field_uses_line_one_when_unknown() {
        let raw = "missing field `command`";
        let diag = parse_error_to_diagnostic(Path::new("step.yml"), raw);

        assert_eq!(diag.rule_id, RuleId::new("parse-error/missing-field"));
        let span = diag.span.as_ref().unwrap();
        assert_eq!(span.start_line, 1);
        assert_eq!(span.start_col, 1);
    }

    #[test]
    fn parse_error_generic_fallback() {
        let raw = "could not find expected ':' at line 4 column 9";
        let diag = parse_error_to_diagnostic(Path::new("c.yml"), raw);

        assert_eq!(diag.rule_id, RuleId::new("parse-error"));
        let span = diag.span.as_ref().unwrap();
        assert_eq!(span.start_line, 4);
        assert_eq!(span.start_col, 9);
    }

    #[test]
    fn parse_error_duplicate_key() {
        let raw = "duplicate key `name` at line 2 column 5";
        let diag = parse_error_to_diagnostic(Path::new("c.yml"), raw);

        assert_eq!(diag.rule_id, RuleId::new("parse-error/duplicate-key"));
        assert!(diag.message.contains("`name`"));
    }

    #[test]
    fn levenshtein_distance() {
        assert_eq!(levenshtein("setting", "settings"), 1);
        assert_eq!(levenshtein("workflow", "workflows"), 1);
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("abc", ""), 3);
        assert_eq!(levenshtein("abc", "abc"), 0);
    }
}
