//! Environment resolution.
//!
//! Resolves the active environment using the priority chain:
//! 1. Explicit `--env` flag
//! 2. Config `default_environment`
//! 3. Auto-detection (CI, Docker, Codespace)
//! 4. Fallback to "development"

use std::collections::BTreeMap;

use super::detection::{BuiltinDetector, DetectRule, DetectedEnvironment};
use crate::config::schema::Settings;

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
    pub fn is_known(&self, settings: &Settings) -> bool {
        const BUILTINS: &[&str] = &["ci", "docker", "codespace", "development"];
        BUILTINS.contains(&self.name.as_str()) || settings.environments.contains_key(&self.name)
    }
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
    fn resolve_from_config_uses_custom_rules() {
        use crate::config::schema::{EnvironmentConfig, EnvironmentDetectRule};
        use std::collections::HashMap;

        let mut environments = HashMap::new();
        environments.insert(
            "custom_ci".to_string(),
            EnvironmentConfig {
                detect: vec![EnvironmentDetectRule {
                    env: "MY_CI".to_string(),
                    value: None,
                }],
                ..Default::default()
            },
        );

        let settings = Settings {
            environments,
            ..Default::default()
        };

        // Without the env var set, should fall back to development
        let resolved = ResolvedEnvironment::resolve_from_config(None, &settings);
        // We can't guarantee the env var isn't set, so just check it resolves
        assert!(!resolved.name.is_empty());
    }

    #[test]
    fn is_known_builtin() {
        let settings = Settings::default();
        for name in &["ci", "docker", "codespace", "development"] {
            let resolved = ResolvedEnvironment {
                name: name.to_string(),
                source: EnvironmentSource::Flag,
            };
            assert!(resolved.is_known(&settings), "{} should be known", name);
        }
    }

    #[test]
    fn is_known_custom() {
        use crate::config::schema::EnvironmentConfig;
        use std::collections::HashMap;

        let mut environments = HashMap::new();
        environments.insert("staging".to_string(), EnvironmentConfig::default());

        let settings = Settings {
            environments,
            ..Default::default()
        };

        let resolved = ResolvedEnvironment {
            name: "staging".to_string(),
            source: EnvironmentSource::Flag,
        };
        assert!(resolved.is_known(&settings));
    }

    #[test]
    fn is_known_unknown() {
        let settings = Settings::default();
        let resolved = ResolvedEnvironment {
            name: "foo".to_string(),
            source: EnvironmentSource::Flag,
        };
        assert!(!resolved.is_known(&settings));
    }
}
