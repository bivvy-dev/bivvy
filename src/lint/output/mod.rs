//! Lint output formatters.
//!
//! This module provides formatters for outputting lint diagnostics
//! in different formats (human-readable, JSON, SARIF).

pub mod human;
pub mod json;
pub mod sarif;

use crate::lint::LintDiagnostic;
use std::io::Write;

/// Output format for lint results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Human,
    Json,
    Sarif,
}

/// Trait for formatting lint output.
pub trait LintFormatter {
    /// Format diagnostics to the given writer.
    fn format<W: Write>(
        &self,
        diagnostics: &[LintDiagnostic],
        writer: &mut W,
    ) -> std::io::Result<()>;
}

pub use human::HumanFormatter;
pub use json::JsonFormatter;
pub use sarif::SarifFormatter;
