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
    Fnm,
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
    Composer,
    Gradle,
    Mix,
    Cargo,
    Go,
    Maven,
    Dotnet,
    Dart,
    Flutter,
    Deno,
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
            system: Self::detect_system_package_manager(project_root),
            version_manager: Self::detect_version_manager(project_root),
            language_managers: Self::detect_language_managers(project_root),
        }
    }

    fn detect_system_package_manager(project_root: &Path) -> Option<PackageManager> {
        // Only suggest a system package manager step if the project has a
        // config file for it (e.g. Brewfile). A tool being installed on the
        // system alone is not enough — the step would have nothing to do.
        if file_exists(project_root, "Brewfile") && command_succeeds("brew --version") {
            return Some(PackageManager::Homebrew);
        }

        // Windows package managers don't have a standard project-level config
        // file, so we don't auto-suggest them. Users can add them manually.

        None
    }

    fn detect_version_manager(project_root: &Path) -> Option<PackageManager> {
        // Check for config files first (cheap)
        if file_exists(project_root, ".mise.toml") || file_exists(project_root, "mise.toml") {
            return Some(PackageManager::Mise);
        }
        if file_exists(project_root, ".tool-versions") {
            return Some(PackageManager::Asdf);
        }
        if file_exists(project_root, ".nvmrc") {
            // Could be nvm or fnm — check if fnm is installed (it reads .nvmrc too)
            if command_succeeds("fnm --version") {
                return Some(PackageManager::Fnm);
            }
            return Some(PackageManager::Nvm);
        }
        if file_exists(project_root, ".node-version") {
            // fnm, volta, and nodenv all read .node-version
            if command_succeeds("fnm --version") {
                return Some(PackageManager::Fnm);
            }
            if command_succeeds("volta --version") {
                return Some(PackageManager::Volta);
            }
            return Some(PackageManager::Nvm);
        }
        if file_exists(project_root, ".ruby-version") {
            return Some(PackageManager::Rbenv);
        }
        if file_exists(project_root, ".python-version") {
            return Some(PackageManager::Pyenv);
        }

        // Only suggest version managers when the project has a config file.
        // A tool being installed globally is not enough — the step would
        // have nothing to act on.

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

        // PHP
        if file_exists(project_root, "composer.json") {
            managers.push(PackageManager::Composer);
        }

        // Kotlin/JVM
        if file_exists(project_root, "build.gradle.kts")
            || file_exists(project_root, "build.gradle")
        {
            managers.push(PackageManager::Gradle);
        }

        // Elixir
        if file_exists(project_root, "mix.exs") {
            managers.push(PackageManager::Mix);
        }

        // Rust
        if file_exists(project_root, "Cargo.toml") {
            managers.push(PackageManager::Cargo);
        }

        // Go
        if file_exists(project_root, "go.mod") {
            managers.push(PackageManager::Go);
        }

        // Maven (Java)
        if file_exists(project_root, "pom.xml") {
            managers.push(PackageManager::Maven);
        }

        // .NET
        if std::fs::read_dir(project_root)
            .map(|entries| {
                entries.filter_map(|e| e.ok()).any(|e| {
                    let name = e.file_name();
                    let name = name.to_string_lossy();
                    name.ends_with(".sln") || name.ends_with(".csproj")
                })
            })
            .unwrap_or(false)
        {
            managers.push(PackageManager::Dotnet);
        }

        // Dart/Flutter
        if file_exists(project_root, "pubspec.yaml") {
            managers.push(PackageManager::Dart);
        }

        // Deno
        if file_exists(project_root, "deno.json") || file_exists(project_root, "deno.jsonc") {
            managers.push(PackageManager::Deno);
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
    fn detect_language_managers_composer() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("composer.json"), "{}").unwrap();

        let detection = PackageManagerDetector::detect(temp.path());

        assert!(detection
            .language_managers
            .contains(&PackageManager::Composer));
    }

    #[test]
    fn detect_language_managers_gradle() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("build.gradle.kts"), "").unwrap();

        let detection = PackageManagerDetector::detect(temp.path());

        assert!(detection
            .language_managers
            .contains(&PackageManager::Gradle));
    }

    #[test]
    fn detect_language_managers_mix() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("mix.exs"), "").unwrap();

        let detection = PackageManagerDetector::detect(temp.path());

        assert!(detection.language_managers.contains(&PackageManager::Mix));
    }

    #[test]
    fn detect_version_manager_nvm_via_nvmrc() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join(".nvmrc"), "18").unwrap();

        let detection = PackageManagerDetector::detect(temp.path());

        // Should detect nvm or fnm depending on what's installed
        assert!(detection.version_manager.is_some());
    }

    #[test]
    fn detect_version_manager_rbenv_via_ruby_version() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join(".ruby-version"), "3.2.0").unwrap();

        let detection = PackageManagerDetector::detect(temp.path());

        assert_eq!(detection.version_manager, Some(PackageManager::Rbenv));
    }

    #[test]
    fn detect_version_manager_pyenv_via_python_version() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join(".python-version"), "3.11").unwrap();

        let detection = PackageManagerDetector::detect(temp.path());

        assert_eq!(detection.version_manager, Some(PackageManager::Pyenv));
    }

    #[test]
    fn detect_empty_project() {
        let temp = TempDir::new().unwrap();

        let detection = PackageManagerDetector::detect(temp.path());

        assert!(detection.system.is_none());
        assert!(detection.version_manager.is_none());
        assert!(detection.language_managers.is_empty());
    }

    #[test]
    fn detect_language_managers_maven() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("pom.xml"), "").unwrap();

        let detection = PackageManagerDetector::detect(temp.path());

        assert!(detection.language_managers.contains(&PackageManager::Maven));
    }

    #[test]
    fn detect_language_managers_dotnet() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("MyApp.csproj"), "").unwrap();

        let detection = PackageManagerDetector::detect(temp.path());

        assert!(detection
            .language_managers
            .contains(&PackageManager::Dotnet));
    }

    #[test]
    fn detect_language_managers_dart() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("pubspec.yaml"), "").unwrap();

        let detection = PackageManagerDetector::detect(temp.path());

        assert!(detection.language_managers.contains(&PackageManager::Dart));
    }

    #[test]
    fn detect_language_managers_deno() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("deno.json"), "{}").unwrap();

        let detection = PackageManagerDetector::detect(temp.path());

        assert!(detection.language_managers.contains(&PackageManager::Deno));
    }
}
