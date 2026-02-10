//! Auto-update prompting and execution.

use anyhow::Result;
use std::process::Command;

use super::{check_for_updates, detect_install_method, InstallMethod, UpdateInfo};
use crate::ui::{Prompt, PromptResult, PromptType, UserInterface};

/// Check for updates and prompt user if update available.
///
/// Returns true if an update was executed.
pub fn check_and_prompt_update(ui: &mut dyn UserInterface) -> Result<bool> {
    // Only check in interactive mode
    if !ui.is_interactive() {
        return Ok(false);
    }

    // Check for updates
    let info = match check_for_updates() {
        Some(info) => info,
        None => return Ok(false),
    };

    // No update available
    if !info.update_available {
        return Ok(false);
    }

    // Detect install method
    let method = detect_install_method();

    // Prompt for update
    if prompt_for_update(ui, &info, &method)? {
        execute_update(&method)?;
        return Ok(true);
    }

    Ok(false)
}

/// Prompt the user to update.
fn prompt_for_update(
    ui: &mut dyn UserInterface,
    info: &UpdateInfo,
    method: &InstallMethod,
) -> Result<bool> {
    ui.message(&format!(
        "A new version of bivvy is available: {} -> {}",
        info.current, info.latest
    ));

    if !method.supports_auto_update() {
        if let Some(url) = &info.release_url {
            ui.message(&format!("Download from: {}", url));
        }
        return Ok(false);
    }

    let prompt = Prompt {
        key: "update_bivvy".to_string(),
        question: "Would you like to update now?".to_string(),
        prompt_type: PromptType::Confirm,
        default: Some("true".to_string()),
    };

    match ui.prompt(&prompt)? {
        PromptResult::Bool(confirmed) => Ok(confirmed),
        _ => Ok(false),
    }
}

/// Execute the update using the appropriate method.
fn execute_update(method: &InstallMethod) -> Result<()> {
    let command = method
        .update_command()
        .ok_or_else(|| anyhow::anyhow!("No update command for install method"))?;

    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        anyhow::bail!("Empty update command");
    }

    let status = Command::new(parts[0]).args(&parts[1..]).status()?;

    if !status.success() {
        anyhow::bail!("Update command failed with exit code: {:?}", status.code());
    }

    Ok(())
}

/// Show update notification without prompting.
pub fn show_update_notification(ui: &mut dyn UserInterface) {
    if let Some(info) = check_for_updates() {
        if info.update_available {
            ui.message(&format!(
                "Update available: {} -> {} (run `bivvy update` to upgrade)",
                info.current, info.latest
            ));
        }
    }
}

/// Suppress update notification for the current session.
pub fn suppress_notification() {
    // This could use a session marker file or environment variable
    std::env::set_var("BIVVY_SUPPRESS_UPDATE", "1");
}

/// Check if update notification is suppressed.
pub fn is_notification_suppressed() -> bool {
    std::env::var("BIVVY_SUPPRESS_UPDATE").is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::MockUI;
    use chrono::Utc;
    use std::path::PathBuf;

    #[test]
    fn suppress_notification_sets_env() {
        suppress_notification();
        assert!(is_notification_suppressed());
    }

    #[test]
    fn install_method_provides_update_command() {
        let cargo = InstallMethod::Cargo;
        assert!(cargo.update_command().is_some());

        let homebrew = InstallMethod::Homebrew;
        assert!(homebrew.update_command().is_some());
    }

    #[test]
    fn check_and_prompt_update_non_interactive() {
        let mut ui = MockUI::new();
        // MockUI is non-interactive, so this should return false
        let result = check_and_prompt_update(&mut ui).unwrap();
        assert!(!result);
    }

    #[test]
    fn prompt_for_update_with_manual_install() {
        let mut ui = MockUI::new();
        let info = UpdateInfo {
            current: "0.1.0".to_string(),
            latest: "0.2.0".to_string(),
            update_available: true,
            release_url: Some("https://github.com/example/releases".to_string()),
            checked_at: Utc::now(),
        };
        let method = InstallMethod::Manual {
            path: PathBuf::from("/tmp/bivvy"),
        };

        let result = prompt_for_update(&mut ui, &info, &method).unwrap();
        assert!(!result); // Manual installs don't support auto-update

        // Should have shown the download URL
        assert!(ui.has_message("Download from:"));
    }

    #[test]
    fn prompt_for_update_with_manual_no_url() {
        let mut ui = MockUI::new();
        let info = UpdateInfo {
            current: "0.1.0".to_string(),
            latest: "0.2.0".to_string(),
            update_available: true,
            release_url: None,
            checked_at: Utc::now(),
        };
        let method = InstallMethod::Manual {
            path: PathBuf::from("/tmp/bivvy"),
        };

        let result = prompt_for_update(&mut ui, &info, &method).unwrap();
        assert!(!result);

        // Should have shown the version info
        assert!(ui.has_message("0.1.0 -> 0.2.0"));
    }

    #[test]
    fn prompt_for_update_with_unknown_install() {
        let mut ui = MockUI::new();
        let info = UpdateInfo {
            current: "0.1.0".to_string(),
            latest: "0.2.0".to_string(),
            update_available: true,
            release_url: None,
            checked_at: Utc::now(),
        };
        let method = InstallMethod::Unknown;

        let result = prompt_for_update(&mut ui, &info, &method).unwrap();
        assert!(!result);
    }

    #[test]
    fn prompt_for_update_with_cargo() {
        let mut ui = MockUI::new();
        // Set response for the confirm prompt
        ui.set_prompt_response("update_bivvy", "false");

        let info = UpdateInfo {
            current: "0.1.0".to_string(),
            latest: "0.2.0".to_string(),
            update_available: true,
            release_url: None,
            checked_at: Utc::now(),
        };
        let method = InstallMethod::Cargo;

        // This will prompt but MockUI returns String, not Bool
        let result = prompt_for_update(&mut ui, &info, &method).unwrap();
        // MockUI returns PromptResult::String, not Bool, so this is Ok(false)
        assert!(!result);
    }

    #[test]
    fn show_update_notification_with_ui() {
        let mut ui = MockUI::new();
        // This will check for updates which may or may not find anything
        // Mostly testing that it doesn't panic
        show_update_notification(&mut ui);
    }

    #[test]
    fn suppress_and_check_notification() {
        // Test the flow: suppress then check
        suppress_notification();
        assert!(is_notification_suppressed());
        // Note: we don't clear env vars because tests run in parallel
        // and other tests may depend on this state
    }
}
