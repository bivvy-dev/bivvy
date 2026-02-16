//! Environment resolution.
//!
//! Resolves the active environment using the priority chain:
//! 1. Explicit `--env` flag
//! 2. Config `default_environment`
//! 3. Auto-detection (CI, Docker, Codespace)
//! 4. Fallback to "development"

use super::detection::{BuiltinDetector, DetectedEnvironment};

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
}
