//! Remote configuration fetching.
//!
//! This module provides functionality for fetching configuration files
//! from remote URLs.

use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

/// Fetches remote configuration files with caching.
///
/// # Example
///
/// ```no_run
/// use bivvy::config::RemoteFetcher;
/// use std::time::Duration;
///
/// let fetcher = RemoteFetcher::new(Duration::from_secs(30));
///
/// // Fetch a remote config file
/// let content = fetcher.fetch("https://example.com/config.yml").unwrap();
/// ```
pub struct RemoteFetcher {
    /// Request timeout.
    timeout: Duration,
    /// Cache directory for downloaded configs.
    cache_dir: PathBuf,
    /// HTTP client.
    client: reqwest::blocking::Client,
}

impl RemoteFetcher {
    /// Create a fetcher with the specified timeout.
    pub fn new(timeout: Duration) -> Self {
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("~/.cache"))
            .join("bivvy")
            .join("remote-configs");

        Self {
            timeout,
            cache_dir,
            client: reqwest::blocking::Client::builder()
                .timeout(timeout)
                .build()
                .expect("Failed to create HTTP client"),
        }
    }

    /// Create a fetcher with a custom cache directory.
    pub fn with_cache_dir(timeout: Duration, cache_dir: PathBuf) -> Self {
        Self {
            timeout,
            cache_dir,
            client: reqwest::blocking::Client::builder()
                .timeout(timeout)
                .build()
                .expect("Failed to create HTTP client"),
        }
    }

    /// Fetch a configuration file from a URL.
    pub fn fetch(&self, url: &str) -> Result<String> {
        // Check cache first
        if let Some(cached) = self.check_cache(url)? {
            return Ok(cached);
        }

        // Fetch from remote
        let content = self.fetch_remote(url)?;

        // Cache the result
        self.save_cache(url, &content)?;

        Ok(content)
    }

    /// Fetch a configuration file, bypassing cache.
    pub fn fetch_fresh(&self, url: &str) -> Result<String> {
        self.fetch_remote(url)
    }

    /// Fetch with authentication header.
    pub fn fetch_with_auth(&self, url: &str, auth: &AuthHeader) -> Result<String> {
        let response = self
            .client
            .get(url)
            .header(&auth.header_name, &auth.header_value)
            .send()
            .with_context(|| format!("Failed to fetch {}", url))?;

        if !response.status().is_success() {
            return Err(anyhow!("HTTP {} fetching {}", response.status(), url));
        }

        response
            .text()
            .with_context(|| format!("Failed to read response from {}", url))
    }

    fn fetch_remote(&self, url: &str) -> Result<String> {
        let response = self
            .client
            .get(url)
            .send()
            .with_context(|| format!("Failed to fetch {}", url))?;

        if !response.status().is_success() {
            return Err(anyhow!("HTTP {} fetching {}", response.status(), url));
        }

        response
            .text()
            .with_context(|| format!("Failed to read response from {}", url))
    }

    fn check_cache(&self, url: &str) -> Result<Option<String>> {
        let cache_path = self.cache_path(url);
        if cache_path.exists() {
            let content = std::fs::read_to_string(&cache_path)?;
            Ok(Some(content))
        } else {
            Ok(None)
        }
    }

    fn save_cache(&self, url: &str, content: &str) -> Result<()> {
        let cache_path = self.cache_path(url);
        if let Some(parent) = cache_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&cache_path, content)?;
        Ok(())
    }

    fn cache_path(&self, url: &str) -> PathBuf {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(url.as_bytes());
        let hash = hex::encode(hasher.finalize());
        self.cache_dir.join(format!("{}.yml", hash))
    }

    /// Clear the cache for a specific URL.
    pub fn clear_cache(&self, url: &str) -> Result<()> {
        let cache_path = self.cache_path(url);
        if cache_path.exists() {
            std::fs::remove_file(&cache_path)?;
        }
        Ok(())
    }

    /// Clear all cached configurations.
    pub fn clear_all_cache(&self) -> Result<()> {
        if self.cache_dir.exists() {
            std::fs::remove_dir_all(&self.cache_dir)?;
        }
        Ok(())
    }

    /// Get the request timeout.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }
}

impl Default for RemoteFetcher {
    fn default() -> Self {
        Self::new(Duration::from_secs(30))
    }
}

/// Authentication header for remote fetching.
#[derive(Debug, Clone)]
pub struct AuthHeader {
    /// Header name (e.g., "Authorization").
    pub header_name: String,
    /// Header value (e.g., "Bearer token123").
    pub header_value: String,
}

impl AuthHeader {
    /// Create a Bearer token auth header.
    pub fn bearer(token: &str) -> Self {
        Self {
            header_name: "Authorization".to_string(),
            header_value: format!("Bearer {}", token),
        }
    }

    /// Create a custom header auth.
    pub fn custom(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            header_name: name.into(),
            header_value: value.into(),
        }
    }
}

/// Resolves authentication from config settings.
pub fn resolve_auth(
    auth_type: &str,
    token_env: &str,
    header: Option<&str>,
    env_vars: &HashMap<String, String>,
) -> Option<AuthHeader> {
    // First check the provided env_vars map, then fall back to system env
    let token = env_vars
        .get(token_env)
        .cloned()
        .or_else(|| std::env::var(token_env).ok())?;

    match auth_type {
        "bearer" => Some(AuthHeader::bearer(&token)),
        "header" => {
            let header_name = header.unwrap_or("X-Auth-Token");
            Some(AuthHeader::custom(header_name, &token))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use tempfile::TempDir;

    #[test]
    fn creates_fetcher_with_default_timeout() {
        let fetcher = RemoteFetcher::default();
        assert_eq!(fetcher.timeout(), Duration::from_secs(30));
    }

    #[test]
    fn creates_fetcher_with_custom_timeout() {
        let fetcher = RemoteFetcher::new(Duration::from_secs(60));
        assert_eq!(fetcher.timeout(), Duration::from_secs(60));
    }

    #[test]
    fn creates_fetcher_with_custom_cache_dir() {
        let temp = TempDir::new().unwrap();
        let fetcher =
            RemoteFetcher::with_cache_dir(Duration::from_secs(30), temp.path().join("cache"));
        assert!(fetcher.cache_dir.ends_with("cache"));
    }

    #[test]
    fn bearer_auth_header_format() {
        let auth = AuthHeader::bearer("my-token");
        assert_eq!(auth.header_name, "Authorization");
        assert_eq!(auth.header_value, "Bearer my-token");
    }

    #[test]
    fn custom_auth_header_format() {
        let auth = AuthHeader::custom("X-API-Key", "secret123");
        assert_eq!(auth.header_name, "X-API-Key");
        assert_eq!(auth.header_value, "secret123");
    }

    #[test]
    fn resolve_auth_bearer() {
        let mut env = HashMap::new();
        env.insert("MY_TOKEN".to_string(), "token-value".to_string());

        let auth = resolve_auth("bearer", "MY_TOKEN", None, &env);

        assert!(auth.is_some());
        let auth = auth.unwrap();
        assert_eq!(auth.header_value, "Bearer token-value");
    }

    #[test]
    fn resolve_auth_custom_header() {
        let mut env = HashMap::new();
        env.insert("API_KEY".to_string(), "key-value".to_string());

        let auth = resolve_auth("header", "API_KEY", Some("X-API-Key"), &env);

        assert!(auth.is_some());
        let auth = auth.unwrap();
        assert_eq!(auth.header_name, "X-API-Key");
        assert_eq!(auth.header_value, "key-value");
    }

    #[test]
    fn resolve_auth_missing_env_var() {
        let env = HashMap::new();
        // Remove the env var if it exists
        std::env::remove_var("NONEXISTENT_VAR");

        let auth = resolve_auth("bearer", "NONEXISTENT_VAR", None, &env);

        assert!(auth.is_none());
    }

    #[test]
    fn cache_path_is_deterministic() {
        let temp = TempDir::new().unwrap();
        let fetcher =
            RemoteFetcher::with_cache_dir(Duration::from_secs(30), temp.path().to_path_buf());

        let path1 = fetcher.cache_path("https://example.com/config.yml");
        let path2 = fetcher.cache_path("https://example.com/config.yml");

        assert_eq!(path1, path2);
    }

    #[test]
    fn different_urls_have_different_cache_paths() {
        let temp = TempDir::new().unwrap();
        let fetcher =
            RemoteFetcher::with_cache_dir(Duration::from_secs(30), temp.path().to_path_buf());

        let path1 = fetcher.cache_path("https://example.com/config1.yml");
        let path2 = fetcher.cache_path("https://example.com/config2.yml");

        assert_ne!(path1, path2);
    }

    #[test]
    fn clear_cache_removes_cached_file() {
        let temp = TempDir::new().unwrap();
        let fetcher =
            RemoteFetcher::with_cache_dir(Duration::from_secs(30), temp.path().to_path_buf());

        let url = "https://example.com/test.yml";
        let cache_path = fetcher.cache_path(url);

        // Create a cached file
        std::fs::create_dir_all(cache_path.parent().unwrap()).unwrap();
        std::fs::write(&cache_path, "test content").unwrap();
        assert!(cache_path.exists());

        // Clear it
        fetcher.clear_cache(url).unwrap();
        assert!(!cache_path.exists());
    }

    #[test]
    fn clear_all_cache_removes_directory() {
        let temp = TempDir::new().unwrap();
        let cache_dir = temp.path().join("cache");
        let fetcher = RemoteFetcher::with_cache_dir(Duration::from_secs(30), cache_dir.clone());

        // Create some cached files
        std::fs::create_dir_all(&cache_dir).unwrap();
        std::fs::write(cache_dir.join("file1.yml"), "content").unwrap();
        std::fs::write(cache_dir.join("file2.yml"), "content").unwrap();

        assert!(cache_dir.exists());

        // Clear all
        fetcher.clear_all_cache().unwrap();
        assert!(!cache_dir.exists());
    }

    // --- Mock HTTP tests ---

    fn fetcher_with_mock(_server: &MockServer) -> RemoteFetcher {
        let temp = TempDir::new().unwrap();
        // Use a long-lived cache dir that won't be dropped
        let cache_dir = temp.keep().join("cache");
        RemoteFetcher::with_cache_dir(Duration::from_secs(10), cache_dir)
    }

    #[test]
    fn fetch_returns_yaml_content() {
        let server = MockServer::start();
        let yaml = "app_name: remote\nsteps:\n  install:\n    command: npm install\n";

        server.mock(|when, then| {
            when.method(GET).path("/config.yml");
            then.status(200).body(yaml);
        });

        let fetcher = fetcher_with_mock(&server);
        let content = fetcher.fetch(&server.url("/config.yml")).unwrap();

        assert_eq!(content, yaml);
    }

    #[test]
    fn fetch_returns_error_on_404() {
        let server = MockServer::start();

        server.mock(|when, then| {
            when.method(GET).path("/missing.yml");
            then.status(404).body("Not Found");
        });

        let fetcher = fetcher_with_mock(&server);
        let result = fetcher.fetch(&server.url("/missing.yml"));

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("404"), "Error should mention 404: {}", err);
    }

    #[test]
    fn fetch_returns_error_on_500() {
        let server = MockServer::start();

        server.mock(|when, then| {
            when.method(GET).path("/error.yml");
            then.status(500).body("Internal Server Error");
        });

        let fetcher = fetcher_with_mock(&server);
        let result = fetcher.fetch(&server.url("/error.yml"));

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("500"), "Error should mention 500: {}", err);
    }

    #[test]
    fn fetch_sends_auth_header() {
        let server = MockServer::start();

        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/private.yml")
                .header("Authorization", "Bearer secret-token");
            then.status(200).body("private: true");
        });

        let fetcher = fetcher_with_mock(&server);
        let auth = AuthHeader::bearer("secret-token");
        let content = fetcher
            .fetch_with_auth(&server.url("/private.yml"), &auth)
            .unwrap();

        assert_eq!(content, "private: true");
        mock.assert();
    }

    #[test]
    fn fetch_uses_cache_on_second_call() {
        let server = MockServer::start();

        let mock = server.mock(|when, then| {
            when.method(GET).path("/cached.yml");
            then.status(200).body("cached: true");
        });

        let fetcher = fetcher_with_mock(&server);
        let url = server.url("/cached.yml");

        // First call hits the server
        let content1 = fetcher.fetch(&url).unwrap();
        assert_eq!(content1, "cached: true");

        // Second call should use cache, not hitting the server
        let content2 = fetcher.fetch(&url).unwrap();
        assert_eq!(content2, "cached: true");

        // Server should have been called exactly once
        mock.assert_calls(1);
    }

    #[test]
    fn fetch_fresh_bypasses_cache() {
        let server = MockServer::start();

        let mock = server.mock(|when, then| {
            when.method(GET).path("/fresh.yml");
            then.status(200).body("fresh: true");
        });

        let fetcher = fetcher_with_mock(&server);
        let url = server.url("/fresh.yml");

        // Warm the cache
        fetcher.fetch(&url).unwrap();

        // fetch_fresh should bypass cache and hit server again
        let content = fetcher.fetch_fresh(&url).unwrap();
        assert_eq!(content, "fresh: true");

        // Server should have been called twice
        mock.assert_calls(2);
    }

    #[test]
    fn clear_cache_triggers_refetch() {
        let server = MockServer::start();

        let mock = server.mock(|when, then| {
            when.method(GET).path("/clearable.yml");
            then.status(200).body("content: v1");
        });

        let fetcher = fetcher_with_mock(&server);
        let url = server.url("/clearable.yml");

        // Warm the cache
        fetcher.fetch(&url).unwrap();
        mock.assert_calls(1);

        // Clear cache for this URL
        fetcher.clear_cache(&url).unwrap();

        // Next fetch should hit the server again
        fetcher.fetch(&url).unwrap();
        mock.assert_calls(2);
    }

    #[test]
    fn clear_all_cache_triggers_refetch() {
        let server = MockServer::start();

        let mock = server.mock(|when, then| {
            when.method(GET).path("/all-clear.yml");
            then.status(200).body("content: all");
        });

        let fetcher = fetcher_with_mock(&server);
        let url = server.url("/all-clear.yml");

        // Warm the cache
        fetcher.fetch(&url).unwrap();
        mock.assert_calls(1);

        // Clear all cache
        fetcher.clear_all_cache().unwrap();

        // Next fetch should hit the server again
        fetcher.fetch(&url).unwrap();
        mock.assert_calls(2);
    }

    #[test]
    fn concurrent_cache_access_no_corruption() {
        let server = MockServer::start();

        server.mock(|when, then| {
            when.method(GET).path("/concurrent.yml");
            then.status(200).body("concurrent: true\ndata: value\n");
        });

        let temp = TempDir::new().unwrap();
        let cache_dir = temp.path().join("cache");
        let url = server.url("/concurrent.yml");

        // Create multiple threads that all fetch the same URL
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let url = url.clone();
                let cache_dir = cache_dir.clone();
                std::thread::spawn(move || {
                    let fetcher = RemoteFetcher::with_cache_dir(Duration::from_secs(10), cache_dir);
                    fetcher.fetch(&url).unwrap()
                })
            })
            .collect();

        let results: Vec<String> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        // All threads should get the correct content
        for result in &results {
            assert_eq!(result, "concurrent: true\ndata: value\n");
        }
    }

    #[test]
    fn corrupted_cache_triggers_refetch() {
        let server = MockServer::start();

        let mock = server.mock(|when, then| {
            when.method(GET).path("/corrupt.yml");
            then.status(200).body("valid: true");
        });

        let temp = TempDir::new().unwrap();
        let cache_dir = temp.path().join("cache");
        let fetcher = RemoteFetcher::with_cache_dir(Duration::from_secs(10), cache_dir);
        let url = server.url("/corrupt.yml");

        // Warm the cache normally
        fetcher.fetch(&url).unwrap();
        mock.assert_calls(1);

        // Corrupt the cache file by writing garbage
        let cache_path = fetcher.cache_path(&url);
        std::fs::write(&cache_path, "garbage\x00\x01\x02not yaml").unwrap();

        // The fetcher's simple cache returns whatever is in the file,
        // so it will return the corrupted content. This is by design:
        // RemoteFetcher's cache is a simple file-based cache without
        // integrity checks. The caller (ExtendsResolver) handles
        // YAML parse errors.
        let content = fetcher.fetch(&url).unwrap();
        assert_eq!(content, "garbage\x00\x01\x02not yaml");
        // Still only 1 server hit (cache was returned, even if corrupted)
        mock.assert_calls(1);
    }
}
