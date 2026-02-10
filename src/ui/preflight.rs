//! Preflight prompt collection.

use std::collections::HashMap;

use crate::error::{BivvyError, Result};

use super::{Prompt, UserInterface};

/// Collects all prompts from steps before execution begins.
pub struct PreflightCollector;

impl PreflightCollector {
    /// Collect all prompts from the given steps interactively.
    ///
    /// Returns a map of prompt key to prompt value.
    pub fn collect(
        prompts: &[Prompt],
        saved_preferences: &HashMap<String, String>,
        ui: &mut dyn UserInterface,
    ) -> Result<HashMap<String, String>> {
        let mut values = HashMap::new();

        if prompts.is_empty() {
            return Ok(values);
        }

        ui.show_header("Before we begin, a few questions:");

        for (idx, prompt) in prompts.iter().enumerate() {
            // Check for saved preference first
            if let Some(saved) = saved_preferences.get(&prompt.key) {
                values.insert(prompt.key.clone(), saved.clone());
                ui.message(&format!(
                    "Using saved preference for '{}': {}",
                    prompt.key, saved
                ));
                continue;
            }

            ui.show_progress(idx + 1, prompts.len());
            let result = ui.prompt(prompt)?;
            values.insert(prompt.key.clone(), result.as_string());
        }

        Ok(values)
    }

    /// Collect prompts in non-interactive mode using defaults.
    pub fn collect_non_interactive(
        prompts: &[Prompt],
        env_overrides: &HashMap<String, String>,
    ) -> Result<HashMap<String, String>> {
        let mut values = HashMap::new();

        for prompt in prompts {
            // Check environment override first
            let env_key = format!("BIVVY_PROMPT_{}", prompt.key.to_uppercase());
            if let Some(override_value) = env_overrides.get(&env_key) {
                values.insert(prompt.key.clone(), override_value.clone());
                continue;
            }

            // Use default if available
            if let Some(default) = &prompt.default {
                values.insert(prompt.key.clone(), default.clone());
            } else {
                return Err(BivvyError::ConfigValidationError {
                    message: format!(
                        "Prompt '{}' has no default value and cannot be used in non-interactive mode",
                        prompt.key
                    ),
                });
            }
        }

        Ok(values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::PromptType;

    fn make_prompt(key: &str, default: Option<&str>) -> Prompt {
        Prompt {
            key: key.to_string(),
            question: "Test question?".to_string(),
            prompt_type: PromptType::Input,
            default: default.map(String::from),
        }
    }

    #[test]
    fn non_interactive_uses_defaults() {
        let prompts = vec![make_prompt("key1", Some("default_value"))];
        let env = HashMap::new();

        let values = PreflightCollector::collect_non_interactive(&prompts, &env).unwrap();

        assert_eq!(values.get("key1"), Some(&"default_value".to_string()));
    }

    #[test]
    fn non_interactive_uses_env_override() {
        let prompts = vec![make_prompt("key1", Some("default"))];
        let mut env = HashMap::new();
        env.insert("BIVVY_PROMPT_KEY1".to_string(), "override".to_string());

        let values = PreflightCollector::collect_non_interactive(&prompts, &env).unwrap();

        assert_eq!(values.get("key1"), Some(&"override".to_string()));
    }

    #[test]
    fn non_interactive_fails_without_default() {
        let prompts = vec![make_prompt("key1", None)];
        let env = HashMap::new();

        let result = PreflightCollector::collect_non_interactive(&prompts, &env);

        assert!(result.is_err());
    }

    #[test]
    fn non_interactive_empty_prompts() {
        let prompts = vec![];
        let env = HashMap::new();

        let values = PreflightCollector::collect_non_interactive(&prompts, &env).unwrap();

        assert!(values.is_empty());
    }

    #[test]
    fn non_interactive_multiple_prompts() {
        let prompts = vec![
            make_prompt("key1", Some("value1")),
            make_prompt("key2", Some("value2")),
        ];
        let env = HashMap::new();

        let values = PreflightCollector::collect_non_interactive(&prompts, &env).unwrap();

        assert_eq!(values.len(), 2);
        assert_eq!(values.get("key1"), Some(&"value1".to_string()));
        assert_eq!(values.get("key2"), Some(&"value2".to_string()));
    }

    #[test]
    fn non_interactive_env_override_takes_precedence() {
        let prompts = vec![make_prompt("db_name", Some("default_db"))];
        let mut env = HashMap::new();
        env.insert("BIVVY_PROMPT_DB_NAME".to_string(), "env_db".to_string());

        let values = PreflightCollector::collect_non_interactive(&prompts, &env).unwrap();

        assert_eq!(values.get("db_name"), Some(&"env_db".to_string()));
    }

    #[test]
    fn non_interactive_mixed_sources() {
        let prompts = vec![
            make_prompt("key1", Some("default1")),
            make_prompt("key2", Some("default2")),
        ];
        let mut env = HashMap::new();
        env.insert("BIVVY_PROMPT_KEY2".to_string(), "override2".to_string());

        let values = PreflightCollector::collect_non_interactive(&prompts, &env).unwrap();

        assert_eq!(values.get("key1"), Some(&"default1".to_string()));
        assert_eq!(values.get("key2"), Some(&"override2".to_string()));
    }
}
