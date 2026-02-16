//! Built-in environment detection.
//!
//! Detects CI, Docker, and Codespace environments by checking
//! well-known environment variables.

use std::collections::BTreeMap;

/// A detected environment from auto-detection.
#[derive(Debug, Clone, PartialEq)]
pub struct DetectedEnvironment {
    /// The environment name (e.g., "ci", "docker", "codespace").
    pub name: String,
    /// The environment variable that triggered detection.
    pub detected_via: String,
}

/// Built-in environment detector.
///
/// Checks well-known environment variables to determine if bivvy
/// is running in CI, Docker, or a Codespace.
///
/// # Example
///
/// ```
/// use bivvy::environment::BuiltinDetector;
///
/// let detector = BuiltinDetector::new();
/// let detected = detector.detect();
/// // Returns Some(DetectedEnvironment) if running in a known environment
/// ```
pub struct BuiltinDetector {
    /// Custom detection rules from config, checked before built-ins.
    /// BTreeMap for deterministic (alphabetical) ordering.
    custom_rules: BTreeMap<String, Vec<DetectRule>>,
}

/// A detection rule: check if an env var is set (optionally to a specific value).
#[derive(Debug, Clone)]
pub struct DetectRule {
    /// The environment variable to check.
    pub env: String,
    /// If set, the variable must equal this value. If None, just checks presence.
    pub value: Option<String>,
}

impl BuiltinDetector {
    /// Create a new detector with only built-in rules.
    pub fn new() -> Self {
        Self {
            custom_rules: BTreeMap::new(),
        }
    }

    /// Add custom detection rules from config.
    ///
    /// Custom rules are checked before built-in rules. The BTreeMap
    /// ensures deterministic ordering when multiple custom environments
    /// could match.
    pub fn with_custom_rules(mut self, rules: BTreeMap<String, Vec<DetectRule>>) -> Self {
        self.custom_rules = rules;
        self
    }

    /// Detect the current environment.
    ///
    /// Returns the first matching environment, checking in order:
    /// 1. Custom rules (alphabetical by environment name)
    /// 2. CI (broadest, most commonly needed classification)
    /// 3. Codespace
    /// 4. Docker
    pub fn detect(&self) -> Option<DetectedEnvironment> {
        self.detect_with_env(|key| std::env::var(key))
    }

    /// Detect with a custom env var lookup (for testing).
    pub fn detect_with_env<F>(&self, env_fn: F) -> Option<DetectedEnvironment>
    where
        F: Fn(&str) -> Result<String, std::env::VarError>,
    {
        // 1. Custom rules first (BTreeMap = alphabetical order)
        // Collect all matching custom environments to warn about ambiguity.
        let matching_custom: Vec<(&String, &str)> = self
            .custom_rules
            .iter()
            .filter_map(|(env_name, rules)| {
                rules
                    .iter()
                    .find(|rule| self.matches_rule(rule, &env_fn))
                    .map(|rule| (env_name, rule.env.as_str()))
            })
            .collect();

        if matching_custom.len() > 1 {
            let names: Vec<&str> = matching_custom.iter().map(|(n, _)| n.as_str()).collect();
            eprintln!(
                "Warning: Multiple custom environments detected: {}. Using '{}' (alphabetically first).",
                names.join(", "),
                names[0],
            );
        }

        if let Some((env_name, detected_via)) = matching_custom.first() {
            return Some(DetectedEnvironment {
                name: (*env_name).clone(),
                detected_via: detected_via.to_string(),
            });
        }

        // 2. CI (broadest, most commonly needed classification --
        //    a Codespace running CI should be classified as CI)
        let ci_vars = [
            "CI",
            "GITHUB_ACTIONS",
            "GITLAB_CI",
            "CIRCLECI",
            "JENKINS_URL",
            "BUILDKITE",
            "TRAVIS",
        ];
        for var in &ci_vars {
            if env_fn(var).is_ok() {
                return Some(DetectedEnvironment {
                    name: "ci".to_string(),
                    detected_via: var.to_string(),
                });
            }
        }

        // TF_BUILD must equal "True" (Azure DevOps)
        if env_fn("TF_BUILD").as_deref() == Ok("True") {
            return Some(DetectedEnvironment {
                name: "ci".to_string(),
                detected_via: "TF_BUILD".to_string(),
            });
        }

        // 3. Codespace
        if env_fn("CODESPACES").is_ok() {
            return Some(DetectedEnvironment {
                name: "codespace".to_string(),
                detected_via: "CODESPACES".to_string(),
            });
        }
        if env_fn("GITPOD_WORKSPACE_ID").is_ok() {
            return Some(DetectedEnvironment {
                name: "codespace".to_string(),
                detected_via: "GITPOD_WORKSPACE_ID".to_string(),
            });
        }

        // 4. Docker
        if env_fn("DOCKER_CONTAINER").is_ok() {
            return Some(DetectedEnvironment {
                name: "docker".to_string(),
                detected_via: "DOCKER_CONTAINER".to_string(),
            });
        }
        // Fallback: check for /.dockerenv file (not unit-testable outside Docker)
        if std::path::Path::new("/.dockerenv").exists() {
            return Some(DetectedEnvironment {
                name: "docker".to_string(),
                detected_via: "/.dockerenv".to_string(),
            });
        }

        None
    }

    /// Check if a single detection rule matches.
    fn matches_rule<F>(&self, rule: &DetectRule, env_fn: &F) -> bool
    where
        F: Fn(&str) -> Result<String, std::env::VarError>,
    {
        match env_fn(&rule.env) {
            Ok(actual) => {
                if let Some(ref expected) = rule.value {
                    &actual == expected
                } else {
                    true // Just check presence
                }
            }
            Err(_) => false,
        }
    }
}

impl Default for BuiltinDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_env(vars: &[(&str, &str)]) -> impl Fn(&str) -> Result<String, std::env::VarError> {
        let map: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |key: &str| map.get(key).cloned().ok_or(std::env::VarError::NotPresent)
    }

    #[test]
    fn detect_nothing_in_clean_env() {
        let detector = BuiltinDetector::new();
        let env_fn = make_env(&[]);
        assert!(detector.detect_with_env(env_fn).is_none());
    }

    #[test]
    fn detect_ci_from_ci_var() {
        let detector = BuiltinDetector::new();
        let env_fn = make_env(&[("CI", "true")]);
        let result = detector.detect_with_env(env_fn).unwrap();
        assert_eq!(result.name, "ci");
        assert_eq!(result.detected_via, "CI");
    }

    #[test]
    fn detect_ci_from_github_actions() {
        let detector = BuiltinDetector::new();
        let env_fn = make_env(&[("GITHUB_ACTIONS", "true")]);
        let result = detector.detect_with_env(env_fn).unwrap();
        assert_eq!(result.name, "ci");
        assert_eq!(result.detected_via, "GITHUB_ACTIONS");
    }

    #[test]
    fn detect_ci_from_gitlab_ci() {
        let detector = BuiltinDetector::new();
        let env_fn = make_env(&[("GITLAB_CI", "true")]);
        let result = detector.detect_with_env(env_fn).unwrap();
        assert_eq!(result.name, "ci");
        assert_eq!(result.detected_via, "GITLAB_CI");
    }

    #[test]
    fn detect_ci_from_circleci() {
        let detector = BuiltinDetector::new();
        let env_fn = make_env(&[("CIRCLECI", "true")]);
        let result = detector.detect_with_env(env_fn).unwrap();
        assert_eq!(result.name, "ci");
        assert_eq!(result.detected_via, "CIRCLECI");
    }

    #[test]
    fn detect_ci_from_jenkins() {
        let detector = BuiltinDetector::new();
        let env_fn = make_env(&[("JENKINS_URL", "http://ci.example.com")]);
        let result = detector.detect_with_env(env_fn).unwrap();
        assert_eq!(result.name, "ci");
        assert_eq!(result.detected_via, "JENKINS_URL");
    }

    #[test]
    fn detect_codespace() {
        let detector = BuiltinDetector::new();
        let env_fn = make_env(&[("CODESPACES", "true")]);
        let result = detector.detect_with_env(env_fn).unwrap();
        assert_eq!(result.name, "codespace");
        assert_eq!(result.detected_via, "CODESPACES");
    }

    #[test]
    fn detect_docker() {
        let detector = BuiltinDetector::new();
        let env_fn = make_env(&[("DOCKER_CONTAINER", "1")]);
        let result = detector.detect_with_env(env_fn).unwrap();
        assert_eq!(result.name, "docker");
        assert_eq!(result.detected_via, "DOCKER_CONTAINER");
    }

    #[test]
    fn ci_takes_priority_over_codespace() {
        let detector = BuiltinDetector::new();
        // Codespaces also sets CI — CI should win because it's the broadest classification
        let env_fn = make_env(&[("CODESPACES", "true"), ("CI", "true")]);
        let result = detector.detect_with_env(env_fn).unwrap();
        assert_eq!(result.name, "ci");
    }

    #[test]
    fn ci_takes_priority_over_docker() {
        let detector = BuiltinDetector::new();
        let env_fn = make_env(&[("CI", "true"), ("DOCKER_CONTAINER", "1")]);
        let result = detector.detect_with_env(env_fn).unwrap();
        assert_eq!(result.name, "ci");
    }

    #[test]
    fn codespace_detected_without_ci() {
        let detector = BuiltinDetector::new();
        // When only CODESPACES is set (no CI var), codespace is detected
        let env_fn = make_env(&[("CODESPACES", "true")]);
        let result = detector.detect_with_env(env_fn).unwrap();
        assert_eq!(result.name, "codespace");
    }

    #[test]
    fn custom_rules_take_priority_over_builtins() {
        let mut custom = BTreeMap::new();
        custom.insert(
            "staging".to_string(),
            vec![DetectRule {
                env: "DEPLOY_ENV".to_string(),
                value: Some("staging".to_string()),
            }],
        );
        let detector = BuiltinDetector::new().with_custom_rules(custom);
        let env_fn = make_env(&[("DEPLOY_ENV", "staging"), ("CI", "true")]);
        let result = detector.detect_with_env(env_fn).unwrap();
        assert_eq!(result.name, "staging");
        assert_eq!(result.detected_via, "DEPLOY_ENV");
    }

    #[test]
    fn custom_rule_value_must_match() {
        let mut custom = BTreeMap::new();
        custom.insert(
            "staging".to_string(),
            vec![DetectRule {
                env: "DEPLOY_ENV".to_string(),
                value: Some("staging".to_string()),
            }],
        );
        let detector = BuiltinDetector::new().with_custom_rules(custom);
        // Value doesn't match
        let env_fn = make_env(&[("DEPLOY_ENV", "production")]);
        assert!(detector.detect_with_env(env_fn).is_none());
    }

    #[test]
    fn custom_rule_presence_only() {
        let mut custom = BTreeMap::new();
        custom.insert(
            "preview".to_string(),
            vec![DetectRule {
                env: "PREVIEW".to_string(),
                value: None,
            }],
        );
        let detector = BuiltinDetector::new().with_custom_rules(custom);
        let env_fn = make_env(&[("PREVIEW", "anything")]);
        let result = detector.detect_with_env(env_fn).unwrap();
        assert_eq!(result.name, "preview");
    }

    #[test]
    fn custom_rules_alphabetical_ordering() {
        let mut custom = BTreeMap::new();
        custom.insert(
            "beta".to_string(),
            vec![DetectRule {
                env: "BETA".to_string(),
                value: None,
            }],
        );
        custom.insert(
            "alpha".to_string(),
            vec![DetectRule {
                env: "ALPHA".to_string(),
                value: None,
            }],
        );
        let detector = BuiltinDetector::new().with_custom_rules(custom);
        // Both match — "alpha" wins because BTreeMap orders alphabetically
        let env_fn = make_env(&[("ALPHA", "1"), ("BETA", "1")]);
        let result = detector.detect_with_env(env_fn).unwrap();
        assert_eq!(result.name, "alpha");
    }

    #[test]
    fn custom_rules_multiple_match_warns() {
        let mut custom = BTreeMap::new();
        custom.insert(
            "beta".to_string(),
            vec![DetectRule {
                env: "BETA".to_string(),
                value: None,
            }],
        );
        custom.insert(
            "alpha".to_string(),
            vec![DetectRule {
                env: "ALPHA".to_string(),
                value: None,
            }],
        );
        let detector = BuiltinDetector::new().with_custom_rules(custom);
        // Both match — should still pick "alpha" (alphabetically first)
        // and emit a warning (tested via stderr capture in integration tests)
        let env_fn = make_env(&[("ALPHA", "1"), ("BETA", "1")]);
        let result = detector.detect_with_env(env_fn).unwrap();
        assert_eq!(result.name, "alpha");
        assert_eq!(result.detected_via, "ALPHA");
    }

    #[test]
    fn custom_rule_multiple_rules_for_same_env() {
        let mut custom = BTreeMap::new();
        custom.insert(
            "staging".to_string(),
            vec![
                DetectRule {
                    env: "DEPLOY_ENV".to_string(),
                    value: Some("staging".to_string()),
                },
                DetectRule {
                    env: "STAGING".to_string(),
                    value: None,
                },
            ],
        );
        let detector = BuiltinDetector::new().with_custom_rules(custom);
        // Second rule matches
        let env_fn = make_env(&[("STAGING", "1")]);
        let result = detector.detect_with_env(env_fn).unwrap();
        assert_eq!(result.name, "staging");
        assert_eq!(result.detected_via, "STAGING");
    }

    #[test]
    fn default_creates_detector() {
        let _detector = BuiltinDetector::default();
    }

    #[test]
    fn detect_ci_from_buildkite() {
        let detector = BuiltinDetector::new();
        let env_fn = make_env(&[("BUILDKITE", "true")]);
        let result = detector.detect_with_env(env_fn).unwrap();
        assert_eq!(result.name, "ci");
        assert_eq!(result.detected_via, "BUILDKITE");
    }

    #[test]
    fn detect_ci_from_travis() {
        let detector = BuiltinDetector::new();
        let env_fn = make_env(&[("TRAVIS", "true")]);
        let result = detector.detect_with_env(env_fn).unwrap();
        assert_eq!(result.name, "ci");
        assert_eq!(result.detected_via, "TRAVIS");
    }

    #[test]
    fn detect_ci_from_tf_build() {
        let detector = BuiltinDetector::new();
        let env_fn = make_env(&[("TF_BUILD", "True")]);
        let result = detector.detect_with_env(env_fn).unwrap();
        assert_eq!(result.name, "ci");
        assert_eq!(result.detected_via, "TF_BUILD");
    }

    #[test]
    fn detect_ci_tf_build_wrong_value() {
        let detector = BuiltinDetector::new();
        // TF_BUILD must be "True", not just present
        let env_fn = make_env(&[("TF_BUILD", "false")]);
        assert!(detector.detect_with_env(env_fn).is_none());
    }

    #[test]
    fn detect_codespace_from_gitpod() {
        let detector = BuiltinDetector::new();
        let env_fn = make_env(&[("GITPOD_WORKSPACE_ID", "abc123")]);
        let result = detector.detect_with_env(env_fn).unwrap();
        assert_eq!(result.name, "codespace");
        assert_eq!(result.detected_via, "GITPOD_WORKSPACE_ID");
    }
}
