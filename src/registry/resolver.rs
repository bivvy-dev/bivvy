//! Template resolution from multiple sources.
//!
//! Resolution order (first match wins):
//! 1. Project-local (.bivvy/templates/)
//! 2. User-local (~/.bivvy/templates/)
//! 3. Remote (by priority) - TODO in M11
//! 4. Built-in

use crate::error::{BivvyError, Result};
use crate::registry::builtin::BuiltinLoader;
use crate::registry::local::LocalLoader;
use crate::registry::template::{Template, TemplateSource};
use std::collections::HashMap;
use std::path::Path;

/// Template registry that resolves templates from multiple sources.
#[derive(Debug, Clone)]
pub struct Registry {
    builtin: BuiltinLoader,
    local: LocalLoader,
    // Remote loader will be added in M11
}

impl Registry {
    /// Create a new registry for a project.
    pub fn new(project_root: Option<&Path>) -> Result<Self> {
        Ok(Self {
            builtin: BuiltinLoader::new()?,
            local: LocalLoader::new(project_root)?,
        })
    }

    /// Resolve a template by name.
    ///
    /// Resolution order (first match wins):
    /// 1. Project-local (.bivvy/templates/)
    /// 2. User-local (~/.bivvy/templates/)
    /// 3. Remote (by priority) - TODO in M11
    /// 4. Built-in
    pub fn resolve(&self, name: &str) -> Result<(&Template, TemplateSource)> {
        // Check local first (includes both project and user)
        if let Some((template, source)) = self.local.get_with_source(name) {
            return Ok((template, source));
        }

        // Check built-in
        if let Some(template) = self.builtin.get(name) {
            return Ok((template, TemplateSource::Builtin));
        }

        Err(BivvyError::UnknownTemplate {
            name: name.to_string(),
        })
    }

    /// Get a template by name (without source info).
    pub fn get(&self, name: &str) -> Option<&Template> {
        self.resolve(name).ok().map(|(t, _)| t)
    }

    /// Check if a template exists.
    pub fn has(&self, name: &str) -> bool {
        self.local.has(name) || self.builtin.get(name).is_some()
    }

    /// Get all available template names.
    pub fn all_template_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .builtin
            .template_names()
            .into_iter()
            .map(|s| s.to_string())
            .collect();

        // Add local templates (may override built-in)
        for name in self.local.template_names() {
            if !names.contains(&name.to_string()) {
                names.push(name.to_string());
            }
        }

        names.sort();
        names
    }

    /// Get templates in detection order for `bivvy init`.
    pub fn detection_order(&self) -> Vec<&Template> {
        // Start with built-in detection order
        self.builtin.detection_order()
    }

    /// Get the builtin loader (for manifest access).
    pub fn builtin(&self) -> &BuiltinLoader {
        &self.builtin
    }

    /// Validate inputs against a template's input contract.
    pub fn validate_inputs(
        &self,
        template_name: &str,
        inputs: &HashMap<String, serde_yaml::Value>,
    ) -> Result<Vec<String>> {
        let template = self
            .get(template_name)
            .ok_or_else(|| BivvyError::UnknownTemplate {
                name: template_name.to_string(),
            })?;

        let mut errors = Vec::new();

        // Check all required inputs are provided
        for (name, contract) in &template.inputs {
            if let Err(e) = contract.validate(name, inputs.get(name)) {
                errors.push(e);
            }
        }

        // Check no unknown inputs are provided
        for key in inputs.keys() {
            if !template.inputs.contains_key(key) {
                errors.push(format!(
                    "Unknown input '{}' for template '{}'",
                    key, template_name
                ));
            }
        }

        Ok(errors)
    }

    /// Get effective input values (provided + defaults).
    pub fn effective_inputs(
        &self,
        template_name: &str,
        provided: &HashMap<String, serde_yaml::Value>,
    ) -> Result<HashMap<String, serde_yaml::Value>> {
        let template = self
            .get(template_name)
            .ok_or_else(|| BivvyError::UnknownTemplate {
                name: template_name.to_string(),
            })?;

        let mut effective = HashMap::new();

        for (name, contract) in &template.inputs {
            if let Some(value) = contract.effective_value(provided.get(name)) {
                effective.insert(name.clone(), value.clone());
            }
        }

        Ok(effective)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn registry_resolves_builtin() {
        let registry = Registry::new(None).unwrap();
        let (template, source) = registry.resolve("brew").unwrap();
        assert_eq!(template.name, "brew");
        assert_eq!(source, TemplateSource::Builtin);
    }

    #[test]
    fn registry_fails_for_unknown() {
        let registry = Registry::new(None).unwrap();
        let result = registry.resolve("nonexistent");
        assert!(matches!(result, Err(BivvyError::UnknownTemplate { .. })));
    }

    #[test]
    fn registry_prefers_local_over_builtin() {
        let temp = TempDir::new().unwrap();
        let templates_dir = temp.path().join(".bivvy").join("templates").join("steps");
        fs::create_dir_all(&templates_dir).unwrap();

        // Create a local template that shadows built-in "brew"
        let custom_brew = r#"
name: brew
description: "Custom brew override"
category: custom
step:
  command: "echo custom brew"
"#;
        fs::write(templates_dir.join("brew.yml"), custom_brew).unwrap();

        let registry = Registry::new(Some(temp.path())).unwrap();
        let (template, source) = registry.resolve("brew").unwrap();

        assert_eq!(template.description, "Custom brew override");
        assert_eq!(source, TemplateSource::Project);
    }

    #[test]
    fn registry_has_works() {
        let registry = Registry::new(None).unwrap();
        assert!(registry.has("brew"));
        assert!(!registry.has("nonexistent"));
    }

    #[test]
    fn registry_all_names_includes_builtin() {
        let registry = Registry::new(None).unwrap();
        let names = registry.all_template_names();
        assert!(names.contains(&"brew".to_string()));
    }

    #[test]
    fn registry_get_convenience_method() {
        let registry = Registry::new(None).unwrap();
        assert!(registry.get("brew").is_some());
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn registry_detection_order() {
        let registry = Registry::new(None).unwrap();
        let order = registry.detection_order();
        // Should return templates in detection order
        assert!(!order.is_empty());
    }

    #[test]
    fn validate_inputs_catches_missing_required() {
        let temp = TempDir::new().unwrap();
        let templates_dir = temp.path().join(".bivvy").join("templates").join("steps");
        fs::create_dir_all(&templates_dir).unwrap();

        let template = r#"
name: with-inputs
description: "Template with inputs"
category: test
inputs:
  required_input:
    description: "A required input"
    type: string
    required: true
step:
  command: "echo ${required_input}"
"#;
        fs::write(templates_dir.join("with-inputs.yml"), template).unwrap();

        let registry = Registry::new(Some(temp.path())).unwrap();
        let errors = registry
            .validate_inputs("with-inputs", &HashMap::new())
            .unwrap();

        assert!(!errors.is_empty());
        assert!(errors[0].contains("required") || errors[0].contains("missing"));
    }

    #[test]
    fn validate_inputs_accepts_valid_inputs() {
        let temp = TempDir::new().unwrap();
        let templates_dir = temp.path().join(".bivvy").join("templates").join("steps");
        fs::create_dir_all(&templates_dir).unwrap();

        let template = r#"
name: with-inputs
description: "Template with inputs"
category: test
inputs:
  name:
    description: "A name"
    type: string
    required: true
step:
  command: "echo ${name}"
"#;
        fs::write(templates_dir.join("with-inputs.yml"), template).unwrap();

        let registry = Registry::new(Some(temp.path())).unwrap();

        let mut inputs = HashMap::new();
        inputs.insert(
            "name".to_string(),
            serde_yaml::Value::String("test".to_string()),
        );

        let errors = registry.validate_inputs("with-inputs", &inputs).unwrap();
        assert!(errors.is_empty());
    }

    #[test]
    fn validate_inputs_catches_unknown_inputs() {
        let registry = Registry::new(None).unwrap();

        let mut inputs = HashMap::new();
        inputs.insert(
            "unknown".to_string(),
            serde_yaml::Value::String("value".to_string()),
        );

        let errors = registry.validate_inputs("brew", &inputs).unwrap();
        assert!(errors.iter().any(|e| e.contains("Unknown input")));
    }

    #[test]
    fn effective_inputs_includes_defaults() {
        let temp = TempDir::new().unwrap();
        let templates_dir = temp.path().join(".bivvy").join("templates").join("steps");
        fs::create_dir_all(&templates_dir).unwrap();

        let template = r#"
name: with-default
description: "Template with default"
category: test
inputs:
  mode:
    description: "Mode"
    type: string
    default: "development"
step:
  command: "echo ${mode}"
"#;
        fs::write(templates_dir.join("with-default.yml"), template).unwrap();

        let registry = Registry::new(Some(temp.path())).unwrap();
        let effective = registry
            .effective_inputs("with-default", &HashMap::new())
            .unwrap();

        assert_eq!(effective.get("mode").unwrap().as_str(), Some("development"));
    }

    #[test]
    fn effective_inputs_prefers_provided() {
        let temp = TempDir::new().unwrap();
        let templates_dir = temp.path().join(".bivvy").join("templates").join("steps");
        fs::create_dir_all(&templates_dir).unwrap();

        let template = r#"
name: with-default
description: "Template with default"
category: test
inputs:
  mode:
    description: "Mode"
    type: string
    default: "development"
step:
  command: "echo ${mode}"
"#;
        fs::write(templates_dir.join("with-default.yml"), template).unwrap();

        let registry = Registry::new(Some(temp.path())).unwrap();

        let mut provided = HashMap::new();
        provided.insert(
            "mode".to_string(),
            serde_yaml::Value::String("production".to_string()),
        );

        let effective = registry
            .effective_inputs("with-default", &provided)
            .unwrap();

        assert_eq!(effective.get("mode").unwrap().as_str(), Some("production"));
    }
}
