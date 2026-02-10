//! Secret pattern matching.
//!
//! This module provides functionality for detecting secret environment variables
//! by matching their names against patterns.

use regex::Regex;

/// A pattern that identifies secret values.
#[derive(Debug, Clone)]
pub struct SecretPattern {
    /// Name of this pattern (for debugging).
    pub name: String,
    /// Regex pattern to match environment variable names.
    pub env_pattern: Regex,
}

/// Built-in patterns for common secrets.
///
/// Each tuple contains (name, regex_pattern).
pub const BUILTIN_PATTERNS: &[(&str, &str)] = &[
    ("api_key", r"(?i)^.*_?(API_?KEY|APIKEY)$"),
    ("secret", r"(?i)^.*_?(SECRET|SECRET_KEY)$"),
    ("token", r"(?i)^.*_?(TOKEN|ACCESS_TOKEN|AUTH_TOKEN)$"),
    ("password", r"(?i)^.*_?(PASSWORD|PASSWD|PWD)$"),
    ("credential", r"(?i)^.*_?CREDENTIAL$"),
    ("private_key", r"(?i)^.*_?PRIVATE_KEY$"),
    (
        "connection_string",
        r"(?i)^.*(CONNECTION_STRING|DATABASE_URL)$",
    ),
];

/// Matches environment variable names against secret patterns.
///
/// # Example
///
/// ```
/// use bivvy::secrets::SecretMatcher;
///
/// let matcher = SecretMatcher::with_builtins();
///
/// // Common secret patterns are detected
/// assert!(matcher.is_secret("API_KEY"));
/// assert!(matcher.is_secret("GITHUB_TOKEN"));
/// assert!(matcher.is_secret("DB_PASSWORD"));
///
/// // Non-secrets are not flagged
/// assert!(!matcher.is_secret("PATH"));
/// assert!(!matcher.is_secret("HOME"));
/// ```
pub struct SecretMatcher {
    patterns: Vec<SecretPattern>,
}

impl SecretMatcher {
    /// Create a matcher with built-in patterns.
    pub fn with_builtins() -> Self {
        let patterns = BUILTIN_PATTERNS
            .iter()
            .map(|(name, pattern)| SecretPattern {
                name: name.to_string(),
                env_pattern: Regex::new(pattern).unwrap(),
            })
            .collect();

        Self { patterns }
    }

    /// Create a matcher with custom patterns.
    pub fn new(patterns: Vec<SecretPattern>) -> Self {
        Self { patterns }
    }

    /// Create a matcher with built-in patterns plus custom exact matches.
    ///
    /// # Example
    ///
    /// ```
    /// use bivvy::secrets::SecretMatcher;
    ///
    /// let custom = vec!["MY_CUSTOM_SECRET".to_string()];
    /// let matcher = SecretMatcher::with_builtins_and_custom(&custom);
    ///
    /// // Custom patterns are detected
    /// assert!(matcher.is_secret("MY_CUSTOM_SECRET"));
    /// // Built-ins still work
    /// assert!(matcher.is_secret("API_KEY"));
    /// ```
    pub fn with_builtins_and_custom(custom_names: &[String]) -> Self {
        let mut matcher = Self::with_builtins();

        for name in custom_names {
            // Add exact match pattern for each custom name
            if let Ok(pattern) = Regex::new(&format!("^{}$", regex::escape(name))) {
                matcher.add_pattern(SecretPattern {
                    name: format!("custom:{}", name),
                    env_pattern: pattern,
                });
            }
        }

        matcher
    }

    /// Add a custom pattern.
    pub fn add_pattern(&mut self, pattern: SecretPattern) {
        self.patterns.push(pattern);
    }

    /// Check if an environment variable name matches any secret pattern.
    pub fn is_secret(&self, env_name: &str) -> bool {
        self.patterns
            .iter()
            .any(|p| p.env_pattern.is_match(env_name))
    }

    /// Get all environment variable names that match secret patterns.
    pub fn find_secrets<'a>(&self, env_names: impl Iterator<Item = &'a str>) -> Vec<&'a str> {
        env_names.filter(|name| self.is_secret(name)).collect()
    }

    /// Get the number of patterns.
    pub fn pattern_count(&self) -> usize {
        self.patterns.len()
    }
}

impl Default for SecretMatcher {
    fn default() -> Self {
        Self::with_builtins()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_api_key_variants() {
        let matcher = SecretMatcher::with_builtins();

        assert!(matcher.is_secret("API_KEY"));
        assert!(matcher.is_secret("GITHUB_API_KEY"));
        assert!(matcher.is_secret("api_key"));
        assert!(matcher.is_secret("MY_APIKEY"));
    }

    #[test]
    fn matches_token_variants() {
        let matcher = SecretMatcher::with_builtins();

        assert!(matcher.is_secret("TOKEN"));
        assert!(matcher.is_secret("ACCESS_TOKEN"));
        assert!(matcher.is_secret("AUTH_TOKEN"));
        assert!(matcher.is_secret("GITHUB_TOKEN"));
    }

    #[test]
    fn matches_password_variants() {
        let matcher = SecretMatcher::with_builtins();

        assert!(matcher.is_secret("PASSWORD"));
        assert!(matcher.is_secret("DB_PASSWORD"));
        assert!(matcher.is_secret("MYSQL_PWD"));
    }

    #[test]
    fn matches_secret_variants() {
        let matcher = SecretMatcher::with_builtins();

        assert!(matcher.is_secret("SECRET"));
        assert!(matcher.is_secret("AWS_SECRET"));
        assert!(matcher.is_secret("SECRET_KEY"));
    }

    #[test]
    fn matches_connection_strings() {
        let matcher = SecretMatcher::with_builtins();

        assert!(matcher.is_secret("DATABASE_URL"));
        assert!(matcher.is_secret("CONNECTION_STRING"));
    }

    #[test]
    fn does_not_match_non_secrets() {
        let matcher = SecretMatcher::with_builtins();

        assert!(!matcher.is_secret("PATH"));
        assert!(!matcher.is_secret("HOME"));
        assert!(!matcher.is_secret("NODE_ENV"));
        assert!(!matcher.is_secret("DEBUG"));
        assert!(!matcher.is_secret("USER"));
    }

    #[test]
    fn finds_secrets_in_list() {
        let matcher = SecretMatcher::with_builtins();
        let env_names = ["PATH", "API_KEY", "HOME", "DATABASE_URL", "DEBUG"];

        let secrets = matcher.find_secrets(env_names.iter().copied());

        assert_eq!(secrets.len(), 2);
        assert!(secrets.contains(&"API_KEY"));
        assert!(secrets.contains(&"DATABASE_URL"));
    }

    #[test]
    fn custom_patterns_work() {
        let custom = vec!["MY_CUSTOM_SECRET".to_string()];
        let matcher = SecretMatcher::with_builtins_and_custom(&custom);

        assert!(matcher.is_secret("MY_CUSTOM_SECRET"));
        assert!(matcher.is_secret("API_KEY")); // Built-in still works
    }

    #[test]
    fn custom_patterns_are_exact_match() {
        let custom = vec!["SECRET_VAR".to_string()];
        let matcher = SecretMatcher::with_builtins_and_custom(&custom);

        assert!(matcher.is_secret("SECRET_VAR"));
        // Should not match partial
        assert!(!matcher.is_secret("SECRET_VAR_EXTRA"));
        assert!(!matcher.is_secret("MY_SECRET_VAR"));
    }

    #[test]
    fn empty_patterns_match_nothing() {
        let matcher = SecretMatcher::new(vec![]);

        assert!(!matcher.is_secret("API_KEY"));
        assert!(!matcher.is_secret("PASSWORD"));
    }

    #[test]
    fn pattern_count_is_correct() {
        let matcher = SecretMatcher::with_builtins();
        assert_eq!(matcher.pattern_count(), BUILTIN_PATTERNS.len());

        let custom = vec!["CUSTOM1".to_string(), "CUSTOM2".to_string()];
        let matcher = SecretMatcher::with_builtins_and_custom(&custom);
        assert_eq!(matcher.pattern_count(), BUILTIN_PATTERNS.len() + 2);
    }

    #[test]
    fn loads_custom_secret_patterns_from_config() {
        use crate::config::BivvyConfig;

        let config = r#"
app_name: test
settings:
  secret_env:
    - MY_CUSTOM_SECRET
    - ANOTHER_SECRET
"#;

        let parsed: BivvyConfig = serde_yaml::from_str(config).unwrap();

        assert_eq!(parsed.settings.secret_env.len(), 2);
        assert!(parsed
            .settings
            .secret_env
            .contains(&"MY_CUSTOM_SECRET".to_string()));
        assert!(parsed
            .settings
            .secret_env
            .contains(&"ANOTHER_SECRET".to_string()));
    }

    #[test]
    fn matcher_includes_custom_patterns_from_config() {
        use crate::config::BivvyConfig;

        let config = r#"
settings:
  secret_env:
    - MY_CUSTOM_SECRET
"#;

        let parsed: BivvyConfig = serde_yaml::from_str(config).unwrap();
        let matcher = SecretMatcher::with_builtins_and_custom(&parsed.settings.secret_env);

        assert!(matcher.is_secret("MY_CUSTOM_SECRET"));
        assert!(matcher.is_secret("API_KEY")); // Built-in still works
    }
}
