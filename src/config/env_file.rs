//! .env file parsing.
//!
//! This module provides functionality for parsing environment variable files
//! in the standard KEY=value format.

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

/// Parses .env files into a map of environment variables.
///
/// # Supported Formats
///
/// - Simple: `KEY=value`
/// - Quoted: `KEY="value with spaces"` or `KEY='single quoted'`
/// - Empty: `KEY=`
/// - Comments: `# This is a comment`
/// - Whitespace around equals: `KEY = value`
/// - Values with equals signs: `URL=https://example.com?foo=bar`
///
/// # Example
///
/// ```
/// use bivvy::config::EnvFileParser;
///
/// let content = r#"
/// # Database config
/// DATABASE_URL=postgres://localhost/db
/// DEBUG="true"
/// EMPTY=
/// "#;
///
/// let vars = EnvFileParser::parse(content).unwrap();
/// assert_eq!(vars.get("DATABASE_URL"), Some(&"postgres://localhost/db".to_string()));
/// assert_eq!(vars.get("DEBUG"), Some(&"true".to_string()));
/// assert_eq!(vars.get("EMPTY"), Some(&"".to_string()));
/// ```
pub struct EnvFileParser;

impl EnvFileParser {
    /// Parse an env file content string into a map of variables.
    pub fn parse(content: &str) -> Result<HashMap<String, String>> {
        let mut vars = HashMap::new();

        for line in content.lines() {
            let line = line.trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Parse KEY=value
            if let Some((key, value)) = Self::parse_line(line) {
                vars.insert(key, value);
            }
        }

        Ok(vars)
    }

    /// Parse a single line.
    fn parse_line(line: &str) -> Option<(String, String)> {
        let eq_pos = line.find('=')?;
        let key = line[..eq_pos].trim().to_string();
        let value = line[eq_pos + 1..].trim();

        // Handle quoted values
        let value = Self::unquote(value);

        Some((key, value))
    }

    /// Remove surrounding quotes from a value.
    fn unquote(value: &str) -> String {
        if (value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\''))
        {
            if value.len() >= 2 {
                value[1..value.len() - 1].to_string()
            } else {
                value.to_string()
            }
        } else {
            value.to_string()
        }
    }

    /// Load and parse an env file from a path.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use bivvy::config::EnvFileParser;
    /// use std::path::Path;
    ///
    /// let vars = EnvFileParser::load(Path::new(".env")).unwrap();
    /// for (key, value) in &vars {
    ///     println!("{}={}", key, value);
    /// }
    /// ```
    pub fn load(path: &Path) -> Result<HashMap<String, String>> {
        let content = std::fs::read_to_string(path)?;
        Self::parse(&content)
    }

    /// Load and parse an env file, returning empty map if file doesn't exist.
    pub fn load_optional(path: &Path) -> Result<HashMap<String, String>> {
        if path.exists() {
            Self::load(path)
        } else {
            Ok(HashMap::new())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_env_file() {
        let content = r#"
KEY1=value1
KEY2=value2
"#;

        let vars = EnvFileParser::parse(content).unwrap();

        assert_eq!(vars.get("KEY1"), Some(&"value1".to_string()));
        assert_eq!(vars.get("KEY2"), Some(&"value2".to_string()));
    }

    #[test]
    fn skips_comments() {
        let content = r#"
# This is a comment
KEY=value
# Another comment
"#;

        let vars = EnvFileParser::parse(content).unwrap();

        assert_eq!(vars.len(), 1);
        assert_eq!(vars.get("KEY"), Some(&"value".to_string()));
    }

    #[test]
    fn handles_quoted_values() {
        let content = r#"
DOUBLE="double quoted"
SINGLE='single quoted'
UNQUOTED=no quotes
"#;

        let vars = EnvFileParser::parse(content).unwrap();

        assert_eq!(vars.get("DOUBLE"), Some(&"double quoted".to_string()));
        assert_eq!(vars.get("SINGLE"), Some(&"single quoted".to_string()));
        assert_eq!(vars.get("UNQUOTED"), Some(&"no quotes".to_string()));
    }

    #[test]
    fn handles_empty_values() {
        let content = "EMPTY=";

        let vars = EnvFileParser::parse(content).unwrap();

        assert_eq!(vars.get("EMPTY"), Some(&"".to_string()));
    }

    #[test]
    fn handles_values_with_equals() {
        let content = "URL=https://example.com?foo=bar";

        let vars = EnvFileParser::parse(content).unwrap();

        assert_eq!(
            vars.get("URL"),
            Some(&"https://example.com?foo=bar".to_string())
        );
    }

    #[test]
    fn handles_whitespace_around_equals() {
        let content = "KEY = value with spaces";

        let vars = EnvFileParser::parse(content).unwrap();

        assert_eq!(vars.get("KEY"), Some(&"value with spaces".to_string()));
    }

    #[test]
    fn skips_empty_lines() {
        let content = r#"
KEY1=value1

KEY2=value2

"#;

        let vars = EnvFileParser::parse(content).unwrap();

        assert_eq!(vars.len(), 2);
    }

    #[test]
    fn handles_lines_without_equals() {
        let content = r#"
KEY1=value1
invalid line without equals
KEY2=value2
"#;

        let vars = EnvFileParser::parse(content).unwrap();

        assert_eq!(vars.len(), 2);
    }

    #[test]
    fn load_optional_returns_empty_for_missing_file() {
        let result = EnvFileParser::load_optional(Path::new("/nonexistent/path/.env"));

        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn complex_env_file() {
        let content = r#"
# Application settings
APP_NAME=MyApp
DEBUG=true

# Database
DATABASE_URL="postgres://user:pass@localhost:5432/db"

# API Keys
API_KEY='secret-key-123'
WEBHOOK_URL=https://api.example.com/webhook?token=abc&id=123
"#;

        let vars = EnvFileParser::parse(content).unwrap();

        assert_eq!(vars.get("APP_NAME"), Some(&"MyApp".to_string()));
        assert_eq!(vars.get("DEBUG"), Some(&"true".to_string()));
        assert_eq!(
            vars.get("DATABASE_URL"),
            Some(&"postgres://user:pass@localhost:5432/db".to_string())
        );
        assert_eq!(vars.get("API_KEY"), Some(&"secret-key-123".to_string()));
        assert!(vars.get("WEBHOOK_URL").unwrap().contains("token=abc"));
    }
}
