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
    Php,
    Gradle,
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

            // Next.js (detected alongside Node.js)
            if any_file_exists(
                project_root,
                &["next.config.js", "next.config.mjs", "next.config.ts"],
            )
            .is_some()
            {
                details.push(
                    DetectionResult::found("Next.js")
                        .with_detail("Next.js config found")
                        .with_template("nextjs"),
                );
            }

            // Vite (detected alongside Node.js)
            if any_file_exists(
                project_root,
                &["vite.config.js", "vite.config.ts", "vite.config.mjs"],
            )
            .is_some()
            {
                details.push(
                    DetectionResult::found("Vite")
                        .with_detail("Vite config found")
                        .with_template("vite"),
                );
            }

            // Remix (detected alongside Node.js)
            if any_file_exists(project_root, &["remix.config.js", "remix.config.ts"]).is_some()
                || file_exists(project_root, "app/root.tsx")
            {
                details.push(
                    DetectionResult::found("Remix")
                        .with_detail("Remix application detected")
                        .with_template("remix"),
                );
            }
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

            // Django (detected alongside Python)
            if file_exists(project_root, "manage.py") {
                details.push(
                    DetectionResult::found("Django")
                        .with_detail("manage.py found")
                        .with_template("django"),
                );
            }
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

        // Gradle (JVM)
        if any_file_exists(project_root, &["build.gradle", "build.gradle.kts"]).is_some() {
            all_types.push(ProjectType::Gradle);
            details.push(
                DetectionResult::found("Gradle")
                    .with_detail("Gradle build file found")
                    .with_template("gradle"),
            );

            // Spring Boot (detected alongside Gradle)
            if file_exists(project_root, "src/main/resources/application.properties")
                || file_exists(project_root, "src/main/resources/application.yml")
            {
                details.push(
                    DetectionResult::found("Spring Boot")
                        .with_detail("Spring Boot application detected")
                        .with_template("spring-boot"),
                );
            }
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
    fn detect_nextjs_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("package.json"), "{}").unwrap();
        fs::write(temp.path().join("next.config.js"), "module.exports = {}").unwrap();
        let detection = ProjectDetector::detect(temp.path());
        assert_eq!(detection.primary_type, ProjectType::Node);
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("nextjs".to_string())));
    }

    #[test]
    fn detect_nextjs_with_ts_config() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("package.json"), "{}").unwrap();
        fs::write(temp.path().join("next.config.ts"), "").unwrap();
        let detection = ProjectDetector::detect(temp.path());
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("nextjs".to_string())));
    }

    #[test]
    fn detect_vite_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("package.json"), "{}").unwrap();
        fs::write(temp.path().join("vite.config.ts"), "").unwrap();
        let detection = ProjectDetector::detect(temp.path());
        assert_eq!(detection.primary_type, ProjectType::Node);
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("vite".to_string())));
    }

    #[test]
    fn detect_remix_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("package.json"), "{}").unwrap();
        fs::write(temp.path().join("remix.config.js"), "").unwrap();
        let detection = ProjectDetector::detect(temp.path());
        assert_eq!(detection.primary_type, ProjectType::Node);
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("remix".to_string())));
    }

    #[test]
    fn detect_remix_project_via_root_tsx() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("package.json"), "{}").unwrap();
        fs::create_dir_all(temp.path().join("app")).unwrap();
        fs::write(temp.path().join("app/root.tsx"), "").unwrap();
        let detection = ProjectDetector::detect(temp.path());
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("remix".to_string())));
    }

    #[test]
    fn detect_django_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("pyproject.toml"), "").unwrap();
        fs::write(temp.path().join("manage.py"), "#!/usr/bin/env python").unwrap();
        let detection = ProjectDetector::detect(temp.path());
        assert_eq!(detection.primary_type, ProjectType::Python);
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("django".to_string())));
    }

    #[test]
    fn detect_spring_boot_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("build.gradle.kts"), "").unwrap();
        fs::create_dir_all(temp.path().join("src/main/resources")).unwrap();
        fs::write(
            temp.path()
                .join("src/main/resources/application.properties"),
            "",
        )
        .unwrap();
        let detection = ProjectDetector::detect(temp.path());
        assert_eq!(detection.primary_type, ProjectType::Gradle);
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("gradle".to_string())));
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("spring-boot".to_string())));
    }
}
