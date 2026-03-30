//! Template resolution from multiple sources.
//!
//! Resolution order (first match wins):
//! 1. Project-local (.bivvy/templates/)
//! 2. User-local (~/.bivvy/templates/)
//! 3. Remote (by priority) - TODO in M11
//! 4. Built-in

use crate::config::schema::TemplateSource as TemplateSourceConfig;
use crate::error::{BivvyError, Result};
use crate::registry::builtin::BuiltinLoader;
use crate::registry::local::LocalLoader;
use crate::registry::remote::RemoteLoader;
use crate::registry::template::{Template, TemplateSource};
use std::collections::HashMap;
use std::path::Path;

/// A parsed template reference, optionally qualified by category.
///
/// Template references in config can use `category/name` syntax
/// (e.g., `rust/version-bump`) to disambiguate templates that share
/// a name across categories.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TemplateRef<'a> {
    name: &'a str,
    category: Option<&'a str>,
}

impl<'a> TemplateRef<'a> {
    /// Parse a template reference string.
    ///
    /// Supports two forms:
    /// - `"name"` — unqualified lookup (first match wins)
    /// - `"category/name"` — qualified lookup (must match both name and category)
    fn parse(input: &'a str) -> Self {
        match input.split_once('/') {
            Some((category, name)) if !category.is_empty() && !name.is_empty() => Self {
                name,
                category: Some(category),
            },
            _ => Self {
                name: input,
                category: None,
            },
        }
    }
}

/// Template registry that resolves templates from multiple sources.
#[derive(Debug, Clone)]
pub struct Registry {
    builtin: BuiltinLoader,
    local: LocalLoader,
    remote: RemoteLoader,
}

impl Registry {
    /// Create a new registry for a project (without remote sources).
    pub fn new(project_root: Option<&Path>) -> Result<Self> {
        Ok(Self {
            builtin: BuiltinLoader::new()?,
            local: LocalLoader::new(project_root)?,
            remote: RemoteLoader::empty(),
        })
    }

    /// Create a registry with remote template sources.
    pub fn with_remote_sources(
        project_root: Option<&Path>,
        sources: &[TemplateSourceConfig],
    ) -> Result<Self> {
        let fetcher = crate::registry::fetch::HttpFetcher::new();
        let cache_dir = crate::cache::default_cache_dir();
        let cache = crate::cache::CacheStore::new(cache_dir);

        let remote = RemoteLoader::new(sources, &fetcher, &cache).map_err(|e| {
            BivvyError::ConfigValidationError {
                message: format!("Failed to load remote templates: {}", e),
            }
        })?;

        Ok(Self {
            builtin: BuiltinLoader::new()?,
            local: LocalLoader::new(project_root)?,
            remote,
        })
    }

    /// Create a registry with a pre-loaded remote loader (for testing).
    #[cfg(test)]
    pub(crate) fn with_remote_loader(
        project_root: Option<&Path>,
        remote: RemoteLoader,
    ) -> Result<Self> {
        Ok(Self {
            builtin: BuiltinLoader::new()?,
            local: LocalLoader::new(project_root)?,
            remote,
        })
    }

    /// Resolve a template by name, with optional `category/name` syntax.
    ///
    /// Template references can be:
    /// - `"name"` — unqualified lookup (first match wins)
    /// - `"category/name"` — qualified lookup (must match both name and category)
    ///
    /// Resolution order (first match wins):
    /// 1. Project-local (.bivvy/templates/)
    /// 2. User-local (~/.bivvy/templates/)
    /// 3. Remote (by priority)
    /// 4. Built-in
    pub fn resolve(&self, input: &str) -> Result<(&Template, TemplateSource)> {
        let tref = TemplateRef::parse(input);

        // Check local first (includes both project and user)
        let local_result = match tref.category {
            Some(cat) => self.local.get_with_source_by_category(tref.name, cat),
            None => self.local.get_with_source(tref.name),
        };
        if let Some((template, source)) = local_result {
            return Ok((template, source));
        }

        // Check remote sources
        let remote_result = match tref.category {
            Some(cat) => self.remote.get_with_priority_by_category(tref.name, cat),
            None => self.remote.get_with_priority(tref.name),
        };
        if let Some((template, priority)) = remote_result {
            return Ok((template, TemplateSource::Remote { priority }));
        }

        // Check built-in
        let builtin_result = match tref.category {
            Some(cat) => self.builtin.get_by_category(tref.name, cat),
            None => self.builtin.get(tref.name),
        };
        if let Some(template) = builtin_result {
            return Ok((template, TemplateSource::Builtin));
        }

        Err(BivvyError::UnknownTemplate {
            name: input.to_string(),
        })
    }

    /// Get a template by name (without source info).
    ///
    /// Supports `category/name` syntax for qualified lookups.
    pub fn get(&self, input: &str) -> Option<&Template> {
        self.resolve(input).ok().map(|(t, _)| t)
    }

    /// Check if a template exists.
    ///
    /// Supports `category/name` syntax for qualified lookups.
    pub fn has(&self, input: &str) -> bool {
        let tref = TemplateRef::parse(input);
        match tref.category {
            Some(cat) => {
                self.local
                    .get_with_source_by_category(tref.name, cat)
                    .is_some()
                    || self
                        .remote
                        .get_with_priority_by_category(tref.name, cat)
                        .is_some()
                    || self.builtin.get_by_category(tref.name, cat).is_some()
            }
            None => {
                self.local.has(tref.name)
                    || self.remote.has(tref.name)
                    || self.builtin.get(tref.name).is_some()
            }
        }
    }

    /// Get all available template names.
    pub fn all_template_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .builtin
            .template_names()
            .into_iter()
            .map(|s| s.to_string())
            .collect();

        // Add remote templates
        for name in self.remote.template_names() {
            if !names.contains(&name.to_string()) {
                names.push(name.to_string());
            }
        }

        // Add local templates (may override both remote and built-in)
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
        let (template, source) = registry.resolve("brew-bundle").unwrap();
        assert_eq!(template.name, "brew-bundle");
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
        assert!(registry.has("brew-bundle"));
        assert!(!registry.has("nonexistent"));
    }

    #[test]
    fn registry_all_names_includes_builtin() {
        let registry = Registry::new(None).unwrap();
        let names = registry.all_template_names();
        assert!(names.contains(&"system/brew-bundle".to_string()));
    }

    #[test]
    fn registry_get_convenience_method() {
        let registry = Registry::new(None).unwrap();
        assert!(registry.get("brew-bundle").is_some());
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn builtin_templates_declare_requires() {
        let registry = Registry::new(None).unwrap();

        let expected: &[(&str, &[&str])] = &[
            ("bundle-install", &["ruby"]),
            ("yarn-install", &["node"]),
            ("npm-install", &["node"]),
            ("pnpm-install", &["node"]),
            ("bun-install", &["node"]),
            ("pip-install", &["python"]),
            ("poetry-install", &["python"]),
            ("uv-sync", &["python"]),
            ("cargo-build", &["rust"]),
            ("brew-bundle", &["brew"]),
        ];

        for (name, reqs) in expected {
            let template = registry.get(name).unwrap_or_else(|| {
                panic!("Built-in template '{}' not found", name);
            });
            let actual: Vec<&str> = template.step.requires.iter().map(|s| s.as_str()).collect();
            assert_eq!(
                actual, *reqs,
                "Template '{}' has wrong requires: {:?}",
                name, actual
            );
        }
    }

    #[test]
    fn install_templates_are_resolvable() {
        let registry = Registry::new(None).unwrap();

        let install_templates = [
            "mise-install",
            "mise-ruby",
            "mise-node",
            "mise-python",
            "brew-install",
            "rust-install",
            "postgres-install",
            "redis-install",
            "docker-install",
        ];

        for name in install_templates {
            let template = registry.get(name).unwrap_or_else(|| {
                panic!("Install template '{}' not found in registry", name);
            });
            assert!(
                template.step.command.is_some(),
                "Install template '{}' has no command",
                name
            );
            assert!(
                template.step.completed_check.is_some(),
                "Install template '{}' has no completed_check",
                name
            );
        }
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

        let errors = registry.validate_inputs("brew-bundle", &inputs).unwrap();
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

    // --- Remote template integration tests ---

    use crate::cache::CacheStore;
    use crate::config::schema::TemplateSource as TemplateSourceConfig;
    use crate::registry::fetch::HttpFetcher;
    use crate::registry::remote::RemoteLoader;
    use httpmock::prelude::*;

    #[test]
    fn registry_resolves_from_remote_http() {
        let server = MockServer::start();

        let template_yaml = r#"
name: remote-tool
description: "Remote tool template"
category: tools
step:
  command: remote-tool run
"#;

        server.mock(|when, then| {
            when.method(GET).path("/templates.yml");
            then.status(200).body(template_yaml);
        });

        let temp = TempDir::new().unwrap();
        let cache = CacheStore::new(temp.path().join("cache"));
        let fetcher = HttpFetcher::new();

        let sources = vec![TemplateSourceConfig {
            url: server.url("/templates.yml"),
            priority: 50,
            timeout: 30,
            cache: None,
            auth: None,
        }];

        let remote = RemoteLoader::new(&sources, &fetcher, &cache).unwrap();
        let registry = Registry::with_remote_loader(None, remote).unwrap();

        let (template, source) = registry.resolve("remote-tool").unwrap();
        assert_eq!(template.name, "remote-tool");
        assert_eq!(source, TemplateSource::Remote { priority: 50 });
    }

    #[test]
    fn registry_prefers_local_over_remote() {
        let server = MockServer::start();

        let remote_yaml = r#"
name: brew
description: "Remote brew override"
category: tools
step:
  command: remote-brew
"#;

        server.mock(|when, then| {
            when.method(GET).path("/templates.yml");
            then.status(200).body(remote_yaml);
        });

        let temp = TempDir::new().unwrap();

        // Create local template with same name
        let templates_dir = temp.path().join(".bivvy").join("templates").join("steps");
        fs::create_dir_all(&templates_dir).unwrap();
        fs::write(
            templates_dir.join("brew.yml"),
            r#"
name: brew
description: "Local brew"
category: tools
step:
  command: local-brew
"#,
        )
        .unwrap();

        let cache = CacheStore::new(temp.path().join("cache"));
        let fetcher = HttpFetcher::new();

        let sources = vec![TemplateSourceConfig {
            url: server.url("/templates.yml"),
            priority: 50,
            timeout: 30,
            cache: None,
            auth: None,
        }];

        let remote = RemoteLoader::new(&sources, &fetcher, &cache).unwrap();
        let registry = Registry::with_remote_loader(Some(temp.path()), remote).unwrap();

        let (template, source) = registry.resolve("brew").unwrap();
        // Local should win over remote
        assert_eq!(source, TemplateSource::Project);
        assert_eq!(template.description, "Local brew");
    }

    #[test]
    fn registry_caches_remote_templates() {
        let server = MockServer::start();

        let template_yaml = r#"
name: cached-tool
description: "Cached tool"
category: tools
step:
  command: cached-tool run
"#;

        let mock = server.mock(|when, then| {
            when.method(GET).path("/templates.yml");
            then.status(200).body(template_yaml);
        });

        let temp = TempDir::new().unwrap();
        let cache = CacheStore::new(temp.path().join("cache"));
        let fetcher = HttpFetcher::new();

        let sources = vec![TemplateSourceConfig {
            url: server.url("/templates.yml"),
            priority: 50,
            timeout: 30,
            cache: None,
            auth: None,
        }];

        // First load - hits server
        let _remote1 = RemoteLoader::new(&sources, &fetcher, &cache).unwrap();
        mock.assert_calls(1);

        // Second load - should use cache
        let _remote2 = RemoteLoader::new(&sources, &fetcher, &cache).unwrap();
        mock.assert_calls(1); // Still 1, cache was used
    }

    #[test]
    fn registry_remote_template_in_all_names() {
        let server = MockServer::start();

        let template_yaml = r#"
name: unique-remote-tool
description: "A unique remote tool"
category: tools
step:
  command: unique-tool
"#;

        server.mock(|when, then| {
            when.method(GET).path("/templates.yml");
            then.status(200).body(template_yaml);
        });

        let temp = TempDir::new().unwrap();
        let cache = CacheStore::new(temp.path().join("cache"));
        let fetcher = HttpFetcher::new();

        let sources = vec![TemplateSourceConfig {
            url: server.url("/templates.yml"),
            priority: 50,
            timeout: 30,
            cache: None,
            auth: None,
        }];

        let remote = RemoteLoader::new(&sources, &fetcher, &cache).unwrap();
        let registry = Registry::with_remote_loader(None, remote).unwrap();

        let names = registry.all_template_names();
        assert!(
            names.contains(&"tools/unique-remote-tool".to_string()),
            "Remote template should appear in all_template_names: {:?}",
            names
        );
    }

    // --- TemplateRef parsing tests ---

    #[test]
    fn template_ref_parse_unqualified() {
        let tref = TemplateRef::parse("version-bump");
        assert_eq!(tref.name, "version-bump");
        assert_eq!(tref.category, None);
    }

    #[test]
    fn template_ref_parse_qualified() {
        let tref = TemplateRef::parse("rust/version-bump");
        assert_eq!(tref.name, "version-bump");
        assert_eq!(tref.category, Some("rust"));
    }

    #[test]
    fn template_ref_parse_empty_category_is_unqualified() {
        let tref = TemplateRef::parse("/version-bump");
        assert_eq!(tref.name, "/version-bump");
        assert_eq!(tref.category, None);
    }

    #[test]
    fn template_ref_parse_trailing_slash_is_unqualified() {
        let tref = TemplateRef::parse("rust/");
        assert_eq!(tref.name, "rust/");
        assert_eq!(tref.category, None);
    }

    #[test]
    fn template_ref_parse_no_slash() {
        let tref = TemplateRef::parse("brew-bundle");
        assert_eq!(tref.name, "brew-bundle");
        assert_eq!(tref.category, None);
    }

    // --- Namespaced resolution tests ---

    #[test]
    fn resolve_builtin_with_correct_category() {
        let registry = Registry::new(None).unwrap();
        let (template, source) = registry.resolve("rust/version-bump").unwrap();
        assert_eq!(template.name, "version-bump");
        assert_eq!(template.category, "rust");
        assert_eq!(source, TemplateSource::Builtin);
    }

    #[test]
    fn resolve_builtin_with_wrong_category_fails() {
        let registry = Registry::new(None).unwrap();
        let result = registry.resolve("iac/version-bump");
        assert!(matches!(result, Err(BivvyError::UnknownTemplate { .. })));
    }

    #[test]
    fn has_with_correct_category() {
        let registry = Registry::new(None).unwrap();
        assert!(registry.has("rust/version-bump"));
        assert!(registry.has("system/brew-bundle"));
    }

    #[test]
    fn has_with_wrong_category() {
        let registry = Registry::new(None).unwrap();
        assert!(!registry.has("ruby/brew-bundle"));
        assert!(!registry.has("iac/version-bump"));
    }

    #[test]
    fn get_with_correct_category() {
        let registry = Registry::new(None).unwrap();
        let template = registry.get("rust/cargo-build").unwrap();
        assert_eq!(template.name, "cargo-build");
        assert_eq!(template.category, "rust");
    }

    #[test]
    fn get_with_wrong_category_returns_none() {
        let registry = Registry::new(None).unwrap();
        assert!(registry.get("python/cargo-build").is_none());
    }

    #[test]
    fn resolve_namespaced_prefers_local_over_builtin() {
        let temp = TempDir::new().unwrap();
        let templates_dir = temp.path().join(".bivvy").join("templates").join("steps");
        fs::create_dir_all(&templates_dir).unwrap();

        let custom = r#"
name: version-bump
description: "Custom version-bump"
category: rust
step:
  command: "echo custom"
"#;
        fs::write(templates_dir.join("version-bump.yml"), custom).unwrap();

        let registry = Registry::new(Some(temp.path())).unwrap();
        let (template, source) = registry.resolve("rust/version-bump").unwrap();
        assert_eq!(template.description, "Custom version-bump");
        assert_eq!(source, TemplateSource::Project);
    }

    #[test]
    fn resolve_namespaced_local_wrong_category_falls_through() {
        let temp = TempDir::new().unwrap();
        let templates_dir = temp.path().join(".bivvy").join("templates").join("steps");
        fs::create_dir_all(&templates_dir).unwrap();

        // Local template has category "custom", but we query "rust/version-bump"
        let custom = r#"
name: version-bump
description: "Custom version-bump"
category: custom
step:
  command: "echo custom"
"#;
        fs::write(templates_dir.join("version-bump.yml"), custom).unwrap();

        let registry = Registry::new(Some(temp.path())).unwrap();
        // Should fall through to builtin since local has wrong category
        let (template, source) = registry.resolve("rust/version-bump").unwrap();
        assert_eq!(source, TemplateSource::Builtin);
        assert_eq!(template.category, "rust");
    }

    #[test]
    fn unqualified_name_still_works() {
        let registry = Registry::new(None).unwrap();
        // All existing unqualified lookups should continue to work
        let (template, _) = registry.resolve("version-bump").unwrap();
        assert_eq!(template.name, "version-bump");
    }
}
