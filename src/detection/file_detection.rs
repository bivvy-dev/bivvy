//! File-based detection.
//!
//! This module will be fully implemented in M7-02.

use std::path::Path;

use super::types::{Detection, DetectionKind, DetectionResult};

/// Detects based on file existence.
pub struct FileDetector {
    name: String,
    files: Vec<String>,
    all_required: bool,
}

impl FileDetector {
    /// Create a detector that matches if any file exists.
    pub fn any(name: &str, files: Vec<String>) -> Self {
        Self {
            name: name.to_string(),
            files,
            all_required: false,
        }
    }

    /// Create a detector that matches only if all files exist.
    pub fn all(name: &str, files: Vec<String>) -> Self {
        Self {
            name: name.to_string(),
            files,
            all_required: true,
        }
    }
}

impl Detection for FileDetector {
    fn name(&self) -> &str {
        &self.name
    }

    fn detect(&self, project_root: &Path) -> DetectionResult {
        let found_files: Vec<_> = self
            .files
            .iter()
            .filter(|f| project_root.join(f).exists())
            .cloned()
            .collect();

        let detected = if self.all_required {
            found_files.len() == self.files.len()
        } else {
            !found_files.is_empty()
        };

        if detected {
            let confidence = found_files.len() as f32 / self.files.len() as f32;
            let mut result = DetectionResult::found(&self.name).with_confidence(confidence);

            for file in &found_files {
                result = result.with_detail(&format!("Found: {}", file));
            }

            if found_files.len() == 1 {
                result = result.with_kind(DetectionKind::FileExists(found_files[0].clone()));
            } else {
                result = result.with_kind(DetectionKind::Multiple(
                    found_files
                        .iter()
                        .map(|f| DetectionKind::FileExists(f.clone()))
                        .collect(),
                ));
            }

            result
        } else {
            DetectionResult::not_found(&self.name)
        }
    }
}

/// Check if a file exists relative to project root.
pub fn file_exists(project_root: &Path, file: &str) -> bool {
    project_root.join(file).exists()
}

/// Check if any of the files exist.
pub fn any_file_exists(project_root: &Path, files: &[&str]) -> Option<String> {
    files
        .iter()
        .find(|f| project_root.join(f).exists())
        .map(|f| f.to_string())
}

/// Check if all files exist.
pub fn all_files_exist(project_root: &Path, files: &[&str]) -> bool {
    files.iter().all(|f| project_root.join(f).exists())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn file_detector_any_found() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("Gemfile"), "").unwrap();

        let detector = FileDetector::any(
            "ruby",
            vec!["Gemfile".to_string(), "Gemfile.lock".to_string()],
        );

        let result = detector.detect(temp.path());
        assert!(result.detected);
        assert!(result.confidence > 0.0);
    }

    #[test]
    fn file_detector_any_not_found() {
        let temp = TempDir::new().unwrap();

        let detector = FileDetector::any("ruby", vec!["Gemfile".to_string()]);

        let result = detector.detect(temp.path());
        assert!(!result.detected);
    }

    #[test]
    fn file_detector_all_requires_all() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("package.json"), "").unwrap();

        let detector = FileDetector::all(
            "yarn",
            vec!["package.json".to_string(), "yarn.lock".to_string()],
        );

        let result = detector.detect(temp.path());
        assert!(!result.detected);
    }

    #[test]
    fn file_detector_all_with_all() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("package.json"), "").unwrap();
        fs::write(temp.path().join("yarn.lock"), "").unwrap();

        let detector = FileDetector::all(
            "yarn",
            vec!["package.json".to_string(), "yarn.lock".to_string()],
        );

        let result = detector.detect(temp.path());
        assert!(result.detected);
        assert_eq!(result.confidence, 1.0);
    }

    #[test]
    fn file_exists_helper() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("test.txt"), "").unwrap();

        assert!(file_exists(temp.path(), "test.txt"));
        assert!(!file_exists(temp.path(), "missing.txt"));
    }

    #[test]
    fn any_file_exists_helper() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("yarn.lock"), "").unwrap();

        let found = any_file_exists(temp.path(), &["package-lock.json", "yarn.lock"]);
        assert_eq!(found, Some("yarn.lock".to_string()));
    }

    #[test]
    fn all_files_exist_helper() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("a.txt"), "").unwrap();
        fs::write(temp.path().join("b.txt"), "").unwrap();

        assert!(all_files_exist(temp.path(), &["a.txt", "b.txt"]));
        assert!(!all_files_exist(temp.path(), &["a.txt", "c.txt"]));
    }
}
