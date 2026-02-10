//! Package manager detection.

use std::path::Path;

use super::command_detection::command_succeeds;
use super::file_detection::file_exists;

/// Detected package manager.
#[derive(Debug, Clone, PartialEq)]
pub enum PackageManager {
    // System
    Homebrew,
    Chocolatey,
    Scoop,
    Winget,
    Apt,
    Yum,
    Pacman,

    // Version managers
    Mise,
    Asdf,
    Volta,
    Nvm,
    Pyenv,
    Rbenv,

    // Language package managers
    Bundler,
    Npm,
    Yarn,
    Pnpm,
    Bun,
    Pip,
    Poetry,
    Uv,
    Cargo,
    Go,
}

/// Result of package manager detection.
#[derive(Debug, Clone)]
pub struct PackageManagerDetection {
    pub system: Option<PackageManager>,
    pub version_manager: Option<PackageManager>,
    pub language_managers: Vec<PackageManager>,
}

/// Detects installed package managers.
pub struct PackageManagerDetector;

impl PackageManagerDetector {
    /// Detect package managers for a project.
    pub fn detect(project_root: &Path) -> PackageManagerDetection {
        PackageManagerDetection {
            system: Self::detect_system_package_manager(),
            version_manager: Self::detect_version_manager(project_root),
            language_managers: Self::detect_language_managers(project_root),
        }
    }

    fn detect_system_package_manager() -> Option<PackageManager> {
        #[cfg(target_os = "macos")]
        if command_succeeds("brew --version") {
            return Some(PackageManager::Homebrew);
        }

        #[cfg(target_os = "linux")]
        {
            if command_succeeds("apt --version") {
                return Some(PackageManager::Apt);
            }
            if command_succeeds("yum --version") {
                return Some(PackageManager::Yum);
            }
            if command_succeeds("pacman --version") {
                return Some(PackageManager::Pacman);
            }
            if command_succeeds("brew --version") {
                return Some(PackageManager::Homebrew);
            }
        }

        #[cfg(target_os = "windows")]
        {
            if command_succeeds("choco --version") {
                return Some(PackageManager::Chocolatey);
            }
            if command_succeeds("scoop --version") {
                return Some(PackageManager::Scoop);
            }
            if command_succeeds("winget --version") {
                return Some(PackageManager::Winget);
            }
        }

        None
    }

    fn detect_version_manager(project_root: &Path) -> Option<PackageManager> {
        // Check for config files first
        if file_exists(project_root, ".mise.toml") || file_exists(project_root, "mise.toml") {
            return Some(PackageManager::Mise);
        }
        if file_exists(project_root, ".tool-versions") {
            return Some(PackageManager::Asdf);
        }

        // Check if tools are installed
        if command_succeeds("mise --version") {
            return Some(PackageManager::Mise);
        }
        if command_succeeds("asdf --version") {
            return Some(PackageManager::Asdf);
        }
        if command_succeeds("volta --version") {
            return Some(PackageManager::Volta);
        }

        None
    }

    fn detect_language_managers(project_root: &Path) -> Vec<PackageManager> {
        let mut managers = Vec::new();

        // Ruby
        if file_exists(project_root, "Gemfile") {
            managers.push(PackageManager::Bundler);
        }

        // Node
        if file_exists(project_root, "yarn.lock") {
            managers.push(PackageManager::Yarn);
        } else if file_exists(project_root, "pnpm-lock.yaml") {
            managers.push(PackageManager::Pnpm);
        } else if file_exists(project_root, "bun.lockb") {
            managers.push(PackageManager::Bun);
        } else if file_exists(project_root, "package-lock.json")
            || file_exists(project_root, "package.json")
        {
            managers.push(PackageManager::Npm);
        }

        // Python
        if file_exists(project_root, "poetry.lock") {
            managers.push(PackageManager::Poetry);
        } else if file_exists(project_root, "uv.lock") {
            managers.push(PackageManager::Uv);
        } else if file_exists(project_root, "requirements.txt")
            || file_exists(project_root, "pyproject.toml")
        {
            managers.push(PackageManager::Pip);
        }

        // Rust
        if file_exists(project_root, "Cargo.toml") {
            managers.push(PackageManager::Cargo);
        }

        // Go
        if file_exists(project_root, "go.mod") {
            managers.push(PackageManager::Go);
        }

        managers
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn detect_version_manager_mise() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join(".mise.toml"), "").unwrap();

        let detection = PackageManagerDetector::detect(temp.path());

        assert_eq!(detection.version_manager, Some(PackageManager::Mise));
    }

    #[test]
    fn detect_version_manager_asdf() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join(".tool-versions"), "").unwrap();

        let detection = PackageManagerDetector::detect(temp.path());

        assert_eq!(detection.version_manager, Some(PackageManager::Asdf));
    }

    #[test]
    fn detect_language_managers_yarn() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("package.json"), "{}").unwrap();
        fs::write(temp.path().join("yarn.lock"), "").unwrap();

        let detection = PackageManagerDetector::detect(temp.path());

        assert!(detection.language_managers.contains(&PackageManager::Yarn));
        assert!(!detection.language_managers.contains(&PackageManager::Npm));
    }

    #[test]
    fn detect_multiple_language_managers() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("Gemfile"), "").unwrap();
        fs::write(temp.path().join("package.json"), "{}").unwrap();
        fs::write(temp.path().join("Cargo.toml"), "").unwrap();

        let detection = PackageManagerDetector::detect(temp.path());

        assert!(detection
            .language_managers
            .contains(&PackageManager::Bundler));
        assert!(detection.language_managers.contains(&PackageManager::Npm));
        assert!(detection.language_managers.contains(&PackageManager::Cargo));
    }

    #[test]
    fn detect_empty_project() {
        let temp = TempDir::new().unwrap();

        let detection = PackageManagerDetector::detect(temp.path());

        // version_manager may be set if mise/asdf/volta is installed globally,
        // so we only check that language_managers is empty for an empty project
        assert!(detection.language_managers.is_empty());
    }
}
