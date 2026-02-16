//! HTTP template fetching.
//!
//! Provides an HTTP client for fetching templates from URLs with
//! support for ETag-based conditional requests.

use anyhow::{bail, Result};
use reqwest::blocking::Client;
use std::time::Duration;

/// Default maximum response body size (10 MB).
const DEFAULT_MAX_RESPONSE_SIZE: u64 = 10 * 1024 * 1024;

/// Fetches templates over HTTP/HTTPS.
pub struct HttpFetcher {
    client: Client,
    timeout: Duration,
    max_response_size: u64,
}

/// Response from fetching a template.
#[derive(Debug)]
pub struct FetchResponse {
    /// The template content.
    pub content: String,
    /// ETag header if present.
    pub etag: Option<String>,
    /// Last-Modified header if present.
    pub last_modified: Option<String>,
}

impl HttpFetcher {
    /// Create a new HTTP fetcher with default 30-second timeout.
    pub fn new() -> Self {
        Self::with_timeout(Duration::from_secs(30))
    }

    /// Create a new HTTP fetcher with custom timeout.
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            client: Client::builder()
                .user_agent("bivvy")
                .timeout(timeout)
                .build()
                .expect("Failed to build HTTP client"),
            timeout,
            max_response_size: DEFAULT_MAX_RESPONSE_SIZE,
        }
    }

    /// Create a new HTTP fetcher with custom timeout and max response size.
    #[cfg(test)]
    pub fn with_max_size(timeout: Duration, max_response_size: u64) -> Self {
        Self {
            client: Client::builder()
                .user_agent("bivvy")
                .timeout(timeout)
                .build()
                .expect("Failed to build HTTP client"),
            timeout,
            max_response_size,
        }
    }

    /// Get the configured timeout.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Fetch a template from a URL.
    pub fn fetch(&self, url: &str) -> Result<FetchResponse> {
        let response = self.client.get(url).send()?;

        if !response.status().is_success() {
            bail!("HTTP {} fetching {}", response.status(), url);
        }

        // Check Content-Length if available
        if let Some(len) = response.content_length() {
            if len > self.max_response_size {
                bail!(
                    "Response too large ({} bytes, max {} bytes) from {}",
                    len,
                    self.max_response_size,
                    url
                );
            }
        }

        let etag = response
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(String::from);

        let last_modified = response
            .headers()
            .get("last-modified")
            .and_then(|v| v.to_str().ok())
            .map(String::from);

        let content = response.text()?;

        // Also check actual content size (in case Content-Length was missing/wrong)
        if content.len() as u64 > self.max_response_size {
            bail!(
                "Response too large ({} bytes, max {} bytes) from {}",
                content.len(),
                self.max_response_size,
                url
            );
        }

        Ok(FetchResponse {
            content,
            etag,
            last_modified,
        })
    }

    /// Fetch with conditional request (If-None-Match).
    ///
    /// Returns `None` if content unchanged (304 Not Modified).
    pub fn fetch_if_changed(&self, url: &str, etag: Option<&str>) -> Result<Option<FetchResponse>> {
        let mut request = self.client.get(url);

        if let Some(etag) = etag {
            request = request.header("If-None-Match", etag);
        }

        let response = request.send()?;

        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            return Ok(None);
        }

        if !response.status().is_success() {
            bail!("HTTP {} fetching {}", response.status(), url);
        }

        // Check Content-Length if available
        if let Some(len) = response.content_length() {
            if len > self.max_response_size {
                bail!(
                    "Response too large ({} bytes, max {} bytes) from {}",
                    len,
                    self.max_response_size,
                    url
                );
            }
        }

        let new_etag = response
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(String::from);

        let last_modified = response
            .headers()
            .get("last-modified")
            .and_then(|v| v.to_str().ok())
            .map(String::from);

        let content = response.text()?;

        // Also check actual content size
        if content.len() as u64 > self.max_response_size {
            bail!(
                "Response too large ({} bytes, max {} bytes) from {}",
                content.len(),
                self.max_response_size,
                url
            );
        }

        Ok(Some(FetchResponse {
            content,
            etag: new_etag,
            last_modified,
        }))
    }
}

impl Default for HttpFetcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    #[test]
    fn default_timeout_is_30_seconds() {
        let fetcher = HttpFetcher::new();
        assert_eq!(fetcher.timeout(), Duration::from_secs(30));
    }

    #[test]
    fn custom_timeout() {
        let fetcher = HttpFetcher::with_timeout(Duration::from_secs(60));
        assert_eq!(fetcher.timeout(), Duration::from_secs(60));
    }

    #[test]
    fn default_creates_fetcher() {
        let fetcher = HttpFetcher::default();
        assert_eq!(fetcher.timeout(), Duration::from_secs(30));
    }

    #[test]
    fn fetch_response_fields() {
        let response = FetchResponse {
            content: "test content".to_string(),
            etag: Some("\"abc123\"".to_string()),
            last_modified: Some("Sat, 01 Jan 2000 00:00:00 GMT".to_string()),
        };

        assert_eq!(response.content, "test content");
        assert_eq!(response.etag, Some("\"abc123\"".to_string()));
        assert!(response.last_modified.is_some());
    }

    #[test]
    fn fetch_response_optional_fields() {
        let response = FetchResponse {
            content: "test".to_string(),
            etag: None,
            last_modified: None,
        };

        assert!(response.etag.is_none());
        assert!(response.last_modified.is_none());
    }

    // --- Mock HTTP tests ---

    #[test]
    fn fetch_captures_etag() {
        let server = MockServer::start();

        server.mock(|when, then| {
            when.method(GET).path("/template.yml");
            then.status(200)
                .header("ETag", "\"v1-abc123\"")
                .body("template: content");
        });

        let fetcher = HttpFetcher::new();
        let resp = fetcher.fetch(&server.url("/template.yml")).unwrap();

        assert_eq!(resp.content, "template: content");
        assert_eq!(resp.etag, Some("\"v1-abc123\"".to_string()));
    }

    #[test]
    fn fetch_captures_last_modified() {
        let server = MockServer::start();

        server.mock(|when, then| {
            when.method(GET).path("/template.yml");
            then.status(200)
                .header("Last-Modified", "Sat, 01 Jan 2025 00:00:00 GMT")
                .body("template: content");
        });

        let fetcher = HttpFetcher::new();
        let resp = fetcher.fetch(&server.url("/template.yml")).unwrap();

        assert_eq!(
            resp.last_modified,
            Some("Sat, 01 Jan 2025 00:00:00 GMT".to_string())
        );
    }

    #[test]
    fn fetch_if_changed_sends_if_none_match() {
        let server = MockServer::start();

        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/template.yml")
                .header("If-None-Match", "\"v1-abc123\"");
            then.status(304);
        });

        let fetcher = HttpFetcher::new();
        let result = fetcher
            .fetch_if_changed(&server.url("/template.yml"), Some("\"v1-abc123\""))
            .unwrap();

        assert!(result.is_none(), "304 should return None");
        mock.assert();
    }

    #[test]
    fn fetch_if_changed_returns_none_on_304() {
        let server = MockServer::start();

        server.mock(|when, then| {
            when.method(GET).path("/unchanged.yml");
            then.status(304);
        });

        let fetcher = HttpFetcher::new();
        let result = fetcher
            .fetch_if_changed(&server.url("/unchanged.yml"), Some("\"old-etag\""))
            .unwrap();

        assert!(result.is_none());
    }

    #[test]
    fn fetch_if_changed_returns_content_on_200() {
        let server = MockServer::start();

        server.mock(|when, then| {
            when.method(GET).path("/changed.yml");
            then.status(200)
                .header("ETag", "\"v2-new\"")
                .body("updated: content");
        });

        let fetcher = HttpFetcher::new();
        let result = fetcher
            .fetch_if_changed(&server.url("/changed.yml"), Some("\"v1-old\""))
            .unwrap();

        assert!(result.is_some());
        let resp = result.unwrap();
        assert_eq!(resp.content, "updated: content");
        assert_eq!(resp.etag, Some("\"v2-new\"".to_string()));
    }

    #[test]
    fn fetch_handles_missing_etag() {
        let server = MockServer::start();

        server.mock(|when, then| {
            when.method(GET).path("/no-etag.yml");
            then.status(200).body("no etag here");
        });

        let fetcher = HttpFetcher::new();
        let resp = fetcher.fetch(&server.url("/no-etag.yml")).unwrap();

        assert_eq!(resp.content, "no etag here");
        assert!(resp.etag.is_none());
        assert!(resp.last_modified.is_none());
    }

    #[test]
    fn fetch_returns_error_on_404() {
        let server = MockServer::start();

        server.mock(|when, then| {
            when.method(GET).path("/missing.yml");
            then.status(404).body("Not Found");
        });

        let fetcher = HttpFetcher::new();
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
            then.status(500).body("Server Error");
        });

        let fetcher = HttpFetcher::new();
        let result = fetcher.fetch(&server.url("/error.yml"));

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("500"), "Error should mention 500: {}", err);
    }

    // --- Edge cases and security ---

    #[test]
    fn wrong_content_type_still_returns_body() {
        let server = MockServer::start();

        server.mock(|when, then| {
            when.method(GET).path("/html-response");
            then.status(200)
                .header("Content-Type", "text/html")
                .body("<html><body>Not YAML</body></html>");
        });

        let fetcher = HttpFetcher::new();
        let resp = fetcher.fetch(&server.url("/html-response")).unwrap();

        // HttpFetcher doesn't validate content type — that's the caller's job.
        // It returns whatever the server sends.
        assert!(resp.content.contains("<html>"));
    }

    #[test]
    fn utf8_content_preserved() {
        let server = MockServer::start();

        let utf8_yaml = "name: テスト\ndescription: \"日本語テンプレート\"\ncategory: 国際化\n";

        server.mock(|when, then| {
            when.method(GET).path("/utf8.yml");
            then.status(200).body(utf8_yaml);
        });

        let fetcher = HttpFetcher::new();
        let resp = fetcher.fetch(&server.url("/utf8.yml")).unwrap();

        assert!(resp.content.contains("テスト"));
        assert!(resp.content.contains("日本語テンプレート"));
        assert!(resp.content.contains("国際化"));
    }

    #[test]
    fn fetch_if_changed_without_etag_does_normal_fetch() {
        let server = MockServer::start();

        server.mock(|when, then| {
            when.method(GET).path("/no-etag-check.yml");
            then.status(200)
                .header("ETag", "\"new-etag\"")
                .body("content");
        });

        let fetcher = HttpFetcher::new();
        // Passing None for etag should still work (no If-None-Match header sent)
        let result = fetcher
            .fetch_if_changed(&server.url("/no-etag-check.yml"), None)
            .unwrap();

        assert!(result.is_some());
        assert_eq!(result.unwrap().etag, Some("\"new-etag\"".to_string()));
    }

    #[test]
    fn large_response_rejected() {
        let server = MockServer::start();

        // Body exceeds the 50-byte limit we set below
        let large_body = "x".repeat(100);
        server.mock(|when, then| {
            when.method(GET).path("/huge.yml");
            then.status(200).body(&large_body);
        });

        // Use a fetcher with a very small max size for testing
        let fetcher = HttpFetcher::with_max_size(Duration::from_secs(10), 50);
        let result = fetcher.fetch(&server.url("/huge.yml"));

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("too large"),
            "Error should mention size limit: {}",
            err
        );
    }

    #[test]
    fn redirect_followed_by_default() {
        let server = MockServer::start();

        // First request redirects to /actual.yml
        server.mock(|when, then| {
            when.method(GET).path("/redirect.yml");
            then.status(301)
                .header("Location", server.url("/actual.yml"));
        });

        server.mock(|when, then| {
            when.method(GET).path("/actual.yml");
            then.status(200).body("redirected: true");
        });

        let fetcher = HttpFetcher::new();
        let resp = fetcher.fetch(&server.url("/redirect.yml")).unwrap();

        assert_eq!(resp.content, "redirected: true");
    }

    #[test]
    fn sends_bivvy_user_agent() {
        let server = MockServer::start();

        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/ua-check.yml")
                .header("User-Agent", "bivvy");
            then.status(200).body("ua: ok");
        });

        let fetcher = HttpFetcher::new();
        let resp = fetcher.fetch(&server.url("/ua-check.yml")).unwrap();

        assert_eq!(resp.content, "ua: ok");
        mock.assert();
    }

    #[test]
    fn empty_response_body_returned() {
        let server = MockServer::start();

        server.mock(|when, then| {
            when.method(GET).path("/empty.yml");
            then.status(200).body("");
        });

        let fetcher = HttpFetcher::new();
        let resp = fetcher.fetch(&server.url("/empty.yml")).unwrap();

        assert_eq!(resp.content, "");
    }
}
