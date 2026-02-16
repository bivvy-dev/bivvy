//! Environment resolution.
//!
//! Resolves the active environment using the priority chain:
//! 1. Explicit `--env` flag
//! 2. Config `default_environment`
//! 3. Auto-detection (CI, Docker, Codespace)
//! 4. Fallback to "development"

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use super::detection::{BuiltinDetector, DetectRule, DetectedEnvironment};
use crate::config::schema::{BivvyConfig, Settings};

/// How the environment was determined.
#[derive(Debug, Clone, PartialEq)]
pub enum EnvironmentSource {
    /// Explicitly set via `--env` flag.
    Flag,
    /// Set via config `default_environment`.
    ConfigDefault,
    /// Auto-detected from an environment variable.
    AutoDetected(String),
    /// Fallback to "development".
    Fallback,
}

impl std::fmt::Display for EnvironmentSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Flag => write!(f, "--env flag"),
            Self::ConfigDefault => write!(f, "config default"),
            Self::AutoDetected(var) => write!(f, "detected via {}", var),
            Self::Fallback => write!(f, "default"),
        }
    }
}

/// A resolved environment with its name and how it was determined.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedEnvironment {
    /// The environment name (e.g., "development", "ci", "staging").
    pub name: String,
    /// How this environment was determined.
    pub source: EnvironmentSource,
}

impl ResolvedEnvironment {
    /// Resolve the environment using the priority chain.
    ///
    /// # Arguments
    ///
    /// * `flag` - Explicit `--env` flag value
    /// * `config_default` - Config `default_environment` value
    /// * `detector` - Built-in environment detector
    ///
    /// # Example
    ///
    /// ```
    /// use bivvy::environment::{BuiltinDetector, ResolvedEnvironment, EnvironmentSource};
    ///
    /// // With explicit flag
    /// let detector = BuiltinDetector::new();
    /// let resolved = ResolvedEnvironment::resolve(
    ///     Some("staging"),
    ///     None,
    ///     &detector,
    /// );
    /// assert_eq!(resolved.name, "staging");
    /// assert_eq!(resolved.source, EnvironmentSource::Flag);
    /// ```
    pub fn resolve(
        flag: Option<&str>,
        config_default: Option<&str>,
        detector: &BuiltinDetector,
    ) -> Self {
        Self::resolve_with_detection(flag, config_default, detector.detect())
    }

    /// Resolve with a pre-computed detection result (for testing).
    pub fn resolve_with_detection(
        flag: Option<&str>,
        config_default: Option<&str>,
        detected: Option<DetectedEnvironment>,
    ) -> Self {
        // 1. Explicit --env flag
        if let Some(name) = flag {
            return Self {
                name: name.to_string(),
                source: EnvironmentSource::Flag,
            };
        }

        // 2. Config default_environment
        if let Some(name) = config_default {
            return Self {
                name: name.to_string(),
                source: EnvironmentSource::ConfigDefault,
            };
        }

        // 3. Auto-detection
        if let Some(env) = detected {
            return Self {
                name: env.name,
                source: EnvironmentSource::AutoDetected(env.detected_via),
            };
        }

        // 4. Fallback
        Self {
            name: "development".to_string(),
            source: EnvironmentSource::Fallback,
        }
    }

    /// Resolve the active environment from CLI args and config settings.
    ///
    /// This is a convenience method that builds custom detection rules from
    /// the config's `settings.environments` entries and delegates to `resolve()`.
    ///
    /// Priority: explicit flag > config default > auto-detection > fallback.
    pub fn resolve_from_config(flag: Option<&str>, settings: &Settings) -> Self {
        let custom_rules: BTreeMap<String, Vec<DetectRule>> = settings
            .environments
            .iter()
            .filter(|(_, env_config)| !env_config.detect.is_empty())
            .map(|(name, env_config)| {
                let rules = env_config
                    .detect
                    .iter()
                    .map(|d| DetectRule {
                        env: d.env.clone(),
                        value: d.value.clone(),
                    })
                    .collect();
                (name.clone(), rules)
            })
            .collect();

        let detector = BuiltinDetector::new().with_custom_rules(custom_rules);
        Self::resolve(flag, settings.default_environment.as_deref(), &detector)
    }

    /// Check whether the resolved environment is known (defined in config or a built-in).
    ///
    /// An environment is considered known if it appears in:
    /// - Built-in environments: "ci", "docker", "codespace", "development"
    /// - Custom environments defined in `settings.environments`
    /// - Environments referenced in steps' `environments` keys or `only_environments` values
    pub fn is_known(&self, config: &BivvyConfig) -> bool {
        known_environments(config).contains(&self.name)
    }
}

/// Returns all known environment names from builtins, config settings, and step references.
///
/// This includes:
/// 1. Built-in environments: "ci", "docker", "codespace", "development"
/// 2. Custom environments defined in `settings.environments`
/// 3. Environments referenced in steps' `environments` keys and `only_environments` values
///
/// # Example
///
/// ```
/// use bivvy::config::schema::BivvyConfig;
/// use bivvy::environment::resolver::known_environments;
///
/// let config: BivvyConfig = serde_yaml::from_str(r#"
/// settings:
///   environments:
///     staging:
///       detect:
///         - env: STAGING
/// steps:
///   deploy:
///     command: "deploy.sh"
///     only_environments:
///       - production
/// "#).unwrap();
///
/// let envs = known_environments(&config);
/// assert!(envs.contains(&"ci".to_string()));
/// assert!(envs.contains(&"staging".to_string()));
/// assert!(envs.contains(&"production".to_string()));
/// ```
pub fn known_environments(config: &BivvyConfig) -> Vec<String> {
    let mut envs = BTreeSet::new();

    // 1. Built-in environments
    for builtin in &["ci", "docker", "codespace", "development"] {
        envs.insert(builtin.to_string());
    }

    // 2. Custom environments from settings.environments
    for name in config.settings.environments.keys() {
        envs.insert(name.clone());
    }

    // 3. Environments referenced in steps
    for step in config.steps.values() {
        // Keys in step.environments (per-environment overrides)
        for env_name in step.environments.keys() {
            envs.insert(env_name.clone());
        }
        // Values in step.only_environments
        for env_name in &step.only_environments {
            envs.insert(env_name.clone());
        }
    }

    envs.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_takes_highest_priority() {
        let detected = Some(DetectedEnvironment {
            name: "ci".to_string(),
            detected_via: "CI".to_string(),
        });
        let resolved = ResolvedEnvironment::resolve_with_detection(
            Some("staging"),
            Some("production"),
            detected,
        );
        assert_eq!(resolved.name, "staging");
        assert_eq!(resolved.source, EnvironmentSource::Flag);
    }

    #[test]
    fn config_default_second_priority() {
        let detected = Some(DetectedEnvironment {
            name: "ci".to_string(),
            detected_via: "CI".to_string(),
        });
        let resolved =
            ResolvedEnvironment::resolve_with_detection(None, Some("production"), detected);
        assert_eq!(resolved.name, "production");
        assert_eq!(resolved.source, EnvironmentSource::ConfigDefault);
    }

    #[test]
    fn auto_detection_third_priority() {
        let detected = Some(DetectedEnvironment {
            name: "ci".to_string(),
            detected_via: "GITHUB_ACTIONS".to_string(),
        });
        let resolved = ResolvedEnvironment::resolve_with_detection(None, None, detected);
        assert_eq!(resolved.name, "ci");
        assert_eq!(
            resolved.source,
            EnvironmentSource::AutoDetected("GITHUB_ACTIONS".to_string())
        );
    }

    #[test]
    fn fallback_to_development() {
        let resolved = ResolvedEnvironment::resolve_with_detection(None, None, None);
        assert_eq!(resolved.name, "development");
        assert_eq!(resolved.source, EnvironmentSource::Fallback);
    }

    #[test]
    fn flag_with_no_other_sources() {
        let resolved = ResolvedEnvironment::resolve_with_detection(Some("test"), None, None);
        assert_eq!(resolved.name, "test");
        assert_eq!(resolved.source, EnvironmentSource::Flag);
    }

    #[test]
    fn config_default_with_no_detection() {
        let resolved = ResolvedEnvironment::resolve_with_detection(None, Some("staging"), None);
        assert_eq!(resolved.name, "staging");
        assert_eq!(resolved.source, EnvironmentSource::ConfigDefault);
    }

    #[test]
    fn resolve_with_real_detector() {
        let detector = BuiltinDetector::new();
        // In test environment, this should either detect CI or fall back
        let resolved = ResolvedEnvironment::resolve(Some("test"), None, &detector);
        assert_eq!(resolved.name, "test");
        assert_eq!(resolved.source, EnvironmentSource::Flag);
    }

    #[test]
    fn source_display_flag() {
        assert_eq!(EnvironmentSource::Flag.to_string(), "--env flag");
    }

    #[test]
    fn source_display_config_default() {
        assert_eq!(
            EnvironmentSource::ConfigDefault.to_string(),
            "config default"
        );
    }

    #[test]
    fn source_display_auto_detected() {
        assert_eq!(
            EnvironmentSource::AutoDetected("CI".to_string()).to_string(),
            "detected via CI"
        );
    }

    #[test]
    fn source_display_fallback() {
        assert_eq!(EnvironmentSource::Fallback.to_string(), "default");
    }

    #[test]
    fn resolve_from_config_uses_flag() {
        let settings = Settings::default();
        let resolved = ResolvedEnvironment::resolve_from_config(Some("staging"), &settings);
        assert_eq!(resolved.name, "staging");
        assert_eq!(resolved.source, EnvironmentSource::Flag);
    }

    #[test]
    fn resolve_custom_rule_detected() {
        use crate::environment::detection::DetectedEnvironment;

        // Simulate a custom rule firing: resolve_with_detection lets us inject
        // a pre-computed detection result without depending on real env vars.
        let detected = Some(DetectedEnvironment {
            name: "staging".to_string(),
            detected_via: "DEPLOY_ENV".to_string(),
        });

        let resolved = ResolvedEnvironment::resolve_with_detection(None, None, detected);
        assert_eq!(resolved.name, "staging");
        assert_eq!(
            resolved.source,
            EnvironmentSource::AutoDetected("DEPLOY_ENV".to_string())
        );
    }

    #[test]
    fn resolve_custom_rule_via_detector() {
        use crate::environment::detection::DetectRule;

        // Build a detector with a custom rule and verify it fires via
        // detect_with_env so we control the env var lookup.
        let mut custom_rules = BTreeMap::new();
        custom_rules.insert(
            "custom_ci".to_string(),
            vec![DetectRule {
                env: "MY_CI".to_string(),
                value: None,
            }],
        );

        let detector = BuiltinDetector::new().with_custom_rules(custom_rules);
        let detected = detector
            .detect_with_env(|key| match key {
                "MY_CI" => Ok("1".to_string()),
                _ => Err(std::env::VarError::NotPresent),
            })
            .unwrap();

        assert_eq!(detected.name, "custom_ci");
        assert_eq!(detected.detected_via, "MY_CI");
    }

    #[test]
    fn resolve_custom_rule_not_present_falls_back() {
        use crate::environment::detection::DetectRule;

        // When the custom rule's env var is NOT set, detection should not
        // match — verify the full chain falls through to development.
        let mut custom_rules = BTreeMap::new();
        custom_rules.insert(
            "custom_ci".to_string(),
            vec![DetectRule {
                env: "MY_CI".to_string(),
                value: None,
            }],
        );

        let detector = BuiltinDetector::new().with_custom_rules(custom_rules);
        let detected = detector.detect_with_env(|_| Err(std::env::VarError::NotPresent));

        // No env vars set at all, so nothing matches
        assert!(detected.is_none());

        // When nothing is detected, resolve falls back to development
        let resolved = ResolvedEnvironment::resolve_with_detection(None, None, None);
        assert_eq!(resolved.name, "development");
        assert_eq!(resolved.source, EnvironmentSource::Fallback);
    }

    #[test]
    fn is_known_builtin() {
        let config = BivvyConfig::default();
        for name in &["ci", "docker", "codespace", "development"] {
            let resolved = ResolvedEnvironment {
                name: name.to_string(),
                source: EnvironmentSource::Flag,
            };
            assert!(resolved.is_known(&config), "{} should be known", name);
        }
    }

    #[test]
    fn is_known_custom() {
        use crate::config::schema::EnvironmentConfig;
        use std::collections::HashMap;

        let mut environments = HashMap::new();
        environments.insert("staging".to_string(), EnvironmentConfig::default());

        let config = BivvyConfig {
            settings: Settings {
                environments,
                ..Default::default()
            },
            ..Default::default()
        };

        let resolved = ResolvedEnvironment {
            name: "staging".to_string(),
            source: EnvironmentSource::Flag,
        };
        assert!(resolved.is_known(&config));
    }

    #[test]
    fn is_known_unknown() {
        let config = BivvyConfig::default();
        let resolved = ResolvedEnvironment {
            name: "foo".to_string(),
            source: EnvironmentSource::Flag,
        };
        assert!(!resolved.is_known(&config));
    }

    #[test]
    fn is_known_from_step_environments_key() {
        use crate::config::schema::{StepConfig, StepEnvironmentOverride};
        use std::collections::HashMap;

        let mut step_envs = HashMap::new();
        step_envs.insert("preview".to_string(), StepEnvironmentOverride::default());

        let mut steps = HashMap::new();
        steps.insert(
            "deploy".to_string(),
            StepConfig {
                environments: step_envs,
                ..Default::default()
            },
        );

        let config = BivvyConfig {
            steps,
            ..Default::default()
        };

        let resolved = ResolvedEnvironment {
            name: "preview".to_string(),
            source: EnvironmentSource::Flag,
        };
        assert!(resolved.is_known(&config));
    }

    #[test]
    fn is_known_from_step_only_environments() {
        use crate::config::schema::StepConfig;
        use std::collections::HashMap;

        let mut steps = HashMap::new();
        steps.insert(
            "seeds".to_string(),
            StepConfig {
                only_environments: vec!["production".to_string()],
                ..Default::default()
            },
        );

        let config = BivvyConfig {
            steps,
            ..Default::default()
        };

        let resolved = ResolvedEnvironment {
            name: "production".to_string(),
            source: EnvironmentSource::Flag,
        };
        assert!(resolved.is_known(&config));
    }

    #[test]
    fn known_environments_includes_builtins() {
        let config = BivvyConfig::default();
        let envs = known_environments(&config);
        assert!(envs.contains(&"ci".to_string()));
        assert!(envs.contains(&"docker".to_string()));
        assert!(envs.contains(&"codespace".to_string()));
        assert!(envs.contains(&"development".to_string()));
    }

    #[test]
    fn known_environments_includes_custom() {
        use crate::config::schema::EnvironmentConfig;
        use std::collections::HashMap;

        let mut environments = HashMap::new();
        environments.insert("staging".to_string(), EnvironmentConfig::default());
        environments.insert("preview".to_string(), EnvironmentConfig::default());

        let config = BivvyConfig {
            settings: Settings {
                environments,
                ..Default::default()
            },
            ..Default::default()
        };

        let envs = known_environments(&config);
        assert!(envs.contains(&"staging".to_string()));
        assert!(envs.contains(&"preview".to_string()));
    }

    #[test]
    fn known_environments_includes_step_references() {
        use crate::config::schema::{StepConfig, StepEnvironmentOverride};
        use std::collections::HashMap;

        let mut step_envs = HashMap::new();
        step_envs.insert("canary".to_string(), StepEnvironmentOverride::default());

        let mut steps = HashMap::new();
        steps.insert(
            "deploy".to_string(),
            StepConfig {
                environments: step_envs,
                only_environments: vec!["production".to_string(), "staging".to_string()],
                ..Default::default()
            },
        );

        let config = BivvyConfig {
            steps,
            ..Default::default()
        };

        let envs = known_environments(&config);
        assert!(envs.contains(&"canary".to_string()));
        assert!(envs.contains(&"production".to_string()));
        assert!(envs.contains(&"staging".to_string()));
        // Builtins still present
        assert!(envs.contains(&"ci".to_string()));
    }

    #[test]
    fn known_environments_deduplicates() {
        use crate::config::schema::{EnvironmentConfig, StepConfig};
        use std::collections::HashMap;

        let mut environments = HashMap::new();
        environments.insert("staging".to_string(), EnvironmentConfig::default());

        let mut steps = HashMap::new();
        steps.insert(
            "deploy".to_string(),
            StepConfig {
                only_environments: vec!["staging".to_string(), "ci".to_string()],
                ..Default::default()
            },
        );

        let config = BivvyConfig {
            settings: Settings {
                environments,
                ..Default::default()
            },
            steps,
            ..Default::default()
        };

        let envs = known_environments(&config);
        // "staging" and "ci" should appear exactly once each
        assert_eq!(envs.iter().filter(|e| *e == "staging").count(), 1);
        assert_eq!(envs.iter().filter(|e| *e == "ci").count(), 1);
    }

    #[test]
    fn known_environments_sorted() {
        let config = BivvyConfig::default();
        let envs = known_environments(&config);
        // BTreeSet yields sorted output
        let mut sorted = envs.clone();
        sorted.sort();
        assert_eq!(envs, sorted);
    }
}
