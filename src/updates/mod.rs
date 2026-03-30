//! Update checking and self-updating functionality.
//!
//! This module provides:
//! - Install method detection (cargo, homebrew, manual)
//! - Version checking against latest release
//! - Auto-update prompting and execution
//! - Background auto-update with binary staging

pub mod auto_update;
pub mod install;
pub mod prompt;
pub mod version;

pub use auto_update::{
    apply_staged_update, perform_background_update, should_spawn_background_update,
    spawn_background_update,
};
pub use install::{detect_install_method, get_install_path, InstallMethod};
pub use prompt::{
    check_and_prompt_update, is_notification_suppressed, show_update_notification,
    suppress_notification,
};
pub use version::{check_for_updates, check_for_updates_fresh, clear_cache, UpdateInfo, VERSION};
