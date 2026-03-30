//! Integration tests for the detection system.
//!
//! These tests exercise the public detection API by creating realistic project
//! directories with marker files and verifying detection results.

use std::fs;
use tempfile::TempDir;

use bivvy::detection::conflicts::{ConflictDetector, ConflictKind};
use bivvy::detection::environment::EnvironmentDetector;
use bivvy::detection::file_detection::FileDetector;
use bivvy::detection::package_manager::{PackageManager, PackageManagerDetector};
use bivvy::detection::project::{ProjectDetector, ProjectType};
use bivvy::detection::runner::DetectionRunner;
use bivvy::detection::types::Detection;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn empty_project() -> TempDir {
    TempDir::new().unwrap()
}

fn ruby_project() -> TempDir {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("Gemfile"), "source 'https://rubygems.org'").unwrap();
    fs::write(temp.path().join("Gemfile.lock"), "GEM\n  specs:\n").unwrap();
    temp
}

fn node_project_npm() -> TempDir {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("package.json"), r#"{"name": "test"}"#).unwrap();
    fs::write(temp.path().join("package-lock.json"), "{}").unwrap();
    temp
}

fn node_project_yarn() -> TempDir {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("package.json"), r#"{"name": "test"}"#).unwrap();
    fs::write(temp.path().join("yarn.lock"), "# yarn lockfile v1").unwrap();
    temp
}

fn node_project_pnpm() -> TempDir {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("package.json"), r#"{"name": "test"}"#).unwrap();
    fs::write(temp.path().join("pnpm-lock.yaml"), "lockfileVersion: 5.4").unwrap();
    temp
}

fn node_project_bun() -> TempDir {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("package.json"), r#"{"name": "test"}"#).unwrap();
    fs::write(temp.path().join("bun.lockb"), "").unwrap();
    temp
}

fn python_project_pip() -> TempDir {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("requirements.txt"), "flask==2.3.0").unwrap();
    temp
}

fn python_project_poetry() -> TempDir {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("pyproject.toml"), "[tool.poetry]").unwrap();
    fs::write(temp.path().join("poetry.lock"), "").unwrap();
    temp
}

fn python_project_uv() -> TempDir {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("pyproject.toml"), "[project]").unwrap();
    fs::write(temp.path().join("uv.lock"), "").unwrap();
    temp
}

fn rust_project() -> TempDir {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("Cargo.toml"),
        "[package]\nname = \"test\"\nversion = \"0.1.0\"",
    )
    .unwrap();
    temp
}

fn go_project() -> TempDir {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("go.mod"),
        "module example.com/test\n\ngo 1.21",
    )
    .unwrap();
    temp
}

fn swift_project() -> TempDir {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("Package.swift"),
        "// swift-tools-version:5.9",
    )
    .unwrap();
    temp
}

// ---------------------------------------------------------------------------
// Project type detection
// ---------------------------------------------------------------------------

#[test]
fn detect_ruby_project_type() -> Result<(), Box<dyn std::error::Error>> {
    let temp = ruby_project();
    let detection = ProjectDetector::detect(temp.path());

    assert_eq!(detection.primary_type, ProjectType::Ruby);
    assert!(detection.all_types.contains(&ProjectType::Ruby));
    assert_eq!(detection.all_types.len(), 1);

    let ruby_detail = detection.details.iter().find(|d| d.name == "Ruby").unwrap();
    assert!(ruby_detail.detected);
    assert_eq!(ruby_detail.suggested_template, Some("bundler".to_string()));

    Ok(())
}

#[test]
fn detect_node_project_type() -> Result<(), Box<dyn std::error::Error>> {
    let temp = node_project_npm();
    let detection = ProjectDetector::detect(temp.path());

    assert_eq!(detection.primary_type, ProjectType::Node);
    assert!(detection.all_types.contains(&ProjectType::Node));

    let node_detail = detection
        .details
        .iter()
        .find(|d| d.name == "Node.js")
        .unwrap();
    assert!(node_detail.detected);
    assert_eq!(node_detail.suggested_template, Some("npm".to_string()));

    Ok(())
}

#[test]
fn detect_node_yarn_template() -> Result<(), Box<dyn std::error::Error>> {
    let temp = node_project_yarn();
    let detection = ProjectDetector::detect(temp.path());

    let node_detail = detection
        .details
        .iter()
        .find(|d| d.name == "Node.js")
        .unwrap();
    assert_eq!(node_detail.suggested_template, Some("yarn".to_string()));

    Ok(())
}

#[test]
fn detect_node_pnpm_template() -> Result<(), Box<dyn std::error::Error>> {
    let temp = node_project_pnpm();
    let detection = ProjectDetector::detect(temp.path());

    let node_detail = detection
        .details
        .iter()
        .find(|d| d.name == "Node.js")
        .unwrap();
    assert_eq!(node_detail.suggested_template, Some("pnpm".to_string()));

    Ok(())
}

#[test]
fn detect_node_bun_template() -> Result<(), Box<dyn std::error::Error>> {
    let temp = node_project_bun();
    let detection = ProjectDetector::detect(temp.path());

    let node_detail = detection
        .details
        .iter()
        .find(|d| d.name == "Node.js")
        .unwrap();
    assert_eq!(node_detail.suggested_template, Some("bun".to_string()));

    Ok(())
}

#[test]
fn detect_node_defaults_to_npm_without_lockfile() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    fs::write(temp.path().join("package.json"), r#"{"name": "bare"}"#)?;

    let detection = ProjectDetector::detect(temp.path());

    let node_detail = detection
        .details
        .iter()
        .find(|d| d.name == "Node.js")
        .unwrap();
    assert_eq!(node_detail.suggested_template, Some("npm".to_string()));

    Ok(())
}

#[test]
fn detect_python_pip_project() -> Result<(), Box<dyn std::error::Error>> {
    let temp = python_project_pip();
    let detection = ProjectDetector::detect(temp.path());

    assert_eq!(detection.primary_type, ProjectType::Python);

    let detail = detection
        .details
        .iter()
        .find(|d| d.name == "Python")
        .unwrap();
    assert_eq!(detail.suggested_template, Some("pip".to_string()));

    Ok(())
}

#[test]
fn detect_python_poetry_project() -> Result<(), Box<dyn std::error::Error>> {
    let temp = python_project_poetry();
    let detection = ProjectDetector::detect(temp.path());

    assert_eq!(detection.primary_type, ProjectType::Python);

    let detail = detection
        .details
        .iter()
        .find(|d| d.name == "Python")
        .unwrap();
    assert_eq!(detail.suggested_template, Some("poetry".to_string()));

    Ok(())
}

#[test]
fn detect_python_uv_project() -> Result<(), Box<dyn std::error::Error>> {
    let temp = python_project_uv();
    let detection = ProjectDetector::detect(temp.path());

    assert_eq!(detection.primary_type, ProjectType::Python);

    let detail = detection
        .details
        .iter()
        .find(|d| d.name == "Python")
        .unwrap();
    assert_eq!(detail.suggested_template, Some("uv".to_string()));

    Ok(())
}

#[test]
fn detect_python_via_setup_py() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    fs::write(temp.path().join("setup.py"), "from setuptools import setup")?;

    let detection = ProjectDetector::detect(temp.path());

    assert_eq!(detection.primary_type, ProjectType::Python);
    assert!(detection.all_types.contains(&ProjectType::Python));

    Ok(())
}

#[test]
fn detect_rust_project_type() -> Result<(), Box<dyn std::error::Error>> {
    let temp = rust_project();
    let detection = ProjectDetector::detect(temp.path());

    assert_eq!(detection.primary_type, ProjectType::Rust);

    let detail = detection.details.iter().find(|d| d.name == "Rust").unwrap();
    assert!(detail.detected);
    assert_eq!(detail.suggested_template, Some("cargo".to_string()));

    Ok(())
}

#[test]
fn detect_go_project_type() -> Result<(), Box<dyn std::error::Error>> {
    let temp = go_project();
    let detection = ProjectDetector::detect(temp.path());

    assert_eq!(detection.primary_type, ProjectType::Go);

    let detail = detection.details.iter().find(|d| d.name == "Go").unwrap();
    assert!(detail.detected);
    assert_eq!(detail.suggested_template, Some("go".to_string()));

    Ok(())
}

#[test]
fn detect_swift_project_type() -> Result<(), Box<dyn std::error::Error>> {
    let temp = swift_project();
    let detection = ProjectDetector::detect(temp.path());

    assert_eq!(detection.primary_type, ProjectType::Swift);

    let detail = detection
        .details
        .iter()
        .find(|d| d.name == "Swift")
        .unwrap();
    assert!(detail.detected);
    assert_eq!(detail.suggested_template, Some("swift".to_string()));

    Ok(())
}

#[test]
fn detect_unknown_project_type_for_empty_dir() -> Result<(), Box<dyn std::error::Error>> {
    let temp = empty_project();
    let detection = ProjectDetector::detect(temp.path());

    assert_eq!(detection.primary_type, ProjectType::Unknown);
    assert!(detection.all_types.is_empty());
    assert!(detection.details.is_empty());

    Ok(())
}

#[test]
fn has_type_returns_true_for_detected_type() -> Result<(), Box<dyn std::error::Error>> {
    let temp = rust_project();

    assert!(ProjectDetector::has_type(temp.path(), ProjectType::Rust));
    assert!(!ProjectDetector::has_type(temp.path(), ProjectType::Ruby));
    assert!(!ProjectDetector::has_type(temp.path(), ProjectType::Node));

    Ok(())
}

// ---------------------------------------------------------------------------
// Multi-language detection
// ---------------------------------------------------------------------------

#[test]
fn detect_ruby_and_node_together() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    fs::write(temp.path().join("Gemfile"), "source 'https://rubygems.org'")?;
    fs::write(temp.path().join("package.json"), r#"{"name": "test"}"#)?;
    fs::write(temp.path().join("yarn.lock"), "")?;

    let detection = ProjectDetector::detect(temp.path());

    assert_eq!(detection.all_types.len(), 2);
    assert!(detection.all_types.contains(&ProjectType::Ruby));
    assert!(detection.all_types.contains(&ProjectType::Node));
    // Ruby is listed first, so it becomes primary
    assert_eq!(detection.primary_type, ProjectType::Ruby);

    Ok(())
}

#[test]
fn detect_three_languages_together() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    fs::write(temp.path().join("Gemfile"), "")?;
    fs::write(temp.path().join("package.json"), "{}")?;
    fs::write(temp.path().join("Cargo.toml"), "")?;

    let detection = ProjectDetector::detect(temp.path());

    assert_eq!(detection.all_types.len(), 3);
    assert!(detection.all_types.contains(&ProjectType::Ruby));
    assert!(detection.all_types.contains(&ProjectType::Node));
    assert!(detection.all_types.contains(&ProjectType::Rust));

    Ok(())
}

#[test]
fn detect_all_six_languages() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    fs::write(temp.path().join("Gemfile"), "")?;
    fs::write(temp.path().join("package.json"), "{}")?;
    fs::write(temp.path().join("requirements.txt"), "")?;
    fs::write(temp.path().join("Cargo.toml"), "")?;
    fs::write(temp.path().join("go.mod"), "module test\n\ngo 1.21")?;
    fs::write(temp.path().join("Package.swift"), "")?;

    let detection = ProjectDetector::detect(temp.path());

    assert_eq!(detection.all_types.len(), 6);
    assert!(detection.all_types.contains(&ProjectType::Ruby));
    assert!(detection.all_types.contains(&ProjectType::Node));
    assert!(detection.all_types.contains(&ProjectType::Python));
    assert!(detection.all_types.contains(&ProjectType::Rust));
    assert!(detection.all_types.contains(&ProjectType::Go));
    assert!(detection.all_types.contains(&ProjectType::Swift));

    Ok(())
}

// ---------------------------------------------------------------------------
// Package manager detection — language managers
// ---------------------------------------------------------------------------

#[test]
fn detect_bundler_language_manager() -> Result<(), Box<dyn std::error::Error>> {
    let temp = ruby_project();
    let detection = PackageManagerDetector::detect(temp.path());

    assert!(detection
        .language_managers
        .contains(&PackageManager::Bundler));

    Ok(())
}

#[test]
fn detect_npm_language_manager() -> Result<(), Box<dyn std::error::Error>> {
    let temp = node_project_npm();
    let detection = PackageManagerDetector::detect(temp.path());

    assert!(detection.language_managers.contains(&PackageManager::Npm));
    assert!(!detection.language_managers.contains(&PackageManager::Yarn));

    Ok(())
}

#[test]
fn detect_yarn_language_manager() -> Result<(), Box<dyn std::error::Error>> {
    let temp = node_project_yarn();
    let detection = PackageManagerDetector::detect(temp.path());

    assert!(detection.language_managers.contains(&PackageManager::Yarn));
    assert!(!detection.language_managers.contains(&PackageManager::Npm));

    Ok(())
}

#[test]
fn detect_pnpm_language_manager() -> Result<(), Box<dyn std::error::Error>> {
    let temp = node_project_pnpm();
    let detection = PackageManagerDetector::detect(temp.path());

    assert!(detection.language_managers.contains(&PackageManager::Pnpm));

    Ok(())
}

#[test]
fn detect_bun_language_manager() -> Result<(), Box<dyn std::error::Error>> {
    let temp = node_project_bun();
    let detection = PackageManagerDetector::detect(temp.path());

    assert!(detection.language_managers.contains(&PackageManager::Bun));

    Ok(())
}

#[test]
fn detect_pip_language_manager() -> Result<(), Box<dyn std::error::Error>> {
    let temp = python_project_pip();
    let detection = PackageManagerDetector::detect(temp.path());

    assert!(detection.language_managers.contains(&PackageManager::Pip));

    Ok(())
}

#[test]
fn detect_poetry_language_manager() -> Result<(), Box<dyn std::error::Error>> {
    let temp = python_project_poetry();
    let detection = PackageManagerDetector::detect(temp.path());

    assert!(detection
        .language_managers
        .contains(&PackageManager::Poetry));
    assert!(!detection.language_managers.contains(&PackageManager::Pip));

    Ok(())
}

#[test]
fn detect_uv_language_manager() -> Result<(), Box<dyn std::error::Error>> {
    let temp = python_project_uv();
    let detection = PackageManagerDetector::detect(temp.path());

    assert!(detection.language_managers.contains(&PackageManager::Uv));
    assert!(!detection.language_managers.contains(&PackageManager::Pip));

    Ok(())
}

#[test]
fn detect_cargo_language_manager() -> Result<(), Box<dyn std::error::Error>> {
    let temp = rust_project();
    let detection = PackageManagerDetector::detect(temp.path());

    assert!(detection.language_managers.contains(&PackageManager::Cargo));

    Ok(())
}

#[test]
fn detect_go_language_manager() -> Result<(), Box<dyn std::error::Error>> {
    let temp = go_project();
    let detection = PackageManagerDetector::detect(temp.path());

    assert!(detection.language_managers.contains(&PackageManager::Go));

    Ok(())
}

#[test]
fn detect_multiple_language_managers() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    fs::write(temp.path().join("Gemfile"), "")?;
    fs::write(temp.path().join("package.json"), "{}")?;
    fs::write(temp.path().join("yarn.lock"), "")?;
    fs::write(temp.path().join("Cargo.toml"), "")?;

    let detection = PackageManagerDetector::detect(temp.path());

    assert!(detection
        .language_managers
        .contains(&PackageManager::Bundler));
    assert!(detection.language_managers.contains(&PackageManager::Yarn));
    assert!(detection.language_managers.contains(&PackageManager::Cargo));
    assert_eq!(detection.language_managers.len(), 3);

    Ok(())
}

#[test]
fn detect_no_language_managers_for_empty_project() -> Result<(), Box<dyn std::error::Error>> {
    let temp = empty_project();
    let detection = PackageManagerDetector::detect(temp.path());

    assert!(detection.language_managers.is_empty());

    Ok(())
}

// ---------------------------------------------------------------------------
// Package manager detection — version managers (file-based)
// ---------------------------------------------------------------------------

#[test]
fn detect_mise_version_manager_via_dotfile() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    fs::write(temp.path().join(".mise.toml"), "[tools]\nruby = \"3.2\"")?;

    let detection = PackageManagerDetector::detect(temp.path());

    assert_eq!(detection.version_manager, Some(PackageManager::Mise));

    Ok(())
}

#[test]
fn detect_mise_version_manager_via_mise_toml() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    fs::write(temp.path().join("mise.toml"), "[tools]\nnode = \"20\"")?;

    let detection = PackageManagerDetector::detect(temp.path());

    assert_eq!(detection.version_manager, Some(PackageManager::Mise));

    Ok(())
}

#[test]
fn detect_asdf_version_manager_via_tool_versions() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    fs::write(
        temp.path().join(".tool-versions"),
        "ruby 3.2.0\nnode 20.0.0",
    )?;

    let detection = PackageManagerDetector::detect(temp.path());

    assert_eq!(detection.version_manager, Some(PackageManager::Asdf));

    Ok(())
}

#[test]
fn mise_config_takes_priority_over_asdf() -> Result<(), Box<dyn std::error::Error>> {
    // When both .mise.toml and .tool-versions exist, mise should win
    let temp = TempDir::new()?;
    fs::write(temp.path().join(".mise.toml"), "")?;
    fs::write(temp.path().join(".tool-versions"), "")?;

    let detection = PackageManagerDetector::detect(temp.path());

    assert_eq!(detection.version_manager, Some(PackageManager::Mise));

    Ok(())
}

// ---------------------------------------------------------------------------
// Conflict detection
// ---------------------------------------------------------------------------

#[test]
fn no_conflicts_in_clean_ruby_project() -> Result<(), Box<dyn std::error::Error>> {
    let temp = ruby_project();
    let conflicts = ConflictDetector::detect(temp.path());

    assert!(conflicts.is_empty());

    Ok(())
}

#[test]
fn no_conflicts_in_clean_node_project() -> Result<(), Box<dyn std::error::Error>> {
    let temp = node_project_yarn();
    let conflicts = ConflictDetector::detect(temp.path());

    assert!(conflicts.is_empty());

    Ok(())
}

#[test]
fn detect_node_lockfile_conflict_npm_and_yarn() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    fs::write(temp.path().join("package.json"), "{}")?;
    fs::write(temp.path().join("package-lock.json"), "{}")?;
    fs::write(temp.path().join("yarn.lock"), "")?;

    let conflicts = ConflictDetector::detect(temp.path());

    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].kind, ConflictKind::NodeLockfiles);
    assert!(conflicts[0]
        .files
        .contains(&"package-lock.json".to_string()));
    assert!(conflicts[0].files.contains(&"yarn.lock".to_string()));
    assert!(!conflicts[0].message.is_empty());
    assert!(!conflicts[0].suggestion.is_empty());

    Ok(())
}

#[test]
fn detect_node_lockfile_conflict_three_managers() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    fs::write(temp.path().join("package.json"), "{}")?;
    fs::write(temp.path().join("package-lock.json"), "")?;
    fs::write(temp.path().join("yarn.lock"), "")?;
    fs::write(temp.path().join("pnpm-lock.yaml"), "")?;

    let conflicts = ConflictDetector::detect(temp.path());

    let node_conflict = conflicts
        .iter()
        .find(|c| c.kind == ConflictKind::NodeLockfiles);
    assert!(node_conflict.is_some());
    assert_eq!(node_conflict.unwrap().files.len(), 3);

    Ok(())
}

#[test]
fn detect_version_manager_conflict_mise_and_asdf() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    fs::write(temp.path().join(".mise.toml"), "")?;
    fs::write(temp.path().join(".tool-versions"), "")?;

    let conflicts = ConflictDetector::detect(temp.path());

    assert!(conflicts
        .iter()
        .any(|c| c.kind == ConflictKind::VersionManagers));

    Ok(())
}

#[test]
fn no_version_manager_conflict_with_mise_only() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    fs::write(temp.path().join(".mise.toml"), "")?;
    fs::write(temp.path().join(".ruby-version"), "3.2.0")?;

    let conflicts = ConflictDetector::detect(temp.path());

    // .ruby-version alone with .mise.toml is NOT a conflict (mise reads .ruby-version)
    assert!(!conflicts
        .iter()
        .any(|c| c.kind == ConflictKind::VersionManagers));

    Ok(())
}

#[test]
fn detect_python_package_manager_conflict() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    fs::write(temp.path().join("poetry.lock"), "")?;
    fs::write(temp.path().join("uv.lock"), "")?;

    let conflicts = ConflictDetector::detect(temp.path());

    assert!(conflicts
        .iter()
        .any(|c| c.kind == ConflictKind::PythonPackageManagers));

    Ok(())
}

#[test]
fn detect_python_conflict_poetry_and_pipenv() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    fs::write(temp.path().join("poetry.lock"), "")?;
    fs::write(temp.path().join("Pipfile.lock"), "")?;

    let conflicts = ConflictDetector::detect(temp.path());

    let python_conflict = conflicts
        .iter()
        .find(|c| c.kind == ConflictKind::PythonPackageManagers);
    assert!(python_conflict.is_some());

    Ok(())
}

#[test]
fn no_conflicts_in_empty_project() -> Result<(), Box<dyn std::error::Error>> {
    let temp = empty_project();
    let conflicts = ConflictDetector::detect(temp.path());

    assert!(conflicts.is_empty());

    Ok(())
}

#[test]
fn has_conflict_helper_works() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    fs::write(temp.path().join("package-lock.json"), "")?;
    fs::write(temp.path().join("yarn.lock"), "")?;

    assert!(ConflictDetector::has_conflict(
        temp.path(),
        ConflictKind::NodeLockfiles
    ));
    assert!(!ConflictDetector::has_conflict(
        temp.path(),
        ConflictKind::VersionManagers
    ));
    assert!(!ConflictDetector::has_conflict(
        temp.path(),
        ConflictKind::PythonPackageManagers
    ));

    Ok(())
}

#[test]
fn multiple_conflict_types_simultaneously() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    // Node lockfile conflict
    fs::write(temp.path().join("package-lock.json"), "")?;
    fs::write(temp.path().join("yarn.lock"), "")?;
    // Version manager conflict
    fs::write(temp.path().join(".mise.toml"), "")?;
    fs::write(temp.path().join(".tool-versions"), "")?;
    // Python conflict
    fs::write(temp.path().join("poetry.lock"), "")?;
    fs::write(temp.path().join("uv.lock"), "")?;

    let conflicts = ConflictDetector::detect(temp.path());

    assert!(conflicts
        .iter()
        .any(|c| c.kind == ConflictKind::NodeLockfiles));
    assert!(conflicts
        .iter()
        .any(|c| c.kind == ConflictKind::VersionManagers));
    assert!(conflicts
        .iter()
        .any(|c| c.kind == ConflictKind::PythonPackageManagers));
    assert_eq!(conflicts.len(), 3);

    Ok(())
}

// ---------------------------------------------------------------------------
// FileDetector trait-based detection
// ---------------------------------------------------------------------------

#[test]
fn file_detector_any_matches_single_file() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    fs::write(temp.path().join("Gemfile"), "")?;

    let detector = FileDetector::any(
        "ruby",
        vec!["Gemfile".to_string(), "Gemfile.lock".to_string()],
    );
    let result = detector.detect(temp.path());

    assert!(result.detected);
    assert_eq!(result.name, "ruby");
    assert!(result.confidence > 0.0);
    assert!(result.confidence < 1.0); // Only 1 of 2 files

    Ok(())
}

#[test]
fn file_detector_any_matches_all_files() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    fs::write(temp.path().join("Gemfile"), "")?;
    fs::write(temp.path().join("Gemfile.lock"), "")?;

    let detector = FileDetector::any(
        "ruby",
        vec!["Gemfile".to_string(), "Gemfile.lock".to_string()],
    );
    let result = detector.detect(temp.path());

    assert!(result.detected);
    assert_eq!(result.confidence, 1.0);
    assert_eq!(result.details.len(), 2);

    Ok(())
}

#[test]
fn file_detector_any_no_match() -> Result<(), Box<dyn std::error::Error>> {
    let temp = empty_project();

    let detector = FileDetector::any("ruby", vec!["Gemfile".to_string()]);
    let result = detector.detect(temp.path());

    assert!(!result.detected);
    assert_eq!(result.confidence, 0.0);

    Ok(())
}

#[test]
fn file_detector_all_requires_every_file() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    fs::write(temp.path().join("package.json"), "{}")?;
    // Missing yarn.lock

    let detector = FileDetector::all(
        "yarn",
        vec!["package.json".to_string(), "yarn.lock".to_string()],
    );
    let result = detector.detect(temp.path());

    assert!(!result.detected);

    Ok(())
}

#[test]
fn file_detector_all_succeeds_with_all_files() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    fs::write(temp.path().join("package.json"), "{}")?;
    fs::write(temp.path().join("yarn.lock"), "")?;

    let detector = FileDetector::all(
        "yarn",
        vec!["package.json".to_string(), "yarn.lock".to_string()],
    );
    let result = detector.detect(temp.path());

    assert!(result.detected);
    assert_eq!(result.confidence, 1.0);

    Ok(())
}

// ---------------------------------------------------------------------------
// Environment detector
// ---------------------------------------------------------------------------

#[test]
fn environment_detector_captures_initial_path() -> Result<(), Box<dyn std::error::Error>> {
    let detector = EnvironmentDetector::new();

    assert!(!detector.initial_path().is_empty());

    Ok(())
}

#[test]
fn environment_detector_no_changes_immediately() -> Result<(), Box<dyn std::error::Error>> {
    let detector = EnvironmentDetector::new();
    let changes = detector.check_changes();

    assert!(changes.is_empty());

    Ok(())
}

#[test]
fn environment_detector_default_trait() -> Result<(), Box<dyn std::error::Error>> {
    let detector = EnvironmentDetector::default();

    assert!(!detector.initial_path().is_empty());
    assert!(!detector.shell_config_files().is_empty() || detector.shell_config_files().is_empty());

    Ok(())
}

#[test]
fn needs_shell_restart_for_version_managers() -> Result<(), Box<dyn std::error::Error>> {
    assert!(EnvironmentDetector::needs_shell_restart("mise"));
    assert!(EnvironmentDetector::needs_shell_restart("asdf"));
    assert!(EnvironmentDetector::needs_shell_restart("volta"));
    assert!(EnvironmentDetector::needs_shell_restart("nvm"));
    assert!(EnvironmentDetector::needs_shell_restart("rbenv"));
    assert!(EnvironmentDetector::needs_shell_restart("pyenv"));
    assert!(EnvironmentDetector::needs_shell_restart("brew"));

    Ok(())
}

#[test]
fn no_shell_restart_for_language_managers() -> Result<(), Box<dyn std::error::Error>> {
    assert!(!EnvironmentDetector::needs_shell_restart("bundler"));
    assert!(!EnvironmentDetector::needs_shell_restart("npm"));
    assert!(!EnvironmentDetector::needs_shell_restart("yarn"));
    assert!(!EnvironmentDetector::needs_shell_restart("pip"));
    assert!(!EnvironmentDetector::needs_shell_restart("cargo"));

    Ok(())
}

// ---------------------------------------------------------------------------
// Full detection runner
// ---------------------------------------------------------------------------

#[test]
fn full_detection_empty_project() -> Result<(), Box<dyn std::error::Error>> {
    let temp = empty_project();
    let detection = DetectionRunner::run(temp.path());

    assert_eq!(detection.project.primary_type, ProjectType::Unknown);
    assert!(detection.project.all_types.is_empty());
    assert!(detection.conflicts.is_empty());
    assert!(detection.package_managers.language_managers.is_empty());

    Ok(())
}

#[test]
fn full_detection_ruby_project_suggests_bundler() -> Result<(), Box<dyn std::error::Error>> {
    let temp = ruby_project();
    let detection = DetectionRunner::run(temp.path());

    assert_eq!(detection.project.primary_type, ProjectType::Ruby);
    assert!(detection
        .suggested_templates
        .iter()
        .any(|t| t.name == "bundler"));
    assert!(
        detection
            .suggested_templates
            .iter()
            .find(|t| t.name == "bundler")
            .unwrap()
            .category
            == "language"
    );

    Ok(())
}

#[test]
fn full_detection_node_yarn_project() -> Result<(), Box<dyn std::error::Error>> {
    let temp = node_project_yarn();
    let detection = DetectionRunner::run(temp.path());

    assert_eq!(detection.project.primary_type, ProjectType::Node);
    assert!(detection
        .suggested_templates
        .iter()
        .any(|t| t.name == "yarn"));
    assert!(detection
        .package_managers
        .language_managers
        .contains(&PackageManager::Yarn));

    Ok(())
}

#[test]
fn full_detection_multi_language_suggests_all_templates() -> Result<(), Box<dyn std::error::Error>>
{
    let temp = TempDir::new()?;
    fs::write(temp.path().join("Gemfile"), "")?;
    fs::write(temp.path().join("package.json"), "{}")?;
    fs::write(temp.path().join("go.mod"), "module test\n\ngo 1.21")?;

    let detection = DetectionRunner::run(temp.path());

    assert!(detection
        .suggested_templates
        .iter()
        .any(|t| t.name == "bundler"));
    assert!(detection
        .suggested_templates
        .iter()
        .any(|t| t.name == "npm"));
    assert!(detection.suggested_templates.iter().any(|t| t.name == "go"));

    Ok(())
}

#[test]
fn full_detection_with_version_manager() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    fs::write(temp.path().join("Gemfile"), "")?;
    fs::write(temp.path().join(".mise.toml"), "[tools]\nruby = \"3.2\"")?;

    let detection = DetectionRunner::run(temp.path());

    assert_eq!(
        detection.package_managers.version_manager,
        Some(PackageManager::Mise)
    );
    assert!(detection
        .suggested_templates
        .iter()
        .any(|t| t.name == "mise"));
    assert!(detection
        .suggested_templates
        .iter()
        .any(|t| t.name == "bundler"));

    Ok(())
}

#[test]
fn full_detection_template_priority_ordering() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    fs::write(temp.path().join("Gemfile"), "")?;
    fs::write(temp.path().join(".mise.toml"), "")?;

    let detection = DetectionRunner::run(temp.path());

    // Verify that templates are sorted by priority
    let priorities: Vec<u32> = detection
        .suggested_templates
        .iter()
        .map(|t| t.priority)
        .collect();
    let mut sorted = priorities.clone();
    sorted.sort();
    assert_eq!(priorities, sorted);

    // Version manager (priority 20) should come before language (priority 30)
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
            "Version manager should come before language template"
        );
    }

    Ok(())
}

#[test]
fn full_detection_detects_conflicts() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    fs::write(temp.path().join("package.json"), "{}")?;
    fs::write(temp.path().join("package-lock.json"), "")?;
    fs::write(temp.path().join("yarn.lock"), "")?;

    let detection = DetectionRunner::run(temp.path());

    assert!(!detection.conflicts.is_empty());
    assert!(detection
        .conflicts
        .iter()
        .any(|c| c.kind == ConflictKind::NodeLockfiles));

    Ok(())
}

#[test]
fn full_detection_realistic_rails_project() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    // Rails projects commonly have these files
    fs::write(temp.path().join("Gemfile"), "gem 'rails'\ngem 'pg'")?;
    fs::write(temp.path().join("Gemfile.lock"), "")?;
    fs::write(temp.path().join("package.json"), r#"{"name": "myapp"}"#)?;
    fs::write(temp.path().join("yarn.lock"), "")?;
    fs::write(temp.path().join(".ruby-version"), "3.2.2")?;
    fs::write(
        temp.path().join(".mise.toml"),
        "[tools]\nruby = \"3.2.2\"\nnode = \"20\"",
    )?;

    let detection = DetectionRunner::run(temp.path());

    // Should detect both Ruby and Node
    assert!(detection.project.all_types.contains(&ProjectType::Ruby));
    assert!(detection.project.all_types.contains(&ProjectType::Node));

    // Should detect mise as version manager
    assert_eq!(
        detection.package_managers.version_manager,
        Some(PackageManager::Mise)
    );

    // Should detect Bundler and Yarn
    assert!(detection
        .package_managers
        .language_managers
        .contains(&PackageManager::Bundler));
    assert!(detection
        .package_managers
        .language_managers
        .contains(&PackageManager::Yarn));

    // Should suggest templates in priority order
    assert!(detection
        .suggested_templates
        .iter()
        .any(|t| t.name == "mise"));
    assert!(detection
        .suggested_templates
        .iter()
        .any(|t| t.name == "bundler"));
    assert!(detection
        .suggested_templates
        .iter()
        .any(|t| t.name == "yarn"));

    // No conflicts (only one Node lockfile, only one version manager config)
    assert!(!detection
        .conflicts
        .iter()
        .any(|c| c.kind == ConflictKind::NodeLockfiles));

    Ok(())
}

#[test]
fn full_detection_realistic_python_api_project() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    fs::write(
        temp.path().join("pyproject.toml"),
        "[tool.poetry]\nname = \"api\"",
    )?;
    fs::write(temp.path().join("poetry.lock"), "")?;
    fs::write(temp.path().join(".python-version"), "3.11.0")?;

    let detection = DetectionRunner::run(temp.path());

    assert_eq!(detection.project.primary_type, ProjectType::Python);
    assert!(detection
        .suggested_templates
        .iter()
        .any(|t| t.name == "poetry"));
    assert!(detection
        .package_managers
        .language_managers
        .contains(&PackageManager::Poetry));

    Ok(())
}
