//! Automatic fix application.
//!
//! This module provides functionality for automatically fixing
//! configuration issues detected by lint rules.

use crate::lint::LintDiagnostic;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// A code fix that can be applied automatically.
#[derive(Debug, Clone)]
pub struct Fix {
    /// File to modify.
    pub file: PathBuf,
    /// Start byte offset.
    pub start: usize,
    /// End byte offset.
    pub end: usize,
    /// Replacement text.
    pub replacement: String,
}

/// Result of attempting to apply fixes.
#[derive(Debug)]
pub struct FixResult {
    /// Number of fixes applied.
    pub applied: usize,
    /// Diagnostics that couldn't be auto-fixed.
    pub unfixable: Vec<LintDiagnostic>,
    /// Errors that occurred during fixing.
    pub errors: Vec<String>,
}

/// Engine for applying automatic fixes.
pub struct FixEngine;

impl FixEngine {
    /// Create a new fix engine.
    pub fn new() -> Self {
        Self
    }

    /// Apply fixes from diagnostics that support auto-fix.
    pub fn apply_fixes(&self, diagnostics: &[LintDiagnostic], fixes: &[Fix]) -> FixResult {
        let mut applied = 0;
        let mut errors = Vec::new();

        // Group fixes by file
        let mut fixes_by_file: HashMap<&Path, Vec<&Fix>> = HashMap::new();
        for fix in fixes {
            fixes_by_file
                .entry(fix.file.as_path())
                .or_default()
                .push(fix);
        }

        // Apply fixes to each file
        for (file, file_fixes) in fixes_by_file {
            match self.apply_fixes_to_file(file, &file_fixes) {
                Ok(count) => applied += count,
                Err(e) => errors.push(format!("{}: {}", file.display(), e)),
            }
        }

        // Collect unfixable diagnostics
        let unfixable = diagnostics
            .iter()
            .filter(|d| d.suggestion.is_none())
            .cloned()
            .collect();

        FixResult {
            applied,
            unfixable,
            errors,
        }
    }

    /// Preview fixes without applying them.
    pub fn preview_fixes(&self, fixes: &[Fix]) -> Vec<String> {
        fixes
            .iter()
            .map(|f| {
                format!(
                    "{}:{}-{}: Replace with '{}'",
                    f.file.display(),
                    f.start,
                    f.end,
                    f.replacement
                )
            })
            .collect()
    }

    fn apply_fixes_to_file(&self, file: &Path, fixes: &[&Fix]) -> Result<usize, String> {
        let content =
            fs::read_to_string(file).map_err(|e| format!("Failed to read file: {}", e))?;

        // Sort fixes by start position (reverse order for safe replacement)
        let mut sorted_fixes = fixes.to_vec();
        sorted_fixes.sort_by(|a, b| b.start.cmp(&a.start));

        let mut new_content = content;
        for fix in &sorted_fixes {
            if fix.start <= new_content.len() && fix.end <= new_content.len() {
                new_content = format!(
                    "{}{}{}",
                    &new_content[..fix.start],
                    &fix.replacement,
                    &new_content[fix.end..]
                );
            }
        }

        fs::write(file, new_content).map_err(|e| format!("Failed to write file: {}", e))?;

        Ok(sorted_fixes.len())
    }
}

impl Default for FixEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn applies_simple_fix() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.yml");
        fs::write(&config_path, "app_name: \"My App\"\n").unwrap();

        let fix = Fix {
            file: config_path.clone(),
            start: 10,
            end: 18,
            replacement: "\"my-app\"".to_string(),
        };

        let engine = FixEngine::new();
        let result = engine.apply_fixes(&[], &[fix]);

        assert_eq!(result.applied, 1);

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("my-app"));
    }

    #[test]
    fn applies_multiple_fixes_to_same_file() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.yml");
        fs::write(&config_path, "aaa bbb ccc").unwrap();

        let fixes = vec![
            Fix {
                file: config_path.clone(),
                start: 0,
                end: 3,
                replacement: "AAA".to_string(),
            },
            Fix {
                file: config_path.clone(),
                start: 8,
                end: 11,
                replacement: "CCC".to_string(),
            },
        ];

        let engine = FixEngine::new();
        let result = engine.apply_fixes(&[], &fixes);

        assert_eq!(result.applied, 2);

        let content = fs::read_to_string(&config_path).unwrap();
        assert_eq!(content, "AAA bbb CCC");
    }

    #[test]
    fn reports_unfixable_diagnostics() {
        use crate::lint::{RuleId, Severity};

        let diagnostics = vec![LintDiagnostic::new(
            RuleId::new("circular-dependency"),
            Severity::Error,
            "Cannot auto-fix circular dependencies",
        )];

        let engine = FixEngine::new();
        let result = engine.apply_fixes(&diagnostics, &[]);

        assert_eq!(result.applied, 0);
        assert_eq!(result.unfixable.len(), 1);
    }

    #[test]
    fn preview_fixes_returns_descriptions() {
        let fix = Fix {
            file: PathBuf::from("config.yml"),
            start: 10,
            end: 20,
            replacement: "new_value".to_string(),
        };

        let engine = FixEngine::new();
        let previews = engine.preview_fixes(&[fix]);

        assert_eq!(previews.len(), 1);
        assert!(previews[0].contains("config.yml:10-20"));
        assert!(previews[0].contains("new_value"));
    }

    #[test]
    fn handles_file_not_found() {
        let fix = Fix {
            file: PathBuf::from("/nonexistent/path/config.yml"),
            start: 0,
            end: 5,
            replacement: "test".to_string(),
        };

        let engine = FixEngine::new();
        let result = engine.apply_fixes(&[], &[fix]);

        assert_eq!(result.applied, 0);
        assert_eq!(result.errors.len(), 1);
    }
}
