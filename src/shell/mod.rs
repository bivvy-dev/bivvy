//! Shell command execution and environment management.

pub mod command;
pub mod debug;
pub mod platform;
pub mod refresh;
pub mod resume;

pub use command::{
    execute, execute_check, execute_quiet, execute_streaming, CommandOptions, CommandResult,
    OutputCallback, OutputLine,
};
pub use platform::{detect_shell, is_ci, is_elevated, ShellInfo, ShellType};
pub use refresh::{PathChangeDetector, PathChangeResult, ShellReloadInfo};
pub use resume::{ReloadChoice, ResumeState};
