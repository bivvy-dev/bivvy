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
    Php,
    Gradle,
    Elixir,
    Swift,
    Terraform,
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

        // PHP
        if file_exists(project_root, "composer.json") {
            all_types.push(ProjectType::Php);
            details.push(
                DetectionResult::found("PHP")
                    .with_detail("composer.json found")
                    .with_template("composer"),
            );

            // Laravel (detected alongside PHP, not as a separate project type)
            if file_exists(project_root, "artisan") {
                details.push(
                    DetectionResult::found("Laravel")
                        .with_detail("artisan found")
                        .with_template("laravel"),
                );
            }
        }

        // Kotlin/JVM
        if file_exists(project_root, "build.gradle.kts")
            || file_exists(project_root, "build.gradle")
        {
            all_types.push(ProjectType::Gradle);
            details.push(
                DetectionResult::found("Kotlin/JVM")
                    .with_detail("Gradle build file found")
                    .with_template("gradle"),
            );
        }

        // Elixir
        if file_exists(project_root, "mix.exs") {
            all_types.push(ProjectType::Elixir);
            details.push(
                DetectionResult::found("Elixir")
                    .with_detail("mix.exs found")
                    .with_template("mix"),
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

        // Terraform
        if any_file_exists(project_root, &["main.tf", "terraform.tf", "versions.tf"]).is_some() {
            all_types.push(ProjectType::Terraform);
            details.push(
                DetectionResult::found("Terraform")
                    .with_detail("Terraform files found")
                    .with_template("terraform"),
            );
        }

        // AWS CDK (detected alongside other project types)
        if file_exists(project_root, "cdk.json") {
            details.push(
                DetectionResult::found("AWS CDK")
                    .with_detail("cdk.json found")
                    .with_template("aws-cdk"),
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
    fn detect_php_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("composer.json"), "{}").unwrap();

        let detection = ProjectDetector::detect(temp.path());

        assert_eq!(detection.primary_type, ProjectType::Php);
        assert!(detection.all_types.contains(&ProjectType::Php));
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("composer".to_string())));
    }

    #[test]
    fn detect_terraform_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("main.tf"), "").unwrap();

        let detection = ProjectDetector::detect(temp.path());

        assert_eq!(detection.primary_type, ProjectType::Terraform);
        assert!(detection.all_types.contains(&ProjectType::Terraform));
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("terraform".to_string())));
    }

    #[test]
    fn detect_gradle_project_with_kts() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("build.gradle.kts"), "").unwrap();

        let detection = ProjectDetector::detect(temp.path());

        assert_eq!(detection.primary_type, ProjectType::Gradle);
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("gradle".to_string())));
    }

    #[test]
    fn detect_gradle_project_with_groovy() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("build.gradle"), "").unwrap();

        let detection = ProjectDetector::detect(temp.path());

        assert_eq!(detection.primary_type, ProjectType::Gradle);
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("gradle".to_string())));
    }

    #[test]
    fn detect_terraform_project_with_versions_tf() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("versions.tf"), "").unwrap();

        let detection = ProjectDetector::detect(temp.path());

        assert_eq!(detection.primary_type, ProjectType::Terraform);
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("terraform".to_string())));
    }

    #[test]
    fn detect_elixir_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("mix.exs"), "").unwrap();

        let detection = ProjectDetector::detect(temp.path());

        assert_eq!(detection.primary_type, ProjectType::Elixir);
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("mix".to_string())));
    }

    #[test]
    fn detect_laravel_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("composer.json"), "{}").unwrap();
        fs::write(temp.path().join("artisan"), "").unwrap();

        let detection = ProjectDetector::detect(temp.path());

        assert_eq!(detection.primary_type, ProjectType::Php);
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("composer".to_string())));
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("laravel".to_string())));
    }

    #[test]
    fn detect_aws_cdk_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("cdk.json"), "{}").unwrap();

        let detection = ProjectDetector::detect(temp.path());

        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("aws-cdk".to_string())));
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
}
