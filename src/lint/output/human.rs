//! Human-readable lint output.
//!
//! This module implements two complementary renderings:
//!
//! 1. **Per-file cards** — the report shown by `bivvy lint`. One card per
//!    file actually inspected, with aligned label/value rows for stats and
//!    an `Errors:` row that prefixes any embedded rustc-style diagnostics.
//! 2. **Rustc-style diagnostic blocks** — the body shown under a card's
//!    `Errors:` row when the file has problems. Mirrors the format used by
//!    `rustc` and `clippy`: a header, an `--> file:line:col` arrow, source
//!    line context with a caret pointing at the offending span, and `help:`
//!    / `note:` continuation lines.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use console::style;

use super::LintFormatter;
use crate::lint::{LintDiagnostic, Severity, Span};

/// One card in the per-file report.
///
/// Carries the file path, a human-readable label (e.g. `"project config"`),
/// the stats rows the card should display, and any diagnostics that pertain
/// to this file. Files that exist for context only (e.g. the project file
/// when running `bivvy lint --workflow X`) should be passed via
/// [`HumanReport::with_context_files`] instead — context files render as a
/// trailing `Loaded for context (not validated): ...` line, not a card.
#[derive(Debug, Clone)]
pub struct FileCard {
    /// Absolute path on disk. Used to read source for diagnostic excerpts.
    pub path: PathBuf,
    /// Display path: relative to project root with `./` prefix, or `$HOME/...`
    /// for paths under the user's home directory. Use [`display_path`] to
    /// build this from a real `path`.
    pub display: String,
    /// Short qualifier shown in parentheses after the path.
    /// Examples: `"project config"`, `"workflow file: release"`.
    pub label: String,
    /// Stat rows to show, in display order. Each is `(label, value)`.
    /// `label` should NOT include the trailing colon — alignment is added
    /// by the formatter. The `Errors:` row is added automatically.
    pub stats: Vec<(String, String)>,
    /// Diagnostics that belong to this file. Their span's file path is not
    /// re-checked; whatever you put here is what gets rendered.
    pub diagnostics: Vec<LintDiagnostic>,
}

/// The full report — a sequence of [`FileCard`]s plus optional context-only
/// file paths.
#[derive(Debug, Default, Clone)]
pub struct HumanReport {
    pub cards: Vec<FileCard>,
    /// Files loaded for context only — rendered as `Loaded for context (not
    /// validated): <display>` after all cards.
    pub context_files: Vec<String>,
    /// Diagnostics that don't belong to any specific card (e.g. rules that
    /// fire without a span). Rendered after all cards in a "(no location)"
    /// fallback block.
    pub no_file_diagnostics: Vec<LintDiagnostic>,
}

impl HumanReport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_card(&mut self, card: FileCard) {
        self.cards.push(card);
    }

    pub fn push_context_file(&mut self, display: impl Into<String>) {
        self.context_files.push(display.into());
    }
}

/// Produce a display-friendly path: relative to `project_root` with `./`
/// prefix, or `$HOME/...` for paths under the user's home dir, or the
/// absolute path otherwise.
///
/// The `home_dir` argument is taken explicitly so the caller can pass the
/// real home dir (`crate::sys::home_dir()`) in production and a temp path
/// in tests.
pub fn display_path(path: &Path, project_root: &Path, home_dir: Option<&Path>) -> String {
    if let Ok(rel) = path.strip_prefix(project_root) {
        let s = rel.to_string_lossy().replace('\\', "/");
        return format!("./{s}");
    }
    if let Some(home) = home_dir {
        if let Ok(rel) = path.strip_prefix(home) {
            let s = rel.to_string_lossy().replace('\\', "/");
            return format!("$HOME/{s}");
        }
    }
    path.display().to_string()
}

/// Formats lint diagnostics for terminal display.
///
/// When constructed via [`HumanFormatter::new`], the [`LintFormatter::format`]
/// method renders a flat list of diagnostics in the rustc-style block format,
/// grouped by source file. To render the per-file report instead (the format
/// used by `bivvy lint`), build a [`HumanReport`] and call
/// [`HumanFormatter::format_report`].
///
/// To render `--> <path>:<line>:<col>` arrows with project-relative or
/// `$HOME`-relative paths instead of absolute paths, configure the formatter
/// via [`HumanFormatter::with_path_display`] before formatting.
pub struct HumanFormatter {
    /// Whether to emit ANSI styling (currently controls bold path headers).
    pub use_color: bool,
    /// Project root to use when rewriting absolute paths in `--> ...` arrows.
    /// `None` means "show paths verbatim".
    project_root: Option<PathBuf>,
    /// Home directory to use when rewriting paths under `$HOME`. `None` means
    /// "don't substitute `$HOME`".
    home_dir: Option<PathBuf>,
    /// Cached file contents, indexed by absolute path. Keeps each unique file
    /// to a single read even when many diagnostics point at it.
    file_cache: std::cell::RefCell<HashMap<PathBuf, Option<Vec<String>>>>,
}

impl HumanFormatter {
    /// Create a new human formatter.
    pub fn new(use_color: bool) -> Self {
        Self {
            use_color,
            project_root: None,
            home_dir: None,
            file_cache: std::cell::RefCell::new(HashMap::new()),
        }
    }

    /// Configure the formatter to rewrite paths in `--> ...` arrows and
    /// `(path:line)` notes to be relative to `project_root` (with `./`
    /// prefix) or `home_dir` (with `$HOME/` prefix).
    pub fn with_path_display(
        mut self,
        project_root: Option<PathBuf>,
        home_dir: Option<PathBuf>,
    ) -> Self {
        self.project_root = project_root;
        self.home_dir = home_dir;
        self
    }

    /// Render a path according to the formatter's display configuration.
    fn rewrite_path(&self, path: &Path) -> String {
        if let Some(ref pr) = self.project_root {
            return display_path(path, pr, self.home_dir.as_deref());
        }
        path.display().to_string()
    }

    /// Read source lines for `path`, caching the result.
    fn lines_for(&self, path: &Path) -> Option<Vec<String>> {
        let key = path.to_path_buf();
        if let Some(v) = self.file_cache.borrow().get(&key) {
            return v.clone();
        }
        let lines = fs::read_to_string(path)
            .ok()
            .map(|s| s.lines().map(|l| l.to_string()).collect::<Vec<_>>());
        self.file_cache.borrow_mut().insert(key, lines.clone());
        lines
    }

    /// Render a single rustc-style diagnostic block to `out`, with each
    /// continuation line prefixed by `indent`.
    fn write_diagnostic<W: Write>(
        &self,
        diag: &LintDiagnostic,
        indent: &str,
        out: &mut W,
    ) -> std::io::Result<()> {
        // Header: error[rule]: message (first line)
        let prefix = severity_prefix(diag.severity);
        let mut msg_lines = diag.message.lines();
        let first = msg_lines.next().unwrap_or("");
        writeln!(out, "{indent}{prefix}[{}]: {first}", diag.rule_id.0)?;
        for cont in msg_lines {
            writeln!(out, "{indent}  {cont}")?;
        }

        // Source location + excerpt (only when we have a span)
        if let Some(ref span) = diag.span {
            let display = self.rewrite_path(&span.file);
            writeln!(
                out,
                "{indent}  --> {}:{}:{}",
                display, span.start_line, span.start_col
            )?;
            self.write_source_excerpt(span, indent, out)?;
        }

        // Suggestion / notes
        if let Some(ref s) = diag.suggestion {
            writeln!(out, "{indent}   = help: {s}")?;
        }
        for related in &diag.related {
            let p = self.rewrite_path(&related.span.file);
            writeln!(
                out,
                "{indent}   = note: {} ({}:{})",
                related.message, p, related.span.start_line
            )?;
        }
        Ok(())
    }

    /// Write the rustc-style source excerpt for a span: 1 line of context
    /// above the span line, the span line itself, the caret line, and 1
    /// line below.
    fn write_source_excerpt<W: Write>(
        &self,
        span: &Span,
        indent: &str,
        out: &mut W,
    ) -> std::io::Result<()> {
        let Some(lines) = self.lines_for(&span.file) else {
            return Ok(());
        };
        if lines.is_empty() {
            return Ok(());
        }

        let total = lines.len();
        let target = span.start_line.min(total);
        if target == 0 {
            return Ok(());
        }

        let start = target.saturating_sub(1).max(1);
        let end = (target + 1).min(total);

        // Gutter width is wide enough for the largest line number we'll show.
        let gutter = end.to_string().len();
        let pad = " ".repeat(gutter);
        writeln!(out, "{indent}{pad} |")?;

        for ln in start..=end {
            let src = lines.get(ln - 1).map(String::as_str).unwrap_or("");
            writeln!(out, "{indent}{:>w$} | {}", ln, src, w = gutter)?;

            if ln == target {
                let col = span.start_col.max(1);
                let mut span_len = span.end_col.saturating_sub(span.start_col).max(1);
                if span.end_col == usize::MAX {
                    // "rest of line" sentinel — clamp to the actual line length.
                    let line_len = src.chars().count();
                    span_len = line_len.saturating_sub(col - 1).max(1);
                }
                let leading = " ".repeat(col.saturating_sub(1));
                let carets = "^".repeat(span_len);
                writeln!(out, "{indent}{pad} | {leading}{carets}")?;
            }
        }
        writeln!(out, "{indent}{pad} |")?;
        Ok(())
    }

    /// Render a [`HumanReport`] to `out`. This is the format used by
    /// `bivvy lint` — per-file cards with aligned stat rows.
    pub fn format_report<W: Write>(
        &self,
        report: &HumanReport,
        out: &mut W,
    ) -> std::io::Result<()> {
        let mut first = true;
        for card in &report.cards {
            if !first {
                writeln!(out)?;
            }
            first = false;
            self.write_card(card, out)?;
        }

        for ctx in &report.context_files {
            if !first {
                writeln!(out)?;
            }
            first = false;
            writeln!(out, "Loaded for context (not validated): {ctx}")?;
        }

        if !report.no_file_diagnostics.is_empty() {
            if !first {
                writeln!(out)?;
            }
            writeln!(out, "(no location)")?;
            for diag in &report.no_file_diagnostics {
                writeln!(out)?;
                self.write_diagnostic(diag, "  ", out)?;
            }
        }

        Ok(())
    }

    fn write_card<W: Write>(&self, card: &FileCard, out: &mut W) -> std::io::Result<()> {
        // Header: bold path + label.
        let path_styled = if self.use_color {
            style(&card.display).bold().to_string()
        } else {
            card.display.clone()
        };
        writeln!(out, "{} ({})", path_styled, card.label)?;

        // Compute alignment width over all rows including the trailing
        // Errors row.
        let error_count = card
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count();
        let warning_count = card
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
            .count();

        let mut rows: Vec<(String, String)> = card.stats.clone();
        if warning_count > 0 {
            rows.push(("Warnings".to_string(), warning_count.to_string()));
        }
        rows.push(("Errors".to_string(), error_count.to_string()));

        let label_width = rows
            .iter()
            .map(|(label, _)| label.chars().count())
            .max()
            .unwrap_or(0);

        for (label, value) in &rows {
            let padding = " ".repeat(label_width.saturating_sub(label.chars().count()));
            writeln!(out, "  {label}:{padding}  {value}")?;
        }

        // Diagnostics, indented under the Errors row.
        if !card.diagnostics.is_empty() {
            // Sort by line ascending (no-line diagnostics first).
            let mut diags: Vec<&LintDiagnostic> = card.diagnostics.iter().collect();
            diags.sort_by_key(|d| d.span.as_ref().map(|s| s.start_line).unwrap_or(0));
            for diag in diags {
                writeln!(out)?;
                self.write_diagnostic(diag, "    ", out)?;
            }
        }

        Ok(())
    }
}

fn severity_prefix(severity: Severity) -> &'static str {
    match severity {
        Severity::Hint => "hint",
        Severity::Warning => "warning",
        Severity::Error => "error",
    }
}

impl LintFormatter for HumanFormatter {
    /// Group `diagnostics` by source file path and render each group as a
    /// rustc-style block sequence. Diagnostics without a span are rendered
    /// at the end under a `(no location)` heading.
    fn format<W: Write>(
        &self,
        diagnostics: &[LintDiagnostic],
        writer: &mut W,
    ) -> std::io::Result<()> {
        // Group by file path, preserving first-seen order.
        let mut order: Vec<PathBuf> = Vec::new();
        let mut groups: HashMap<PathBuf, Vec<&LintDiagnostic>> = HashMap::new();
        let mut no_file: Vec<&LintDiagnostic> = Vec::new();

        for diag in diagnostics {
            if let Some(ref span) = diag.span {
                if !groups.contains_key(&span.file) {
                    order.push(span.file.clone());
                }
                groups.entry(span.file.clone()).or_default().push(diag);
            } else {
                no_file.push(diag);
            }
        }

        let mut first = true;
        for file in &order {
            if !first {
                writeln!(writer)?;
            }
            first = false;

            let display = self.rewrite_path(file);
            let header = if self.use_color {
                style(&display).bold().to_string()
            } else {
                display
            };
            writeln!(writer, "{header}")?;

            let mut group: Vec<&LintDiagnostic> = groups.get(file).cloned().unwrap_or_default();
            group.sort_by_key(|d| d.span.as_ref().map(|s| s.start_line).unwrap_or(0));
            for diag in group {
                writeln!(writer)?;
                self.write_diagnostic(diag, "  ", writer)?;
            }
        }

        if !no_file.is_empty() {
            if !first {
                writeln!(writer)?;
            }
            writeln!(writer, "(no location)")?;
            for diag in no_file {
                writeln!(writer)?;
                self.write_diagnostic(diag, "  ", writer)?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::{RuleId, Span};
    use tempfile::TempDir;

    fn write_temp(content: &str) -> (TempDir, PathBuf) {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("config.yml");
        fs::write(&path, content).unwrap();
        (temp, path)
    }

    #[test]
    fn formats_diagnostic_with_span_and_excerpt() {
        let (_t, path) = write_temp("line one\nbroken line\nline three\n");
        let formatter = HumanFormatter::new(false);
        let span = Span::new(&path, 2, 1, 2, 7);
        let diag = LintDiagnostic::new(RuleId::new("test"), Severity::Error, "msg").with_span(span);

        let mut out = Vec::new();
        formatter.format(&[diag], &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();

        assert!(s.contains("error[test]: msg"));
        assert!(s.contains("--> "));
        assert!(s.contains(":2:1"));
        assert!(s.contains("broken line"));
        assert!(s.contains("^^^^^^"));
    }

    #[test]
    fn caret_at_first_line_no_above_context() {
        let (_t, path) = write_temp("alpha\nbeta\ngamma\n");
        let formatter = HumanFormatter::new(false);
        let diag = LintDiagnostic::new(RuleId::new("r"), Severity::Error, "m")
            .with_span(Span::new(&path, 1, 1, 1, 6));

        let mut out = Vec::new();
        formatter.format(&[diag], &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();

        // Line 1 should appear, line 2 as context, but no line 0.
        assert!(s.contains(" 1 | alpha"));
        assert!(s.contains(" 2 | beta"));
        assert!(!s.contains(" 0 |"));
    }

    #[test]
    fn caret_at_last_line_no_below_context() {
        let (_t, path) = write_temp("alpha\nbeta\ngamma\n");
        let formatter = HumanFormatter::new(false);
        let diag = LintDiagnostic::new(RuleId::new("r"), Severity::Error, "m")
            .with_span(Span::new(&path, 3, 1, 3, 6));

        let mut out = Vec::new();
        formatter.format(&[diag], &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();

        assert!(s.contains(" 2 | beta"));
        assert!(s.contains(" 3 | gamma"));
        // No line 4
        assert!(!s.contains(" 4 |"));
    }

    #[test]
    fn caret_aligns_with_span_column() {
        let (_t, path) = write_temp("aaaaa\nbbbbbb\nccccccc\n");
        let formatter = HumanFormatter::new(false);
        // Caret at column 4, length 3 (cols 4..7)
        let diag = LintDiagnostic::new(RuleId::new("r"), Severity::Error, "m")
            .with_span(Span::new(&path, 2, 4, 2, 7));

        let mut out = Vec::new();
        formatter.format(&[diag], &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();

        // The caret line begins after "  | " and should have 3 spaces (cols 1..3)
        // followed by "^^^".
        let caret_line = s
            .lines()
            .find(|l| l.contains("^^^"))
            .expect("caret line missing");
        let after = caret_line
            .split_once(" | ")
            .map(|(_, b)| b)
            .unwrap_or(caret_line);
        assert_eq!(after, "   ^^^", "got: {after:?}");
    }

    #[test]
    fn groups_diagnostics_by_file() {
        let (_t1, path1) = write_temp("hello world\n");
        let temp2 = TempDir::new().unwrap();
        let path2 = temp2.path().join("other.yml");
        fs::write(&path2, "second file\n").unwrap();

        let formatter = HumanFormatter::new(false);
        let diags = vec![
            LintDiagnostic::new(RuleId::new("r1"), Severity::Error, "first")
                .with_span(Span::new(&path1, 1, 1, 1, 5)),
            LintDiagnostic::new(RuleId::new("r2"), Severity::Error, "second")
                .with_span(Span::new(&path2, 1, 1, 1, 6)),
            LintDiagnostic::new(RuleId::new("r3"), Severity::Warning, "first again")
                .with_span(Span::new(&path1, 1, 5, 1, 11)),
        ];

        let mut out = Vec::new();
        formatter.format(&diags, &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();

        // The first file should appear in two diagnostics, the second in one.
        let first_idx = s.find(&path1.display().to_string()).unwrap();
        let second_idx = s.find(&path2.display().to_string()).unwrap();
        assert_ne!(first_idx, second_idx);
    }

    #[test]
    fn format_report_renders_card_with_aligned_rows() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("cfg.yml");
        fs::write(&path, "# empty\n").unwrap();

        let formatter = HumanFormatter::new(false);
        let mut report = HumanReport::new();
        report.push_card(FileCard {
            path: path.clone(),
            display: "./cfg.yml".to_string(),
            label: "project config".to_string(),
            stats: vec![
                ("Steps".to_string(), "3 defined".to_string()),
                ("Workflows".to_string(), "1 (default)".to_string()),
            ],
            diagnostics: vec![],
        });

        let mut out = Vec::new();
        formatter.format_report(&report, &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();

        assert!(s.contains("./cfg.yml (project config)"), "got:\n{s}");
        assert!(s.contains("Errors:     0"), "got:\n{s}");
        // The "Steps" label should be padded to align with "Workflows".
        assert!(s.contains("Steps:      3 defined"), "got:\n{s}");
        assert!(s.contains("Workflows:  1 (default)"), "got:\n{s}");
    }

    #[test]
    fn format_report_includes_diagnostic_under_errors_row() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("cfg.yml");
        fs::write(&path, "alpha\nbroken\n").unwrap();

        let formatter = HumanFormatter::new(false);
        let mut report = HumanReport::new();
        report.push_card(FileCard {
            path: path.clone(),
            display: "./cfg.yml".to_string(),
            label: "project config".to_string(),
            stats: vec![],
            diagnostics: vec![
                LintDiagnostic::new(RuleId::new("rule"), Severity::Error, "boom")
                    .with_span(Span::new(&path, 2, 1, 2, 7)),
            ],
        });

        let mut out = Vec::new();
        formatter.format_report(&report, &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();

        assert!(s.contains("Errors:  1"));
        assert!(s.contains("error[rule]: boom"));
        assert!(s.contains("broken"));
        assert!(s.contains("^"));
    }

    #[test]
    fn format_report_emits_blank_line_between_cards() {
        let temp = TempDir::new().unwrap();
        let p1 = temp.path().join("a.yml");
        let p2 = temp.path().join("b.yml");
        fs::write(&p1, "a\n").unwrap();
        fs::write(&p2, "b\n").unwrap();

        let formatter = HumanFormatter::new(false);
        let mut report = HumanReport::new();
        for (name, p) in [("a", &p1), ("b", &p2)] {
            report.push_card(FileCard {
                path: p.clone(),
                display: format!("./{name}.yml"),
                label: "project config".to_string(),
                stats: vec![],
                diagnostics: vec![],
            });
        }

        let mut out = Vec::new();
        formatter.format_report(&report, &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();

        // Find both card headers and verify a blank line separates them.
        let lines: Vec<&str> = s.lines().collect();
        let i1 = lines.iter().position(|l| l.starts_with("./a.yml")).unwrap();
        let i2 = lines.iter().position(|l| l.starts_with("./b.yml")).unwrap();
        assert!(i2 > i1);
        // Between i1's card and i2's header, there must be at least one
        // empty line.
        assert!(
            (i1..i2).any(|k| lines[k].is_empty()),
            "no blank between cards: {s}"
        );
    }

    #[test]
    fn format_report_includes_context_files() {
        let formatter = HumanFormatter::new(false);
        let mut report = HumanReport::new();
        report.push_context_file("./.bivvy/config.yml");

        let mut out = Vec::new();
        formatter.format_report(&report, &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("Loaded for context (not validated): ./.bivvy/config.yml"));
    }

    #[test]
    fn format_report_warnings_row_only_when_present() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("c.yml");
        fs::write(&path, "x\n").unwrap();
        let formatter = HumanFormatter::new(false);

        let mut without = HumanReport::new();
        without.push_card(FileCard {
            path: path.clone(),
            display: "./c.yml".into(),
            label: "project config".into(),
            stats: vec![],
            diagnostics: vec![],
        });
        let mut out_w = Vec::new();
        formatter.format_report(&without, &mut out_w).unwrap();
        let no_warn = String::from_utf8(out_w).unwrap();
        assert!(!no_warn.contains("Warnings"));

        let mut with = HumanReport::new();
        with.push_card(FileCard {
            path: path.clone(),
            display: "./c.yml".into(),
            label: "project config".into(),
            stats: vec![],
            diagnostics: vec![LintDiagnostic::new(
                RuleId::new("r"),
                Severity::Warning,
                "w",
            )],
        });
        let mut out_y = Vec::new();
        formatter.format_report(&with, &mut out_y).unwrap();
        let with_warn = String::from_utf8(out_y).unwrap();
        assert!(with_warn.contains("Warnings:"));
    }

    #[test]
    fn display_path_uses_dot_slash_for_project_relative() {
        let project = PathBuf::from("/proj");
        let p = PathBuf::from("/proj/.bivvy/config.yml");
        let s = display_path(&p, &project, None);
        assert_eq!(s, "./.bivvy/config.yml");
    }

    #[test]
    fn display_path_uses_home_for_home_relative() {
        let project = PathBuf::from("/proj");
        let home = PathBuf::from("/home/me");
        let p = PathBuf::from("/home/me/.bivvy/config.yml");
        let s = display_path(&p, &project, Some(&home));
        assert_eq!(s, "$HOME/.bivvy/config.yml");
    }

    #[test]
    fn display_path_falls_back_to_absolute() {
        let project = PathBuf::from("/proj");
        let p = PathBuf::from("/elsewhere/file.yml");
        let s = display_path(&p, &project, None);
        assert_eq!(s, "/elsewhere/file.yml");
    }
}
