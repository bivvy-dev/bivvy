//! Remote template loading from HTTP sources.
//!
//! Fetches templates from HTTP URLs defined in `template_sources`
//! and caches them using the shared cache infrastructure.

use anyhow::{Context, Result};
use std::collections::HashMap;

use super::fetch::HttpFetcher;
use super::template::Template;
use crate::cache::{parse_ttl, CacheStore};
use crate::config::schema::TemplateSource as TemplateSourceConfig;

/// Loads templates from remote HTTP sources with caching.
#[derive(Debug, Clone)]
pub struct RemoteLoader {
    /// Fetched and parsed templates, keyed by name.
    templates: HashMap<String, (Template, u32)>,
}

impl RemoteLoader {
    /// Create a new remote loader by fetching templates from the given sources.
    pub fn new(
        sources: &[TemplateSourceConfig],
        fetcher: &HttpFetcher,
        cache: &CacheStore,
    ) -> Result<Self> {
        let mut templates = HashMap::new();

        for source in sources {
            match Self::load_source(source, fetcher, cache) {
                Ok(loaded) => {
                    for template in loaded {
                        // Only insert if not already present (higher priority wins)
                        templates
                            .entry(template.name.clone())
                            .or_insert((template, source.priority));
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to load templates from {}: {}", source.url, e);
                }
            }
        }

        Ok(Self { templates })
    }

    /// Create an empty remote loader (no sources).
    pub fn empty() -> Self {
        Self {
            templates: HashMap::new(),
        }
    }

    /// Load templates from a single source.
    fn load_source(
        source: &TemplateSourceConfig,
        fetcher: &HttpFetcher,
        cache: &CacheStore,
    ) -> Result<Vec<Template>> {
        let source_id = format!("http:{}", source.url);

        // Check cache first
        if let Some(entry) = cache.load(&source_id, "_index")? {
            if !entry.is_expired() {
                let content = cache.read_content(&entry)?;
                return Self::parse_templates(&content);
            }
        }

        // Fetch from remote
        let response = fetcher
            .fetch(&source.url)
            .with_context(|| format!("Failed to fetch template source: {}", source.url))?;

        // Cache the response
        let ttl_seconds = source
            .cache
            .as_ref()
            .and_then(|c| parse_ttl(&c.ttl).ok())
            .map(|d| d.num_seconds() as u64)
            .unwrap_or(604800);
        cache.store(&source_id, "_index", &response.content, ttl_seconds)?;

        Self::parse_templates(&response.content)
    }

    /// Parse a YAML response containing one or more templates.
    ///
    /// Supports two formats:
    /// 1. A single template YAML document
    /// 2. A YAML array of templates
    fn parse_templates(content: &str) -> Result<Vec<Template>> {
        // Try parsing as a list first
        if let Ok(templates) = serde_yaml::from_str::<Vec<Template>>(content) {
            return Ok(templates);
        }

        // Try as a single template
        if let Ok(template) = serde_yaml::from_str::<Template>(content) {
            return Ok(vec![template]);
        }

        anyhow::bail!(
            "Remote source returned content that is neither a template nor a list of templates"
        )
    }

    /// Get a template by name.
    pub fn get(&self, name: &str) -> Option<&Template> {
        self.templates.get(name).map(|(t, _)| t)
    }

    /// Get a template with its priority.
    pub fn get_with_priority(&self, name: &str) -> Option<(&Template, u32)> {
        self.templates.get(name).map(|(t, p)| (t, *p))
    }

    /// Check if a template exists.
    pub fn has(&self, name: &str) -> bool {
        self.templates.contains_key(name)
    }

    /// Get all template names.
    pub fn template_names(&self) -> Vec<&str> {
        self.templates.keys().map(|s| s.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_loader_has_no_templates() {
        let loader = RemoteLoader::empty();
        assert!(!loader.has("anything"));
        assert!(loader.template_names().is_empty());
    }

    #[test]
    fn parse_single_template() {
        let yaml = r#"
name: remote-tool
description: "A remote tool"
category: tools
step:
  command: echo hello
"#;

        let templates = RemoteLoader::parse_templates(yaml).unwrap();
        assert_eq!(templates.len(), 1);
        assert_eq!(templates[0].name, "remote-tool");
    }

    #[test]
    fn parse_template_list() {
        let yaml = r#"
- name: tool-a
  description: "Tool A"
  category: tools
  step:
    command: echo a
- name: tool-b
  description: "Tool B"
  category: tools
  step:
    command: echo b
"#;

        let templates = RemoteLoader::parse_templates(yaml).unwrap();
        assert_eq!(templates.len(), 2);
        assert_eq!(templates[0].name, "tool-a");
        assert_eq!(templates[1].name, "tool-b");
    }

    #[test]
    fn parse_invalid_content_returns_error() {
        let result = RemoteLoader::parse_templates("not a template at all: [[[");
        assert!(result.is_err());
    }
}
