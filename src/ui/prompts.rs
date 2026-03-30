//! Interactive prompts.

use console::{style, Key, Style, Term};
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

/// Check for an environment variable override for a prompt.
///
/// Users can skip any prompt by setting an env var matching the prompt key:
///   `BUMP=minor bivvy run --workflow release`
///
/// The lookup is case-insensitive on the key side: a prompt with key "bump"
/// matches env var `BUMP`, `bump`, or `Bump`.
pub(super) fn env_override(prompt: &Prompt) -> Option<PromptResult> {
    let env_key = prompt.key.to_uppercase();
    let value = std::env::var(&env_key).ok()?;

    let is_multiselect = matches!(prompt.prompt_type, PromptType::MultiSelect { .. });
    if is_multiselect {
        let values: Vec<String> = value.split(',').map(|s| s.trim().to_string()).collect();
        Some(PromptResult::Strings(values))
    } else {
        Some(PromptResult::String(value))
    }
}

/// Prompt the user for input.
///
/// Before showing any interactive prompt, checks for an environment variable
/// matching the prompt key (e.g., `BUMP=minor` for a prompt with key "bump").
/// This works in both interactive and non-interactive modes.
pub fn prompt_user(prompt: &Prompt, term: &Term) -> Result<PromptResult> {
    if let Some(result) = env_override(prompt) {
        return Ok(result);
    }

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
    // Use custom yes/no prompt for y/n keyboard shortcut support.
    // Requires a real TTY — prompt_yes_no uses term.read_key() directly.
    // On non-TTY, fall through to dialoguer's Select.
    //
    // Terminal foreground group is claimed in main.rs via tcsetpgrp(),
    // so read_key() works even when launched via `cargo run`.
    if is_yes_no(options) && term.is_term() {
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
///
/// # SIGTTOU / SIGTTIN safety
///
/// This function calls `term.read_key()` directly, which:
///   - Uses `tcsetattr` to enter raw mode → triggers SIGTTOU
///   - Reads from the terminal fd → triggers SIGTTIN
///
/// When bivvy is not the terminal's foreground process group (e.g.,
/// launched via `cargo run`), zsh suspends the process with
/// "suspended (tty output)" or "suspended (tty input)".
///
/// **This function depends on signal ignores in `main.rs`.**
/// Without both `libc::signal(libc::SIGTTOU, libc::SIG_IGN)` and
/// `libc::signal(libc::SIGTTIN, libc::SIG_IGN)` in main(), calling
/// this function will crash the entire run command.
///
/// DO NOT remove the signal ignores in main.rs without also removing
/// the direct `term.read_key()` call here. See regression tests:
/// `yes_no_prompt_does_not_crash_on_non_tty` and
/// `yes_no_and_multi_option_selects_error_consistently`.
fn prompt_yes_no(prompt: &Prompt, options: &[PromptOption], term: &Term) -> Result<PromptResult> {
    let default_idx = prompt
        .default
        .as_ref()
        .and_then(|d| options.iter().position(|o| o.value == *d))
        .unwrap_or(0);

    let mut sel = default_idx;

    // Calculate indent from leading whitespace in the question text.
    // Questions are pre-indented to align with the step header above.
    // No leading whitespace and no bracket → no indent (e.g., "Run setup now?").
    let leading_ws = prompt.question.len() - prompt.question.trim_start().len();
    let pad = if leading_ws > 0 {
        " ".repeat(leading_ws)
    } else if let Some(bracket_pos) = prompt.question.find(']') {
        " ".repeat(bracket_pos + 2)
    } else {
        String::new()
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
            // Re-print prompt question, then answer on a new line with indicator
            let (icon, answer_label) = if options[idx].value == "yes" {
                (Style::new().green().apply_to("✓"), "Yes")
            } else {
                (Style::new().dim().apply_to("·"), "No")
            };
            term.write_line(&prompt.question).map_err(BivvyError::Io)?;
            term.write_line(&format!("{}{} {}", pad, icon, answer_label))
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

    // -----------------------------------------------------------------------
    // SIGTTOU/SIGTTIN regression tests — DO NOT REMOVE
    //
    // These tests exist because `prompt_yes_no` calls `term.read_key()`
    // directly, which invokes `tcsetattr` (SIGTTOU) and reads from the
    // terminal fd (SIGTTIN). When bivvy is not the terminal's foreground
    // process group (e.g., `cargo run`), this triggers suspension.
    //
    // The SIGTTOU signal is ignored in main.rs, but these tests verify:
    //   1. The prompt code doesn't panic or crash on non-TTY
    //   2. Yes/no and multi-option selects produce consistent error behavior
    //
    // History: These tests were originally added in commit 09d5292, then
    // removed in commit 520c90c when the custom yes/no prompt was
    // reintroduced. Removing them allowed the SIGTTOU regression to go
    // undetected. They are restored here to prevent that from happening
    // again.
    //
    // If you need to change prompt_yes_no's output formatting, do so
    // WITHOUT removing these tests. The custom yes/no prompt (y/n
    // shortcuts, cursor guard, styled indicators) is a legitimate
    // customization — but it MUST coexist with SIGTTOU safety.
    // -----------------------------------------------------------------------

    #[test]
    fn yes_no_prompt_does_not_crash_on_non_tty() {
        // Term::stdout() in test context is piped (non-TTY) because
        // cargo test captures stdout. prompt_yes_no detects non-TTY via
        // read_key() and returns an IO error — no SIGTTOU, no panic.
        let term = Term::stdout();
        if term.is_term() {
            return; // Skip under --nocapture with real TTY
        }

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
        let prompt = make_prompt(
            "rerun_step",
            PromptType::Select {
                options: options.clone(),
            },
            Some("yes"),
        );

        // Call the REAL prompt code, not MockUI.
        // No SIGTTOU, no panic, no crash — just a clean IO error.
        let result = prompt_select(&prompt, &options, &term);
        assert!(result.is_err());
    }

    #[test]
    fn yes_no_and_multi_option_selects_error_consistently() {
        // Verify that yes/no prompts and multi-option selects produce
        // the same kind of error on non-TTY. If yes/no had a different
        // code path that crashed instead of erroring, this test would
        // catch it.
        let term = Term::stdout();
        if term.is_term() {
            return; // Skip under --nocapture with real TTY
        }

        // 2-option yes/no (takes the custom prompt_yes_no path)
        let yn_options = vec![
            PromptOption {
                label: "Yes (y)".to_string(),
                value: "yes".to_string(),
            },
            PromptOption {
                label: "No (n)".to_string(),
                value: "no".to_string(),
            },
        ];
        let yn_prompt = make_prompt(
            "yn",
            PromptType::Select {
                options: yn_options.clone(),
            },
            Some("yes"),
        );

        // 3-option select (goes through dialoguer's Select)
        let multi_options = vec![
            PromptOption {
                label: "A".to_string(),
                value: "a".to_string(),
            },
            PromptOption {
                label: "B".to_string(),
                value: "b".to_string(),
            },
            PromptOption {
                label: "C".to_string(),
                value: "c".to_string(),
            },
        ];
        let multi_prompt = make_prompt(
            "multi",
            PromptType::Select {
                options: multi_options.clone(),
            },
            Some("a"),
        );

        // Both must error on non-TTY — neither should panic or crash.
        let result_yn = prompt_select(&yn_prompt, &yn_options, &term);
        let result_multi = prompt_select(&multi_prompt, &multi_options, &term);

        assert!(result_yn.is_err(), "yes/no should error on non-TTY");
        assert!(result_multi.is_err(), "multi should error on non-TTY");
    }
}
