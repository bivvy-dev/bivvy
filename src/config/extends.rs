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

/// Known private/internal hostnames that should be blocked for SSRF prevention.
const BLOCKED_HOSTS: &[&str] = &["localhost", "127.0.0.1", "0.0.0.0", "[::1]"];

/// Known cloud metadata IP addresses.
const BLOCKED_IPS: &[&str] = &["169.254.169.254", "100.100.100.200"];

/// Extract the host from an HTTP/HTTPS URL.
fn extract_host(url: &str) -> Option<String> {
    // Strip scheme
    let without_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    // Take everything before the first '/' or end
    let host_port = without_scheme.split('/').next()?;
    // Strip port if present
    let host = if host_port.starts_with('[') {
        // IPv6: [::1]:8080
        host_port.split(']').next().map(|h| format!("{}]", h))
    } else {
        Some(host_port.split(':').next().unwrap_or(host_port).to_string())
    };
    host
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
        // SSRF prevention: block private/internal addresses
        if let Some(host) = extract_host(&ext.url) {
            let host_lower = host.to_lowercase();
            if BLOCKED_HOSTS.iter().any(|&h| host_lower == h) {
                return Err(anyhow!(
                    "Extends URL must not reference localhost or loopback addresses: {}",
                    ext.url
                ));
            }
            if BLOCKED_IPS.iter().any(|&ip| host_lower == ip) {
                return Err(anyhow!(
                    "Extends URL must not reference internal/metadata IP addresses: {}",
                    ext.url
                ));
            }
            // Block 10.x.x.x, 172.16-31.x.x, 192.168.x.x ranges
            if let Ok(ip) = host.parse::<std::net::Ipv4Addr>() {
                if ip.is_private() || ip.is_loopback() || ip.is_link_local() {
                    return Err(anyhow!(
                        "Extends URL must not reference private IP addresses: {}",
                        ext.url
                    ));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use tempfile::TempDir;

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

    #[test]
    fn validate_extends_blocks_localhost() {
        let extends = vec![ExtendsConfig {
            url: "http://localhost/config.yml".to_string(),
        }];

        let result = validate_extends(&extends);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("localhost"));
    }

    #[test]
    fn validate_extends_blocks_127_0_0_1() {
        let extends = vec![ExtendsConfig {
            url: "http://127.0.0.1/config.yml".to_string(),
        }];

        let result = validate_extends(&extends);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("localhost"));
    }

    #[test]
    fn validate_extends_blocks_metadata_ip() {
        let extends = vec![ExtendsConfig {
            url: "http://169.254.169.254/latest/meta-data/".to_string(),
        }];

        let result = validate_extends(&extends);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("metadata"));
    }

    #[test]
    fn validate_extends_blocks_private_10_range() {
        let extends = vec![ExtendsConfig {
            url: "http://10.0.0.1/config.yml".to_string(),
        }];

        let result = validate_extends(&extends);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("private"));
    }

    #[test]
    fn validate_extends_blocks_private_192_168_range() {
        let extends = vec![ExtendsConfig {
            url: "http://192.168.1.1/config.yml".to_string(),
        }];

        let result = validate_extends(&extends);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("private"));
    }

    #[test]
    fn validate_extends_allows_public_ip() {
        let extends = vec![ExtendsConfig {
            url: "https://1.2.3.4/config.yml".to_string(),
        }];

        assert!(validate_extends(&extends).is_ok());
    }

    // --- Mock HTTP extends integration tests ---

    fn resolver_with_mock(_server: &MockServer) -> ExtendsResolver {
        let temp = TempDir::new().unwrap();
        let cache_dir = temp.keep().join("cache");
        let fetcher = RemoteFetcher::with_cache_dir(Duration::from_secs(10), cache_dir);
        ExtendsResolver::with_fetcher(fetcher)
    }

    #[test]
    fn single_extends_merges_base() {
        let server = MockServer::start();

        let base_yaml = r#"
app_name: BaseApp
steps:
  install:
    command: npm install
    title: Install dependencies
"#;

        server.mock(|when, then| {
            when.method(GET).path("/base.yml");
            then.status(200).body(base_yaml);
        });

        let config = BivvyConfig {
            extends: Some(vec![ExtendsConfig {
                url: server.url("/base.yml"),
            }]),
            ..Default::default()
        };

        let resolver = resolver_with_mock(&server);
        let resolved = resolver.resolve(&config).unwrap();

        // Base steps appear in result
        assert!(resolved.steps.contains_key("install"));
        assert_eq!(
            resolved.steps["install"].command,
            Some("npm install".to_string())
        );
    }

    #[test]
    fn local_overrides_base() {
        let server = MockServer::start();

        let base_yaml = r#"
app_name: BaseApp
steps:
  install:
    command: npm install
    title: Base install
"#;

        server.mock(|when, then| {
            when.method(GET).path("/base.yml");
            then.status(200).body(base_yaml);
        });

        let mut steps = std::collections::HashMap::new();
        steps.insert(
            "install".to_string(),
            super::super::schema::StepConfig {
                command: Some("yarn install".to_string()),
                title: Some("Local install".to_string()),
                ..Default::default()
            },
        );

        let config = BivvyConfig {
            app_name: Some("LocalApp".to_string()),
            extends: Some(vec![ExtendsConfig {
                url: server.url("/base.yml"),
            }]),
            steps,
            ..Default::default()
        };

        let resolver = resolver_with_mock(&server);
        let resolved = resolver.resolve(&config).unwrap();

        // Local values override base
        assert_eq!(resolved.app_name, Some("LocalApp".to_string()));
        assert_eq!(
            resolved.steps["install"].command,
            Some("yarn install".to_string())
        );
        assert_eq!(
            resolved.steps["install"].title,
            Some("Local install".to_string())
        );
    }

    #[test]
    fn chained_extends() {
        let server = MockServer::start();

        // C is the deepest base
        let c_yaml = r#"
app_name: AppC
steps:
  lint:
    command: eslint .
"#;

        // B extends C
        let b_yaml = format!(
            r#"
extends:
  - url: {}
app_name: AppB
steps:
  test:
    command: jest
"#,
            server.url("/c.yml")
        );

        server.mock(|when, then| {
            when.method(GET).path("/c.yml");
            then.status(200).body(c_yaml);
        });

        server.mock(|when, then| {
            when.method(GET).path("/b.yml");
            then.status(200).body(b_yaml);
        });

        // A extends B
        let config = BivvyConfig {
            app_name: Some("AppA".to_string()),
            extends: Some(vec![ExtendsConfig {
                url: server.url("/b.yml"),
            }]),
            ..Default::default()
        };

        let resolver = resolver_with_mock(&server);
        let resolved = resolver.resolve(&config).unwrap();

        // A's name wins over B and C
        assert_eq!(resolved.app_name, Some("AppA".to_string()));
        // Steps from both B and C are present
        assert!(resolved.steps.contains_key("lint"), "lint from C");
        assert!(resolved.steps.contains_key("test"), "test from B");
    }

    #[test]
    fn circular_extends_detected() {
        let server = MockServer::start();

        // A extends B, B extends A — circular
        let b_yaml = format!(
            r#"
extends:
  - url: {}
app_name: AppB
"#,
            server.url("/a.yml")
        );

        // We need A's URL to exist for B's fetch, but it won't matter
        // because the cycle is detected before A is re-fetched
        let a_yaml = format!(
            r#"
extends:
  - url: {}
app_name: AppA
"#,
            server.url("/b.yml")
        );

        server.mock(|when, then| {
            when.method(GET).path("/a.yml");
            then.status(200).body(a_yaml);
        });

        server.mock(|when, then| {
            when.method(GET).path("/b.yml");
            then.status(200).body(b_yaml);
        });

        let config = BivvyConfig {
            extends: Some(vec![ExtendsConfig {
                url: server.url("/a.yml"),
            }]),
            ..Default::default()
        };

        let resolver = resolver_with_mock(&server);
        let result = resolver.resolve(&config);

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Circular extends"),
            "Error should mention circular: {}",
            err
        );
    }

    #[test]
    fn depth_limit_enforced() {
        let server = MockServer::start();

        // Create a chain that's deeper than max_depth (set to 2)
        // depth3 extends nothing
        let depth3_yaml = "app_name: depth3\n";
        // depth2 extends depth3
        let depth2_yaml = format!(
            "extends:\n  - url: {}\napp_name: depth2\n",
            server.url("/depth3.yml")
        );
        // depth1 extends depth2
        let depth1_yaml = format!(
            "extends:\n  - url: {}\napp_name: depth1\n",
            server.url("/depth2.yml")
        );

        server.mock(|when, then| {
            when.method(GET).path("/depth3.yml");
            then.status(200).body(depth3_yaml);
        });
        server.mock(|when, then| {
            when.method(GET).path("/depth2.yml");
            then.status(200).body(depth2_yaml);
        });
        server.mock(|when, then| {
            when.method(GET).path("/depth1.yml");
            then.status(200).body(depth1_yaml);
        });

        let config = BivvyConfig {
            extends: Some(vec![ExtendsConfig {
                url: server.url("/depth1.yml"),
            }]),
            ..Default::default()
        };

        // Use max_depth=2, chain is deeper
        let temp = TempDir::new().unwrap();
        let cache_dir = temp.keep().join("cache");
        let fetcher = RemoteFetcher::with_cache_dir(Duration::from_secs(10), cache_dir);
        let resolver = ExtendsResolver {
            fetcher,
            max_depth: 2,
        };

        let result = resolver.resolve(&config);

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("depth exceeds"),
            "Error should mention depth: {}",
            err
        );
    }

    #[test]
    fn unreachable_url_reports_which_url() {
        let server = MockServer::start();

        // Only the first URL works
        server.mock(|when, then| {
            when.method(GET).path("/good.yml");
            then.status(200).body("app_name: good\n");
        });

        // Second URL returns 404
        server.mock(|when, then| {
            when.method(GET).path("/bad.yml");
            then.status(404).body("Not Found");
        });

        let config = BivvyConfig {
            extends: Some(vec![
                ExtendsConfig {
                    url: server.url("/good.yml"),
                },
                ExtendsConfig {
                    url: server.url("/bad.yml"),
                },
            ]),
            ..Default::default()
        };

        let resolver = resolver_with_mock(&server);
        let result = resolver.resolve(&config);

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("404"), "Error should contain 404: {}", err);
    }

    #[test]
    fn invalid_yaml_response() {
        let server = MockServer::start();

        server.mock(|when, then| {
            when.method(GET).path("/invalid.yml");
            then.status(200).body("not: valid: yaml: [{{{{");
        });

        let config = BivvyConfig {
            extends: Some(vec![ExtendsConfig {
                url: server.url("/invalid.yml"),
            }]),
            ..Default::default()
        };

        let resolver = resolver_with_mock(&server);
        let result = resolver.resolve(&config);

        assert!(result.is_err());
    }

    #[test]
    fn extends_cleared_from_result() {
        let server = MockServer::start();

        server.mock(|when, then| {
            when.method(GET).path("/base.yml");
            then.status(200).body("app_name: base\n");
        });

        let config = BivvyConfig {
            extends: Some(vec![ExtendsConfig {
                url: server.url("/base.yml"),
            }]),
            ..Default::default()
        };

        let resolver = resolver_with_mock(&server);
        let resolved = resolver.resolve(&config).unwrap();

        assert!(
            resolved.extends.is_none(),
            "extends should be cleared after resolution"
        );
    }

    #[test]
    fn multiple_extends_merge_in_order() {
        let server = MockServer::start();

        let first_yaml = r#"
app_name: First
steps:
  step_a:
    command: from-first
  step_shared:
    command: from-first
"#;

        let second_yaml = r#"
app_name: Second
steps:
  step_b:
    command: from-second
  step_shared:
    command: from-second
"#;

        server.mock(|when, then| {
            when.method(GET).path("/first.yml");
            then.status(200).body(first_yaml);
        });

        server.mock(|when, then| {
            when.method(GET).path("/second.yml");
            then.status(200).body(second_yaml);
        });

        let config = BivvyConfig {
            extends: Some(vec![
                ExtendsConfig {
                    url: server.url("/first.yml"),
                },
                ExtendsConfig {
                    url: server.url("/second.yml"),
                },
            ]),
            ..Default::default()
        };

        let resolver = resolver_with_mock(&server);
        let resolved = resolver.resolve(&config).unwrap();

        // Both unique steps present
        assert!(resolved.steps.contains_key("step_a"));
        assert!(resolved.steps.contains_key("step_b"));

        // Later base overrides earlier for shared step
        assert_eq!(
            resolved.steps["step_shared"].command,
            Some("from-second".to_string())
        );
    }
}
