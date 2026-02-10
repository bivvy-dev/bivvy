//! Detection trait and result types.

use std::path::Path;

/// Trait for detection implementations.
pub trait Detection {
    /// The name of this detector.
    fn name(&self) -> &str;

    /// Check if this detection applies.
    fn detect(&self, project_root: &Path) -> DetectionResult;
}

/// Result of a detection check.
#[derive(Debug, Clone)]
pub struct DetectionResult {
    /// Name of what was detected.
    pub name: String,

    /// Whether the detection matched.
    pub detected: bool,

    /// Kind of detection that matched.
    pub kind: Option<DetectionKind>,

    /// Confidence level (0.0 - 1.0).
    pub confidence: f32,

    /// Additional details about what was detected.
    pub details: Vec<String>,

    /// Suggested template to use.
    pub suggested_template: Option<String>,
}

impl DetectionResult {
    /// Create a positive detection result.
    pub fn found(name: &str) -> Self {
        Self {
            name: name.to_string(),
            detected: true,
            kind: None,
            confidence: 1.0,
            details: Vec::new(),
            suggested_template: None,
        }
    }

    /// Create a negative detection result.
    pub fn not_found(name: &str) -> Self {
        Self {
            name: name.to_string(),
            detected: false,
            kind: None,
            confidence: 0.0,
            details: Vec::new(),
            suggested_template: None,
        }
    }

    /// Set the detection kind.
    pub fn with_kind(mut self, kind: DetectionKind) -> Self {
        self.kind = Some(kind);
        self
    }

    /// Set the confidence level.
    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence;
        self
    }

    /// Add a detail.
    pub fn with_detail(mut self, detail: &str) -> Self {
        self.details.push(detail.to_string());
        self
    }

    /// Set the suggested template.
    pub fn with_template(mut self, template: &str) -> Self {
        self.suggested_template = Some(template.to_string());
        self
    }
}

/// Kind of detection that matched.
#[derive(Debug, Clone, PartialEq)]
pub enum DetectionKind {
    /// File exists at path.
    FileExists(String),

    /// Command succeeded.
    CommandSucceeds(String),

    /// Multiple conditions matched.
    Multiple(Vec<DetectionKind>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection_result_found() {
        let result = DetectionResult::found("ruby");
        assert!(result.detected);
        assert_eq!(result.name, "ruby");
        assert_eq!(result.confidence, 1.0);
    }

    #[test]
    fn detection_result_not_found() {
        let result = DetectionResult::not_found("ruby");
        assert!(!result.detected);
        assert_eq!(result.confidence, 0.0);
    }

    #[test]
    fn detection_result_builder() {
        let result = DetectionResult::found("ruby")
            .with_kind(DetectionKind::FileExists("Gemfile".to_string()))
            .with_confidence(0.9)
            .with_detail("Gemfile found")
            .with_template("bundler");

        assert!(result.detected);
        assert_eq!(result.confidence, 0.9);
        assert_eq!(result.details, vec!["Gemfile found"]);
        assert_eq!(result.suggested_template, Some("bundler".to_string()));
    }

    #[test]
    fn detection_kind_multiple() {
        let kind = DetectionKind::Multiple(vec![
            DetectionKind::FileExists("Gemfile".to_string()),
            DetectionKind::FileExists("Gemfile.lock".to_string()),
        ]);

        assert!(matches!(kind, DetectionKind::Multiple(_)));
    }
}
