//! Variable interpolation for configuration values.
//!
//! Bivvy supports variable interpolation in config values using `${variable}` syntax.
//!
//! # Syntax
//!
//! - `${variable_name}` - replaced with variable value
//! - `$${escaped}` - produces literal `${escaped}` in output
//!
//! # Example
//!
//! ```yaml
//! command: "echo Hello, ${name}!"
//! # With name="World", produces: echo Hello, World!
//! ```

use crate::error::{BivvyError, Result};
use std::collections::{HashMap, HashSet};

/// A segment of an interpolated string.
#[derive(Debug, Clone, PartialEq)]
pub enum Segment {
    /// Literal text
    Literal(String),
    /// Variable reference: ${name}
    Variable(String),
}

/// Parse a string containing ${var} interpolations.
///
/// Supports:
/// - `${variable_name}` - variable interpolation
/// - `$${escaped}` - literal `${escaped}` in output
///
/// # Returns
///
/// Vec of segments representing the parsed string
pub fn parse_interpolation(input: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut chars = input.chars().peekable();
    let mut current_literal = String::new();

    while let Some(c) = chars.next() {
        if c == '$' {
            match chars.peek() {
                Some('$') => {
                    // Escaped: $$ becomes $
                    chars.next();
                    if chars.peek() == Some(&'{') {
                        // $${...} -> literal ${...}
                        chars.next(); // consume {
                        current_literal.push('$');
                        current_literal.push('{');
                        // Read until closing brace
                        while let Some(&c) = chars.peek() {
                            chars.next();
                            current_literal.push(c);
                            if c == '}' {
                                break;
                            }
                        }
                    } else {
                        current_literal.push('$');
                    }
                }
                Some('{') => {
                    // Start of variable
                    chars.next(); // consume {

                    // Flush current literal
                    if !current_literal.is_empty() {
                        segments.push(Segment::Literal(std::mem::take(&mut current_literal)));
                    }

                    // Read variable name until }
                    let mut var_name = String::new();
                    while let Some(&c) = chars.peek() {
                        if c == '}' {
                            chars.next();
                            break;
                        }
                        var_name.push(chars.next().unwrap());
                    }

                    segments.push(Segment::Variable(var_name));
                }
                _ => {
                    current_literal.push(c);
                }
            }
        } else {
            current_literal.push(c);
        }
    }

    // Flush remaining literal
    if !current_literal.is_empty() {
        segments.push(Segment::Literal(current_literal));
    }

    segments
}

/// Extract all variable names from an interpolated string.
///
/// Returns unique variable names found in the string.
pub fn extract_variables(input: &str) -> HashSet<String> {
    parse_interpolation(input)
        .into_iter()
        .filter_map(|seg| match seg {
            Segment::Variable(name) => Some(name),
            _ => None,
        })
        .collect()
}

/// Check if a string contains any interpolation.
pub fn has_interpolation(input: &str) -> bool {
    parse_interpolation(input)
        .iter()
        .any(|seg| matches!(seg, Segment::Variable(_)))
}

/// Context for variable resolution.
///
/// Variables are resolved in priority order:
/// 1. Prompt values from current run (highest priority)
/// 2. Saved preferences from previous runs
/// 3. Environment variables
/// 4. Built-in variables (lowest priority)
#[derive(Debug, Default)]
pub struct InterpolationContext {
    /// Prompt values from current run
    pub prompts: HashMap<String, String>,

    /// Saved preferences from previous runs
    pub preferences: HashMap<String, String>,

    /// Environment variables
    pub env: HashMap<String, String>,

    /// Built-in variables (project_name, project_root, bivvy_version)
    pub builtins: HashMap<String, String>,
}

impl InterpolationContext {
    /// Create a new context with built-in variables.
    pub fn new() -> Self {
        let mut builtins = HashMap::new();
        builtins.insert(
            "bivvy_version".to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
        );

        Self {
            builtins,
            ..Default::default()
        }
    }

    /// Add project information to builtins.
    pub fn with_project(mut self, name: &str, root: &std::path::Path) -> Self {
        self.builtins
            .insert("project_name".to_string(), name.to_string());
        self.builtins
            .insert("project_root".to_string(), root.display().to_string());
        self
    }

    /// Add environment variables from a HashMap.
    pub fn with_env(mut self, env: HashMap<String, String>) -> Self {
        self.env = env;
        self
    }

    /// Resolve a variable name to its value.
    ///
    /// Resolution order: prompts > preferences > env > builtins
    pub fn resolve(&self, name: &str) -> Option<String> {
        self.prompts
            .get(name)
            .or_else(|| self.preferences.get(name))
            .or_else(|| self.env.get(name))
            .or_else(|| self.builtins.get(name))
            .cloned()
    }
}

/// Resolve all variables in an interpolated string.
///
/// # Errors
///
/// Returns `ConfigValidationError` if any variable is not found in the context.
pub fn resolve_string(input: &str, context: &InterpolationContext) -> Result<String> {
    let segments = parse_interpolation(input);
    let mut result = String::new();

    for segment in segments {
        match segment {
            Segment::Literal(text) => result.push_str(&text),
            Segment::Variable(name) => {
                let value =
                    context
                        .resolve(&name)
                        .ok_or_else(|| BivvyError::ConfigValidationError {
                            message: format!("Unresolved variable: ${{{}}}", name),
                        })?;
                result.push_str(&value);
            }
        }
    }

    Ok(result)
}

/// Resolve string with optional default value for missing variables.
///
/// Unlike `resolve_string`, this never fails - missing variables
/// are replaced with the provided default.
pub fn resolve_string_with_default(
    input: &str,
    context: &InterpolationContext,
    default: &str,
) -> String {
    let segments = parse_interpolation(input);
    let mut result = String::new();

    for segment in segments {
        match segment {
            Segment::Literal(text) => result.push_str(&text),
            Segment::Variable(name) => {
                let value = context
                    .resolve(&name)
                    .unwrap_or_else(|| default.to_string());
                result.push_str(&value);
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_literal_only() {
        let result = parse_interpolation("hello world");
        assert_eq!(result, vec![Segment::Literal("hello world".to_string())]);
    }

    #[test]
    fn parse_single_variable() {
        let result = parse_interpolation("${name}");
        assert_eq!(result, vec![Segment::Variable("name".to_string())]);
    }

    #[test]
    fn parse_variable_with_surrounding_text() {
        let result = parse_interpolation("hello ${name}!");
        assert_eq!(
            result,
            vec![
                Segment::Literal("hello ".to_string()),
                Segment::Variable("name".to_string()),
                Segment::Literal("!".to_string()),
            ]
        );
    }

    #[test]
    fn parse_multiple_variables() {
        let result = parse_interpolation("${a} and ${b}");
        assert_eq!(
            result,
            vec![
                Segment::Variable("a".to_string()),
                Segment::Literal(" and ".to_string()),
                Segment::Variable("b".to_string()),
            ]
        );
    }

    #[test]
    fn parse_escaped_dollar_brace() {
        let result = parse_interpolation("$${NOT_INTERPOLATED}");
        assert_eq!(
            result,
            vec![Segment::Literal("${NOT_INTERPOLATED}".to_string())]
        );
    }

    #[test]
    fn parse_mixed_escaped_and_real() {
        let result = parse_interpolation("echo '$${SKIP}' && ${real}");
        assert_eq!(
            result,
            vec![
                Segment::Literal("echo '${SKIP}' && ".to_string()),
                Segment::Variable("real".to_string()),
            ]
        );
    }

    #[test]
    fn parse_adjacent_variables() {
        let result = parse_interpolation("${a}${b}");
        assert_eq!(
            result,
            vec![
                Segment::Variable("a".to_string()),
                Segment::Variable("b".to_string()),
            ]
        );
    }

    #[test]
    fn parse_empty_string() {
        let result = parse_interpolation("");
        assert!(result.is_empty());
    }

    #[test]
    fn parse_dollar_without_brace() {
        let result = parse_interpolation("price is $100");
        assert_eq!(result, vec![Segment::Literal("price is $100".to_string())]);
    }

    #[test]
    fn extract_variables_returns_unique_names() {
        let vars = extract_variables("${a} ${b} ${a}");
        assert!(vars.contains("a"));
        assert!(vars.contains("b"));
        assert_eq!(vars.len(), 2);
    }

    #[test]
    fn extract_variables_empty_for_literal() {
        let vars = extract_variables("no variables here");
        assert!(vars.is_empty());
    }

    #[test]
    fn has_interpolation_returns_true_for_variables() {
        assert!(has_interpolation("hello ${name}"));
        assert!(!has_interpolation("hello world"));
        assert!(!has_interpolation("$${escaped}"));
    }

    #[test]
    fn parse_variable_with_underscore() {
        let result = parse_interpolation("${my_variable_name}");
        assert_eq!(
            result,
            vec![Segment::Variable("my_variable_name".to_string())]
        );
    }

    #[test]
    fn parse_variable_with_numbers() {
        let result = parse_interpolation("${var123}");
        assert_eq!(result, vec![Segment::Variable("var123".to_string())]);
    }

    #[test]
    fn resolve_string_replaces_variables() {
        let mut ctx = InterpolationContext::new();
        ctx.prompts.insert("name".to_string(), "world".to_string());

        let result = resolve_string("hello ${name}!", &ctx).unwrap();
        assert_eq!(result, "hello world!");
    }

    #[test]
    fn resolve_string_uses_priority_order() {
        let mut ctx = InterpolationContext::new();
        ctx.prompts
            .insert("var".to_string(), "from_prompt".to_string());
        ctx.preferences
            .insert("var".to_string(), "from_prefs".to_string());
        ctx.env.insert("var".to_string(), "from_env".to_string());

        // Prompts have highest priority
        let result = resolve_string("${var}", &ctx).unwrap();
        assert_eq!(result, "from_prompt");

        // Remove prompts, should use preferences
        ctx.prompts.clear();
        let result = resolve_string("${var}", &ctx).unwrap();
        assert_eq!(result, "from_prefs");

        // Remove preferences, should use env
        ctx.preferences.clear();
        let result = resolve_string("${var}", &ctx).unwrap();
        assert_eq!(result, "from_env");
    }

    #[test]
    fn resolve_string_fails_on_missing_variable() {
        let ctx = InterpolationContext::new();
        let result = resolve_string("${missing}", &ctx);
        assert!(matches!(
            result,
            Err(BivvyError::ConfigValidationError { .. })
        ));
    }

    #[test]
    fn resolve_string_with_default_uses_default() {
        let ctx = InterpolationContext::new();
        let result = resolve_string_with_default("${missing}", &ctx, "default");
        assert_eq!(result, "default");
    }

    #[test]
    fn resolve_string_with_default_prefers_context() {
        let mut ctx = InterpolationContext::new();
        ctx.env.insert("found".to_string(), "value".to_string());
        let result = resolve_string_with_default("${found}", &ctx, "default");
        assert_eq!(result, "value");
    }

    #[test]
    fn context_includes_builtin_bivvy_version() {
        let ctx = InterpolationContext::new();
        assert!(ctx.builtins.contains_key("bivvy_version"));
    }

    #[test]
    fn context_with_project_adds_project_vars() {
        use std::path::Path;
        let ctx = InterpolationContext::new().with_project("myapp", Path::new("/home/user/myapp"));

        assert_eq!(ctx.builtins.get("project_name"), Some(&"myapp".to_string()));
        assert!(ctx.builtins.get("project_root").unwrap().contains("myapp"));
    }

    #[test]
    fn context_with_env_sets_env() {
        let mut env = HashMap::new();
        env.insert("MY_VAR".to_string(), "my_value".to_string());

        let ctx = InterpolationContext::new().with_env(env);
        assert_eq!(ctx.env.get("MY_VAR"), Some(&"my_value".to_string()));
    }

    #[test]
    fn resolve_multiple_variables() {
        let mut ctx = InterpolationContext::new();
        ctx.prompts.insert("first".to_string(), "Hello".to_string());
        ctx.prompts
            .insert("second".to_string(), "World".to_string());

        let result = resolve_string("${first}, ${second}!", &ctx).unwrap();
        assert_eq!(result, "Hello, World!");
    }

    #[test]
    fn resolve_preserves_escaped() {
        let ctx = InterpolationContext::new();
        let result = resolve_string("$${NOT_RESOLVED}", &ctx).unwrap();
        assert_eq!(result, "${NOT_RESOLVED}");
    }
}
