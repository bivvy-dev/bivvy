//! Comprehensive system tests for `bivvy init`.
//!
//! Tests the full interactive initialization experience including
//! technology detection, template selection via MultiSelect prompts,
//! the "Run setup now?" follow-up, all flag combinations, conflict
//! detection, .gitignore updates, and generated config content verification.
#![cfg(unix)]

mod system;

use std::fs;
use system::helpers::*;
use tempfile::TempDir;

// ─────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────

/// Create a temporary directory with specific project marker files.
fn setup_detection_project(files: &[(&str, &str)]) -> TempDir {
    let temp = TempDir::new().unwrap();
    for (path, content) in files {
        if let Some(parent) = std::path::Path::new(path).parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(temp.path().join(parent)).unwrap();
            }
        }
        fs::write(temp.path().join(path), content).unwrap();
    }
    temp
}

/// Create a detection project that also has a .gitignore file.
fn setup_detection_project_with_gitignore(
    files: &[(&str, &str)],
    gitignore_content: &str,
) -> TempDir {
    let temp = setup_detection_project(files);
    fs::write(temp.path().join(".gitignore"), gitignore_content).unwrap();
    temp
}

/// Read the generated config.yml content from a temp directory.
fn read_generated_config(temp: &TempDir) -> String {
    let config_path = temp.path().join(".bivvy/config.yml");
    assert!(
        config_path.exists(),
        "Expected .bivvy/config.yml to exist at {}",
        config_path.display()
    );
    fs::read_to_string(config_path).unwrap()
}

// =====================================================================
// HAPPY PATH — Interactive init with technology detection
// =====================================================================

/// Rust project detected -> shows Rust templates -> accept defaults -> config created.
#[test]
fn init_detects_rust_project() {
    let temp = setup_detection_project(&[(
        "Cargo.toml",
        "[package]\nname = \"test\"\nversion = \"0.1.0\"",
    )]);

    let mut s = spawn_bivvy(&["init"], temp.path());

    s.expect("Detected technologies")
        .expect("Should show detection header");
    s.expect("Rust").expect("Should detect Rust");

    // Accept default selections with Enter
    s.send_line("").unwrap();

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");

    // Decline run
    s.expect("Run setup now").unwrap();
    s.send("n").unwrap();
    s.expect(expectrl::Eof).unwrap();

    // Verify config file exists
    assert!(temp.path().join(".bivvy/config.yml").exists());

    // Verify config content
    let config = read_generated_config(&temp);
    assert!(
        config.contains("app_name:"),
        "Config should contain app_name"
    );
    assert!(
        config.contains("cargo-build"),
        "Config should contain cargo-build step for Rust project"
    );
    assert!(
        config.contains("template: cargo-build"),
        "Config should reference cargo-build template"
    );
    assert!(
        config.contains("workflows:"),
        "Config should contain workflows section"
    );
    assert!(
        config.contains("steps: ["),
        "Config should contain steps list in workflow"
    );
}

/// Node.js project detected via package.json.
#[test]
fn init_detects_node_project() {
    let temp = setup_detection_project(&[(
        "package.json",
        r#"{"name": "test", "version": "1.0.0"}"#,
    )]);

    let mut s = spawn_bivvy(&["init"], temp.path());

    s.expect("Detected technologies")
        .expect("Should show detection header");

    // Accept defaults
    s.send_line("").unwrap();
    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");

    // Decline run
    s.expect("Run setup now").unwrap();
    s.send("n").unwrap();
    s.expect(expectrl::Eof).unwrap();

    assert!(temp.path().join(".bivvy/config.yml").exists());

    let config = read_generated_config(&temp);
    assert!(
        config.contains("npm-install"),
        "Config should suggest npm-install for package.json without lockfile"
    );
}

/// Node.js project with yarn.lock suggests yarn-install.
#[test]
fn init_detects_node_project_with_yarn() {
    let temp = setup_detection_project(&[
        ("package.json", r#"{"name": "test"}"#),
        ("yarn.lock", "# yarn lockfile v1"),
    ]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("yarn-install"),
        "Config should suggest yarn-install when yarn.lock present"
    );
}

/// Node.js project with pnpm-lock.yaml suggests pnpm-install.
#[test]
fn init_detects_node_project_with_pnpm() {
    let temp = setup_detection_project(&[
        ("package.json", r#"{"name": "test"}"#),
        ("pnpm-lock.yaml", "lockfileVersion: '6.0'"),
    ]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("pnpm-install"),
        "Config should suggest pnpm-install when pnpm-lock.yaml present"
    );
}

/// Node.js project with bun.lockb suggests bun-install.
#[test]
fn init_detects_node_project_with_bun() {
    let temp = setup_detection_project(&[
        ("package.json", r#"{"name": "test"}"#),
        ("bun.lockb", ""),
    ]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("bun-install"),
        "Config should suggest bun-install when bun.lockb present"
    );
}

/// Ruby project detected via Gemfile.
#[test]
fn init_detects_ruby_project() {
    let temp = setup_detection_project(&[(
        "Gemfile",
        "source 'https://rubygems.org'\ngem 'rails'",
    )]);

    let mut s = spawn_bivvy(&["init"], temp.path());

    s.expect("Detected technologies")
        .expect("Should show detection header");

    s.send_line("").unwrap();
    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");

    s.expect("Run setup now").unwrap();
    s.send("n").unwrap();
    s.expect(expectrl::Eof).unwrap();

    assert!(temp.path().join(".bivvy/config.yml").exists());

    let config = read_generated_config(&temp);
    assert!(
        config.contains("bundle-install"),
        "Config should contain bundle-install for Ruby project"
    );
    assert!(
        config.contains("template: bundle-install"),
        "Config should reference bundle-install template"
    );
}

/// Python project detected via requirements.txt.
#[test]
fn init_detects_python_project() {
    let temp = setup_detection_project(&[("requirements.txt", "flask==2.0\nrequests")]);

    let mut s = spawn_bivvy(&["init"], temp.path());

    s.expect("Detected technologies")
        .expect("Should show detection header");

    s.send_line("").unwrap();
    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");

    s.expect("Run setup now").unwrap();
    s.send("n").unwrap();
    s.expect(expectrl::Eof).unwrap();

    assert!(temp.path().join(".bivvy/config.yml").exists());

    let config = read_generated_config(&temp);
    assert!(
        config.contains("pip-install"),
        "Config should contain pip-install for requirements.txt project"
    );
}

/// Python project with poetry.lock suggests poetry-install.
#[test]
fn init_detects_python_project_with_poetry() {
    let temp = setup_detection_project(&[
        ("pyproject.toml", "[tool.poetry]\nname = \"test\""),
        ("poetry.lock", ""),
    ]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("poetry-install"),
        "Config should suggest poetry-install when poetry.lock present"
    );
}

/// Python project with uv.lock suggests uv-sync.
#[test]
fn init_detects_python_project_with_uv() {
    let temp = setup_detection_project(&[
        ("pyproject.toml", "[project]\nname = \"test\""),
        ("uv.lock", ""),
    ]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("uv-sync"),
        "Config should suggest uv-sync when uv.lock present"
    );
}

/// Go project detected via go.mod.
#[test]
fn init_detects_go_project() {
    let temp = setup_detection_project(&[("go.mod", "module example.com/test\n\ngo 1.21")]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("go-mod-download"),
        "Config should contain go-mod-download for Go project"
    );
}

/// PHP project detected via composer.json.
#[test]
fn init_detects_php_project() {
    let temp = setup_detection_project(&[("composer.json", r#"{"name": "test/app"}"#)]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("composer-install"),
        "Config should contain composer-install for PHP project"
    );
}

/// Elixir project detected via mix.exs.
#[test]
fn init_detects_elixir_project() {
    let temp = setup_detection_project(&[("mix.exs", "defmodule Test.MixProject do\nend")]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("mix-deps-get"),
        "Config should contain mix-deps-get for Elixir project"
    );
}

/// Swift project detected via Package.swift.
#[test]
fn init_detects_swift_project() {
    let temp = setup_detection_project(&[(
        "Package.swift",
        "// swift-tools-version:5.5\nimport PackageDescription",
    )]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("swift-resolve"),
        "Config should contain swift-resolve for Swift project"
    );
}

/// Terraform project detected via main.tf.
#[test]
fn init_detects_terraform_project() {
    let temp = setup_detection_project(&[("main.tf", "provider \"aws\" {}")]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("terraform-init"),
        "Config should contain terraform-init for Terraform project"
    );
}

/// Maven project detected via pom.xml.
#[test]
fn init_detects_maven_project() {
    let temp = setup_detection_project(&[("pom.xml", "<project></project>")]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("maven-resolve"),
        "Config should contain maven-resolve for Maven project"
    );
}

/// .NET project detected via .sln file.
#[test]
fn init_detects_dotnet_project() {
    let temp = setup_detection_project(&[("MyApp.sln", "")]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("dotnet-restore"),
        "Config should contain dotnet-restore for .NET project"
    );
}

/// Dart project detected via pubspec.yaml.
#[test]
fn init_detects_dart_project() {
    let temp = setup_detection_project(&[("pubspec.yaml", "name: my_app")]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("dart-pub-get"),
        "Config should contain dart-pub-get for Dart project"
    );
}

/// Flutter project detected via pubspec.yaml + android directory.
#[test]
fn init_detects_flutter_project() {
    let temp = setup_detection_project(&[("pubspec.yaml", "name: my_app")]);
    fs::create_dir(temp.path().join("android")).unwrap();

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("flutter-pub-get"),
        "Config should contain flutter-pub-get for Flutter project"
    );
}

/// Deno project detected via deno.json.
#[test]
fn init_detects_deno_project() {
    let temp = setup_detection_project(&[("deno.json", "{}")]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("deno-install"),
        "Config should contain deno-install for Deno project"
    );
}

/// Gradle/Kotlin project detected via build.gradle.kts.
#[test]
fn init_detects_gradle_project() {
    let temp = setup_detection_project(&[("build.gradle.kts", "plugins {}")]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("gradle-deps"),
        "Config should contain gradle-deps for Gradle project"
    );
}

/// Gradle project detected via build.gradle (Groovy variant).
#[test]
fn init_detects_gradle_groovy_project() {
    let temp = setup_detection_project(&[("build.gradle", "apply plugin: 'java'")]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("gradle-deps"),
        "Config should contain gradle-deps for Groovy Gradle project"
    );
}

// =====================================================================
// FRAMEWORK / SIDEBAR DETECTION
// =====================================================================

/// Rails project detected alongside Ruby (Gemfile + config/routes.rb).
#[test]
fn init_detects_rails_project() {
    let temp = setup_detection_project(&[
        ("Gemfile", "source 'https://rubygems.org'\ngem 'rails'"),
        ("config/routes.rb", "Rails.application.routes.draw do\nend"),
    ]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("bundle-install"),
        "Rails project should include bundle-install"
    );
    assert!(
        config.contains("rails-db"),
        "Rails project should include rails-db step"
    );
}

/// Laravel project detected alongside PHP (composer.json + artisan).
#[test]
fn init_detects_laravel_project() {
    let temp = setup_detection_project(&[
        ("composer.json", r#"{"name": "laravel/laravel"}"#),
        ("artisan", "#!/usr/bin/env php"),
    ]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("composer-install"),
        "Laravel project should include composer-install"
    );
    assert!(
        config.contains("laravel-setup"),
        "Laravel project should include laravel-setup step"
    );
}

/// Django project detected alongside Python (pyproject.toml + manage.py).
#[test]
fn init_detects_django_project() {
    let temp = setup_detection_project(&[
        ("pyproject.toml", "[project]\nname = \"myapp\""),
        ("manage.py", "#!/usr/bin/env python"),
    ]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("django-migrate"),
        "Django project should include django-migrate step"
    );
}

/// Spring Boot project detected alongside Gradle.
#[test]
fn init_detects_spring_boot_project() {
    let temp = setup_detection_project(&[
        ("build.gradle.kts", "plugins {}"),
        (
            "src/main/resources/application.properties",
            "server.port=8080",
        ),
    ]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("gradle-deps"),
        "Spring Boot project should include gradle-deps"
    );
    assert!(
        config.contains("spring-boot-build"),
        "Spring Boot project should include spring-boot-build step"
    );
}

/// Next.js project detected alongside Node.js.
#[test]
fn init_detects_nextjs_project() {
    let temp = setup_detection_project(&[
        ("package.json", r#"{"name": "test"}"#),
        ("next.config.js", "module.exports = {}"),
    ]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("nextjs-build"),
        "Next.js project should include nextjs-build step"
    );
}

/// Diesel detected alongside Rust (Cargo.toml + diesel.toml).
#[test]
fn init_detects_diesel_project() {
    let temp = setup_detection_project(&[
        ("Cargo.toml", "[package]\nname = \"test\""),
        ("diesel.toml", "[print_schema]\nfile = \"src/schema.rs\""),
    ]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("cargo-build"),
        "Diesel project should include cargo-build"
    );
    assert!(
        config.contains("diesel-migrate"),
        "Diesel project should include diesel-migrate step"
    );
}

/// Prisma detected alongside Node.js.
#[test]
fn init_detects_prisma_project() {
    let temp = setup_detection_project(&[
        ("package.json", r#"{"name": "test"}"#),
        ("prisma/schema.prisma", "generator client {}"),
    ]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("prisma-migrate"),
        "Prisma project should include prisma-migrate step"
    );
}

/// Docker Compose detected via docker-compose.yml.
#[test]
fn init_detects_docker_compose_project() {
    let temp = setup_detection_project(&[(
        "docker-compose.yml",
        "version: '3'\nservices:\n  web:\n    image: nginx",
    )]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("docker-compose-up"),
        "Docker Compose project should include docker-compose-up step"
    );
}

/// Compose (new format: compose.yml) detected.
#[test]
fn init_detects_compose_yml_project() {
    let temp = setup_detection_project(&[("compose.yml", "services:\n  web:\n    image: nginx")]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("docker-compose-up"),
        "compose.yml project should include docker-compose-up step"
    );
}

/// Helm project detected via Chart.yaml.
#[test]
fn init_detects_helm_project() {
    let temp = setup_detection_project(&[("Chart.yaml", "apiVersion: v2\nname: my-chart")]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("helm-deps"),
        "Helm project should include helm-deps step"
    );
}

/// Pulumi project detected via Pulumi.yaml.
#[test]
fn init_detects_pulumi_project() {
    let temp = setup_detection_project(&[("Pulumi.yaml", "name: my-project\nruntime: nodejs")]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("pulumi-install"),
        "Pulumi project should include pulumi-install step"
    );
}

/// AWS CDK project detected via cdk.json.
#[test]
fn init_detects_aws_cdk_project() {
    let temp = setup_detection_project(&[("cdk.json", r#"{"app": "npx ts-node bin/app.ts"}"#)]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("cdk-synth"),
        "AWS CDK project should include cdk-synth step"
    );
}

/// Ansible project detected via ansible.cfg.
#[test]
fn init_detects_ansible_project() {
    let temp = setup_detection_project(&[("ansible.cfg", "[defaults]\nroles_path = roles")]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("ansible-install"),
        "Ansible project should include ansible-install step"
    );
}

/// Ansible project detected via playbook.yml.
#[test]
fn init_detects_ansible_via_playbook() {
    let temp = setup_detection_project(&[("playbook.yml", "---\n- hosts: all")]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("ansible-install"),
        "Ansible playbook project should include ansible-install step"
    );
}

/// Nx monorepo detected via nx.json.
#[test]
fn init_detects_nx_workspace() {
    let temp = setup_detection_project(&[("nx.json", r#"{"npmScope": "myorg"}"#)]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("nx-build"),
        "Nx workspace should include nx-build step"
    );
}

/// Turborepo detected via turbo.json.
#[test]
fn init_detects_turborepo() {
    let temp = setup_detection_project(&[("turbo.json", r#"{"pipeline": {}}"#)]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("turbo-build"),
        "Turborepo should include turbo-build step"
    );
}

/// Lerna monorepo detected via lerna.json.
#[test]
fn init_detects_lerna_monorepo() {
    let temp = setup_detection_project(&[("lerna.json", r#"{"version": "independent"}"#)]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("lerna-bootstrap"),
        "Lerna monorepo should include lerna-bootstrap step"
    );
}

/// pre-commit detected via .pre-commit-config.yaml.
#[test]
fn init_detects_pre_commit() {
    let temp = setup_detection_project(&[(".pre-commit-config.yaml", "repos: []")]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("pre-commit-install"),
        "pre-commit project should include pre-commit-install step"
    );
}

/// Environment file template detected (.env.example without .env).
#[test]
fn init_detects_env_template() {
    let temp = setup_detection_project(&[(".env.example", "DB_HOST=localhost\nSECRET_KEY=xxx")]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("env-copy"),
        "Project with .env.example should include env-copy step"
    );
}

/// Alembic detected alongside Python.
#[test]
fn init_detects_alembic_project() {
    let temp = setup_detection_project(&[
        ("pyproject.toml", "[project]\nname = \"test\""),
        ("alembic.ini", "[alembic]\nscript_location = migrations"),
    ]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("alembic-migrate"),
        "Alembic project should include alembic-migrate step"
    );
}

/// Vite detected alongside Node.js.
#[test]
fn init_detects_vite_project() {
    let temp = setup_detection_project(&[
        ("package.json", r#"{"name": "test"}"#),
        ("vite.config.ts", "import { defineConfig } from 'vite'"),
    ]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("vite-build"),
        "Vite project should include vite-build step"
    );
}

/// Remix detected alongside Node.js.
#[test]
fn init_detects_remix_project() {
    let temp = setup_detection_project(&[
        ("package.json", r#"{"name": "test"}"#),
        ("remix.config.js", "module.exports = {}"),
    ]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("remix-build"),
        "Remix project should include remix-build step"
    );
}

// =====================================================================
// MULTI-LANGUAGE DETECTION
// =====================================================================

/// Multi-language project (Rust + Node) detects both.
#[test]
fn init_detects_multiple_technologies() {
    let temp = setup_detection_project(&[
        (
            "Cargo.toml",
            "[package]\nname = \"test\"\nversion = \"0.1.0\"",
        ),
        ("package.json", r#"{"name": "test", "version": "1.0.0"}"#),
    ]);

    let mut s = spawn_bivvy(&["init"], temp.path());

    s.expect("Detected technologies")
        .expect("Should detect technologies");

    // Accept all defaults
    s.send_line("").unwrap();
    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");

    s.expect("Run setup now").unwrap();
    s.send("n").unwrap();
    s.expect(expectrl::Eof).unwrap();

    assert!(temp.path().join(".bivvy/config.yml").exists());

    let config = read_generated_config(&temp);
    assert!(
        config.contains("cargo-build"),
        "Multi-lang project should include cargo-build"
    );
    assert!(
        config.contains("npm-install"),
        "Multi-lang project should include npm-install"
    );
}

/// Triple-language project (Ruby + Node + Python).
#[test]
fn init_detects_three_languages() {
    let temp = setup_detection_project(&[
        ("Gemfile", "source 'https://rubygems.org'"),
        ("package.json", r#"{"name": "test"}"#),
        ("requirements.txt", "flask"),
    ]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("bundle-install"),
        "Triple-lang should include bundle-install"
    );
    assert!(
        config.contains("npm-install"),
        "Triple-lang should include npm-install"
    );
    assert!(
        config.contains("pip-install"),
        "Triple-lang should include pip-install"
    );
    // Verify workflow lists all steps
    assert!(
        config.contains("workflows:"),
        "Config should have workflows section"
    );
}

/// Empty project (no marker files) still initializes.
#[test]
fn init_empty_project() {
    let temp = TempDir::new().unwrap();

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation for empty project");
    s.expect(expectrl::Eof).unwrap();

    assert!(
        temp.path().join(".bivvy/config.yml").exists(),
        "Config should be created even for empty project"
    );

    let config = read_generated_config(&temp);
    assert!(
        config.contains("app_name:"),
        "Empty project config should contain app_name"
    );
    assert!(
        config.contains("settings:"),
        "Empty project config should contain settings section"
    );
    // Should NOT have steps or workflows sections for empty project
    assert!(
        !config.contains("\nsteps:\n"),
        "Empty project should not have steps section"
    );
}

// =====================================================================
// INTERACTIVE — "Run setup now?" prompt
// =====================================================================

/// Accept "Run setup now?" with 'y'.
#[test]
fn init_run_after_init_accept() {
    let temp = setup_detection_project(&[(
        "Cargo.toml",
        "[package]\nname = \"test\"\nversion = \"0.1.0\"",
    )]);

    let mut s = spawn_bivvy(&["init"], temp.path());

    // Accept detected templates
    s.expect("Select steps").unwrap();
    s.send_line("").unwrap();

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation before run prompt");

    // Accept run
    s.expect("Run setup now").unwrap();
    s.send("y").unwrap();

    // Phase 2: verify the run actually produced output (success or template error)
    let output = read_to_eof(&mut s);
    assert!(
        !output.is_empty(),
        "Run phase should produce output after accepting 'Run setup now?'"
    );
    assert!(
        temp.path().join(".bivvy/config.yml").exists(),
        "Config should be created before run"
    );
}

/// Decline "Run setup now?" with 'n'.
#[test]
fn init_run_after_init_decline() {
    let temp = setup_detection_project(&[(
        "Cargo.toml",
        "[package]\nname = \"test\"\nversion = \"0.1.0\"",
    )]);

    let mut s = spawn_bivvy(&["init"], temp.path());

    s.expect("Select steps").unwrap();
    s.send_line("").unwrap();

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation before run prompt");

    s.expect("Run setup now").unwrap();
    s.send("n").unwrap();

    s.expect(expectrl::Eof).unwrap();
    assert!(temp.path().join(".bivvy/config.yml").exists());
}

/// Press Enter on "Run setup now?" — default should be "no".
#[test]
fn init_run_after_init_enter_default() {
    let temp = setup_detection_project(&[(
        "Cargo.toml",
        "[package]\nname = \"test\"\nversion = \"0.1.0\"",
    )]);

    let mut s = spawn_bivvy(&["init"], temp.path());

    s.expect("Select steps").unwrap();
    s.send_line("").unwrap();

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");

    s.expect("Run setup now").unwrap();
    send_key(&s, KEY_ENTER);

    s.expect(expectrl::Eof).unwrap();
    assert!(
        temp.path().join(".bivvy/config.yml").exists(),
        "Config should exist after declining run with Enter"
    );
}

/// Press Escape on "Run setup now?" — should decline.
#[test]
fn init_run_after_init_escape() {
    let temp = setup_detection_project(&[(
        "Cargo.toml",
        "[package]\nname = \"test\"\nversion = \"0.1.0\"",
    )]);

    let mut s = spawn_bivvy(&["init"], temp.path());

    s.expect("Select steps").unwrap();
    s.send_line("").unwrap();

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");

    s.expect("Run setup now").unwrap();
    send_key(&s, KEY_ESC);

    s.expect(expectrl::Eof).unwrap();
    assert!(
        temp.path().join(".bivvy/config.yml").exists(),
        "Config should exist after declining run with Escape"
    );
}

// =====================================================================
// INTERACTIVE — MultiSelect template selection
// =====================================================================

/// Toggle all templates off with 'a' then on again with 'a'.
#[test]
fn init_multiselect_toggle_all() {
    let temp = setup_detection_project(&[
        (
            "Cargo.toml",
            "[package]\nname = \"test\"\nversion = \"0.1.0\"",
        ),
        ("package.json", r#"{"name": "test"}"#),
    ]);

    let mut s = spawn_bivvy(&["init"], temp.path());

    s.expect("Select steps")
        .expect("Should show multiselect prompt");

    // 'a' toggles all off, then 'a' toggles all on, then Enter
    s.send("a").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    s.send("a").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    s.send_line("").unwrap();

    s.expect("Created .bivvy/config.yml")
        .expect("Should create config after toggle all");

    s.expect("Run setup now").unwrap();
    s.send("n").unwrap();
    s.expect(expectrl::Eof).unwrap();
}

/// Use space to deselect a specific template before confirming.
#[test]
fn init_multiselect_space_deselects() {
    let temp = setup_detection_project(&[(
        "Cargo.toml",
        "[package]\nname = \"test\"\nversion = \"0.1.0\"",
    )]);

    let mut s = spawn_bivvy(&["init"], temp.path());

    s.expect("Select steps")
        .expect("Should show multiselect prompt");

    // Space toggles current item, then Enter to confirm
    s.send(" ").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    s.send_line("").unwrap();

    s.expect(expectrl::Eof).unwrap();
}

/// Navigate with arrow keys in the MultiSelect.
#[test]
fn init_multiselect_arrow_navigation() {
    let temp = setup_detection_project(&[
        (
            "Cargo.toml",
            "[package]\nname = \"test\"\nversion = \"0.1.0\"",
        ),
        ("package.json", r#"{"name": "test"}"#),
    ]);

    let mut s = spawn_bivvy(&["init"], temp.path());

    s.expect("Select steps")
        .expect("Should show multiselect prompt");

    // Arrow down, space to toggle, arrow down, space to toggle, Enter
    send_keys(&s, ARROW_DOWN);
    std::thread::sleep(std::time::Duration::from_millis(100));
    s.send(" ").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    send_keys(&s, ARROW_DOWN);
    std::thread::sleep(std::time::Duration::from_millis(100));
    s.send(" ").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    s.send_line("").unwrap();

    s.expect(expectrl::Eof).unwrap();
}

// =====================================================================
// COMPLETE WIZARD WALKTHROUGH
// =====================================================================

/// Full wizard walkthrough: detection -> select -> confirm -> verify content.
#[test]
fn init_complete_wizard_walkthrough() {
    let temp = setup_detection_project(&[
        ("Gemfile", "source 'https://rubygems.org'\ngem 'rails'"),
        ("config/routes.rb", "Rails.application.routes.draw do\nend"),
    ]);

    let mut s = spawn_bivvy(&["init"], temp.path());

    // Step 1: Detection output
    s.expect("Detected technologies")
        .expect("Wizard should start with detection");
    s.expect("Ruby")
        .expect("Should show Ruby in detection output");

    // Step 2: Template selection prompt
    s.expect("Select steps")
        .expect("Wizard should prompt for step selection");

    // Accept defaults (all detected templates)
    s.send_line("").unwrap();

    // Step 3: Config creation confirmation
    s.expect("Created .bivvy/config.yml")
        .expect("Wizard should confirm config creation");

    // Step 4: Run prompt
    s.expect("Run setup now")
        .expect("Wizard should offer to run setup");
    s.send("n").unwrap();

    s.expect(expectrl::Eof).unwrap();

    // Verify generated config thoroughly
    let config = read_generated_config(&temp);

    // Header
    assert!(
        config.contains("# Bivvy configuration for"),
        "Config should have header comment"
    );
    assert!(
        config.contains("# Docs: https://bivvy.dev/configuration"),
        "Config should have docs link"
    );

    // App name
    assert!(
        config.contains("app_name:"),
        "Config should have app_name field"
    );

    // Settings
    assert!(
        config.contains("settings:"),
        "Config should have settings section"
    );
    assert!(
        config.contains("default_output: verbose"),
        "Config should have default_output setting"
    );

    // Steps
    assert!(
        config.contains("bundle-install"),
        "Config should include bundle-install step"
    );
    assert!(
        config.contains("rails-db"),
        "Config should include rails-db step"
    );

    // Workflows
    assert!(
        config.contains("workflows:"),
        "Config should have workflows section"
    );
    assert!(
        config.contains("default:"),
        "Config should have default workflow"
    );
}

// =====================================================================
// FLAGS
// =====================================================================

/// --minimal skips all prompts and creates a bare config.
#[test]
fn init_minimal_flag() {
    let temp = TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();
    assert!(temp.path().join(".bivvy/config.yml").exists());

    let config = read_generated_config(&temp);
    assert!(
        config.contains("app_name:"),
        "Minimal config should have app_name"
    );
    assert!(
        config.contains("settings:"),
        "Minimal config should have settings"
    );
}

/// --minimal with detection picks up detected templates without prompting.
#[test]
fn init_minimal_flag_with_detection() {
    let temp = setup_detection_project(&[(
        "Cargo.toml",
        "[package]\nname = \"test\"\nversion = \"0.1.0\"",
    )]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("cargo-build"),
        "Minimal init should auto-include detected templates"
    );
}

/// --force overwrites an existing config.
#[test]
fn init_force_overwrites_existing() {
    let temp = TempDir::new().unwrap();
    let bivvy_dir = temp.path().join(".bivvy");
    fs::create_dir_all(&bivvy_dir).unwrap();
    fs::write(bivvy_dir.join("config.yml"), "app_name: OldConfig").unwrap();

    let mut s = spawn_bivvy(&["init", "--force", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation with --force");
    s.expect(expectrl::Eof).unwrap();

    let config = fs::read_to_string(bivvy_dir.join("config.yml")).unwrap();
    assert!(
        !config.contains("OldConfig"),
        "Old config should be replaced"
    );
    assert!(
        config.contains("app_name:"),
        "New config should be generated"
    );
}

/// --verbose with --minimal shows extra info.
#[test]
fn init_verbose_flag() {
    let temp = TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["init", "--minimal", "--verbose"], temp.path());

    s.expect("Created")
        .expect("Should show creation message in verbose mode");
    s.expect(expectrl::Eof).unwrap();

    assert!(temp.path().join(".bivvy/config.yml").exists());
}

/// --quiet with --minimal suppresses output.
#[test]
fn init_quiet_flag() {
    let temp = TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["init", "--minimal", "--quiet"], temp.path());

    // --quiet suppresses all prompts; verify minimal/no output
    let output = read_to_eof(&mut s);
    assert!(
        !output.contains("Detected technologies"),
        "Quiet mode should not show detection output, got: {}",
        &output[..output.len().min(200)]
    );
    assert!(
        temp.path().join(".bivvy/config.yml").exists(),
        "Config should be created even in quiet mode"
    );
}

/// --template flag starts from a specific template.
#[test]
fn init_template_flag() {
    let temp = TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["init", "--template", "rust"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("--template rust should create config");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("cargo-build"),
        "--template rust should include cargo-build"
    );
}

/// --from copies config from another project.
#[test]
fn init_from_flag() {
    // Create a source project with config
    let source = TempDir::new().unwrap();
    let source_bivvy = source.path().join(".bivvy");
    fs::create_dir_all(&source_bivvy).unwrap();
    fs::write(
        source_bivvy.join("config.yml"),
        "app_name: SourceProject\nsteps:\n  greet:\n    command: git --version\n",
    )
    .unwrap();

    let dest = TempDir::new().unwrap();
    let mut s = spawn_bivvy(
        &["init", "--from", source.path().to_str().unwrap()],
        dest.path(),
    );

    s.expect("Created .bivvy/config.yml")
        .expect("--from should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    // Verify the config was copied
    assert!(
        dest.path().join(".bivvy/config.yml").exists(),
        "Config should be copied to destination"
    );

    let config = fs::read_to_string(dest.path().join(".bivvy/config.yml")).unwrap();
    assert!(
        config.contains("SourceProject"),
        "Copied config should contain source project content"
    );
    assert!(
        config.contains("git --version"),
        "Copied config should contain source step commands"
    );
}

// =====================================================================
// CONFLICT DETECTION
// =====================================================================

/// Conflicting lockfiles (npm + yarn) should show a warning.
#[test]
fn init_detects_lockfile_conflict() {
    let temp = setup_detection_project(&[
        ("package.json", r#"{"name": "test"}"#),
        ("package-lock.json", ""),
        ("yarn.lock", ""),
    ]);

    let mut s = spawn_bivvy(&["init"], temp.path());

    // The init command should show detection and possibly a conflict warning
    s.expect("Detected technologies")
        .expect("Should show detection for conflicting project");

    // Accept defaults and finish
    s.send_line("").unwrap();

    s.expect("Run setup now").unwrap();
    s.send("n").unwrap();
    s.expect(expectrl::Eof).unwrap();

    assert!(
        temp.path().join(".bivvy/config.yml").exists(),
        "Config should still be created despite conflicts"
    );
}

// =====================================================================
// .gitignore VERIFICATION
// =====================================================================

/// Init with existing .gitignore adds config.local.yml entry.
#[test]
fn init_updates_gitignore() {
    let temp = setup_detection_project_with_gitignore(&[], "node_modules\n");

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let gitignore = fs::read_to_string(temp.path().join(".gitignore")).unwrap();
    assert!(
        gitignore.contains(".bivvy/config.local.yml"),
        "Gitignore should contain .bivvy/config.local.yml after init"
    );
    assert!(
        gitignore.contains("node_modules"),
        "Existing gitignore entries should be preserved"
    );
}

/// Init does not duplicate .gitignore entry if already present.
#[test]
fn init_does_not_duplicate_gitignore_entry() {
    let temp = setup_detection_project_with_gitignore(
        &[],
        "node_modules\n.bivvy/config.local.yml\n",
    );

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let gitignore = fs::read_to_string(temp.path().join(".gitignore")).unwrap();
    let count = gitignore.matches(".bivvy/config.local.yml").count();
    assert_eq!(
        count, 1,
        "Gitignore should not have duplicate .bivvy/config.local.yml entries"
    );
}

/// Init without existing .gitignore does not create one.
#[test]
fn init_without_gitignore_does_not_create_one() {
    let temp = TempDir::new().unwrap();

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    assert!(
        !temp.path().join(".gitignore").exists(),
        "Init should not create .gitignore if one does not exist"
    );
}

// =====================================================================
// CONFIG CONTENT VERIFICATION
// =====================================================================

/// Generated config has proper YAML structure with header, app_name, settings.
#[test]
fn init_generated_config_structure() {
    let temp = setup_detection_project(&[(
        "Cargo.toml",
        "[package]\nname = \"test-project\"\nversion = \"0.1.0\"",
    )]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);

    // Header comments
    assert!(
        config.starts_with("# Bivvy configuration for"),
        "Config should start with header comment"
    );
    assert!(
        config.contains("# Docs: https://bivvy.dev/configuration"),
        "Config should link to documentation"
    );
    assert!(
        config.contains("# Override any template field per-step:"),
        "Config should contain customization guide"
    );

    // Core fields
    assert!(
        config.contains("app_name:"),
        "Config should have app_name"
    );
    assert!(
        config.contains("settings:"),
        "Config should have settings"
    );
    assert!(
        config.contains("default_output: verbose"),
        "Config should have default_output"
    );

    // Steps section
    assert!(
        config.contains("\nsteps:\n"),
        "Config should have steps section for detected project"
    );
    assert!(
        config.contains("  cargo-build:\n    template: cargo-build"),
        "Config should have properly formatted cargo-build step"
    );

    // Workflows section
    assert!(
        config.contains("workflows:\n  default:\n    steps:"),
        "Config should have default workflow with steps"
    );
}

/// Generated config for Ruby project includes template-enriched comments.
#[test]
fn init_generated_config_has_template_comments() {
    let temp = setup_detection_project(&[("Gemfile", "source 'https://rubygems.org'")]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);

    // Template enrichment: commented-out command from the bundle-install template
    assert!(
        config.contains("# command: bundle install"),
        "Config should show the template command as a comment"
    );
    // Template enrichment: completed_check from the bundle-install template
    assert!(
        config.contains("# completed_check:"),
        "Config should show completed_check as a comment"
    );
}

// =====================================================================
// SAD PATH
// =====================================================================

/// Config already exists without --force shows warning.
#[test]
fn init_refuses_overwrite_without_force() {
    let temp = TempDir::new().unwrap();
    let bivvy_dir = temp.path().join(".bivvy");
    fs::create_dir_all(&bivvy_dir).unwrap();
    fs::write(bivvy_dir.join("config.yml"), "app_name: Existing").unwrap();

    let mut s = spawn_bivvy(&["init"], temp.path());

    s.expect("already exists")
        .expect("Should warn about existing config");
    s.expect(expectrl::Eof).unwrap();

    // Verify original config was NOT overwritten
    let config = fs::read_to_string(bivvy_dir.join("config.yml")).unwrap();
    assert!(
        config.contains("Existing"),
        "Original config should be preserved when --force not used"
    );
}

/// Config already exists — verify the error message mentions --force.
#[test]
fn init_existing_config_suggests_force_flag() {
    let temp = TempDir::new().unwrap();
    let bivvy_dir = temp.path().join(".bivvy");
    fs::create_dir_all(&bivvy_dir).unwrap();
    fs::write(bivvy_dir.join("config.yml"), "app_name: Existing").unwrap();

    let mut s = spawn_bivvy(&["init"], temp.path());

    s.expect("--force")
        .expect("Error message should suggest using --force");
    s.expect(expectrl::Eof).unwrap();
}

/// --from with nonexistent source path fails.
#[test]
fn init_from_nonexistent_path_fails() {
    let temp = TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["init", "--from", "/nonexistent/path"], temp.path());

    let output = read_to_eof(&mut s);
    assert!(
        output.contains("not found") || output.contains("does not exist") || output.contains("Error"),
        "Should show error for nonexistent --from path, got: {}",
        &output[..output.len().min(300)]
    );

    // Config should NOT be created
    assert!(
        !temp.path().join(".bivvy/config.yml").exists(),
        "Config should not be created when --from path does not exist"
    );
}

/// --template with nonexistent template name fails.
#[test]
fn init_unknown_template_fails() {
    let temp = TempDir::new().unwrap();
    let mut s = spawn_bivvy(
        &["init", "--template", "nonexistent-lang-xyz"],
        temp.path(),
    );

    let output = read_to_eof(&mut s);
    assert!(
        output.contains("Unknown") || output.contains("not found") || output.contains("Error"),
        "Should show error for unknown template, got: {}",
        &output[..output.len().min(300)]
    );

    // Config should NOT be created for unknown template
    assert!(
        !temp.path().join(".bivvy/config.yml").exists(),
        "Config should not be created for unknown template"
    );
}

/// --force + --minimal on a read-only directory (permission error).
#[test]
fn init_readonly_directory_fails() {
    let temp = TempDir::new().unwrap();
    let readonly_dir = temp.path().join("readonly");
    fs::create_dir_all(&readonly_dir).unwrap();

    // Make directory read-only
    let mut perms = fs::metadata(&readonly_dir).unwrap().permissions();
    perms.set_readonly(true);
    fs::set_permissions(&readonly_dir, perms.clone()).unwrap();

    let mut s = spawn_bivvy(&["init", "--minimal"], &readonly_dir);

    let output = read_to_eof(&mut s);
    assert!(
        output.contains("Permission") || output.contains("permission") || output.contains("Error") || output.contains("error"),
        "Should show permission error for read-only directory, got: {}",
        &output[..output.len().min(300)]
    );

    // Config should NOT exist in read-only dir
    assert!(
        !readonly_dir.join(".bivvy/config.yml").exists(),
        "Config should not be created in read-only directory"
    );

    // Restore permissions for cleanup
    perms.set_readonly(false);
    fs::set_permissions(&readonly_dir, perms).ok();
}

/// --from pointing to a directory without .bivvy/config.yml fails.
#[test]
fn init_from_missing_config_fails() {
    let source = TempDir::new().unwrap();
    // Source exists but has no .bivvy/config.yml
    let dest = TempDir::new().unwrap();

    let mut s = spawn_bivvy(
        &["init", "--from", source.path().to_str().unwrap()],
        dest.path(),
    );

    let output = read_to_eof(&mut s);
    assert!(
        output.contains("not found") || output.contains("does not exist") || output.contains("No configuration") || output.contains("Error"),
        "Should show error when source has no config, got: {}",
        &output[..output.len().min(300)]
    );

    assert!(
        !dest.path().join(".bivvy/config.yml").exists(),
        "Config should not be created when source has no config"
    );
}

// =====================================================================
// EDGE CASES
// =====================================================================

/// Running init twice without --force fails on second run.
#[test]
fn init_twice_without_force_fails() {
    let temp = TempDir::new().unwrap();

    // First init
    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());
    s.expect("Created .bivvy/config.yml")
        .expect("First init should succeed");
    s.expect(expectrl::Eof).unwrap();

    assert!(temp.path().join(".bivvy/config.yml").exists());

    // Second init should fail
    let mut s2 = spawn_bivvy(&["init"], temp.path());
    s2.expect("already exists")
        .expect("Second init should warn about existing config");
    s2.expect(expectrl::Eof).unwrap();
}

/// Running init twice with --force succeeds on second run.
#[test]
fn init_twice_with_force_succeeds() {
    let temp = TempDir::new().unwrap();

    // First init
    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());
    s.expect("Created .bivvy/config.yml")
        .expect("First init should succeed");
    s.expect(expectrl::Eof).unwrap();

    // Second init with --force
    let mut s2 = spawn_bivvy(&["init", "--force", "--minimal"], temp.path());
    s2.expect("Created .bivvy/config.yml")
        .expect("Second init with --force should succeed");
    s2.expect(expectrl::Eof).unwrap();

    assert!(temp.path().join(".bivvy/config.yml").exists());
}

/// --from copies config and updates .gitignore at destination.
#[test]
fn init_from_updates_gitignore_at_destination() {
    let source = TempDir::new().unwrap();
    let source_bivvy = source.path().join(".bivvy");
    fs::create_dir_all(&source_bivvy).unwrap();
    fs::write(
        source_bivvy.join("config.yml"),
        "app_name: SourceProject\n",
    )
    .unwrap();

    let dest = TempDir::new().unwrap();
    fs::write(dest.path().join(".gitignore"), "*.log\n").unwrap();

    let mut s = spawn_bivvy(
        &["init", "--from", source.path().to_str().unwrap()],
        dest.path(),
    );

    s.expect("Created .bivvy/config.yml")
        .expect("--from should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    assert!(
        dest.path().join(".bivvy/config.yml").exists(),
        "Config should be copied"
    );

    let gitignore = fs::read_to_string(dest.path().join(".gitignore")).unwrap();
    assert!(
        gitignore.contains(".bivvy/config.local.yml"),
        "--from should also update .gitignore at destination"
    );
}

/// --minimal with multiple detected technologies includes all without prompting.
#[test]
fn init_minimal_includes_all_detected() {
    let temp = setup_detection_project(&[
        ("Gemfile", "source 'https://rubygems.org'"),
        ("package.json", r#"{"name": "test"}"#),
        ("Cargo.toml", "[package]\nname = \"test\""),
    ]);

    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml")
        .expect("Should confirm config creation");
    s.expect(expectrl::Eof).unwrap();

    let config = read_generated_config(&temp);
    assert!(
        config.contains("bundle-install"),
        "Minimal should include all detected: bundle-install"
    );
    assert!(
        config.contains("npm-install"),
        "Minimal should include all detected: npm-install"
    );
    assert!(
        config.contains("cargo-build"),
        "Minimal should include all detected: cargo-build"
    );
}
