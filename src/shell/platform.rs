//! Platform-specific shell detection.

use std::path::PathBuf;

/// Information about the current shell environment.
#[derive(Debug, Clone)]
pub struct ShellInfo {
    /// Shell executable path.
    pub executable: PathBuf,

    /// Shell name (bash, zsh, fish, powershell, cmd).
    pub name: ShellType,

    /// Config files that affect this shell.
    pub config_files: Vec<PathBuf>,

    /// Command to reload the shell.
    pub reload_command: String,
}

/// Known shell types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellType {
    Bash,
    Zsh,
    Fish,
    PowerShell,
    Cmd,
    Unknown,
}

impl ShellType {
    /// Parse shell type from executable name.
    pub fn from_executable(exe: &str) -> Self {
        let name = std::path::Path::new(exe)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();

        match name.as_str() {
            "bash" => ShellType::Bash,
            "zsh" => ShellType::Zsh,
            "fish" => ShellType::Fish,
            "powershell" | "pwsh" => ShellType::PowerShell,
            "cmd" => ShellType::Cmd,
            _ => ShellType::Unknown,
        }
    }
}

/// Detect the current shell environment.
pub fn detect_shell() -> ShellInfo {
    let executable = get_shell_executable();
    let shell_type = ShellType::from_executable(&executable.to_string_lossy());

    ShellInfo {
        executable: executable.clone(),
        name: shell_type,
        config_files: get_config_files(shell_type),
        reload_command: get_reload_command(shell_type, &executable),
    }
}

fn get_shell_executable() -> PathBuf {
    if cfg!(target_os = "windows") {
        std::env::var("COMSPEC")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("cmd.exe"))
    } else {
        std::env::var("SHELL")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/bin/sh"))
    }
}

fn get_config_files(shell_type: ShellType) -> Vec<PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();

    match shell_type {
        ShellType::Bash => vec![
            home.join(".bashrc"),
            home.join(".bash_profile"),
            home.join(".profile"),
        ],
        ShellType::Zsh => vec![
            home.join(".zshrc"),
            home.join(".zprofile"),
            home.join(".zshenv"),
        ],
        ShellType::Fish => vec![home.join(".config/fish/config.fish")],
        ShellType::PowerShell => {
            if let Some(docs) = dirs::document_dir() {
                vec![
                    docs.join("PowerShell/Microsoft.PowerShell_profile.ps1"),
                    docs.join("WindowsPowerShell/Microsoft.PowerShell_profile.ps1"),
                ]
            } else {
                vec![]
            }
        }
        ShellType::Cmd | ShellType::Unknown => vec![],
    }
}

fn get_reload_command(shell_type: ShellType, executable: &std::path::Path) -> String {
    match shell_type {
        ShellType::Bash | ShellType::Zsh => {
            format!("exec {} && bivvy run --continue", executable.display())
        }
        ShellType::Fish => "exec fish && bivvy run --continue".to_string(),
        ShellType::PowerShell => "& $PROFILE; bivvy run --continue".to_string(),
        ShellType::Cmd => "start cmd /k \"bivvy run --continue\"".to_string(),
        ShellType::Unknown => {
            "# Please restart your shell and run: bivvy run --continue".to_string()
        }
    }
}

/// Check if running in a CI environment.
///
/// Used to auto-detect CI and force non-interactive mode in `main()`,
/// and to suppress noisy progress bars in [`NonInteractiveUI`](crate::ui::NonInteractiveUI).
/// Checks common CI environment variables: `CI`, `GITHUB_ACTIONS`,
/// `GITLAB_CI`, `CIRCLECI`, `TRAVIS`, `JENKINS_URL`.
pub fn is_ci() -> bool {
    std::env::var("CI").is_ok()
        || std::env::var("GITHUB_ACTIONS").is_ok()
        || std::env::var("GITLAB_CI").is_ok()
        || std::env::var("CIRCLECI").is_ok()
        || std::env::var("TRAVIS").is_ok()
        || std::env::var("JENKINS_URL").is_ok()
}

/// Check if running as root/admin.
pub fn is_elevated() -> bool {
    #[cfg(unix)]
    {
        // SAFETY: geteuid() is a simple syscall that returns the effective user ID
        unsafe { libc::geteuid() == 0 }
    }

    #[cfg(windows)]
    {
        std::env::var("ADMIN").is_ok()
    }

    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_type_from_executable() {
        assert_eq!(ShellType::from_executable("/bin/bash"), ShellType::Bash);
        assert_eq!(ShellType::from_executable("/usr/bin/zsh"), ShellType::Zsh);
        assert_eq!(ShellType::from_executable("/usr/bin/fish"), ShellType::Fish);
        assert_eq!(ShellType::from_executable("pwsh"), ShellType::PowerShell);
        assert_eq!(ShellType::from_executable("cmd.exe"), ShellType::Cmd);
        assert_eq!(ShellType::from_executable("unknown"), ShellType::Unknown);
    }

    #[test]
    fn detect_shell_returns_info() {
        let info = detect_shell();
        assert!(!info.executable.as_os_str().is_empty());
    }

    #[test]
    fn config_files_for_bash() {
        let files = get_config_files(ShellType::Bash);
        assert!(files.iter().any(|f| f.ends_with(".bashrc")));
    }

    #[test]
    fn config_files_for_zsh() {
        let files = get_config_files(ShellType::Zsh);
        assert!(files.iter().any(|f| f.ends_with(".zshrc")));
    }

    #[test]
    fn reload_command_includes_bivvy_continue() {
        let cmd = get_reload_command(ShellType::Bash, &PathBuf::from("/bin/bash"));
        assert!(cmd.contains("--continue"));
    }

    #[test]
    fn is_ci_detects_environment() {
        // Just ensure function doesn't panic
        let _ = is_ci();
    }
}
