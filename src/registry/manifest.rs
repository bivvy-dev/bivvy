//! Registry manifest definitions.
//!
//! The manifest defines template categories and detection order
//! for `bivvy init` auto-detection.

use serde::{Deserialize, Serialize};

/// Registry manifest defining available templates.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RegistryManifest {
    /// Manifest version.
    #[serde(default = "default_manifest_version")]
    pub version: u32,

    /// Template categories.
    #[serde(default)]
    pub categories: Vec<Category>,

    /// Detection order during `bivvy init`.
    #[serde(default)]
    pub detection_order: Vec<String>,
}

fn default_manifest_version() -> u32 {
    1
}

/// Template category for organization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Category {
    /// Category name.
    pub name: String,

    /// Human-readable description.
    pub description: String,

    /// Templates in this category.
    #[serde(default)]
    pub templates: Vec<String>,
}

impl RegistryManifest {
    /// Get templates in detection order.
    pub fn templates_in_order(&self) -> Vec<&str> {
        self.detection_order.iter().map(|s| s.as_str()).collect()
    }

    /// Get all template names from all categories.
    pub fn all_template_names(&self) -> Vec<&str> {
        self.categories
            .iter()
            .flat_map(|c| c.templates.iter().map(|s| s.as_str()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_registry_manifest() {
        let yaml = r#"
version: 1
categories:
  - name: system
    description: "System-level tools"
    templates: [brew]
  - name: node
    description: "Node.js ecosystem"
    templates: [npm, yarn, pnpm]
detection_order:
  - brew
  - yarn
  - npm
"#;
        let manifest: RegistryManifest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(manifest.version, 1);
        assert_eq!(manifest.categories.len(), 2);
        assert_eq!(manifest.detection_order.len(), 3);
    }

    #[test]
    fn templates_in_order() {
        let manifest = RegistryManifest {
            version: 1,
            categories: vec![],
            detection_order: vec!["a".to_string(), "b".to_string()],
        };
        assert_eq!(manifest.templates_in_order(), vec!["a", "b"]);
    }

    #[test]
    fn all_template_names() {
        let manifest = RegistryManifest {
            version: 1,
            categories: vec![
                Category {
                    name: "cat1".to_string(),
                    description: "".to_string(),
                    templates: vec!["a".to_string(), "b".to_string()],
                },
                Category {
                    name: "cat2".to_string(),
                    description: "".to_string(),
                    templates: vec!["c".to_string()],
                },
            ],
            detection_order: vec![],
        };
        let names = manifest.all_template_names();
        assert!(names.contains(&"a"));
        assert!(names.contains(&"b"));
        assert!(names.contains(&"c"));
    }
}
