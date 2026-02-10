//! Local template loading from user and project directories.

use crate::error::Result;
use crate::registry::template::{Template, TemplateSource};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Loader for local templates (user and project).
#[derive(Debug, Clone)]
pub struct LocalLoader {
    templates: HashMap<String, (Template, TemplateSource)>,
}

impl LocalLoader {
    /// Load templates from user and project directories.
    pub fn new(project_root: Option<&Path>) -> Result<Self> {
        let mut templates = HashMap::new();

        // Load user templates (~/.bivvy/templates/)
        if let Some(user_dir) = Self::user_templates_dir() {
            Self::load_from_dir(&user_dir, TemplateSource::User, &mut templates)?;
        }

        // Load project templates (.bivvy/templates/)
        if let Some(root) = project_root {
            let project_dir = root.join(".bivvy").join("templates");
            Self::load_from_dir(&project_dir, TemplateSource::Project, &mut templates)?;
        }

        Ok(Self { templates })
    }

    /// Get user templates directory.
    fn user_templates_dir() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".bivvy").join("templates"))
    }

    /// Load templates from a directory (recursively).
    fn load_from_dir(
        dir: &Path,
        source: TemplateSource,
        templates: &mut HashMap<String, (Template, TemplateSource)>,
    ) -> Result<()> {
        if !dir.exists() {
            return Ok(());
        }

        // Load from steps/ subdirectory
        let steps_dir = dir.join("steps");
        if steps_dir.exists() {
            Self::load_templates_recursive(&steps_dir, source, templates)?;
        }

        Ok(())
    }

    fn load_templates_recursive(
        dir: &Path,
        source: TemplateSource,
        templates: &mut HashMap<String, (Template, TemplateSource)>,
    ) -> Result<()> {
        if !dir.is_dir() {
            return Ok(());
        }

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                Self::load_templates_recursive(&path, source, templates)?;
            } else if path
                .extension()
                .map(|e| e == "yml" || e == "yaml")
                .unwrap_or(false)
            {
                let content = fs::read_to_string(&path)?;
                let template: Template = serde_yaml::from_str(&content).map_err(|e| {
                    crate::error::BivvyError::ConfigParseError {
                        path: path.clone(),
                        message: e.to_string(),
                    }
                })?;

                // Project templates override user templates
                if source == TemplateSource::Project || !templates.contains_key(&template.name) {
                    templates.insert(template.name.clone(), (template, source));
                }
            }
        }

        Ok(())
    }

    /// Get a template by name.
    pub fn get(&self, name: &str) -> Option<&Template> {
        self.templates.get(name).map(|(t, _)| t)
    }

    /// Get a template with its source.
    pub fn get_with_source(&self, name: &str) -> Option<(&Template, TemplateSource)> {
        self.templates.get(name).map(|(t, s)| (t, *s))
    }

    /// Get all template names.
    pub fn template_names(&self) -> Vec<&str> {
        self.templates.keys().map(|s| s.as_str()).collect()
    }

    /// Check if a template exists.
    pub fn has(&self, name: &str) -> bool {
        self.templates.contains_key(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn local_loader_empty_without_templates() {
        let temp = TempDir::new().unwrap();
        let loader = LocalLoader::new(Some(temp.path())).unwrap();
        assert!(loader.template_names().is_empty());
    }

    #[test]
    fn local_loader_finds_project_templates() {
        let temp = TempDir::new().unwrap();
        let templates_dir = temp.path().join(".bivvy").join("templates").join("steps");
        fs::create_dir_all(&templates_dir).unwrap();

        let template_yaml = r#"
name: custom
description: "A custom template"
category: custom
step:
  command: "echo custom"
"#;
        fs::write(templates_dir.join("custom.yml"), template_yaml).unwrap();

        let loader = LocalLoader::new(Some(temp.path())).unwrap();
        assert!(loader.has("custom"));
        assert!(loader.get("custom").is_some());
    }

    #[test]
    fn local_loader_returns_source() {
        let temp = TempDir::new().unwrap();
        let templates_dir = temp.path().join(".bivvy").join("templates").join("steps");
        fs::create_dir_all(&templates_dir).unwrap();

        let template_yaml = r#"
name: test
description: "Test"
category: test
step:
  command: "echo test"
"#;
        fs::write(templates_dir.join("test.yml"), template_yaml).unwrap();

        let loader = LocalLoader::new(Some(temp.path())).unwrap();
        let (_, source) = loader.get_with_source("test").unwrap();
        assert_eq!(source, TemplateSource::Project);
    }

    #[test]
    fn local_loader_loads_nested_templates() {
        let temp = TempDir::new().unwrap();
        let templates_dir = temp
            .path()
            .join(".bivvy")
            .join("templates")
            .join("steps")
            .join("category");
        fs::create_dir_all(&templates_dir).unwrap();

        let template_yaml = r#"
name: nested
description: "Nested template"
category: test
step:
  command: "echo nested"
"#;
        fs::write(templates_dir.join("nested.yml"), template_yaml).unwrap();

        let loader = LocalLoader::new(Some(temp.path())).unwrap();
        assert!(loader.has("nested"));
    }
}
