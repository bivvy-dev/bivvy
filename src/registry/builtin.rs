//! Built-in templates embedded at compile time.

use crate::error::Result;
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

/// Load all built-in templates.
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
                            templates.insert(template.name.clone(), template);
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
        .map(|t| t.contains_key(name))
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

    /// Get a template by name.
    pub fn get(&self, name: &str) -> Option<&Template> {
        self.templates.get(name)
    }

    /// Get all template names.
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
            .filter_map(|name| self.templates.get(name))
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
        assert!(templates.contains_key("brew"));
    }

    #[test]
    fn load_templates_includes_yarn() {
        let templates = load_templates().unwrap();
        assert!(templates.contains_key("yarn"));
    }

    #[test]
    fn has_template_returns_true_for_builtin() {
        assert!(has_template("brew"));
        assert!(has_template("yarn"));
    }

    #[test]
    fn has_template_returns_false_for_unknown() {
        assert!(!has_template("nonexistent"));
    }

    #[test]
    fn brew_template_has_correct_fields() {
        let templates = load_templates().unwrap();
        let brew = &templates["brew"];
        assert_eq!(brew.category, "system");
        assert!(brew.step.command.is_some());
        assert!(!brew.detects.is_empty());
    }

    #[test]
    fn builtin_loader_new() {
        let loader = BuiltinLoader::new().unwrap();
        assert!(!loader.template_names().is_empty());
    }

    #[test]
    fn builtin_loader_get() {
        let loader = BuiltinLoader::new().unwrap();
        assert!(loader.get("brew").is_some());
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
        let expected = [
            "brew",
            "apt",
            "yum",
            "pacman",
            "chocolatey",
            "scoop",
            "mise",
            "asdf",
            "volta",
            "bundler",
            "yarn",
            "npm",
            "pnpm",
            "bun",
            "pip",
            "poetry",
            "uv",
            "cargo",
            "go",
            "swift",
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

        for name in templates.keys() {
            assert!(
                template_names.contains(&name.as_str()),
                "Template '{}' missing from manifest categories",
                name
            );
        }
    }

    #[test]
    fn manifest_detection_order_references_valid_templates() {
        let manifest = load_manifest().unwrap();
        let templates = load_templates().unwrap();

        for name in &manifest.detection_order {
            assert!(
                templates.contains_key(name),
                "Detection order references unknown template '{}'",
                name
            );
        }
    }

    #[test]
    fn bundler_template_has_correct_fields() {
        let templates = load_templates().unwrap();
        let bundler = &templates["bundler"];
        assert_eq!(bundler.category, "ruby");
        assert_eq!(bundler.step.command.as_deref(), Some("bundle install"));
        assert!(bundler.step.completed_check.is_some());
        assert!(!bundler.detects.is_empty());
        assert!(!bundler.step.watches.is_empty());
    }

    #[test]
    fn npm_template_has_file_exists_check() {
        let templates = load_templates().unwrap();
        let npm = &templates["npm"];
        assert_eq!(npm.category, "node");
        assert!(npm.step.completed_check.is_some());
    }

    #[test]
    fn cargo_template_detects_cargo_toml() {
        let templates = load_templates().unwrap();
        let cargo = &templates["cargo"];
        assert_eq!(cargo.category, "rust");
        assert!(cargo
            .detects
            .iter()
            .any(|d| d.file.as_deref() == Some("Cargo.toml")));
    }
}
