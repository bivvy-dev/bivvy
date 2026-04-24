//! Detection aggregation and ordering.

use std::path::Path;

use crate::registry::TemplateName;

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
    pub name: TemplateName,
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
                PackageManager::Homebrew => (TemplateName::BrewBundle, "system"),
                PackageManager::Chocolatey => (TemplateName::ChocoInstall, "windows"),
                PackageManager::Apt => (TemplateName::AptInstall, "system"),
                PackageManager::Yum => (TemplateName::YumInstall, "system"),
                PackageManager::Pacman => (TemplateName::PacmanInstall, "system"),
                _ => return suggestions,
            };

            suggestions.push(SuggestedTemplate {
                name,
                category: category.to_string(),
                reason: "System package manager detected".to_string(),
                priority: 10,
            });
        }

        // Version manager (priority 20)
        if let Some(ref vm) = pm.version_manager {
            let name = match vm {
                PackageManager::Mise => TemplateName::MiseTools,
                PackageManager::Asdf => TemplateName::AsdfTools,
                PackageManager::Volta => TemplateName::VoltaSetup,
                PackageManager::Fnm => TemplateName::FnmSetup,
                PackageManager::Nvm => TemplateName::NvmNode,
                PackageManager::Rbenv => TemplateName::RbenvRuby,
                PackageManager::Pyenv => TemplateName::PyenvPython,
                _ => return suggestions,
            };

            suggestions.push(SuggestedTemplate {
                name,
                category: "version_manager".to_string(),
                reason: format!("{:?} detected", vm),
                priority: 20,
            });
        }

        // Language templates from project detection (priority 30)
        for detail in &project.details {
            if let Some(template) = detail.suggested_template {
                suggestions.push(SuggestedTemplate {
                    name: template,
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
    use crate::registry::TemplateName;
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
            .any(|t| t.name == TemplateName::BundleInstall));
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
            .position(|t| t.name == TemplateName::MiseTools);
        let bundler_pos = detection
            .suggested_templates
            .iter()
            .position(|t| t.name == TemplateName::BundleInstall);

        if let (Some(mise), Some(bundler)) = (mise_pos, bundler_pos) {
            assert!(
                mise < bundler,
                "Version manager should come before language"
            );
        }
    }

    #[test]
    fn full_detection_php_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("composer.json"), "{}").unwrap();

        let detection = DetectionRunner::run(temp.path());

        assert!(detection
            .suggested_templates
            .iter()
            .any(|t| t.name == TemplateName::ComposerInstall));
    }

    #[test]
    fn full_detection_terraform_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("main.tf"), "").unwrap();

        let detection = DetectionRunner::run(temp.path());

        assert!(detection
            .suggested_templates
            .iter()
            .any(|t| t.name == TemplateName::TerraformInit));
    }

    #[test]
    fn full_detection_kotlin_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("build.gradle.kts"), "").unwrap();

        let detection = DetectionRunner::run(temp.path());

        assert!(detection
            .suggested_templates
            .iter()
            .any(|t| t.name == TemplateName::GradleDeps));
    }

    #[test]
    fn full_detection_elixir_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("mix.exs"), "").unwrap();

        let detection = DetectionRunner::run(temp.path());

        assert!(detection
            .suggested_templates
            .iter()
            .any(|t| t.name == TemplateName::MixDepsGet));
    }

    #[test]
    fn full_detection_laravel_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("composer.json"), "{}").unwrap();
        fs::write(temp.path().join("artisan"), "").unwrap();

        let detection = DetectionRunner::run(temp.path());

        assert!(detection
            .suggested_templates
            .iter()
            .any(|t| t.name == TemplateName::ComposerInstall));
        assert!(detection
            .suggested_templates
            .iter()
            .any(|t| t.name == TemplateName::LaravelSetup));
    }

    #[test]
    fn full_detection_aws_cdk_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("cdk.json"), "{}").unwrap();

        let detection = DetectionRunner::run(temp.path());

        assert!(detection
            .suggested_templates
            .iter()
            .any(|t| t.name == TemplateName::CdkSynth));
    }

    #[test]
    fn full_detection_docker_compose_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("compose.yml"), "services:").unwrap();

        let detection = DetectionRunner::run(temp.path());

        assert!(detection
            .suggested_templates
            .iter()
            .any(|t| t.name == TemplateName::DockerComposeUp));
    }

    #[test]
    fn full_detection_helm_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("Chart.yaml"), "").unwrap();

        let detection = DetectionRunner::run(temp.path());

        assert!(detection
            .suggested_templates
            .iter()
            .any(|t| t.name == TemplateName::HelmDeps));
    }

    #[test]
    fn full_detection_pulumi_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("Pulumi.yaml"), "").unwrap();

        let detection = DetectionRunner::run(temp.path());

        assert!(detection
            .suggested_templates
            .iter()
            .any(|t| t.name == TemplateName::PulumiInstall));
    }

    #[test]
    fn full_detection_rails_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("Gemfile"), "").unwrap();
        fs::create_dir_all(temp.path().join("config")).unwrap();
        fs::write(temp.path().join("config/routes.rb"), "").unwrap();

        let detection = DetectionRunner::run(temp.path());

        assert!(detection
            .suggested_templates
            .iter()
            .any(|t| t.name == TemplateName::BundleInstall));
        assert!(detection
            .suggested_templates
            .iter()
            .any(|t| t.name == TemplateName::RailsDb));
    }

    #[test]
    fn full_detection_prisma_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("package.json"), "{}").unwrap();
        fs::create_dir_all(temp.path().join("prisma")).unwrap();
        fs::write(temp.path().join("prisma/schema.prisma"), "").unwrap();

        let detection = DetectionRunner::run(temp.path());

        assert!(detection
            .suggested_templates
            .iter()
            .any(|t| t.name == TemplateName::PrismaMigrate));
    }

    #[test]
    fn full_detection_env_copy() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join(".env.example"), "SECRET=xxx").unwrap();
        let detection = DetectionRunner::run(temp.path());
        assert!(detection
            .suggested_templates
            .iter()
            .any(|t| t.name == TemplateName::EnvCopy));
    }

    #[test]
    fn full_detection_pre_commit() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join(".pre-commit-config.yaml"), "repos: []").unwrap();
        let detection = DetectionRunner::run(temp.path());
        assert!(detection
            .suggested_templates
            .iter()
            .any(|t| t.name == TemplateName::PreCommitInstall));
    }

    #[test]
    fn full_detection_nx_workspace() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("nx.json"), "{}").unwrap();
        let detection = DetectionRunner::run(temp.path());
        assert!(detection
            .suggested_templates
            .iter()
            .any(|t| t.name == TemplateName::NxBuild));
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
            .any(|t| t.name == TemplateName::BundleInstall));
        assert!(detection
            .suggested_templates
            .iter()
            .any(|t| t.name == TemplateName::NpmInstall));
        assert!(detection
            .suggested_templates
            .iter()
            .any(|t| t.name == TemplateName::CargoBuild));
    }

    #[test]
    fn full_detection_maven_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("pom.xml"), "").unwrap();

        let detection = DetectionRunner::run(temp.path());

        assert!(detection
            .suggested_templates
            .iter()
            .any(|t| t.name == TemplateName::MavenResolve));
    }

    #[test]
    fn full_detection_dotnet_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("MyApp.sln"), "").unwrap();

        let detection = DetectionRunner::run(temp.path());

        assert!(detection
            .suggested_templates
            .iter()
            .any(|t| t.name == TemplateName::DotnetRestore));
    }

    #[test]
    fn full_detection_deno_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("deno.json"), "{}").unwrap();

        let detection = DetectionRunner::run(temp.path());

        assert!(detection
            .suggested_templates
            .iter()
            .any(|t| t.name == TemplateName::DenoInstall));
    }

    #[test]
    fn full_detection_nextjs_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("package.json"), "{}").unwrap();
        fs::write(temp.path().join("next.config.js"), "").unwrap();
        let detection = DetectionRunner::run(temp.path());
        assert!(detection
            .suggested_templates
            .iter()
            .any(|t| t.name == TemplateName::NextjsBuild));
    }

    #[test]
    fn full_detection_django_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("pyproject.toml"), "").unwrap();
        fs::write(temp.path().join("manage.py"), "").unwrap();
        let detection = DetectionRunner::run(temp.path());
        assert!(detection
            .suggested_templates
            .iter()
            .any(|t| t.name == TemplateName::DjangoMigrate));
    }

    #[test]
    fn full_detection_spring_boot_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("build.gradle.kts"), "").unwrap();
        fs::create_dir_all(temp.path().join("src/main/resources")).unwrap();
        fs::write(
            temp.path()
                .join("src/main/resources/application.properties"),
            "",
        )
        .unwrap();
        let detection = DetectionRunner::run(temp.path());
        assert!(detection
            .suggested_templates
            .iter()
            .any(|t| t.name == TemplateName::SpringBootBuild));
    }
}
