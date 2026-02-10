//! Interactive prompts.

use console::{style, Term};
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Confirm, Input, MultiSelect, Select};

use crate::error::{BivvyError, Result};

use super::{Prompt, PromptOption, PromptResult, PromptType};

/// Convert dialoguer errors to BivvyError.
fn map_dialoguer_err(e: dialoguer::Error) -> BivvyError {
    BivvyError::Io(e.into())
}

/// Dialoguer theme without the default yellow `?` prefix.
fn prompt_theme() -> ColorfulTheme {
    ColorfulTheme {
        prompt_prefix: style("".to_string()),
        ..ColorfulTheme::default()
    }
}

/// Prompt the user for input.
pub fn prompt_user(prompt: &Prompt, term: &Term) -> Result<PromptResult> {
    match &prompt.prompt_type {
        PromptType::Confirm => prompt_confirm(prompt, term),
        PromptType::Input => prompt_input(prompt, term),
        PromptType::Select { options } => prompt_select(prompt, options, term),
        PromptType::MultiSelect { options } => prompt_multiselect(prompt, options, term),
    }
}

fn prompt_confirm(prompt: &Prompt, term: &Term) -> Result<PromptResult> {
    let default = prompt
        .default
        .as_ref()
        .map(|s| s.to_lowercase() == "true" || s == "y" || s == "yes")
        .unwrap_or(true);

    let result = Confirm::new()
        .with_prompt(&prompt.question)
        .default(default)
        .interact_on(term)
        .map_err(map_dialoguer_err)?;

    Ok(PromptResult::Bool(result))
}

fn prompt_input(prompt: &Prompt, term: &Term) -> Result<PromptResult> {
    let input = Input::<String>::new().with_prompt(&prompt.question);

    let result: String = if let Some(default) = &prompt.default {
        input
            .default(default.clone())
            .interact_on(term)
            .map_err(map_dialoguer_err)?
    } else {
        input.interact_on(term).map_err(map_dialoguer_err)?
    };

    Ok(PromptResult::String(result))
}

fn prompt_select(prompt: &Prompt, options: &[PromptOption], term: &Term) -> Result<PromptResult> {
    let labels: Vec<_> = options.iter().map(|o| o.label.as_str()).collect();

    let default_idx = prompt
        .default
        .as_ref()
        .and_then(|d| options.iter().position(|o| o.value == *d))
        .unwrap_or(0);

    let selection = Select::new()
        .with_prompt(&prompt.question)
        .items(&labels)
        .default(default_idx)
        .interact_on(term)
        .map_err(map_dialoguer_err)?;

    Ok(PromptResult::String(options[selection].value.clone()))
}

fn prompt_multiselect(
    prompt: &Prompt,
    options: &[PromptOption],
    term: &Term,
) -> Result<PromptResult> {
    let labels: Vec<_> = options.iter().map(|o| o.label.as_str()).collect();

    let default_values: Vec<&str> = prompt
        .default
        .as_deref()
        .map(|d| d.split(',').collect())
        .unwrap_or_default();
    let defaults: Vec<bool> = options
        .iter()
        .map(|o| default_values.contains(&o.value.as_str()))
        .collect();

    let selections = MultiSelect::with_theme(&prompt_theme())
        .with_prompt(&prompt.question)
        .items(&labels)
        .defaults(&defaults)
        .interact_on(term)
        .map_err(map_dialoguer_err)?;

    let values: Vec<String> = selections
        .iter()
        .map(|&i| options[i].value.clone())
        .collect();

    Ok(PromptResult::Strings(values))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_prompt(key: &str, prompt_type: PromptType, default: Option<&str>) -> Prompt {
        Prompt {
            key: key.to_string(),
            question: "Test question?".to_string(),
            prompt_type,
            default: default.map(String::from),
        }
    }

    #[test]
    fn prompt_creation() {
        let prompt = make_prompt("test", PromptType::Input, Some("default"));
        assert_eq!(prompt.key, "test");
        assert_eq!(prompt.default, Some("default".to_string()));
    }

    #[test]
    fn prompt_type_confirm_creation() {
        let prompt = make_prompt("confirm", PromptType::Confirm, None);
        assert!(matches!(prompt.prompt_type, PromptType::Confirm));
    }

    #[test]
    fn prompt_type_select_with_options() {
        let options = vec![
            PromptOption {
                label: "Option A".to_string(),
                value: "a".to_string(),
            },
            PromptOption {
                label: "Option B".to_string(),
                value: "b".to_string(),
            },
        ];
        let prompt = make_prompt(
            "select",
            PromptType::Select {
                options: options.clone(),
            },
            None,
        );
        if let PromptType::Select { options: stored } = prompt.prompt_type {
            assert_eq!(stored.len(), 2);
            assert_eq!(stored[0].value, "a");
        } else {
            panic!("Expected Select variant");
        }
    }

    #[test]
    fn prompt_type_multiselect_with_options() {
        let options = vec![PromptOption {
            label: "Feature 1".to_string(),
            value: "f1".to_string(),
        }];
        let prompt = make_prompt(
            "multi",
            PromptType::MultiSelect {
                options: options.clone(),
            },
            None,
        );
        if let PromptType::MultiSelect { options: stored } = prompt.prompt_type {
            assert_eq!(stored.len(), 1);
        } else {
            panic!("Expected MultiSelect variant");
        }
    }
}
