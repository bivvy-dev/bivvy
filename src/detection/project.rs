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

        // --- Cross-cutting sidebar detections ---

        // Environment file setup (detected alongside any project type)
        if any_file_exists(
            project_root,
            &[".env.example", ".env.sample", ".env.template"],
        )
        .is_some()
            && !file_exists(project_root, ".env")
        {
            details.push(
                DetectionResult::found("Environment setup")
                    .with_detail("Environment template file found")
                    .with_template("env-copy"),
            );
        }

        // Pre-commit hooks (detected alongside any project type)
        if file_exists(project_root, ".pre-commit-config.yaml") {
            details.push(
                DetectionResult::found("pre-commit")
                    .with_detail(".pre-commit-config.yaml found")
                    .with_template("pre-commit"),
            );
        }

        // Monorepo/workspace detection (detected alongside other project types)
        if file_exists(project_root, "nx.json") {
            details.push(
                DetectionResult::found("Nx")
                    .with_detail("Nx workspace detected")
                    .with_template("nx"),
            );
        }

        if file_exists(project_root, "turbo.json") {
            details.push(
                DetectionResult::found("Turborepo")
                    .with_detail("Turborepo workspace detected")
                    .with_template("turborepo"),
            );
        }

        if file_exists(project_root, "lerna.json") {
            details.push(
                DetectionResult::found("Lerna")
                    .with_detail("Lerna monorepo detected")
                    .with_template("lerna"),
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
    fn detect_env_copy_needed() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join(".env.example"), "DB_HOST=localhost").unwrap();
        let detection = ProjectDetector::detect(temp.path());
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("env-copy".to_string())));
    }

    #[test]
    fn detect_env_copy_not_needed_when_env_exists() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join(".env.example"), "DB_HOST=localhost").unwrap();
        fs::write(temp.path().join(".env"), "DB_HOST=localhost").unwrap();
        let detection = ProjectDetector::detect(temp.path());
        assert!(!detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("env-copy".to_string())));
    }

    #[test]
    fn detect_env_copy_with_sample() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join(".env.sample"), "").unwrap();
        let detection = ProjectDetector::detect(temp.path());
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("env-copy".to_string())));
    }

    #[test]
    fn detect_pre_commit() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join(".pre-commit-config.yaml"), "repos: []").unwrap();
        let detection = ProjectDetector::detect(temp.path());
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("pre-commit".to_string())));
    }

    #[test]
    fn detect_nx_workspace() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("nx.json"), "{}").unwrap();
        let detection = ProjectDetector::detect(temp.path());
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("nx".to_string())));
    }

    #[test]
    fn detect_turborepo_workspace() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("turbo.json"), "{}").unwrap();
        let detection = ProjectDetector::detect(temp.path());
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("turborepo".to_string())));
    }

    #[test]
    fn detect_lerna_monorepo() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("lerna.json"), "{}").unwrap();
        let detection = ProjectDetector::detect(temp.path());
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("lerna".to_string())));
    }
}
