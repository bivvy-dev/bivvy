//! Remote template source configuration.
//!
//! This module defines types for configuring HTTP and Git-based
//! remote template sources with caching support.

use serde::{Deserialize, Serialize};

/// A remote source for templates.
///
/// This enum distinguishes between HTTP/HTTPS URL sources and
/// Git repository sources, each with their own configuration options.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum RemoteSource {
    /// HTTP/HTTPS URL source.
    #[serde(rename = "http")]
    Http {
        /// Base URL for templates.
        url: String,
        /// Cache configuration.
        #[serde(default)]
        cache: RemoteCacheConfig,
    },
    /// Git repository source.
    #[serde(rename = "git")]
    Git {
        /// Repository URL.
        url: String,
        /// Branch or tag to use.
        #[serde(rename = "ref")]
        git_ref: Option<String>,
        /// Path within repository.
        path: Option<String>,
        /// Cache configuration.
        #[serde(default)]
        cache: RemoteCacheConfig,
    },
}

/// Cache configuration for a remote template source.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RemoteCacheConfig {
    /// Time-to-live in seconds.
    #[serde(default = "default_ttl")]
    pub ttl: u64,
    /// Cache invalidation strategy.
    #[serde(default)]
    pub strategy: RemoteCacheStrategy,
}

/// Cache invalidation strategy for remote sources.
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RemoteCacheStrategy {
    /// Use TTL only.
    #[default]
    Ttl,
    /// Use HTTP ETag headers.
    Etag,
    /// Use git commit checking.
    Git,
}

fn default_ttl() -> u64 {
    604800 // 7 days in seconds
}

impl Default for RemoteCacheConfig {
    fn default() -> Self {
        Self {
            ttl: default_ttl(),
            strategy: RemoteCacheStrategy::default(),
        }
    }
}

impl RemoteSource {
    /// Get the cache configuration for this source.
    pub fn cache_config(&self) -> &RemoteCacheConfig {
        match self {
            Self::Http { cache, .. } => cache,
            Self::Git { cache, .. } => cache,
        }
    }

    /// Get the URL for this source.
    pub fn url(&self) -> &str {
        match self {
            Self::Http { url, .. } => url,
            Self::Git { url, .. } => url,
        }
    }

    /// Get a unique identifier for this source.
    pub fn id(&self) -> String {
        match self {
            Self::Http { url, .. } => format!("http:{}", url),
            Self::Git { url, git_ref, .. } => {
                format!("git:{}@{}", url, git_ref.as_deref().unwrap_or("HEAD"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_http_source() {
        let yaml = r#"
type: http
url: https://example.com/templates
cache:
  ttl: 3600
"#;

        let source: RemoteSource = serde_yaml::from_str(yaml).unwrap();

        match source {
            RemoteSource::Http { url, cache } => {
                assert_eq!(url, "https://example.com/templates");
                assert_eq!(cache.ttl, 3600);
            }
            _ => panic!("Expected HTTP source"),
        }
    }

    #[test]
    fn parses_git_source() {
        let yaml = r#"
type: git
url: https://github.com/org/templates.git
ref: main
path: templates/
cache:
  strategy: git
"#;

        let source: RemoteSource = serde_yaml::from_str(yaml).unwrap();

        match source {
            RemoteSource::Git {
                url,
                git_ref,
                path,
                cache,
            } => {
                assert_eq!(url, "https://github.com/org/templates.git");
                assert_eq!(git_ref, Some("main".to_string()));
                assert_eq!(path, Some("templates/".to_string()));
                assert_eq!(cache.strategy, RemoteCacheStrategy::Git);
            }
            _ => panic!("Expected Git source"),
        }
    }

    #[test]
    fn default_cache_config() {
        let config = RemoteCacheConfig::default();

        assert_eq!(config.ttl, 604800);
        assert_eq!(config.strategy, RemoteCacheStrategy::Ttl);
    }

    #[test]
    fn source_ids_are_unique() {
        let http1 = RemoteSource::Http {
            url: "https://a.com".to_string(),
            cache: RemoteCacheConfig::default(),
        };
        let http2 = RemoteSource::Http {
            url: "https://b.com".to_string(),
            cache: RemoteCacheConfig::default(),
        };

        assert_ne!(http1.id(), http2.id());
    }

    #[test]
    fn http_source_id_format() {
        let source = RemoteSource::Http {
            url: "https://example.com".to_string(),
            cache: RemoteCacheConfig::default(),
        };

        assert_eq!(source.id(), "http:https://example.com");
    }

    #[test]
    fn git_source_id_format() {
        let source = RemoteSource::Git {
            url: "https://github.com/org/repo.git".to_string(),
            git_ref: Some("main".to_string()),
            path: None,
            cache: RemoteCacheConfig::default(),
        };

        assert_eq!(source.id(), "git:https://github.com/org/repo.git@main");
    }

    #[test]
    fn git_source_id_defaults_to_head() {
        let source = RemoteSource::Git {
            url: "https://github.com/org/repo.git".to_string(),
            git_ref: None,
            path: None,
            cache: RemoteCacheConfig::default(),
        };

        assert_eq!(source.id(), "git:https://github.com/org/repo.git@HEAD");
    }

    #[test]
    fn cache_config_accessor() {
        let source = RemoteSource::Http {
            url: "https://example.com".to_string(),
            cache: RemoteCacheConfig {
                ttl: 1000,
                strategy: RemoteCacheStrategy::Etag,
            },
        };

        assert_eq!(source.cache_config().ttl, 1000);
        assert_eq!(source.cache_config().strategy, RemoteCacheStrategy::Etag);
    }

    #[test]
    fn url_accessor() {
        let http = RemoteSource::Http {
            url: "https://example.com".to_string(),
            cache: RemoteCacheConfig::default(),
        };
        let git = RemoteSource::Git {
            url: "https://github.com/org/repo.git".to_string(),
            git_ref: None,
            path: None,
            cache: RemoteCacheConfig::default(),
        };

        assert_eq!(http.url(), "https://example.com");
        assert_eq!(git.url(), "https://github.com/org/repo.git");
    }
}
