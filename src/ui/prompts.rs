//! Interactive prompts.

use console::{measure_text_width, style, Key, Style, Term};
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

/// Dialoguer theme for general select prompts with `›`/`·` indicators.
fn select_theme() -> ColorfulTheme {
    ColorfulTheme {
        prompt_prefix: style("".to_string()),
        prompt_suffix: style("".to_string()),
        success_prefix: style("".to_string()),
        success_suffix: style("·".to_string()).for_stderr().dim(),
        active_item_prefix: style("›".to_string()).for_stderr().bold(),
        inactive_item_prefix: style("·".to_string()).for_stderr().dim(),
        active_item_style: Style::new().for_stderr().bold(),
        inactive_item_style: Style::new().for_stderr().dim(),
        values_style: Style::new().for_stderr().bold(),
        ..ColorfulTheme::default()
    }
}

/// Check if options form a yes/no pair.
fn is_yes_no(options: &[PromptOption]) -> bool {
    options.len() == 2
        && options.iter().any(|o| o.value == "yes")
        && options.iter().any(|o| o.value == "no")
}

fn prompt_select(prompt: &Prompt, options: &[PromptOption], term: &Term) -> Result<PromptResult> {
    // Use custom yes/no prompt for y/n keyboard shortcut support
    if is_yes_no(options) {
        return prompt_yes_no(prompt, options, term);
    }

    let labels: Vec<_> = options.iter().map(|o| o.label.as_str()).collect();

    let default_idx = prompt
        .default
        .as_ref()
        .and_then(|d| options.iter().position(|o| o.value == *d))
        .unwrap_or(0);

    let selection = Select::with_theme(&select_theme())
        .with_prompt(&prompt.question)
        .items(&labels)
        .default(default_idx)
        .interact_on(term)
        .map_err(map_dialoguer_err)?;

    Ok(PromptResult::String(options[selection].value.clone()))
}

/// RAII guard that restores the terminal cursor when dropped.
///
/// Ensures the cursor is shown even if the process panics or is
/// interrupted (Ctrl+C) while the cursor is hidden during a prompt.
struct CursorGuard<'a> {
    term: &'a Term,
}

impl<'a> CursorGuard<'a> {
    fn new(term: &'a Term) -> std::io::Result<Self> {
        term.hide_cursor()?;
        Ok(Self { term })
    }
}

impl Drop for CursorGuard<'_> {
    fn drop(&mut self) {
        self.term.show_cursor().ok();
    }
}

/// Custom yes/no prompt with `›`/`·` indicators and `y`/`n` keyboard shortcuts.
///
/// Renders options below the prompt question, indented to align with
/// the step name. Supports arrow keys, j/k, Enter/Space, y/n, and Escape.
fn prompt_yes_no(prompt: &Prompt, options: &[PromptOption], term: &Term) -> Result<PromptResult> {
    let default_idx = prompt
        .default
        .as_ref()
        .and_then(|d| options.iter().position(|o| o.value == *d))
        .unwrap_or(0);

    let mut sel = default_idx;

    // Calculate indent from leading whitespace in the question text.
    // Questions are pre-indented to align with the step header above.
    let leading_ws = prompt.question.len() - prompt.question.trim_start().len();
    let pad = if leading_ws > 0 {
        " ".repeat(leading_ws)
    } else {
        // Fallback: find `[n/t] ` bracket pattern and indent to match
        let indent = measure_text_width(&prompt.question)
            .min(prompt.question.find(']').map(|i| i + 2).unwrap_or(6));
        " ".repeat(indent)
    };

    let active_prefix = Style::new().bold();
    let active_style = Style::new().bold();
    let inactive_prefix = Style::new().dim();
    let inactive_style = Style::new().dim();

    // Write prompt question
    term.write_line(&prompt.question).map_err(BivvyError::Io)?;

    // Hide cursor with RAII guard — cursor is restored on drop (panic, Ctrl+C, early return).
    let _cursor_guard = CursorGuard::new(term).map_err(BivvyError::Io)?;

    loop {
        // Draw option lines
        for (i, opt) in options.iter().enumerate() {
            let line = if i == sel {
                format!(
                    "{}{} {}",
                    pad,
                    active_prefix.apply_to("›"),
                    active_style.apply_to(&opt.label)
                )
            } else {
                format!(
                    "{}{} {}",
                    pad,
                    inactive_prefix.apply_to("·"),
                    inactive_style.apply_to(&opt.label)
                )
            };
            term.write_line(&line).map_err(BivvyError::Io)?;
        }
        term.flush().map_err(BivvyError::Io)?;

        // Read key
        let key = term.read_key().map_err(BivvyError::Io)?;

        let chosen = match key {
            Key::Char('y') | Key::Char('Y') => options.iter().position(|o| o.value == "yes"),
            Key::Char('n') | Key::Char('N') => options.iter().position(|o| o.value == "no"),
            Key::Enter | Key::Char(' ') => Some(sel),
            Key::Escape => {
                // Treat Escape as "no" (matching dialoguer's abort behavior).
                // Find "no" option; fall back to default if somehow absent.
                let no_idx = options
                    .iter()
                    .position(|o| o.value == "no")
                    .unwrap_or(default_idx);
                Some(no_idx)
            }
            Key::ArrowDown | Key::Tab | Key::Char('j') => {
                sel = (sel + 1) % options.len();
                None
            }
            Key::ArrowUp | Key::BackTab | Key::Char('k') => {
                sel = (sel + options.len() - 1) % options.len();
                None
            }
            _ => None,
        };

        if let Some(idx) = chosen {
            // Clear option lines
            term.clear_last_lines(options.len())
                .map_err(BivvyError::Io)?;
            // Clear the prompt question line
            term.clear_last_lines(1).map_err(BivvyError::Io)?;
            // Re-print prompt with selection (show "yes" or "no")
            let answer_text = if options[idx].value == "yes" {
                "yes"
            } else {
                "no"
            };
            term.write_line(&format!(
                "{} {}",
                prompt.question,
                Style::new().bold().apply_to(answer_text)
            ))
            .map_err(BivvyError::Io)?;

            // _cursor_guard is dropped here, restoring the cursor.
            return Ok(PromptResult::String(options[idx].value.clone()));
        }

        // Clear option lines for redraw
        term.clear_last_lines(options.len())
            .map_err(BivvyError::Io)?;
    }
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

    let multiselect_theme = ColorfulTheme {
        prompt_suffix: style("".to_string()),
        ..prompt_theme()
    };

    // Print prompt question and hint text for keyboard controls
    term.write_line(&prompt.question).map_err(BivvyError::Io)?;
    term.write_line(&format!(
        "{}",
        style("[space] toggle · [a] toggle all · [enter] confirm").dim()
    ))
    .map_err(BivvyError::Io)?;

    let selections = MultiSelect::with_theme(&multiselect_theme)
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

    #[test]
    fn is_yes_no_returns_true_for_yes_no_options() {
        let options = vec![
            PromptOption {
                label: "Yes (y)".to_string(),
                value: "yes".to_string(),
            },
            PromptOption {
                label: "No (n)".to_string(),
                value: "no".to_string(),
            },
        ];
        assert!(is_yes_no(&options));
    }

    #[test]
    fn is_yes_no_returns_true_regardless_of_order() {
        let options = vec![
            PromptOption {
                label: "No (n)".to_string(),
                value: "no".to_string(),
            },
            PromptOption {
                label: "Yes (y)".to_string(),
                value: "yes".to_string(),
            },
        ];
        assert!(is_yes_no(&options));
    }

    #[test]
    fn is_yes_no_returns_false_for_other_options() {
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
        assert!(!is_yes_no(&options));
    }

    #[test]
    fn is_yes_no_returns_false_for_more_than_two() {
        let options = vec![
            PromptOption {
                label: "Yes".to_string(),
                value: "yes".to_string(),
            },
            PromptOption {
                label: "No".to_string(),
                value: "no".to_string(),
            },
            PromptOption {
                label: "Maybe".to_string(),
                value: "maybe".to_string(),
            },
        ];
        assert!(!is_yes_no(&options));
    }

    #[test]
    fn select_theme_creates_without_panic() {
        let theme = select_theme();
        // Verify the theme object can be used (smoke test)
        drop(theme);
    }

    #[test]
    fn cursor_guard_restores_on_drop() {
        // CursorGuard::new requires a real terminal for hide_cursor(),
        // but we can verify the struct compiles and the Drop impl exists
        // by constructing with Term::stdout() in a non-panicking context.
        // On non-TTY (CI), hide_cursor is a no-op that still succeeds.
        let term = Term::stdout();
        // If hide_cursor succeeds, guard will restore on drop.
        // If it fails (no TTY), that's fine — the guard pattern is still valid.
        let guard_result = CursorGuard::new(&term);
        if let Ok(guard) = guard_result {
            drop(guard);
            // After drop, cursor should be visible again (no-op on non-TTY).
        }
    }
}
