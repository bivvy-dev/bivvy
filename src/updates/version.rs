//! Version checking against latest release.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Current version of bivvy.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// GitHub API URL for releases.
const GITHUB_API_URL: &str = "https://api.github.com/repos/bivvy-dev/bivvy/releases/latest";

/// How often to check for updates (1 day).
const CHECK_INTERVAL_SECS: i64 = 86400;

/// Information about an available update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateInfo {
    /// Current version.
    pub current: String,
    /// Latest available version.
    pub latest: String,
    /// Whether an update is available.
    pub update_available: bool,
    /// Release URL.
    pub release_url: Option<String>,
    /// When this check was performed.
    pub checked_at: DateTime<Utc>,
}

/// Cached update check result.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct UpdateCache {
    /// Last update info.
    info: UpdateInfo,
    /// When the cache was written.
    cached_at: DateTime<Utc>,
}

/// Check for available updates.
///
/// Returns cached result if within check interval.
pub fn check_for_updates() -> Option<UpdateInfo> {
    // Check cache first
    if let Some(cached) = load_cache() {
        let age = Utc::now()
            .signed_duration_since(cached.cached_at)
            .num_seconds();
        if age < CHECK_INTERVAL_SECS {
            return Some(cached.info);
        }
    }

    // Fetch latest version
    match fetch_latest_version() {
        Ok(info) => {
            // Cache the result
            let _ = save_cache(&info);
            Some(info)
        }
        Err(_) => {
            // Return cached result even if expired
            load_cache().map(|c| c.info)
        }
    }
}

/// Check for updates without using cache.
pub fn check_for_updates_fresh() -> Result<UpdateInfo> {
    fetch_latest_version()
}

/// Fetch the latest version from GitHub.
fn fetch_latest_version() -> Result<UpdateInfo> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("bivvy")
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let response: serde_json::Value = client
        .get(GITHUB_API_URL)
        .send()?
        .json()
        .context("Failed to parse GitHub API response")?;

    let tag = response["tag_name"]
        .as_str()
        .context("No tag_name in response")?
        .trim_start_matches('v');

    let release_url = response["html_url"].as_str().map(String::from);

    let update_available = is_newer_version(tag, VERSION);

    Ok(UpdateInfo {
        current: VERSION.to_string(),
        latest: tag.to_string(),
        update_available,
        release_url,
        checked_at: Utc::now(),
    })
}

/// Compare versions to check if `latest` is newer than `current`.
fn is_newer_version(latest: &str, current: &str) -> bool {
    let parse_version = |v: &str| -> Vec<u32> {
        v.split('.')
            .take(3)
            .filter_map(|s| s.parse().ok())
            .collect()
    };

    let latest_parts = parse_version(latest);
    let current_parts = parse_version(current);

    // Compare component by component
    for (l, c) in latest_parts.iter().zip(current_parts.iter()) {
        if l > c {
            return true;
        }
        if l < c {
            return false;
        }
    }

    // If all components equal, check if latest has more components
    latest_parts.len() > current_parts.len()
}

/// Get the cache file path.
fn cache_path() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("bivvy").join("update_check.json"))
}

/// Load cached update info.
fn load_cache() -> Option<UpdateCache> {
    let path = cache_path()?;
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Save update info to cache.
fn save_cache(info: &UpdateInfo) -> Result<()> {
    let path = cache_path().context("No cache directory")?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let cache = UpdateCache {
        info: info.clone(),
        cached_at: Utc::now(),
    };

    let content = serde_json::to_string_pretty(&cache)?;
    fs::write(path, content)?;

    Ok(())
}

/// Clear the update check cache.
pub fn clear_cache() -> Result<()> {
    if let Some(path) = cache_path() {
        if path.exists() {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_constant_exists() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn is_newer_version_basic() {
        assert!(is_newer_version("0.2.0", "0.1.0"));
        assert!(is_newer_version("1.0.0", "0.9.0"));
        assert!(is_newer_version("0.1.1", "0.1.0"));
    }

    #[test]
    fn is_newer_version_same() {
        assert!(!is_newer_version("0.1.0", "0.1.0"));
        assert!(!is_newer_version("1.0.0", "1.0.0"));
    }

    #[test]
    fn is_newer_version_older() {
        assert!(!is_newer_version("0.1.0", "0.2.0"));
        assert!(!is_newer_version("0.9.0", "1.0.0"));
    }

    #[test]
    fn update_info_creation() {
        let info = UpdateInfo {
            current: "0.1.0".to_string(),
            latest: "0.2.0".to_string(),
            update_available: true,
            release_url: Some("https://github.com/bivvy-dev/bivvy/releases".to_string()),
            checked_at: Utc::now(),
        };

        assert!(info.update_available);
        assert_eq!(info.current, "0.1.0");
        assert_eq!(info.latest, "0.2.0");
    }

    #[test]
    fn cache_path_is_valid() {
        let path = cache_path();
        // Should return a path on most systems
        if let Some(p) = path {
            assert!(p.ends_with("update_check.json"));
        }
    }

    #[test]
    fn is_newer_version_handles_prerelease() {
        // Function only considers first 3 components
        // 0.1.0.1 and 0.1.0 are considered equal
        assert!(!is_newer_version("0.1.0.1", "0.1.0"));
    }

    #[test]
    fn is_newer_version_major_bump() {
        assert!(is_newer_version("2.0.0", "1.9.9"));
        assert!(is_newer_version("10.0.0", "9.99.99"));
    }

    #[test]
    fn is_newer_version_minor_bump() {
        assert!(is_newer_version("1.2.0", "1.1.99"));
        assert!(!is_newer_version("1.1.0", "1.2.0"));
    }

    #[test]
    fn is_newer_version_patch_bump() {
        assert!(is_newer_version("1.0.5", "1.0.4"));
        assert!(!is_newer_version("1.0.4", "1.0.5"));
    }

    #[test]
    fn is_newer_version_with_single_component() {
        assert!(is_newer_version("2", "1"));
        assert!(!is_newer_version("1", "2"));
    }

    #[test]
    fn is_newer_version_with_two_components() {
        assert!(is_newer_version("1.1", "1.0"));
        assert!(!is_newer_version("1.0", "1.1"));
    }

    #[test]
    fn clear_cache_works() {
        // Clear cache should not panic even if file doesn't exist
        let result = clear_cache();
        assert!(result.is_ok());
    }

    #[test]
    fn update_info_serialization() {
        let info = UpdateInfo {
            current: "0.1.0".to_string(),
            latest: "0.2.0".to_string(),
            update_available: true,
            release_url: None,
            checked_at: Utc::now(),
        };

        let json = serde_json::to_string(&info).unwrap();
        let parsed: UpdateInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.current, info.current);
        assert_eq!(parsed.latest, info.latest);
        assert_eq!(parsed.update_available, info.update_available);
    }

    #[test]
    fn update_info_with_no_release_url() {
        let info = UpdateInfo {
            current: "0.1.0".to_string(),
            latest: "0.2.0".to_string(),
            update_available: false,
            release_url: None,
            checked_at: Utc::now(),
        };

        assert!(!info.update_available);
        assert!(info.release_url.is_none());
    }

    #[test]
    fn is_newer_version_empty_string() {
        // Edge case: empty strings
        // When latest is empty, it has no components, so can't be newer
        assert!(!is_newer_version("", "0.1.0"));
        // When current is empty, latest has more components, so is "newer"
        assert!(is_newer_version("0.1.0", ""));
    }

    #[test]
    fn is_newer_version_invalid_format() {
        // Edge case: non-numeric components are filtered out
        // "abc" parses to empty vec, so can't be newer than 0.1.0
        assert!(!is_newer_version("abc", "0.1.0"));
        // Current "abc" parses to empty, so 0.1.0 is newer (has more components)
        assert!(is_newer_version("0.1.0", "abc"));
    }
}
