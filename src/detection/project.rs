//! Project type detection.

use std::path::Path;

use super::file_detection::{any_file_exists, file_exists};
use super::types::DetectionResult;

/// Detected project type.
#[derive(Debug, Clone, PartialEq)]
pub enum ProjectType {
    Ruby,
    Node,
    Python,
    Rust,
    Go,
    Swift,
    Maven,
    Dotnet,
    Dart,
    Deno,
    Unknown,
}

/// Result of project detection.
#[derive(Debug, Clone)]
pub struct ProjectDetection {
    pub primary_type: ProjectType,
    pub all_types: Vec<ProjectType>,
    pub details: Vec<DetectionResult>,
}

/// Detects project types based on marker files.
pub struct ProjectDetector;

impl ProjectDetector {
    /// Detect all project types in a directory.
    pub fn detect(project_root: &Path) -> ProjectDetection {
        let mut all_types = Vec::new();
        let mut details = Vec::new();

        // Ruby
        if file_exists(project_root, "Gemfile") {
            all_types.push(ProjectType::Ruby);
            details.push(
                DetectionResult::found("Ruby")
                    .with_detail("Gemfile found")
                    .with_template("bundler"),
            );
        }

        // Node
        if file_exists(project_root, "package.json") {
            all_types.push(ProjectType::Node);

            let lockfile = any_file_exists(
                project_root,
                &[
                    "yarn.lock",
                    "package-lock.json",
                    "pnpm-lock.yaml",
                    "bun.lockb",
                ],
            );

            let template = match lockfile.as_deref() {
                Some("yarn.lock") => "yarn",
                Some("pnpm-lock.yaml") => "pnpm",
                Some("bun.lockb") => "bun",
                _ => "npm",
            };

            details.push(
                DetectionResult::found("Node.js")
                    .with_detail("package.json found")
                    .with_template(template),
            );
        }

        // Python
        if file_exists(project_root, "pyproject.toml")
            || file_exists(project_root, "requirements.txt")
            || file_exists(project_root, "setup.py")
        {
            all_types.push(ProjectType::Python);

            let template = if file_exists(project_root, "poetry.lock") {
                "poetry"
            } else if file_exists(project_root, "uv.lock") {
                "uv"
            } else {
                "pip"
            };

            details.push(
                DetectionResult::found("Python")
                    .with_detail("Python project detected")
                    .with_template(template),
            );
        }

        // Rust
        if file_exists(project_root, "Cargo.toml") {
            all_types.push(ProjectType::Rust);
            details.push(
                DetectionResult::found("Rust")
                    .with_detail("Cargo.toml found")
                    .with_template("cargo"),
            );
        }

        // Go
        if file_exists(project_root, "go.mod") {
            all_types.push(ProjectType::Go);
            details.push(
                DetectionResult::found("Go")
                    .with_detail("go.mod found")
                    .with_template("go"),
            );
        }

        // Swift
        if file_exists(project_root, "Package.swift") {
            all_types.push(ProjectType::Swift);
            details.push(
                DetectionResult::found("Swift")
                    .with_detail("Package.swift found")
                    .with_template("swift"),
            );
        }

        // Maven (Java)
        if file_exists(project_root, "pom.xml") {
            all_types.push(ProjectType::Maven);
            details.push(
                DetectionResult::found("Maven (Java)")
                    .with_detail("pom.xml found")
                    .with_template("maven"),
            );
        }

        // .NET (C#)
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
            all_types.push(ProjectType::Dotnet);
            details.push(
                DetectionResult::found(".NET")
                    .with_detail(".NET solution or project found")
                    .with_template("dotnet"),
            );
        }

        // Dart/Flutter
        if file_exists(project_root, "pubspec.yaml") {
            all_types.push(ProjectType::Dart);

            let flutter_dirs = ["android", "ios", "web", "macos", "linux", "windows"];
            let is_flutter = flutter_dirs
                .iter()
                .any(|dir| project_root.join(dir).is_dir());

            if is_flutter {
                details.push(
                    DetectionResult::found("Flutter")
                        .with_detail("pubspec.yaml found")
                        .with_template("flutter"),
                );
            } else {
                details.push(
                    DetectionResult::found("Dart")
                        .with_detail("pubspec.yaml found")
                        .with_template("dart"),
                );
            }
        }

        // Deno
        if file_exists(project_root, "deno.json")
            || file_exists(project_root, "deno.jsonc")
            || file_exists(project_root, "deno.lock")
        {
            all_types.push(ProjectType::Deno);
            details.push(
                DetectionResult::found("Deno")
                    .with_detail("Deno configuration found")
                    .with_template("deno"),
            );
        }

        let primary_type = all_types.first().cloned().unwrap_or(ProjectType::Unknown);

        ProjectDetection {
            primary_type,
            all_types,
            details,
        }
    }

    /// Check if a specific project type is detected.
    pub fn has_type(project_root: &Path, project_type: ProjectType) -> bool {
        Self::detect(project_root).all_types.contains(&project_type)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn detect_ruby_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("Gemfile"), "source 'https://rubygems.org'").unwrap();

        let detection = ProjectDetector::detect(temp.path());

        assert_eq!(detection.primary_type, ProjectType::Ruby);
        assert!(detection.all_types.contains(&ProjectType::Ruby));
    }

    #[test]
    fn detect_node_project_with_yarn() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("package.json"), "{}").unwrap();
        fs::write(temp.path().join("yarn.lock"), "").unwrap();

        let detection = ProjectDetector::detect(temp.path());

        assert_eq!(detection.primary_type, ProjectType::Node);
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("yarn".to_string())));
    }

    #[test]
    fn detect_python_project_with_poetry() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("pyproject.toml"), "").unwrap();
        fs::write(temp.path().join("poetry.lock"), "").unwrap();

        let detection = ProjectDetector::detect(temp.path());

        assert_eq!(detection.primary_type, ProjectType::Python);
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("poetry".to_string())));
    }

    #[test]
    fn detect_multi_language_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("Gemfile"), "").unwrap();
        fs::write(temp.path().join("package.json"), "{}").unwrap();

        let detection = ProjectDetector::detect(temp.path());

        assert!(detection.all_types.contains(&ProjectType::Ruby));
        assert!(detection.all_types.contains(&ProjectType::Node));
    }

    #[test]
    fn detect_unknown_project() {
        let temp = TempDir::new().unwrap();

        let detection = ProjectDetector::detect(temp.path());

        assert_eq!(detection.primary_type, ProjectType::Unknown);
        assert!(detection.all_types.is_empty());
    }

    #[test]
    fn has_type_helper() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("Cargo.toml"), "").unwrap();

        assert!(ProjectDetector::has_type(temp.path(), ProjectType::Rust));
        assert!(!ProjectDetector::has_type(temp.path(), ProjectType::Ruby));
    }

    #[test]
    fn detect_maven_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("pom.xml"), "").unwrap();

        let detection = ProjectDetector::detect(temp.path());

        assert_eq!(detection.primary_type, ProjectType::Maven);
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("maven".to_string())));
    }

    #[test]
    fn detect_dotnet_project_sln() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("MyApp.sln"), "").unwrap();

        let detection = ProjectDetector::detect(temp.path());

        assert_eq!(detection.primary_type, ProjectType::Dotnet);
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("dotnet".to_string())));
    }

    #[test]
    fn detect_dotnet_project_csproj() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("MyApp.csproj"), "").unwrap();

        let detection = ProjectDetector::detect(temp.path());

        assert_eq!(detection.primary_type, ProjectType::Dotnet);
    }

    #[test]
    fn detect_dart_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("pubspec.yaml"), "name: my_app").unwrap();

        let detection = ProjectDetector::detect(temp.path());

        assert_eq!(detection.primary_type, ProjectType::Dart);
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("dart".to_string())));
    }

    #[test]
    fn detect_flutter_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("pubspec.yaml"), "name: my_app").unwrap();
        fs::create_dir(temp.path().join("android")).unwrap();

        let detection = ProjectDetector::detect(temp.path());

        assert_eq!(detection.primary_type, ProjectType::Dart);
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("flutter".to_string())));
    }

    #[test]
    fn detect_deno_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("deno.json"), "{}").unwrap();

        let detection = ProjectDetector::detect(temp.path());

        assert_eq!(detection.primary_type, ProjectType::Deno);
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("deno".to_string())));
    }
}
