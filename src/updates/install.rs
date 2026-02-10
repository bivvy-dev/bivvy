//! Install method detection.
//!
//! Detects how bivvy was installed to determine the update mechanism.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

/// How bivvy was installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallMethod {
    /// Installed via cargo install
    Cargo,
    /// Installed via Homebrew
    Homebrew,
    /// Downloaded binary or built from source
    Manual { path: PathBuf },
    /// Could not determine install method
    Unknown,
}

impl InstallMethod {
    /// Get the update command for this install method.
    pub fn update_command(&self) -> Option<String> {
        match self {
            InstallMethod::Cargo => Some("cargo install bivvy --force".to_string()),
            InstallMethod::Homebrew => Some("brew upgrade bivvy".to_string()),
            InstallMethod::Manual { .. } => None,
            InstallMethod::Unknown => None,
        }
    }

    /// Check if this method supports automatic updates.
    pub fn supports_auto_update(&self) -> bool {
        matches!(self, InstallMethod::Cargo | InstallMethod::Homebrew)
    }

    /// Get a human-readable name for this install method.
    pub fn name(&self) -> &str {
        match self {
            InstallMethod::Cargo => "cargo",
            InstallMethod::Homebrew => "homebrew",
            InstallMethod::Manual { .. } => "manual",
            InstallMethod::Unknown => "unknown",
        }
    }
}

/// Detect how bivvy was installed.
pub fn detect_install_method() -> InstallMethod {
    // Get the current executable path
    let exe_path = match env::current_exe() {
        Ok(path) => path,
        Err(_) => return InstallMethod::Unknown,
    };

    // Check if it's in cargo bin directory
    if is_cargo_install(&exe_path) {
        return InstallMethod::Cargo;
    }

    // Check if it's managed by Homebrew
    if is_homebrew_install(&exe_path) {
        return InstallMethod::Homebrew;
    }

    // Manual install (downloaded binary or built from source)
    InstallMethod::Manual { path: exe_path }
}

fn is_cargo_install(exe_path: &Path) -> bool {
    // Check if executable is in ~/.cargo/bin/
    if let Some(home) = dirs::home_dir() {
        let cargo_bin = home.join(".cargo").join("bin");
        if exe_path.starts_with(&cargo_bin) {
            return true;
        }
    }

    // Check CARGO_HOME environment variable
    if let Ok(cargo_home) = env::var("CARGO_HOME") {
        let cargo_bin = PathBuf::from(cargo_home).join("bin");
        if exe_path.starts_with(&cargo_bin) {
            return true;
        }
    }

    false
}

fn is_homebrew_install(exe_path: &Path) -> bool {
    // Check common Homebrew paths
    let homebrew_paths = [
        "/usr/local/Cellar/",          // Intel macOS
        "/opt/homebrew/Cellar/",       // ARM macOS
        "/home/linuxbrew/.linuxbrew/", // Linux
    ];

    let exe_str = exe_path.to_string_lossy();
    for prefix in &homebrew_paths {
        if exe_str.starts_with(prefix) {
            return true;
        }
    }

    // Also check if it's a symlink from Homebrew bin
    let homebrew_bins = ["/usr/local/bin/bivvy", "/opt/homebrew/bin/bivvy"];

    for bin in &homebrew_bins {
        if exe_path.to_string_lossy() == *bin {
            // Check if it's actually a Homebrew-managed binary
            if let Ok(output) = Command::new("brew")
                .args(["list", "--formula", "bivvy"])
                .output()
            {
                if output.status.success() {
                    return true;
                }
            }
        }
    }

    false
}

/// Get the path where bivvy is installed.
pub fn get_install_path() -> Option<PathBuf> {
    env::current_exe().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_method_name() {
        assert_eq!(InstallMethod::Cargo.name(), "cargo");
        assert_eq!(InstallMethod::Homebrew.name(), "homebrew");
        assert_eq!(
            InstallMethod::Manual {
                path: PathBuf::from("/usr/bin/bivvy")
            }
            .name(),
            "manual"
        );
        assert_eq!(InstallMethod::Unknown.name(), "unknown");
    }

    #[test]
    fn install_method_update_command() {
        assert_eq!(
            InstallMethod::Cargo.update_command(),
            Some("cargo install bivvy --force".to_string())
        );
        assert_eq!(
            InstallMethod::Homebrew.update_command(),
            Some("brew upgrade bivvy".to_string())
        );
        assert!(InstallMethod::Manual {
            path: PathBuf::from("/tmp/bivvy")
        }
        .update_command()
        .is_none());
        assert!(InstallMethod::Unknown.update_command().is_none());
    }

    #[test]
    fn install_method_supports_auto_update() {
        assert!(InstallMethod::Cargo.supports_auto_update());
        assert!(InstallMethod::Homebrew.supports_auto_update());
        assert!(!InstallMethod::Manual {
            path: PathBuf::from("/tmp/bivvy")
        }
        .supports_auto_update());
        assert!(!InstallMethod::Unknown.supports_auto_update());
    }

    #[test]
    fn detect_install_method_returns_something() {
        let method = detect_install_method();
        // Should not panic and should return a valid variant
        let _ = method.name();
    }

    #[test]
    fn get_install_path_returns_path() {
        let path = get_install_path();
        // In test context, this should return the test binary path
        assert!(path.is_some());
    }

    #[test]
    fn is_cargo_install_with_cargo_bin() {
        // Test with a path that looks like cargo bin
        if let Some(home) = dirs::home_dir() {
            let cargo_path = home.join(".cargo").join("bin").join("bivvy");
            assert!(is_cargo_install(&cargo_path));
        }
    }

    #[test]
    fn is_cargo_install_with_random_path() {
        let random_path = PathBuf::from("/tmp/bivvy");
        assert!(!is_cargo_install(&random_path));
    }

    #[test]
    fn is_homebrew_install_with_cellar_path() {
        let intel_path = PathBuf::from("/usr/local/Cellar/bivvy/0.1.0/bin/bivvy");
        assert!(is_homebrew_install(&intel_path));

        let arm_path = PathBuf::from("/opt/homebrew/Cellar/bivvy/0.1.0/bin/bivvy");
        assert!(is_homebrew_install(&arm_path));

        let linux_path = PathBuf::from("/home/linuxbrew/.linuxbrew/bin/bivvy");
        assert!(is_homebrew_install(&linux_path));
    }

    #[test]
    fn is_homebrew_install_with_random_path() {
        let random_path = PathBuf::from("/tmp/bivvy");
        assert!(!is_homebrew_install(&random_path));
    }

    #[test]
    fn install_method_equality() {
        assert_eq!(InstallMethod::Cargo, InstallMethod::Cargo);
        assert_eq!(InstallMethod::Homebrew, InstallMethod::Homebrew);
        assert_eq!(InstallMethod::Unknown, InstallMethod::Unknown);
        assert_eq!(
            InstallMethod::Manual {
                path: PathBuf::from("/a")
            },
            InstallMethod::Manual {
                path: PathBuf::from("/a")
            }
        );
        assert_ne!(
            InstallMethod::Manual {
                path: PathBuf::from("/a")
            },
            InstallMethod::Manual {
                path: PathBuf::from("/b")
            }
        );
        assert_ne!(InstallMethod::Cargo, InstallMethod::Homebrew);
    }

    #[test]
    fn install_method_clone() {
        let method = InstallMethod::Cargo;
        let cloned = method.clone();
        assert_eq!(method, cloned);

        let manual = InstallMethod::Manual {
            path: PathBuf::from("/test"),
        };
        let cloned_manual = manual.clone();
        assert_eq!(manual, cloned_manual);
    }

    #[test]
    fn install_method_debug() {
        let method = InstallMethod::Cargo;
        let debug = format!("{:?}", method);
        assert!(debug.contains("Cargo"));

        let manual = InstallMethod::Manual {
            path: PathBuf::from("/test"),
        };
        let debug_manual = format!("{:?}", manual);
        assert!(debug_manual.contains("Manual"));
        assert!(debug_manual.contains("/test"));
    }
}
