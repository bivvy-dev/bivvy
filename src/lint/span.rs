//! Source location spans.
//!
//! This module provides types for tracking source locations
//! in configuration files, enabling precise error reporting.

use std::path::PathBuf;

/// A source location span representing a range in a file.
#[derive(Debug, Clone)]
pub struct Span {
    /// File path.
    pub file: PathBuf,
    /// Starting line (1-indexed).
    pub start_line: usize,
    /// Starting column (1-indexed).
    pub start_col: usize,
    /// Ending line (1-indexed).
    pub end_line: usize,
    /// Ending column (1-indexed).
    pub end_col: usize,
}

impl Span {
    /// Create a span covering a single line.
    pub fn line(file: impl Into<PathBuf>, line: usize) -> Self {
        Self {
            file: file.into(),
            start_line: line,
            start_col: 1,
            end_line: line,
            end_col: usize::MAX,
        }
    }

    /// Create a span with precise positions.
    pub fn new(
        file: impl Into<PathBuf>,
        start_line: usize,
        start_col: usize,
        end_line: usize,
        end_col: usize,
    ) -> Self {
        Self {
            file: file.into(),
            start_line,
            start_col,
            end_line,
            end_col,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_line_constructor() {
        let span = Span::line("config.yml", 10);

        assert_eq!(span.start_line, 10);
        assert_eq!(span.end_line, 10);
        assert_eq!(span.start_col, 1);
    }

    #[test]
    fn span_new_constructor() {
        let span = Span::new("config.yml", 10, 5, 10, 20);

        assert_eq!(span.start_line, 10);
        assert_eq!(span.start_col, 5);
        assert_eq!(span.end_col, 20);
    }

    #[test]
    fn span_file_path() {
        let span = Span::line("path/to/config.yml", 1);

        assert_eq!(span.file, PathBuf::from("path/to/config.yml"));
    }
}
