//! Environment variable handling.
//!
//! This module handles loading environment variables from various sources
//! and detecting sensitive values that should be masked in output.

use crate::error::{BivvyError, Result};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Built-in secret patterns that are always masked.
///
/// These patterns use simple glob-style matching where `*` matches
/// any characters.
pub const SECRET_PATTERNS: &[&str] = &[
    "*_KEY",
    "*_SECRET",
    "*_TOKEN",
    "*_PASSWORD",
    "*_CREDENTIAL",
    "API_KEY",
    "SECRET_KEY",
    "ACCESS_TOKEN",
    "AUTH_TOKEN",
    "DATABASE_URL",
    "REDIS_URL",
    "AWS_*",
    "GITHUB_TOKEN",
    "OPENAI_API_KEY",
];

/// Load environment variables from system.
pub fn load_system_env() -> HashMap<String, String> {
    std::env::vars().collect()
}

/// Load environment variables from a dotenv-style file.
///
/// # Format
///
/// ```text
/// # Comment
/// KEY=value
/// QUOTED="value with spaces"
/// SINGLE='also works'
/// ```
///
/// # Errors
///
/// Returns `ConfigNotFound` if the file doesn't exist.
/// Returns `ConfigParseError` for invalid lines.
pub fn load_env_file(path: &Path) -> Result<HashMap<String, String>> {
    let content = fs::read_to_string(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            BivvyError::ConfigNotFound {
                path: path.to_path_buf(),
            }
        } else {
            BivvyError::Io(e)
        }
    })?;

    parse_dotenv(&content, path)
}

/// Load env file if it exists, return empty map otherwise.
pub fn load_env_file_optional(path: &Path) -> HashMap<String, String> {
    load_env_file(path).unwrap_or_default()
}

/// Strip a single layer of matching `"`/`"` or `'`/`'` quotes, if present.
///
/// Returns the original string slice when there are no matching quotes,
/// when the open and close quote types differ, or when the input is too
/// short to have both. UTF-8 safe: uses `strip_prefix`/`strip_suffix`
/// which operate on `char` boundaries.
fn strip_matching_quotes(value: &str) -> &str {
    if let Some(inner) = value.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        return inner;
    }
    if let Some(inner) = value.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
        return inner;
    }
    value
}

/// Parse dotenv-style content.
fn parse_dotenv(content: &str, source_path: &Path) -> Result<HashMap<String, String>> {
    let mut env = HashMap::new();

    for (line_num, line) in content.lines().enumerate() {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Parse KEY=value
        if let Some(eq_pos) = line.find('=') {
            let key = line[..eq_pos].trim().to_string();
            let raw = line[eq_pos + 1..].trim();

            // Remove surrounding matching quotes if present. Use
            // strip_prefix/strip_suffix so that values containing multibyte
            // characters at either boundary do not panic on byte-offset
            // slicing.
            let value = strip_matching_quotes(raw).to_string();

            env.insert(key, value);
        } else {
            return Err(BivvyError::ConfigParseError {
                path: source_path.to_path_buf(),
                message: format!("Invalid line {}: {}", line_num + 1, line),
            });
        }
    }

    Ok(env)
}

/// Check if an environment variable name matches secret patterns.
///
/// Checks against both built-in `SECRET_PATTERNS` and any additional
/// patterns provided.
pub fn is_secret(name: &str, additional_patterns: &[String]) -> bool {
    let all_patterns: Vec<&str> = SECRET_PATTERNS
        .iter()
        .copied()
        .chain(additional_patterns.iter().map(|s| s.as_str()))
        .collect();

    for pattern in all_patterns {
        if matches_pattern(name, pattern) {
            return true;
        }
    }

    false
}

/// Simple glob-style pattern matching (* matches any characters).
fn matches_pattern(name: &str, pattern: &str) -> bool {
    // *MIDDLE* — contains. Use strip_prefix + strip_suffix so multibyte
    // characters between the asterisks aren't sliced on a byte boundary.
    if let Some(middle) = pattern.strip_prefix('*').and_then(|p| p.strip_suffix('*')) {
        name.contains(middle)
    } else if let Some(suffix) = pattern.strip_prefix('*') {
        // *SUFFIX - ends with
        name.ends_with(suffix)
    } else if let Some(prefix) = pattern.strip_suffix('*') {
        // PREFIX* - starts with
        name.starts_with(prefix)
    } else {
        // Exact match
        name == pattern
    }
}

/// Merge environment maps in precedence order.
///
/// Values in `overlay` take precedence over values in `base`.
pub fn merge_env(
    base: &HashMap<String, String>,
    overlay: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut result = base.clone();
    result.extend(overlay.iter().map(|(k, v)| (k.clone(), v.clone())));
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn parse_dotenv_basic() {
        let content = "KEY=value\nOTHER=123";
        let env = parse_dotenv(content, Path::new("test")).unwrap();
        assert_eq!(env.get("KEY"), Some(&"value".to_string()));
        assert_eq!(env.get("OTHER"), Some(&"123".to_string()));
    }

    #[test]
    fn parse_dotenv_strips_quotes() {
        let content = "QUOTED=\"hello world\"\nSINGLE='test'";
        let env = parse_dotenv(content, Path::new("test")).unwrap();
        assert_eq!(env.get("QUOTED"), Some(&"hello world".to_string()));
        assert_eq!(env.get("SINGLE"), Some(&"test".to_string()));
    }

    #[test]
    fn parse_dotenv_skips_comments_and_empty() {
        let content = "# Comment\n\nKEY=value\n  # Another comment";
        let env = parse_dotenv(content, Path::new("test")).unwrap();
        assert_eq!(env.len(), 1);
        assert!(env.contains_key("KEY"));
    }

    #[test]
    fn parse_dotenv_handles_equals_in_value() {
        let content = "URL=postgres://user:pass@host/db?param=value";
        let env = parse_dotenv(content, Path::new("test")).unwrap();
        assert_eq!(
            env.get("URL"),
            Some(&"postgres://user:pass@host/db?param=value".to_string())
        );
    }

    #[test]
    fn parse_dotenv_rejects_invalid_lines() {
        let content = "VALID=true\ninvalid line\nOTHER=value";
        let result = parse_dotenv(content, Path::new("test"));
        assert!(matches!(result, Err(BivvyError::ConfigParseError { .. })));
    }

    #[test]
    fn load_env_file_works() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join(".env");
        fs::write(&path, "TEST=value").unwrap();

        let env = load_env_file(&path).unwrap();
        assert_eq!(env.get("TEST"), Some(&"value".to_string()));
    }

    #[test]
    fn load_env_file_not_found() {
        let result = load_env_file(Path::new("/nonexistent/.env"));
        assert!(matches!(result, Err(BivvyError::ConfigNotFound { .. })));
    }

    #[test]
    fn load_env_file_optional_returns_empty() {
        let env = load_env_file_optional(Path::new("/nonexistent/.env"));
        assert!(env.is_empty());
    }

    #[test]
    fn is_secret_matches_builtin_patterns() {
        assert!(is_secret("API_KEY", &[]));
        assert!(is_secret("AWS_ACCESS_KEY", &[]));
        assert!(is_secret("DATABASE_URL", &[]));
        assert!(is_secret("MY_SECRET", &[]));
        assert!(is_secret("AUTH_TOKEN", &[]));
        assert!(!is_secret("RAILS_ENV", &[]));
        assert!(!is_secret("DEBUG", &[]));
    }

    #[test]
    fn is_secret_matches_custom_patterns() {
        let custom = vec!["CUSTOM_*".to_string()];
        assert!(is_secret("CUSTOM_VALUE", &custom));
        assert!(!is_secret("CUSTOM_VALUE", &[]));
    }

    #[test]
    fn is_secret_matches_suffix_patterns() {
        assert!(is_secret("MY_PASSWORD", &[]));
        assert!(is_secret("DB_CREDENTIAL", &[]));
    }

    #[test]
    fn merge_env_overlays_correctly() {
        let mut base = HashMap::new();
        base.insert("A".to_string(), "1".to_string());
        base.insert("B".to_string(), "2".to_string());

        let mut overlay = HashMap::new();
        overlay.insert("B".to_string(), "3".to_string());
        overlay.insert("C".to_string(), "4".to_string());

        let result = merge_env(&base, &overlay);
        assert_eq!(result.get("A"), Some(&"1".to_string()));
        assert_eq!(result.get("B"), Some(&"3".to_string()));
        assert_eq!(result.get("C"), Some(&"4".to_string()));
    }

    #[test]
    fn parse_dotenv_handles_multibyte_quoted_value() {
        // Quoted value with a multibyte character at the closing boundary.
        // Byte-offset slicing would panic; strip_prefix/strip_suffix is safe.
        let content = "GREETING=\"caf\u{00E9}\"";
        let env = parse_dotenv(content, Path::new("test")).unwrap();
        assert_eq!(env.get("GREETING"), Some(&"caf\u{00E9}".to_string()));
    }

    #[test]
    fn parse_dotenv_keeps_mismatched_quotes() {
        // Mismatched quotes shouldn't be stripped — neither prefix matches
        // the suffix, so the value is returned as-is.
        let content = "MIXED=\"caf\u{00E9}'";
        let env = parse_dotenv(content, Path::new("test")).unwrap();
        assert_eq!(env.get("MIXED"), Some(&"\"caf\u{00E9}'".to_string()));
    }

    #[test]
    fn matches_pattern_contains_handles_multibyte_middle() {
        // Pattern *cafe-acute* matches names that contain the multibyte
        // middle. Byte-slicing the pattern would panic.
        assert!(matches_pattern(
            "PREFIX_caf\u{00E9}_SUFFIX",
            "*caf\u{00E9}*"
        ));
        assert!(!matches_pattern("PREFIX_cafe_SUFFIX", "*caf\u{00E9}*"));
    }

    #[test]
    fn load_system_env_returns_current_env() {
        std::env::set_var("BIVVY_TEST_VAR", "test_value");
        let env = load_system_env();
        assert_eq!(env.get("BIVVY_TEST_VAR"), Some(&"test_value".to_string()));
        std::env::remove_var("BIVVY_TEST_VAR");
    }
}
