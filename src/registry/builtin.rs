//! Built-in templates embedded at compile time.

use crate::error::Result;
use crate::registry::detector::DetectorFile;
use crate::registry::manifest::RegistryManifest;
use crate::registry::template::{Template, TemplateSource};
use include_dir::{include_dir, Dir};
use std::collections::HashMap;

/// Embedded templates directory.
static TEMPLATES_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/templates");

/// Load the built-in registry manifest.
pub fn load_manifest() -> Result<RegistryManifest> {
    let manifest_file = TEMPLATES_DIR.get_file("registry.yml").ok_or_else(|| {
        crate::error::BivvyError::ConfigNotFound {
            path: "templates/registry.yml".into(),
        }
    })?;

    let content = manifest_file.contents_utf8().ok_or_else(|| {
        crate::error::BivvyError::ConfigParseError {
            path: "templates/registry.yml".into(),
            message: "Invalid UTF-8".to_string(),
        }
    })?;

    serde_yaml::from_str(content).map_err(|e| crate::error::BivvyError::ConfigParseError {
        path: "templates/registry.yml".into(),
        message: e.to_string(),
    })
}

/// Load the built-in detector definitions.
pub fn load_detectors() -> Result<DetectorFile> {
    let detector_file = TEMPLATES_DIR.get_file("detectors.yml").ok_or_else(|| {
        crate::error::BivvyError::ConfigNotFound {
            path: "templates/detectors.yml".into(),
        }
    })?;

    let content = detector_file.contents_utf8().ok_or_else(|| {
        crate::error::BivvyError::ConfigParseError {
            path: "templates/detectors.yml".into(),
            message: "Invalid UTF-8".to_string(),
        }
    })?;

    serde_yaml::from_str(content).map_err(|e| crate::error::BivvyError::ConfigParseError {
        path: "templates/detectors.yml".into(),
        message: e.to_string(),
    })
}

/// Build a qualified key for a template (`category/name`).
fn qualified_key(template: &Template) -> String {
    format!("{}/{}", template.category, template.name)
}

/// Load all built-in templates, keyed by `category/name`.
pub fn load_templates() -> Result<HashMap<String, Template>> {
    let mut templates = HashMap::new();
    load_templates_from_dir(&TEMPLATES_DIR, "steps", &mut templates)?;
    Ok(templates)
}

fn load_templates_from_dir(
    dir: &Dir<'_>,
    prefix: &str,
    templates: &mut HashMap<String, Template>,
) -> Result<()> {
    let steps_dir = dir.get_dir(prefix);

    if let Some(steps) = steps_dir {
        for entry in steps.dirs() {
            // Each subdirectory is a category (ruby, node, common, etc.)
            for file in entry.files() {
                if let Some(ext) = file.path().extension() {
                    if ext == "yml" || ext == "yaml" {
                        if let Some(content) = file.contents_utf8() {
                            let template: Template =
                                serde_yaml::from_str(content).map_err(|e| {
                                    crate::error::BivvyError::ConfigParseError {
                                        path: file.path().to_path_buf(),
                                        message: e.to_string(),
                                    }
                                })?;
                            let key = qualified_key(&template);
                            templates.insert(key, template);
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Check if a template exists in built-ins.
pub fn has_template(name: &str) -> bool {
    load_templates()
        .map(|t| {
            // Check qualified key first, then unqualified name
            t.contains_key(name) || t.values().any(|tmpl| tmpl.name == name)
        })
        .unwrap_or(false)
}

/// Loader for built-in templates.
#[derive(Debug, Clone)]
pub struct BuiltinLoader {
    templates: HashMap<String, Template>,
    manifest: RegistryManifest,
}

impl BuiltinLoader {
    /// Initialize the built-in loader.
    pub fn new() -> Result<Self> {
        Ok(Self {
            templates: load_templates()?,
            manifest: load_manifest()?,
        })
    }

    /// Get a template by name (unqualified — returns first match).
    pub fn get(&self, name: &str) -> Option<&Template> {
        // Try as qualified key first
        if let Some(t) = self.templates.get(name) {
            return Some(t);
        }
        // Fall back to unqualified name match
        self.templates.values().find(|t| t.name == name)
    }

    /// Get a template by name, filtering by category.
    pub fn get_by_category(&self, name: &str, category: &str) -> Option<&Template> {
        let key = format!("{}/{}", category, name);
        self.templates.get(&key)
    }

    /// Get all template names (qualified as `category/name`).
    pub fn template_names(&self) -> Vec<&str> {
        self.templates.keys().map(|s| s.as_str()).collect()
    }

    /// Get templates filtered by current platform.
    pub fn templates_for_current_platform(&self) -> Vec<&Template> {
        self.templates
            .values()
            .filter(|t| t.platforms.iter().any(|p| p.is_current()))
            .collect()
    }

    /// Get templates in detection order for current platform.
    pub fn detection_order(&self) -> Vec<&Template> {
        self.manifest
            .detection_order
            .iter()
            .filter_map(|name| self.get(name))
            .filter(|t| t.platforms.iter().any(|p| p.is_current()))
            .collect()
    }

    /// Get the manifest.
    pub fn manifest(&self) -> &RegistryManifest {
        &self.manifest
    }

    /// Get source type for built-in templates.
    pub fn source() -> TemplateSource {
        TemplateSource::Builtin
    }
}

impl Default for BuiltinLoader {
    fn default() -> Self {
        Self::new().expect("Built-in templates should always load")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_manifest_works() {
        let manifest = load_manifest().unwrap();
        assert!(manifest.version >= 1);
        assert!(!manifest.categories.is_empty());
    }

    #[test]
    fn load_templates_includes_brew() {
        let templates = load_templates().unwrap();
        assert!(templates.contains_key("system/brew-bundle"));
    }

    #[test]
    fn load_templates_includes_yarn() {
        let templates = load_templates().unwrap();
        assert!(templates.contains_key("node/yarn-install"));
    }

    #[test]
    fn has_template_returns_true_for_builtin() {
        assert!(has_template("brew-bundle"));
        assert!(has_template("yarn-install"));
    }

    #[test]
    fn has_template_returns_false_for_unknown() {
        assert!(!has_template("nonexistent"));
    }

    #[test]
    fn brew_template_has_correct_fields() {
        let templates = load_templates().unwrap();
        let brew = &templates["system/brew-bundle"];
        assert_eq!(brew.category, "system");
        assert!(brew.step.command.is_some());
        assert!(!brew.detectors.is_empty());
    }

    #[test]
    fn builtin_loader_new() {
        let loader = BuiltinLoader::new().unwrap();
        assert!(!loader.template_names().is_empty());
    }

    #[test]
    fn builtin_loader_get() {
        let loader = BuiltinLoader::new().unwrap();
        assert!(loader.get("brew-bundle").is_some());
        assert!(loader.get("nonexistent").is_none());
    }

    #[test]
    fn builtin_loader_detection_order() {
        let loader = BuiltinLoader::new().unwrap();
        let order = loader.detection_order();
        // Should return templates, possibly filtered by platform
        // At minimum, some templates should be available
        assert!(!order.is_empty() || loader.template_names().is_empty());
    }

    #[test]
    fn builtin_loader_source() {
        assert_eq!(BuiltinLoader::source(), TemplateSource::Builtin);
    }

    #[test]
    fn builtin_loader_manifest() {
        let loader = BuiltinLoader::new().unwrap();
        let manifest = loader.manifest();
        assert!(manifest.version >= 1);
    }

    #[test]
    fn builtin_loader_templates_for_current_platform() {
        let loader = BuiltinLoader::new().unwrap();
        let templates = loader.templates_for_current_platform();
        // At least some templates should support current platform
        assert!(!templates.is_empty());
    }

    #[test]
    fn all_expected_templates_load() {
        let templates = load_templates().unwrap();
        // Keys are qualified as "category/name" based on each template's
        // `category` field (not file path).
        let expected = [
            // System
            "system/brew-bundle",
            "system/apt-install",
            "system/yum-install",
            "system/pacman-install",
            // Windows
            "windows/choco-install",
            "windows/scoop-install",
            // Version managers
            "version_manager/mise-tools",
            "version_manager/asdf-tools",
            "version_manager/volta-setup",
            "version_manager/fnm-setup",
            // Ruby
            "ruby/bundle-install",
            "ruby/rails-db",
            "ruby/version-bump",
            // Node
            "node/yarn-install",
            "node/npm-install",
            "node/pnpm-install",
            "node/bun-install",
            "node/prisma-migrate",
            "node/nextjs-build",
            "node/vite-build",
            "node/remix-build",
            "node/version-bump",
            // Python
            "python/pip-install",
            "python/poetry-install",
            "python/uv-sync",
            "python/alembic-migrate",
            "python/django-migrate",
            "python/version-bump",
            // PHP
            "php/composer-install",
            "php/laravel-setup",
            "php/version-bump",
            // Gradle/Kotlin
            "gradle/gradle-deps",
            "gradle/spring-boot-build",
            "kotlin/version-bump",
            // Elixir
            "elixir/mix-deps-get",
            "elixir/version-bump",
            // Rust
            "rust/cargo-build",
            "rust/version-bump",
            "rust/diesel-migrate",
            // Go
            "go/go-mod-download",
            "go/version-bump",
            // Swift
            "swift/swift-resolve",
            "swift/version-bump",
            // IaC
            "iac/terraform-init",
            "iac/cdk-synth",
            "iac/pulumi-install",
            "iac/ansible-install",
            // Java
            "java/maven-resolve",
            "java/version-bump",
            // .NET
            "dotnet/dotnet-restore",
            "dotnet/version-bump",
            // Dart
            "dart/dart-pub-get",
            "dart/flutter-pub-get",
            // Deno
            "deno/deno-install",
            // Containers
            "containers/docker-compose-up",
            "containers/helm-deps",
            // Install
            "install/mise-install",
            "install/mise-ruby",
            "install/mise-node",
            "install/mise-python",
            "install/mise-php",
            "install/mise-elixir",
            "install/asdf-ruby",
            "install/asdf-node",
            "install/asdf-python",
            "install/asdf-php",
            "install/asdf-elixir",
            "install/nvm-node",
            "install/fnm-node",
            "install/volta-node",
            "install/rbenv-ruby",
            "install/pyenv-python",
            "install/brew-ruby",
            "install/brew-node",
            "install/brew-python",
            "install/brew-php",
            "install/brew-elixir",
            "install/brew-go",
            "install/brew-install",
            "install/rust-install",
            "install/postgres-install",
            "install/redis-install",
            "install/docker-install",
            // Common
            "common/env-copy",
            "common/pre-commit-install",
            // Monorepo
            "monorepo/nx-build",
            "monorepo/turbo-build",
            "monorepo/lerna-bootstrap",
        ];
        for name in &expected {
            assert!(
                templates.contains_key(*name),
                "Template '{}' should be loaded",
                name
            );
        }
        assert_eq!(templates.len(), expected.len());
    }

    #[test]
    fn all_templates_have_required_fields() {
        let templates = load_templates().unwrap();
        for (name, tmpl) in &templates {
            assert!(!tmpl.name.is_empty(), "{} has empty name", name);
            assert!(
                !tmpl.description.is_empty(),
                "{} has empty description",
                name
            );
            assert!(!tmpl.category.is_empty(), "{} has empty category", name);
            assert!(!tmpl.version.is_empty(), "{} has empty version", name);
            assert!(!tmpl.platforms.is_empty(), "{} has no platforms", name);
        }
    }

    #[test]
    fn manifest_lists_all_templates() {
        let manifest = load_manifest().unwrap();
        let template_names = manifest.all_template_names();
        let templates = load_templates().unwrap();

        for template in templates.values() {
            assert!(
                template_names.contains(&template.name.as_str()),
                "Template '{}' (category '{}') missing from manifest categories",
                template.name,
                template.category
            );
        }
    }

    #[test]
    fn manifest_detection_order_references_valid_templates() {
        let manifest = load_manifest().unwrap();
        let templates = load_templates().unwrap();

        for name in &manifest.detection_order {
            assert!(
                templates.values().any(|t| t.name == *name),
                "Detection order references unknown template '{}'",
                name
            );
        }
    }

    #[test]
    fn bundler_template_has_correct_fields() {
        let templates = load_templates().unwrap();
        let bundler = &templates["ruby/bundle-install"];
        assert_eq!(bundler.category, "ruby");
        assert_eq!(bundler.step.command.as_deref(), Some("bundle install"));
        assert!(bundler.step.completed_check.is_some());
        assert!(!bundler.detectors.is_empty());
        assert!(bundler
            .detectors
            .contains(&"gemfile-present.file".to_string()));
        assert!(!bundler.step.watches.is_empty());
    }

    #[test]
    fn npm_template_has_file_exists_check() {
        let templates = load_templates().unwrap();
        let npm = &templates["node/npm-install"];
        assert_eq!(npm.category, "node");
        assert!(npm.step.completed_check.is_some());
    }

    #[test]
    fn cargo_template_detects_cargo_toml() {
        let templates = load_templates().unwrap();
        let cargo = &templates["rust/cargo-build"];
        assert_eq!(cargo.category, "rust");
        assert!(!cargo.detectors.is_empty());
        assert!(cargo
            .detectors
            .contains(&"cargo-toml-present.file".to_string()));
    }

    #[test]
    fn load_detectors_succeeds() {
        let detectors = load_detectors().unwrap();
        assert!(!detectors.detectors.is_empty());
    }

    #[test]
    fn all_detectors_have_checks() {
        let detectors = load_detectors().unwrap();
        for (name, def) in &detectors.detectors {
            assert!(
                def.has_checks(),
                "Detector '{}' has no checks defined",
                name
            );
        }
    }

    #[test]
    fn expected_detectors_exist() {
        let detectors = load_detectors().unwrap();
        let expected = [
            "ruby-installed",
            "gemfile-present",
            "rails-project",
            "node-installed",
            "package-json-present",
            "cargo-toml-present",
            "terraform-project",
            "docker-compose-project",
        ];
        for name in &expected {
            assert!(
                detectors.detectors.contains_key(*name),
                "Missing detector: {}",
                name
            );
        }
    }
}
