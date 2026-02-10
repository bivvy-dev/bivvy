//! Configuration inheritance resolution.
//!
//! This module handles the `extends:` configuration feature, which allows
//! configs to inherit from remote base configurations.

use std::collections::HashSet;
use std::time::Duration;

use anyhow::{anyhow, Result};

use super::merger::deep_merge;
use super::remote::RemoteFetcher;
use super::schema::{BivvyConfig, ExtendsConfig};

/// Resolves configuration inheritance chain.
///
/// # Example
///
/// ```no_run
/// use bivvy::config::{BivvyConfig, ExtendsResolver};
///
/// let resolver = ExtendsResolver::new();
///
/// // Resolve a config that extends a remote base
/// let config: BivvyConfig = serde_yaml::from_str(r#"
/// extends:
///   - url: https://example.com/base-config.yml
/// app_name: MyApp
/// "#).unwrap();
///
/// let resolved = resolver.resolve(&config).unwrap();
/// ```
pub struct ExtendsResolver {
    fetcher: RemoteFetcher,
    max_depth: usize,
}

impl ExtendsResolver {
    /// Create a resolver with default settings.
    pub fn new() -> Self {
        Self {
            fetcher: RemoteFetcher::new(Duration::from_secs(30)),
            max_depth: 10,
        }
    }

    /// Create a resolver with a custom fetcher.
    pub fn with_fetcher(fetcher: RemoteFetcher) -> Self {
        Self {
            fetcher,
            max_depth: 10,
        }
    }

    /// Create a resolver with custom max depth.
    pub fn with_max_depth(max_depth: usize) -> Self {
        Self {
            fetcher: RemoteFetcher::default(),
            max_depth,
        }
    }

    /// Resolve all extends references and merge configs.
    pub fn resolve(&self, config: &BivvyConfig) -> Result<BivvyConfig> {
        self.resolve_with_visited(config, &mut HashSet::new(), 0)
    }

    fn resolve_with_visited(
        &self,
        config: &BivvyConfig,
        visited: &mut HashSet<String>,
        depth: usize,
    ) -> Result<BivvyConfig> {
        if depth > self.max_depth {
            return Err(anyhow!(
                "Config extends depth exceeds maximum of {}",
                self.max_depth
            ));
        }

        // If no extends, return config as-is
        let extends = match &config.extends {
            Some(e) if !e.is_empty() => e,
            _ => return Ok(config.clone()),
        };

        // Convert current config to Value for merging
        let config_value = serde_yaml::to_value(config)?;

        // Start with empty Value
        let mut merged = serde_yaml::Value::Mapping(serde_yaml::Mapping::new());

        // Process each base config in order
        for ext in extends {
            // Check for cycles
            if visited.contains(&ext.url) {
                return Err(anyhow!("Circular extends detected: {}", ext.url));
            }
            visited.insert(ext.url.clone());

            // Fetch and parse base config
            let content = self.fetcher.fetch(&ext.url)?;
            let base_config: BivvyConfig = serde_yaml::from_str(&content)?;

            // Recursively resolve base's extends
            let resolved_base = self.resolve_with_visited(&base_config, visited, depth + 1)?;

            // Convert to Value and merge
            let base_value = serde_yaml::to_value(&resolved_base)?;
            merged = deep_merge(&merged, &base_value);
        }

        // Finally merge the current config on top
        let final_value = deep_merge(&merged, &config_value);

        // Deserialize back to BivvyConfig
        let mut final_config: BivvyConfig = serde_yaml::from_value(final_value)?;

        // Clear extends from final config (already resolved)
        final_config.extends = None;

        Ok(final_config)
    }

    /// Get the maximum extends depth.
    pub fn max_depth(&self) -> usize {
        self.max_depth
    }
}

impl Default for ExtendsResolver {
    fn default() -> Self {
        Self::new()
    }
}

/// Validate extends configuration.
pub fn validate_extends(extends: &[ExtendsConfig]) -> Result<()> {
    for ext in extends {
        if ext.url.is_empty() {
            return Err(anyhow!("Extends URL cannot be empty"));
        }
        if !ext.url.starts_with("http://") && !ext.url.starts_with("https://") {
            return Err(anyhow!("Extends URL must be HTTP/HTTPS: {}", ext.url));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolver_has_default_max_depth() {
        let resolver = ExtendsResolver::new();
        assert_eq!(resolver.max_depth(), 10);
    }

    #[test]
    fn resolver_custom_max_depth() {
        let resolver = ExtendsResolver::with_max_depth(5);
        assert_eq!(resolver.max_depth(), 5);
    }

    #[test]
    fn config_without_extends_unchanged() {
        let resolver = ExtendsResolver::new();
        let config = BivvyConfig {
            app_name: Some("test".to_string()),
            ..Default::default()
        };

        let resolved = resolver.resolve(&config).unwrap();

        assert_eq!(resolved.app_name, Some("test".to_string()));
    }

    #[test]
    fn empty_extends_unchanged() {
        let resolver = ExtendsResolver::new();
        let config = BivvyConfig {
            app_name: Some("test".to_string()),
            extends: Some(vec![]),
            ..Default::default()
        };

        let resolved = resolver.resolve(&config).unwrap();

        assert_eq!(resolved.app_name, Some("test".to_string()));
    }

    #[test]
    fn validate_extends_accepts_valid_urls() {
        let extends = vec![
            ExtendsConfig {
                url: "https://example.com/config.yml".to_string(),
            },
            ExtendsConfig {
                url: "http://internal.example.com/base.yml".to_string(),
            },
        ];

        assert!(validate_extends(&extends).is_ok());
    }

    #[test]
    fn validate_extends_rejects_empty_url() {
        let extends = vec![ExtendsConfig {
            url: "".to_string(),
        }];

        let result = validate_extends(&extends);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be empty"));
    }

    #[test]
    fn validate_extends_rejects_non_http_url() {
        let extends = vec![ExtendsConfig {
            url: "file:///etc/config.yml".to_string(),
        }];

        let result = validate_extends(&extends);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("HTTP/HTTPS"));
    }

    #[test]
    fn validate_extends_rejects_relative_path() {
        let extends = vec![ExtendsConfig {
            url: "./base-config.yml".to_string(),
        }];

        let result = validate_extends(&extends);
        assert!(result.is_err());
    }
}
