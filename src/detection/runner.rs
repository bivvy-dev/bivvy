//! Detection aggregation and ordering.

use std::path::Path;

use super::{
    conflicts::{Conflict, ConflictDetector},
    package_manager::{PackageManager, PackageManagerDetection, PackageManagerDetector},
    project::{ProjectDetection, ProjectDetector},
};

/// Aggregates all detection results.
#[derive(Debug)]
pub struct FullDetection {
    pub project: ProjectDetection,
    pub package_managers: PackageManagerDetection,
    pub conflicts: Vec<Conflict>,
    pub suggested_templates: Vec<SuggestedTemplate>,
}

/// A suggested template from detection.
#[derive(Debug, Clone)]
pub struct SuggestedTemplate {
    pub name: String,
    pub category: String,
    pub reason: String,
    pub priority: u32,
}

/// Runs all detectors and aggregates results.
pub struct DetectionRunner;

impl DetectionRunner {
    /// Run all detectors on a project.
    pub fn run(project_root: &Path) -> FullDetection {
        let project = ProjectDetector::detect(project_root);
        let package_managers = PackageManagerDetector::detect(project_root);
        let conflicts = ConflictDetector::detect(project_root);

        let suggested_templates = Self::suggest_templates(&project, &package_managers);

        FullDetection {
            project,
            package_managers,
            conflicts,
            suggested_templates,
        }
    }

    fn suggest_templates(
        project: &ProjectDetection,
        pm: &PackageManagerDetection,
    ) -> Vec<SuggestedTemplate> {
        let mut suggestions = Vec::new();

        // System package manager (priority 10)
        if let Some(ref system_pm) = pm.system {
            let (name, category) = match system_pm {
                PackageManager::Homebrew => ("brew", "system"),
                PackageManager::Chocolatey => ("chocolatey", "windows"),
                PackageManager::Apt => ("apt", "system"),
                PackageManager::Yum => ("yum", "system"),
                PackageManager::Pacman => ("pacman", "system"),
                _ => ("unknown", "system"),
            };

            suggestions.push(SuggestedTemplate {
                name: name.to_string(),
                category: category.to_string(),
                reason: "System package manager detected".to_string(),
                priority: 10,
            });
        }

        // Version manager (priority 20)
        if let Some(ref vm) = pm.version_manager {
            let name = match vm {
                PackageManager::Mise => "mise",
                PackageManager::Asdf => "asdf",
                PackageManager::Volta => "volta",
                _ => "unknown",
            };

            suggestions.push(SuggestedTemplate {
                name: name.to_string(),
                category: "version_manager".to_string(),
                reason: format!("{:?} detected", vm),
                priority: 20,
            });
        }

        // Language templates from project detection (priority 30)
        for detail in &project.details {
            if let Some(ref template) = detail.suggested_template {
                suggestions.push(SuggestedTemplate {
                    name: template.clone(),
                    category: "language".to_string(),
                    reason: detail.details.first().cloned().unwrap_or_default(),
                    priority: 30,
                });
            }
        }

        // Sort by priority
        suggestions.sort_by_key(|s| s.priority);

        suggestions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn full_detection_empty_project() {
        let temp = TempDir::new().unwrap();

        let detection = DetectionRunner::run(temp.path());

        assert!(detection.project.all_types.is_empty());
        assert!(detection.conflicts.is_empty());
    }

    #[test]
    fn full_detection_ruby_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("Gemfile"), "").unwrap();

        let detection = DetectionRunner::run(temp.path());

        assert!(detection
            .suggested_templates
            .iter()
            .any(|t| t.name == "bundler"));
    }

    #[test]
    fn full_detection_with_conflict() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("package.json"), "{}").unwrap();
        fs::write(temp.path().join("package-lock.json"), "").unwrap();
        fs::write(temp.path().join("yarn.lock"), "").unwrap();

        let detection = DetectionRunner::run(temp.path());

        assert!(!detection.conflicts.is_empty());
    }

    #[test]
    fn suggestions_sorted_by_priority() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("Gemfile"), "").unwrap();
        fs::write(temp.path().join(".mise.toml"), "").unwrap();

        let detection = DetectionRunner::run(temp.path());

        let mise_pos = detection
            .suggested_templates
            .iter()
            .position(|t| t.name == "mise");
        let bundler_pos = detection
            .suggested_templates
            .iter()
            .position(|t| t.name == "bundler");

        if let (Some(mise), Some(bundler)) = (mise_pos, bundler_pos) {
            assert!(
                mise < bundler,
                "Version manager should come before language"
            );
        }
    }

    #[test]
    fn full_detection_multi_language() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("Gemfile"), "").unwrap();
        fs::write(temp.path().join("package.json"), "{}").unwrap();
        fs::write(temp.path().join("Cargo.toml"), "").unwrap();

        let detection = DetectionRunner::run(temp.path());

        assert!(detection
            .suggested_templates
            .iter()
            .any(|t| t.name == "bundler"));
        assert!(detection
            .suggested_templates
            .iter()
            .any(|t| t.name == "npm"));
        assert!(detection
            .suggested_templates
            .iter()
            .any(|t| t.name == "cargo"));
    }
}
