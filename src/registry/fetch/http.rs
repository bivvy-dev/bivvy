//! HTTP template fetching.
//!
//! Provides an HTTP client for fetching templates from URLs with
//! support for ETag-based conditional requests.

use anyhow::{bail, Result};
use reqwest::blocking::Client;
use std::time::Duration;

/// Fetches templates over HTTP/HTTPS.
pub struct HttpFetcher {
    client: Client,
    timeout: Duration,
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
}
