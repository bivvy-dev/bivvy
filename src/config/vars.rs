//! User-defined variable evaluation.
//!
//! Evaluates `vars:` definitions from config into resolved string values
//! that can be injected into [`InterpolationContext`](super::InterpolationContext).

use crate::config::schema::VarDefinition;
use crate::error::{BivvyError, Result};
use crate::shell::execute_quiet;
use std::collections::HashMap;
use std::path::Path;

/// Evaluate all variable definitions into resolved string values.
///
/// Static vars pass through unchanged. Computed vars run the command via
/// `shell::execute_quiet` and use trimmed stdout. Fails if any computed
/// var's command returns non-zero.
pub fn evaluate_vars(
    vars: &HashMap<String, VarDefinition>,
    project_root: &Path,
) -> Result<HashMap<String, String>> {
    let mut resolved = HashMap::new();

    for (name, def) in vars {
        let value = match def {
            VarDefinition::Static(s) => s.clone(),
            VarDefinition::Computed { command } => {
                let result = execute_quiet(command, Some(project_root))?;
                if !result.success {
                    return Err(BivvyError::ConfigValidationError {
                        message: format!(
                            "Variable '{}' command failed (exit {:?}): {}",
                            name,
                            result.exit_code,
                            result.stderr.trim()
                        ),
                    });
                }
                result.stdout.trim().to_string()
            }
        };
        resolved.insert(name.clone(), value);
    }

    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluate_static_var() {
        let mut vars = HashMap::new();
        vars.insert(
            "name".to_string(),
            VarDefinition::Static("bivvy".to_string()),
        );

        let temp = tempfile::TempDir::new().unwrap();
        let result = evaluate_vars(&vars, temp.path()).unwrap();

        assert_eq!(result.get("name"), Some(&"bivvy".to_string()));
    }

    #[test]
    fn evaluate_computed_var() {
        let mut vars = HashMap::new();
        vars.insert(
            "greeting".to_string(),
            VarDefinition::Computed {
                command: "echo hello".to_string(),
            },
        );

        let temp = tempfile::TempDir::new().unwrap();
        let result = evaluate_vars(&vars, temp.path()).unwrap();

        assert_eq!(result.get("greeting"), Some(&"hello".to_string()));
    }

    #[test]
    fn evaluate_computed_var_trims_whitespace() {
        let mut vars = HashMap::new();
        vars.insert(
            "val".to_string(),
            VarDefinition::Computed {
                command: "printf '  hello  \n\n'".to_string(),
            },
        );

        let temp = tempfile::TempDir::new().unwrap();
        let result = evaluate_vars(&vars, temp.path()).unwrap();

        assert_eq!(result.get("val"), Some(&"hello".to_string()));
    }

    #[test]
    fn evaluate_computed_var_fails_on_nonzero() {
        let mut vars = HashMap::new();
        vars.insert(
            "bad".to_string(),
            VarDefinition::Computed {
                command: "exit 1".to_string(),
            },
        );

        let temp = tempfile::TempDir::new().unwrap();
        let result = evaluate_vars(&vars, temp.path());

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("bad"),
            "error should mention var name: {}",
            err
        );
    }

    #[test]
    fn evaluate_empty_vars() {
        let vars = HashMap::new();
        let temp = tempfile::TempDir::new().unwrap();
        let result = evaluate_vars(&vars, temp.path()).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn evaluate_mixed_vars() {
        let mut vars = HashMap::new();
        vars.insert(
            "static_var".to_string(),
            VarDefinition::Static("hello".to_string()),
        );
        vars.insert(
            "computed_var".to_string(),
            VarDefinition::Computed {
                command: "echo world".to_string(),
            },
        );

        let temp = tempfile::TempDir::new().unwrap();
        let result = evaluate_vars(&vars, temp.path()).unwrap();

        assert_eq!(result.get("static_var"), Some(&"hello".to_string()));
        assert_eq!(result.get("computed_var"), Some(&"world".to_string()));
    }
}
