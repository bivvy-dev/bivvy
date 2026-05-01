//! Remote template loading from HTTP and Git sources.
//!
//! Fetches templates from URLs defined in `template_sources` and caches
//! them using the shared cache infrastructure (HTTP) or the on-disk clone
//! directory (Git).

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use super::fetch::{GitFetcher, HttpFetcher};
use super::template::Template;
use crate::cache::{parse_ttl, CacheStore};
use crate::config::schema::{TemplateSource as TemplateSourceConfig, TemplateSourceKind};

/// Loads templates from remote sources with caching.
#[derive(Debug, Clone)]
pub struct RemoteLoader {
    /// Fetched and parsed templates, keyed by `category/name`.
    templates: HashMap<String, (Template, u32)>,
}

impl RemoteLoader {
    /// Create a new remote loader by fetching templates from the given sources.
    ///
    /// HTTP sources are fetched into the shared `CacheStore`; Git sources are
    /// cloned (or updated) into `git_fetcher`'s clone directory and their
    /// templates directory is walked for `*.yml`/`*.yaml` files.
    pub fn new(
        sources: &[TemplateSourceConfig],
        http_fetcher: &HttpFetcher,
        git_fetcher: &GitFetcher,
        cache: &CacheStore,
    ) -> Result<Self> {
        let mut templates = HashMap::new();

        // Visit sources in priority order (lower `priority` number wins on
        // collision). Stable sort preserves declaration order among ties so
        // the user's listed-first source breaks the tie.
        let mut ordered: Vec<&TemplateSourceConfig> = sources.iter().collect();
        ordered.sort_by_key(|s| s.priority);

        for source in ordered {
            let result = match source.effective_kind() {
                TemplateSourceKind::Http => Self::load_http_source(source, http_fetcher, cache),
                TemplateSourceKind::Git => Self::load_git_source(source, git_fetcher),
            };

            match result {
                Ok(loaded) => {
                    for template in loaded {
                        // Higher-priority sources (lower number) win on collision.
                        let key = format!("{}/{}", template.category, template.name);
                        templates.entry(key).or_insert((template, source.priority));
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

    /// Load templates from a single HTTP source.
    fn load_http_source(
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

    /// Load templates from a single Git source.
    ///
    /// Clones (or updates) the repository, then walks `source.path` (or the
    /// repository root if `path` is unset) for `*.yml`/`*.yaml` files. Each
    /// file is parsed as a [`Template`]. Files that fail to parse are
    /// reported via `tracing::warn!` and skipped.
    ///
    /// URL scheme validation is the caller's responsibility: production code
    /// goes through [`Registry::with_remote_sources`](crate::registry::Registry::with_remote_sources),
    /// which validates each source up front.
    fn load_git_source(
        source: &TemplateSourceConfig,
        fetcher: &GitFetcher,
    ) -> Result<Vec<Template>> {
        let result = fetcher
            .fetch_unchecked(&source.url, source.git_ref.as_deref())
            .with_context(|| format!("Failed to clone git source: {}", source.url))?;

        let walk_root = match &source.path {
            Some(sub) => result.local_path.join(sub),
            None => result.local_path.clone(),
        };

        if !walk_root.exists() {
            anyhow::bail!(
                "Git source path does not exist: {} (in {})",
                source.path.as_deref().unwrap_or(""),
                source.url
            );
        }

        let mut templates = Vec::new();
        Self::walk_templates(&walk_root, &mut templates);
        Ok(templates)
    }

    /// Recursively walk `dir` collecting templates from `*.yml` / `*.yaml` files.
    /// Parse failures are logged and skipped so a single bad file does not
    /// abort the whole source.
    fn walk_templates(dir: &Path, out: &mut Vec<Template>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };

        for entry in entries.flatten() {
            let path = entry.path();

            // Skip the .git directory and any other dotfiles.
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with('.'))
                .unwrap_or(false)
            {
                continue;
            }

            if path.is_dir() {
                Self::walk_templates(&path, out);
                continue;
            }

            let is_yaml = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e == "yml" || e == "yaml")
                .unwrap_or(false);
            if !is_yaml {
                continue;
            }

            match fs::read_to_string(&path) {
                Ok(content) => match Self::parse_templates(&content) {
                    Ok(parsed) => out.extend(parsed),
                    Err(e) => {
                        tracing::warn!("Failed to parse template at {}: {}", path.display(), e);
                    }
                },
                Err(e) => {
                    tracing::warn!("Failed to read template at {}: {}", path.display(), e);
                }
            }
        }
    }

    /// Parse a YAML string containing one or more templates.
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

    /// Get a template by name (unqualified — returns first match).
    pub fn get(&self, name: &str) -> Option<&Template> {
        if let Some((t, _)) = self.templates.get(name) {
            return Some(t);
        }
        self.templates
            .values()
            .find(|(t, _)| t.name == name)
            .map(|(t, _)| t)
    }

    /// Get a template with its priority (unqualified — returns first match).
    pub fn get_with_priority(&self, name: &str) -> Option<(&Template, u32)> {
        if let Some((t, p)) = self.templates.get(name) {
            return Some((t, *p));
        }
        self.templates
            .values()
            .find(|(t, _)| t.name == name)
            .map(|(t, p)| (t, *p))
    }

    /// Get a template with its priority, filtering by category.
    pub fn get_with_priority_by_category(
        &self,
        name: &str,
        category: &str,
    ) -> Option<(&Template, u32)> {
        let key = format!("{}/{}", category, name);
        self.templates.get(&key).map(|(t, p)| (t, *p))
    }

    /// Check if a template exists (by unqualified name).
    pub fn has(&self, name: &str) -> bool {
        self.templates.contains_key(name) || self.templates.values().any(|(t, _)| t.name == name)
    }

    /// Get all template names (qualified as `category/name`).
    pub fn template_names(&self) -> Vec<&str> {
        self.templates.keys().map(|s| s.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::fetch::git::create_bare_repo_with_templates;
    use crate::registry::fetch::HttpFetcher;
    use std::sync::Mutex;
    use tempfile::TempDir;

    // Serialize git-process tests to avoid flaky failures under parallel execution.
    static GIT_LOCK: Mutex<()> = Mutex::new(());

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

    fn make_source(url: String, kind: TemplateSourceKind) -> TemplateSourceConfig {
        TemplateSourceConfig {
            kind: Some(kind),
            url,
            git_ref: Some("main".to_string()),
            path: None,
            priority: 50,
            timeout: 30,
            cache: None,
            auth: None,
        }
    }

    #[test]
    fn loads_templates_from_git_source_root() {
        let _lock = GIT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = TempDir::new().unwrap();

        let template_yaml = r#"
name: git-tool
description: "Tool from a git remote"
category: tools
step:
  command: git-tool run
"#;
        let bare = create_bare_repo_with_templates(temp.path(), &[("git-tool.yml", template_yaml)]);

        let cache = CacheStore::new(temp.path().join("cache"));
        let http = HttpFetcher::new();
        let git = GitFetcher::new(temp.path().join("clones"));

        let sources = vec![make_source(
            bare.to_string_lossy().to_string(),
            TemplateSourceKind::Git,
        )];

        let loader = RemoteLoader::new(&sources, &http, &git, &cache).unwrap();
        let template = loader
            .get("git-tool")
            .expect("git-tool template should be loaded from the git source");
        assert_eq!(template.name, "git-tool");
        assert_eq!(template.description, "Tool from a git remote");
    }

    #[test]
    fn git_source_loads_only_from_configured_subdir() {
        let _lock = GIT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = TempDir::new().unwrap();

        let included_yaml = r#"
name: included-tool
description: "Included via path"
category: tools
step:
  command: included
"#;
        let excluded_yaml = r#"
name: excluded-tool
description: "Outside the configured path"
category: tools
step:
  command: excluded
"#;
        let bare = create_bare_repo_with_templates(
            temp.path(),
            &[
                ("templates/included-tool.yml", included_yaml),
                ("docs/excluded-tool.yml", excluded_yaml),
            ],
        );

        let cache = CacheStore::new(temp.path().join("cache"));
        let http = HttpFetcher::new();
        let git = GitFetcher::new(temp.path().join("clones"));

        let mut source = make_source(bare.to_string_lossy().to_string(), TemplateSourceKind::Git);
        source.path = Some("templates".to_string());

        let loader = RemoteLoader::new(&[source], &http, &git, &cache).unwrap();
        assert!(
            loader.has("included-tool"),
            "included-tool should be loaded from the configured subdir"
        );
        assert!(
            !loader.has("excluded-tool"),
            "excluded-tool is outside the path filter and should not be loaded"
        );
    }

    #[test]
    fn git_source_recursively_walks_subdirectories() {
        let _lock = GIT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = TempDir::new().unwrap();

        let nested_yaml = r#"
name: nested-tool
description: "Nested under a category dir"
category: build
step:
  command: nested
"#;
        let bare = create_bare_repo_with_templates(
            temp.path(),
            &[("steps/build/nested-tool.yml", nested_yaml)],
        );

        let cache = CacheStore::new(temp.path().join("cache"));
        let http = HttpFetcher::new();
        let git = GitFetcher::new(temp.path().join("clones"));

        let source = make_source(bare.to_string_lossy().to_string(), TemplateSourceKind::Git);

        let loader = RemoteLoader::new(&[source], &http, &git, &cache).unwrap();
        assert!(
            loader.has("nested-tool"),
            "templates in nested subdirectories should be discovered"
        );
    }

    #[test]
    fn git_source_skips_unparseable_yaml() {
        let _lock = GIT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = TempDir::new().unwrap();

        let valid_yaml = r#"
name: ok-tool
description: "Parses cleanly"
category: tools
step:
  command: ok
"#;
        let invalid_yaml = "this: is: not: valid: template: yaml: [";
        let bare = create_bare_repo_with_templates(
            temp.path(),
            &[("ok-tool.yml", valid_yaml), ("broken.yml", invalid_yaml)],
        );

        let cache = CacheStore::new(temp.path().join("cache"));
        let http = HttpFetcher::new();
        let git = GitFetcher::new(temp.path().join("clones"));

        let sources = vec![make_source(
            bare.to_string_lossy().to_string(),
            TemplateSourceKind::Git,
        )];

        let loader = RemoteLoader::new(&sources, &http, &git, &cache).unwrap();
        assert!(
            loader.has("ok-tool"),
            "valid template should still be loaded even if a sibling fails to parse"
        );
    }
}
