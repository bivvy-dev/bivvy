//! Stage 3: Classify error signal lines into categories.
//!
//! Scans error signal lines for category signals — words and phrases that
//! indicate what kind of failure occurred. Multiple categories can fire.
//! Each match adds a confidence delta, with diminishing returns for
//! repeated matches of the same category.

use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;

use super::segment::TaggedLine;
use super::DiagnosticDetails;

/// Error category taxonomy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCategory {
    NotFound,
    ConnectionRefused,
    VersionMismatch,
    SyncIssue,
    PermissionDenied,
    PortConflict,
    BuildFailure,
    ResourceLimit,
    AuthFailure,
    SystemConstraint,
}

/// A category match with confidence.
#[derive(Debug, Clone)]
struct RawCategoryMatch {
    category: ErrorCategory,
    confidence: f32,
    hit_count: u32,
}

// === Category signal patterns ===

static RE_NOT_FOUND: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(not found|could not find|can't find|does not exist|no such|missing|not installed|no module named|cannot find)").unwrap()
});

static RE_CONNECTION_REFUSED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(connection refused|cannot connect|could not connect|not running|server not available)").unwrap()
});

static RE_VERSION_MISMATCH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(version mismatch|incompatible|is not compatible|required version|expected version|engine.*incompatible)").unwrap()
});

/// Structural signal: two different version numbers on the same line
/// (e.g., "server version: 16.13; pg_dump version: 14.21").
static RE_VERSION_PAIR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"version[:\s]+(\d+\.\d+).*version[:\s]+(\d+\.\d+)").unwrap());

static RE_SYNC_ISSUE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(out of sync|inconsistent|needs to be updated|lock file|integrity check failed|checksum mismatch|not consistent)").unwrap()
});

static RE_PERMISSION_DENIED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(permission denied|access denied|not permitted|EACCES)").unwrap()
});

static RE_PORT_CONFLICT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(already allocated|already in use|address in use|bind failed|EADDRINUSE)")
        .unwrap()
});

static RE_BUILD_FAILURE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(failed to build|build failed|compilation failed|native extensions?|linker|extconf\.rb failed)").unwrap()
});

static RE_RESOURCE_LIMIT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(limit|ENOSPC|too many|exceeded|quota)\b").unwrap());

static RE_AUTH_FAILURE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(certificate|publickey|authentication failed|unauthorized|\b401\b|\b403\b)")
        .unwrap()
});

static RE_SYSTEM_CONSTRAINT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(externally[- ]managed|managed environment|system python)").unwrap()
});

// === Data extraction patterns ===

static RE_EXTRACT_MODULE_NAME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(?:no module named|cannot find module)\s+['"]?(\S+?)['"]?$"#).unwrap()
});

static RE_EXTRACT_NOT_FOUND_TARGET: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(?:not found|could not find|does not exist|no such|missing|not installed|cannot find)\s*:?\s*['"]?([^\s'",:]+)"#).unwrap()
});

static RE_EXTRACT_COMMAND_NOT_FOUND: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"command not found:\s*(\S+)").unwrap());

static RE_EXTRACT_DB_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"database "([^"]+)" does not exist"#).unwrap());

static RE_EXTRACT_ROLE_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"role "([^"]+)" does not exist"#).unwrap());

static RE_EXTRACT_PORT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:port\s+|:::)(\d{2,5})").unwrap());

static RE_EXTRACT_HOST: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?:host\s+["\']?)([^\s"',)]+)"#).unwrap());

static RE_EXTRACT_VERSIONS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:server version|version):\s*(\d+\.\d+(?:\.\d+)?).*?(?:pg_dump version|version):\s*(\d+\.\d+(?:\.\d+)?)").unwrap()
});

static RE_EXTRACT_VERSIONS_ALT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"version\s+(\d+\.\d+(?:\.\d+)?)\s+.*?version\s+(\d+\.\d+(?:\.\d+)?)").unwrap()
});

static RE_EXTRACT_PERMISSION_TARGET: LazyLock<Regex> = LazyLock::new(|| {
    // Match "bash: ./gradlew: Permission denied" or "Permission denied: /path"
    Regex::new(r"(?::\s*(\S+)\s*:\s*[Pp]ermission denied|[Pp]ermission denied[:\s]+([^\s(]+))")
        .unwrap()
});

static RE_EXTRACT_BUILD_TARGET: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(?:failed to build|installing)\s+(\S+)").unwrap());

struct CategoryMatcher {
    category: ErrorCategory,
    regex: &'static LazyLock<Regex>,
    base_delta: f32,
}

const CATEGORY_MATCHERS: &[CategoryMatcher] = &[
    CategoryMatcher {
        category: ErrorCategory::NotFound,
        regex: &RE_NOT_FOUND,
        base_delta: 0.3,
    },
    CategoryMatcher {
        category: ErrorCategory::ConnectionRefused,
        regex: &RE_CONNECTION_REFUSED,
        base_delta: 0.3,
    },
    CategoryMatcher {
        category: ErrorCategory::VersionMismatch,
        regex: &RE_VERSION_MISMATCH,
        base_delta: 0.3,
    },
    CategoryMatcher {
        category: ErrorCategory::SyncIssue,
        regex: &RE_SYNC_ISSUE,
        base_delta: 0.3,
    },
    CategoryMatcher {
        category: ErrorCategory::PermissionDenied,
        regex: &RE_PERMISSION_DENIED,
        base_delta: 0.3,
    },
    CategoryMatcher {
        category: ErrorCategory::PortConflict,
        regex: &RE_PORT_CONFLICT,
        base_delta: 0.3,
    },
    CategoryMatcher {
        category: ErrorCategory::BuildFailure,
        regex: &RE_BUILD_FAILURE,
        base_delta: 0.3,
    },
    CategoryMatcher {
        category: ErrorCategory::ResourceLimit,
        regex: &RE_RESOURCE_LIMIT,
        base_delta: 0.2,
    },
    CategoryMatcher {
        category: ErrorCategory::AuthFailure,
        regex: &RE_AUTH_FAILURE,
        base_delta: 0.3,
    },
    CategoryMatcher {
        category: ErrorCategory::SystemConstraint,
        regex: &RE_SYSTEM_CONSTRAINT,
        base_delta: 0.3,
    },
];

/// Classify error signal lines into categories with confidence scores.
///
/// Returns categories sorted by confidence (highest first) and extracted
/// diagnostic details.
pub fn classify(lines: &[TaggedLine]) -> (Vec<super::CategoryMatch>, DiagnosticDetails) {
    // Scan all lines, not just error signals — segmentation is advisory,
    // not exclusionary. Some error-relevant content (e.g., "connection refused")
    // may not be tagged as ErrorSignal by Stage 2.
    let error_lines: Vec<&str> = lines
        .iter()
        .filter(|l| !l.text.trim().is_empty())
        .map(|l| l.text.as_str())
        .collect();

    let mut category_scores: HashMap<ErrorCategory, RawCategoryMatch> = HashMap::new();
    let mut details = DiagnosticDetails::default();

    for line in &error_lines {
        for matcher in CATEGORY_MATCHERS {
            if matcher.regex.is_match(line) {
                let entry = category_scores
                    .entry(matcher.category)
                    .or_insert(RawCategoryMatch {
                        category: matcher.category,
                        confidence: 0.0,
                        hit_count: 0,
                    });

                // Diminishing returns: +0.3, +0.15, +0.1, ...
                let delta = match entry.hit_count {
                    0 => matcher.base_delta,
                    1 => matcher.base_delta * 0.5,
                    _ => matcher.base_delta * 0.33,
                };
                entry.confidence = (entry.confidence + delta).min(0.7);
                entry.hit_count += 1;
            }
        }

        // Supplemental: two different version numbers on the same line
        // is a strong structural signal for version mismatch.
        if let Some(caps) = RE_VERSION_PAIR.captures(line) {
            if caps.get(1).map(|m| m.as_str()) != caps.get(2).map(|m| m.as_str()) {
                let entry = category_scores
                    .entry(ErrorCategory::VersionMismatch)
                    .or_insert(RawCategoryMatch {
                        category: ErrorCategory::VersionMismatch,
                        confidence: 0.0,
                        hit_count: 0,
                    });
                let delta = match entry.hit_count {
                    0 => 0.3,
                    1 => 0.15,
                    _ => 0.1,
                };
                entry.confidence = (entry.confidence + delta).min(0.7);
                entry.hit_count += 1;
            }
        }

        // Extract structured details
        extract_details(line, &mut details);
    }

    // Also scan all lines for version info (it may span error + non-error lines)
    let all_text: String = lines
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    extract_version_details(&all_text, &mut details);

    let mut categories: Vec<super::CategoryMatch> = category_scores
        .into_values()
        .map(|raw| super::CategoryMatch {
            category: raw.category,
            confidence: raw.confidence,
        })
        .collect();

    categories.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    (categories, details)
}

fn extract_details(line: &str, details: &mut DiagnosticDetails) {
    // Extract module name (Python-style)
    if let Some(caps) = RE_EXTRACT_MODULE_NAME.captures(line) {
        if details.target.is_none() {
            details.target = Some(caps[1].to_string());
        }
    }

    // Extract command not found target
    if let Some(caps) = RE_EXTRACT_COMMAND_NOT_FOUND.captures(line) {
        if details.target.is_none() {
            details.target = Some(caps[1].to_string());
        }
    }

    // Extract database name
    if let Some(caps) = RE_EXTRACT_DB_NAME.captures(line) {
        if details.target.is_none() {
            details.target = Some(caps[1].to_string());
        }
    }

    // Extract role name
    if let Some(caps) = RE_EXTRACT_ROLE_NAME.captures(line) {
        if details.target.is_none() {
            details.target = Some(caps[1].to_string());
        }
    }

    // Extract generic not-found target (lower priority)
    if details.target.is_none() {
        if let Some(caps) = RE_EXTRACT_NOT_FOUND_TARGET.captures(line) {
            let target = caps[1].to_string();
            // Filter out very generic matches
            if target.len() > 1 && !target.contains('/') {
                details.target = Some(target);
            }
        }
    }

    // Extract port
    if let Some(caps) = RE_EXTRACT_PORT.captures(line) {
        if details.port.is_none() {
            if let Ok(port) = caps[1].parse::<u16>() {
                details.port = Some(port);
            }
        }
    }

    // Extract host
    if let Some(caps) = RE_EXTRACT_HOST.captures(line) {
        if details.host.is_none() {
            details.host = Some(caps[1].to_string());
        }
    }

    // Extract permission denied target
    if let Some(caps) = RE_EXTRACT_PERMISSION_TARGET.captures(line) {
        if details.target.is_none() {
            // Group 1: target before "Permission denied" (e.g., "bash: ./gradlew: Permission denied")
            // Group 2: target after "Permission denied" (e.g., "Permission denied: /path")
            let target = caps
                .get(1)
                .or_else(|| caps.get(2))
                .map(|m| m.as_str().to_string());
            if let Some(t) = target {
                details.target = Some(t);
            }
        }
    }

    // Extract build failure target
    if let Some(caps) = RE_EXTRACT_BUILD_TARGET.captures(line) {
        if details.target.is_none() {
            details.target = Some(caps[1].to_string());
        }
    }
}

fn extract_version_details(text: &str, details: &mut DiagnosticDetails) {
    // Try "server version: X; pg_dump version: Y" pattern
    if let Some(caps) = RE_EXTRACT_VERSIONS.captures(text) {
        details.version_need = Some(caps[1].to_string());
        details.version_have = Some(caps[2].to_string());
        return;
    }
    // Try generic "version X ... version Y" pattern
    if let Some(caps) = RE_EXTRACT_VERSIONS_ALT.captures(text) {
        details.version_have = Some(caps[1].to_string());
        details.version_need = Some(caps[2].to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::diagnostic::segment::segment;

    fn classify_text(text: &str) -> (Vec<super::super::CategoryMatch>, DiagnosticDetails) {
        let lines = segment(text);
        classify(&lines)
    }

    #[test]
    fn connection_refused_classified() {
        let (cats, _) = classify_text("could not connect to server: Connection refused");
        assert!(!cats.is_empty());
        assert_eq!(cats[0].category, ErrorCategory::ConnectionRefused);
        assert!(cats[0].confidence >= 0.3);
    }

    #[test]
    fn module_not_found_classified() {
        let (cats, details) = classify_text("ModuleNotFoundError: No module named 'dotenv'");
        assert!(!cats.is_empty());
        assert_eq!(cats[0].category, ErrorCategory::NotFound);
        assert_eq!(details.target.as_deref(), Some("dotenv"));
    }

    #[test]
    fn version_mismatch_classified() {
        let (cats, _) =
            classify_text("pg_dump: error: aborting because of server version mismatch");
        assert!(cats
            .iter()
            .any(|c| c.category == ErrorCategory::VersionMismatch));
    }

    #[test]
    fn build_failure_plus_not_found() {
        let text = "failed to build gem native extension\nCan't find the 'libpq-fe.h' header";
        let (cats, _) = classify_text(text);
        let categories: Vec<_> = cats.iter().map(|c| c.category).collect();
        assert!(categories.contains(&ErrorCategory::BuildFailure));
        assert!(categories.contains(&ErrorCategory::NotFound));
    }

    #[test]
    fn pg_dump_version_details_extracted() {
        let text = "pg_dump: error: server version: 16.13 (Homebrew); pg_dump version: 14.21 (Homebrew)\npg_dump: error: aborting because of server version mismatch";
        let (_, details) = classify_text(text);
        assert_eq!(details.version_need.as_deref(), Some("16.13"));
        assert_eq!(details.version_have.as_deref(), Some("14.21"));
    }

    #[test]
    fn port_conflict_classified() {
        let (cats, details) =
            classify_text("Error: listen EADDRINUSE: address already in use :::3000");
        assert!(!cats.is_empty());
        assert_eq!(cats[0].category, ErrorCategory::PortConflict);
        assert_eq!(details.port, Some(3000));
    }

    #[test]
    fn permission_denied_classified() {
        let (cats, details) = classify_text("bash: ./gradlew: Permission denied");
        assert!(!cats.is_empty());
        assert_eq!(cats[0].category, ErrorCategory::PermissionDenied);
        assert_eq!(details.target.as_deref(), Some("./gradlew"));
    }

    #[test]
    fn system_constraint_classified() {
        let (cats, _) = classify_text(
            "error: externally-managed-environment\n× This environment is externally managed",
        );
        assert!(cats
            .iter()
            .any(|c| c.category == ErrorCategory::SystemConstraint));
    }

    #[test]
    fn diminishing_returns() {
        let text = "error: not found\nerror: not found\nerror: not found";
        let (cats, _) = classify_text(text);
        let nf = cats
            .iter()
            .find(|c| c.category == ErrorCategory::NotFound)
            .unwrap();
        // 0.3 + 0.15 + 0.1 = 0.55, capped at 0.7
        assert!(nf.confidence > 0.3);
        assert!(nf.confidence <= 0.7);
    }

    #[test]
    fn category_confidence_capped_at_0_7() {
        let text = "not found\nnot found\nnot found\nnot found\nnot found\nnot found\nnot found\nnot found\nnot found\nnot found";
        let lines = segment(text);
        let (cats, _) = classify(&lines);
        if let Some(nf) = cats.iter().find(|c| c.category == ErrorCategory::NotFound) {
            assert!(nf.confidence <= 0.7);
        }
    }

    #[test]
    fn database_not_exist_extracts_name() {
        let (cats, details) = classify_text("FATAL:  database \"myapp_dev\" does not exist");
        assert!(cats.iter().any(|c| c.category == ErrorCategory::NotFound));
        assert_eq!(details.target.as_deref(), Some("myapp_dev"));
    }

    #[test]
    fn command_not_found_extracts_name() {
        let (cats, details) = classify_text("bash: command not found: jq");
        assert!(cats.iter().any(|c| c.category == ErrorCategory::NotFound));
        assert_eq!(details.target.as_deref(), Some("jq"));
    }

    #[test]
    fn sync_issue_classified() {
        let (cats, _) = classify_text("Warning: poetry.lock is not consistent with pyproject.toml");
        assert!(cats.iter().any(|c| c.category == ErrorCategory::SyncIssue));
    }

    #[test]
    fn auth_failure_classified() {
        let (cats, _) = classify_text("fatal: Permission denied (publickey)");
        assert!(cats
            .iter()
            .any(|c| c.category == ErrorCategory::AuthFailure));
    }

    #[test]
    fn resource_limit_classified() {
        let (cats, _) = classify_text("Error: ENOSPC: no space left on device");
        assert!(cats
            .iter()
            .any(|c| c.category == ErrorCategory::ResourceLimit));
    }

    #[test]
    fn resource_limit_from_limit_keyword() {
        let (cats, _) = classify_text("error: rate limit exceeded for API calls");
        assert!(cats
            .iter()
            .any(|c| c.category == ErrorCategory::ResourceLimit));
    }

    #[test]
    fn empty_input_no_categories() {
        let (cats, _) = classify_text("");
        assert!(cats.is_empty());
    }

    #[test]
    fn host_extraction() {
        let (_, details) = classify_text(
            "could not connect to server: Connection refused\nIs the server running on host \"localhost\" (::1) and accepting TCP/IP connections on port 5432?",
        );
        assert_eq!(details.host.as_deref(), Some("localhost"));
        assert_eq!(details.port, Some(5432));
    }
}
