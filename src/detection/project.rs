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
                    .with_template("bundle-install"),
            );

            // Rails (detected alongside Ruby)
            if file_exists(project_root, "config/routes.rb")
                || file_exists(project_root, "config/application.rb")
            {
                details.push(
                    DetectionResult::found("Rails")
                        .with_detail("Rails application detected")
                        .with_template("rails-db"),
                );
            }
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
                Some("yarn.lock") => "yarn-install",
                Some("pnpm-lock.yaml") => "pnpm-install",
                Some("bun.lockb") => "bun-install",
                _ => "npm-install",
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
                        .with_template("nextjs-build"),
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
                        .with_template("vite-build"),
                );
            }

            // Remix (detected alongside Node.js)
            if any_file_exists(project_root, &["remix.config.js", "remix.config.ts"]).is_some()
                || file_exists(project_root, "app/root.tsx")
            {
                details.push(
                    DetectionResult::found("Remix")
                        .with_detail("Remix application detected")
                        .with_template("remix-build"),
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
                "poetry-install"
            } else if file_exists(project_root, "uv.lock") {
                "uv-sync"
            } else {
                "pip-install"
            };

            details.push(
                DetectionResult::found("Python")
                    .with_detail("Python project detected")
                    .with_template(template),
            );

            // Alembic (detected alongside Python)
            if file_exists(project_root, "alembic.ini")
                || file_exists(project_root, "alembic/env.py")
            {
                details.push(
                    DetectionResult::found("Alembic")
                        .with_detail("Alembic configuration found")
                        .with_template("alembic-migrate"),
                );
            }

            // Django (detected alongside Python)
            if file_exists(project_root, "manage.py") {
                details.push(
                    DetectionResult::found("Django")
                        .with_detail("manage.py found")
                        .with_template("django-migrate"),
                );
            }
        }

        // Rust
        if file_exists(project_root, "Cargo.toml") {
            all_types.push(ProjectType::Rust);
            details.push(
                DetectionResult::found("Rust")
                    .with_detail("Cargo.toml found")
                    .with_template("cargo-build"),
            );

            // Diesel (detected alongside Rust)
            if file_exists(project_root, "diesel.toml") {
                details.push(
                    DetectionResult::found("Diesel")
                        .with_detail("diesel.toml found")
                        .with_template("diesel-migrate"),
                );
            }
        }

        // Go
        if file_exists(project_root, "go.mod") {
            all_types.push(ProjectType::Go);
            details.push(
                DetectionResult::found("Go")
                    .with_detail("go.mod found")
                    .with_template("go-mod-download"),
            );
        }

        // PHP
        if file_exists(project_root, "composer.json") {
            all_types.push(ProjectType::Php);
            details.push(
                DetectionResult::found("PHP")
                    .with_detail("composer.json found")
                    .with_template("composer-install"),
            );

            // Laravel (detected alongside PHP, not as a separate project type)
            if file_exists(project_root, "artisan") {
                details.push(
                    DetectionResult::found("Laravel")
                        .with_detail("artisan found")
                        .with_template("laravel-setup"),
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
                    .with_template("gradle-deps"),
            );

            // Spring Boot (detected alongside Gradle)
            if file_exists(project_root, "src/main/resources/application.properties")
                || file_exists(project_root, "src/main/resources/application.yml")
            {
                details.push(
                    DetectionResult::found("Spring Boot")
                        .with_detail("Spring Boot application detected")
                        .with_template("spring-boot-build"),
                );
            }
        }

        // Elixir
        if file_exists(project_root, "mix.exs") {
            all_types.push(ProjectType::Elixir);
            details.push(
                DetectionResult::found("Elixir")
                    .with_detail("mix.exs found")
                    .with_template("mix-deps-get"),
            );
        }

        // Swift
        if file_exists(project_root, "Package.swift") {
            all_types.push(ProjectType::Swift);
            details.push(
                DetectionResult::found("Swift")
                    .with_detail("Package.swift found")
                    .with_template("swift-resolve"),
            );
        }

        // Terraform
        if any_file_exists(project_root, &["main.tf", "terraform.tf", "versions.tf"]).is_some() {
            all_types.push(ProjectType::Terraform);
            details.push(
                DetectionResult::found("Terraform")
                    .with_detail("Terraform files found")
                    .with_template("terraform-init"),
            );
        }

        // AWS CDK (detected alongside other project types)
        if file_exists(project_root, "cdk.json") {
            details.push(
                DetectionResult::found("AWS CDK")
                    .with_detail("cdk.json found")
                    .with_template("cdk-synth"),
            );
        }

        // Maven (Java)
        if file_exists(project_root, "pom.xml") {
            all_types.push(ProjectType::Maven);
            details.push(
                DetectionResult::found("Maven (Java)")
                    .with_detail("pom.xml found")
                    .with_template("maven-resolve"),
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
                    .with_template("dotnet-restore"),
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
                        .with_template("flutter-pub-get"),
                );
            } else {
                details.push(
                    DetectionResult::found("Dart")
                        .with_detail("pubspec.yaml found")
                        .with_template("dart-pub-get"),
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
                    .with_template("deno-install"),
            );
        }

        // Docker Compose (detected alongside other project types)
        if any_file_exists(
            project_root,
            &[
                "docker-compose.yml",
                "docker-compose.yaml",
                "compose.yml",
                "compose.yaml",
            ],
        )
        .is_some()
        {
            details.push(
                DetectionResult::found("Docker Compose")
                    .with_detail("Docker Compose file found")
                    .with_template("docker-compose-up"),
            );
        }

        // Kubernetes/Helm (detected alongside other project types)
        if file_exists(project_root, "Chart.yaml") {
            details.push(
                DetectionResult::found("Helm")
                    .with_detail("Chart.yaml found")
                    .with_template("helm-deps"),
            );
        }

        // Pulumi (detected alongside other project types)
        if file_exists(project_root, "Pulumi.yaml") {
            details.push(
                DetectionResult::found("Pulumi")
                    .with_detail("Pulumi.yaml found")
                    .with_template("pulumi-install"),
            );
        }

        // Ansible (detected alongside other project types)
        if file_exists(project_root, "ansible.cfg")
            || any_file_exists(
                project_root,
                &["playbook.yml", "playbook.yaml", "site.yml", "site.yaml"],
            )
            .is_some()
        {
            details.push(
                DetectionResult::found("Ansible")
                    .with_detail("Ansible configuration found")
                    .with_template("ansible-install"),
            );
        }

        // Prisma (detected alongside other project types)
        if file_exists(project_root, "prisma/schema.prisma") {
            details.push(
                DetectionResult::found("Prisma")
                    .with_detail("Prisma schema found")
                    .with_template("prisma-migrate"),
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
                    .with_template("pre-commit-install"),
            );
        }

        // Monorepo/workspace detection (detected alongside other project types)
        if file_exists(project_root, "nx.json") {
            details.push(
                DetectionResult::found("Nx")
                    .with_detail("Nx workspace detected")
                    .with_template("nx-build"),
            );
        }

        if file_exists(project_root, "turbo.json") {
            details.push(
                DetectionResult::found("Turborepo")
                    .with_detail("Turborepo workspace detected")
                    .with_template("turbo-build"),
            );
        }

        if file_exists(project_root, "lerna.json") {
            details.push(
                DetectionResult::found("Lerna")
                    .with_detail("Lerna monorepo detected")
                    .with_template("lerna-bootstrap"),
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
            .any(|d| d.suggested_template == Some("yarn-install".to_string())));
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
            .any(|d| d.suggested_template == Some("poetry-install".to_string())));
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
            .any(|d| d.suggested_template == Some("composer-install".to_string())));
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
            .any(|d| d.suggested_template == Some("terraform-init".to_string())));
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
            .any(|d| d.suggested_template == Some("gradle-deps".to_string())));
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
            .any(|d| d.suggested_template == Some("gradle-deps".to_string())));
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
            .any(|d| d.suggested_template == Some("terraform-init".to_string())));
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
            .any(|d| d.suggested_template == Some("mix-deps-get".to_string())));
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
            .any(|d| d.suggested_template == Some("composer-install".to_string())));
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("laravel-setup".to_string())));
    }

    #[test]
    fn detect_aws_cdk_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("cdk.json"), "{}").unwrap();

        let detection = ProjectDetector::detect(temp.path());

        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("cdk-synth".to_string())));
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
    fn detect_rails_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("Gemfile"), "source 'https://rubygems.org'").unwrap();
        fs::create_dir_all(temp.path().join("config")).unwrap();
        fs::write(temp.path().join("config/routes.rb"), "").unwrap();

        let detection = ProjectDetector::detect(temp.path());

        assert_eq!(detection.primary_type, ProjectType::Ruby);
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("bundle-install".to_string())));
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("rails-db".to_string())));
    }

    #[test]
    fn detect_prisma_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("package.json"), "{}").unwrap();
        fs::create_dir_all(temp.path().join("prisma")).unwrap();
        fs::write(temp.path().join("prisma/schema.prisma"), "").unwrap();

        let detection = ProjectDetector::detect(temp.path());

        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("prisma-migrate".to_string())));
    }

    #[test]
    fn detect_diesel_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("Cargo.toml"), "").unwrap();
        fs::write(temp.path().join("diesel.toml"), "").unwrap();

        let detection = ProjectDetector::detect(temp.path());

        assert_eq!(detection.primary_type, ProjectType::Rust);
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("cargo-build".to_string())));
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("diesel-migrate".to_string())));
    }

    #[test]
    fn detect_alembic_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("pyproject.toml"), "").unwrap();
        fs::write(temp.path().join("alembic.ini"), "").unwrap();

        let detection = ProjectDetector::detect(temp.path());

        assert_eq!(detection.primary_type, ProjectType::Python);
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("alembic-migrate".to_string())));
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
            .any(|d| d.suggested_template == Some("maven-resolve".to_string())));
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
            .any(|d| d.suggested_template == Some("dotnet-restore".to_string())));
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
            .any(|d| d.suggested_template == Some("dart-pub-get".to_string())));
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
            .any(|d| d.suggested_template == Some("flutter-pub-get".to_string())));
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
            .any(|d| d.suggested_template == Some("deno-install".to_string())));
    }

    #[test]
    fn detect_docker_compose_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("docker-compose.yml"), "version: '3'").unwrap();

        let detection = ProjectDetector::detect(temp.path());

        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("docker-compose-up".to_string())));
    }

    #[test]
    fn detect_docker_compose_new_format() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("compose.yml"), "services:").unwrap();

        let detection = ProjectDetector::detect(temp.path());

        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("docker-compose-up".to_string())));
    }

    #[test]
    fn detect_helm_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("Chart.yaml"), "apiVersion: v2").unwrap();

        let detection = ProjectDetector::detect(temp.path());

        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("helm-deps".to_string())));
    }

    #[test]
    fn detect_pulumi_project() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("Pulumi.yaml"), "name: my-project").unwrap();

        let detection = ProjectDetector::detect(temp.path());

        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("pulumi-install".to_string())));
    }

    #[test]
    fn detect_ansible_project_with_cfg() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("ansible.cfg"), "[defaults]").unwrap();

        let detection = ProjectDetector::detect(temp.path());

        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("ansible-install".to_string())));
    }

    #[test]
    fn detect_ansible_project_with_playbook() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("playbook.yml"), "---").unwrap();

        let detection = ProjectDetector::detect(temp.path());

        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("ansible-install".to_string())));
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
            .any(|d| d.suggested_template == Some("nextjs-build".to_string())));
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
            .any(|d| d.suggested_template == Some("nextjs-build".to_string())));
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
            .any(|d| d.suggested_template == Some("vite-build".to_string())));
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
            .any(|d| d.suggested_template == Some("remix-build".to_string())));
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
            .any(|d| d.suggested_template == Some("remix-build".to_string())));
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
            .any(|d| d.suggested_template == Some("django-migrate".to_string())));
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
            .any(|d| d.suggested_template == Some("gradle-deps".to_string())));
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("spring-boot-build".to_string())));
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
            .any(|d| d.suggested_template == Some("pre-commit-install".to_string())));
    }

    #[test]
    fn detect_nx_workspace() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("nx.json"), "{}").unwrap();
        let detection = ProjectDetector::detect(temp.path());
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("nx-build".to_string())));
    }

    #[test]
    fn detect_turborepo_workspace() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("turbo.json"), "{}").unwrap();
        let detection = ProjectDetector::detect(temp.path());
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("turbo-build".to_string())));
    }

    #[test]
    fn detect_lerna_monorepo() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("lerna.json"), "{}").unwrap();
        let detection = ProjectDetector::detect(temp.path());
        assert!(detection
            .details
            .iter()
            .any(|d| d.suggested_template == Some("lerna-bootstrap".to_string())));
    }
}
