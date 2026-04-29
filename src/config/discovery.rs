//! Lightweight discovery of bivvy config files.
//!
//! This module answers "what files exist?" and parses minimal headers.
//! It never performs deep merge or full schema parsing — those live in
//! [`crate::config::loader`]. Discovery is the cheap front end that lets
//! commands like `bivvy list` and `bivvy lint <name>` operate on a single
//! file without paying the merge tax.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{BivvyError, Result};

/// Lightweight discovery of bivvy config files for a project.
#[derive(Debug, Clone)]
pub struct Discovery {
    project_root: PathBuf,
}

impl Discovery {
    /// Create a new discovery rooted at the given project directory.
    pub fn new(project_root: &Path) -> Self {
        Self {
            project_root: project_root.to_path_buf(),
        }
    }

    /// Path to `.bivvy/config.yml` if it exists.
    pub fn project_config_path(&self) -> Option<PathBuf> {
        let path = self.project_root.join(".bivvy").join("config.yml");
        path.exists().then_some(path)
    }

    /// Path to `.bivvy/config.local.yml` if it exists.
    pub fn local_config_path(&self) -> Option<PathBuf> {
        let path = self.project_root.join(".bivvy").join("config.local.yml");
        path.exists().then_some(path)
    }

    /// All YAML files in `.bivvy/workflows/`, sorted.
    pub fn workflow_files(&self) -> Vec<PathBuf> {
        Self::yaml_files(&self.project_root.join(".bivvy").join("workflows"))
    }

    /// All YAML files in `.bivvy/steps/`, sorted.
    pub fn step_files(&self) -> Vec<PathBuf> {
        Self::yaml_files(&self.project_root.join(".bivvy").join("steps"))
    }

    /// Filename stems of every workflow file in `.bivvy/workflows/`, sorted.
    pub fn workflow_names(&self) -> Vec<String> {
        Self::file_stems(&self.workflow_files())
    }

    /// Filename stems of every step file in `.bivvy/steps/`, sorted.
    pub fn step_file_names(&self) -> Vec<String> {
        Self::file_stems(&self.step_files())
    }

    /// Return the path to `.bivvy/workflows/<name>.yml` if it exists.
    pub fn workflow_path(&self, name: &str) -> Option<PathBuf> {
        let primary = self
            .project_root
            .join(".bivvy")
            .join("workflows")
            .join(format!("{name}.yml"));
        if primary.exists() {
            return Some(primary);
        }
        let alt = self
            .project_root
            .join(".bivvy")
            .join("workflows")
            .join(format!("{name}.yaml"));
        alt.exists().then_some(alt)
    }

    /// Return the path to `.bivvy/steps/<name>.yml` if it exists.
    pub fn step_path(&self, name: &str) -> Option<PathBuf> {
        let primary = self
            .project_root
            .join(".bivvy")
            .join("steps")
            .join(format!("{name}.yml"));
        if primary.exists() {
            return Some(primary);
        }
        let alt = self
            .project_root
            .join(".bivvy")
            .join("steps")
            .join(format!("{name}.yaml"));
        alt.exists().then_some(alt)
    }

    /// Light parse: extracts the description and step list from a workflow
    /// file without deserializing the full schema. Works with both new
    /// (`WorkflowFile`) and legacy (`WorkflowConfig`) shapes.
    pub fn workflow_header(&self, name: &str) -> Result<WorkflowHeader> {
        let path = self
            .workflow_path(name)
            .ok_or_else(|| BivvyError::ConfigNotFound {
                path: self
                    .project_root
                    .join(".bivvy")
                    .join("workflows")
                    .join(format!("{name}.yml")),
            })?;
        let content = fs::read_to_string(&path).map_err(BivvyError::Io)?;
        let value: serde_yaml::Value =
            serde_yaml::from_str(&content).map_err(|e| BivvyError::ConfigParseError {
                path: path.clone(),
                message: e.to_string(),
            })?;

        let description = value
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // New format: workflow.steps. Legacy format: top-level steps.
        let step_names = if let Some(seq) = value
            .get("workflow")
            .and_then(|w| w.get("steps"))
            .and_then(|s| s.as_sequence())
        {
            seq.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        } else if let Some(seq) = value.get("steps").and_then(|s| s.as_sequence()) {
            seq.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        } else {
            Vec::new()
        };

        Ok(WorkflowHeader {
            name: name.to_string(),
            description,
            step_names,
        })
    }

    fn yaml_files(dir: &Path) -> Vec<PathBuf> {
        if !dir.is_dir() {
            return Vec::new();
        }
        let mut files: Vec<PathBuf> = fs::read_dir(dir)
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|ext| ext == "yml" || ext == "yaml")
            })
            .collect();
        files.sort();
        files
    }

    fn file_stems(paths: &[PathBuf]) -> Vec<String> {
        paths
            .iter()
            .filter_map(|p| {
                p.file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
            })
            .collect()
    }
}

/// Lightweight header info for a workflow file.
#[derive(Debug, Clone)]
pub struct WorkflowHeader {
    /// Workflow name (filename stem).
    pub name: String,

    /// Human-readable description (`description:` in either format).
    pub description: Option<String>,

    /// Step names referenced by the workflow's ordering list.
    pub step_names: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup(workflows: &[(&str, &str)], steps: &[(&str, &str)]) -> TempDir {
        let temp = TempDir::new().unwrap();
        let bivvy = temp.path().join(".bivvy");
        fs::create_dir_all(&bivvy).unwrap();
        if !workflows.is_empty() {
            let dir = bivvy.join("workflows");
            fs::create_dir_all(&dir).unwrap();
            for (name, content) in workflows {
                fs::write(dir.join(format!("{name}.yml")), content).unwrap();
            }
        }
        if !steps.is_empty() {
            let dir = bivvy.join("steps");
            fs::create_dir_all(&dir).unwrap();
            for (name, content) in steps {
                fs::write(dir.join(format!("{name}.yml")), content).unwrap();
            }
        }
        temp
    }

    #[test]
    fn workflow_files_are_sorted() {
        let temp = setup(
            &[
                ("zebra", "steps: []"),
                ("alpha", "steps: []"),
                ("middle", "steps: []"),
            ],
            &[],
        );
        let d = Discovery::new(temp.path());
        let names = d.workflow_names();
        assert_eq!(names, vec!["alpha", "middle", "zebra"]);
    }

    #[test]
    fn workflow_files_ignores_non_yaml() {
        let temp = TempDir::new().unwrap();
        let dir = temp.path().join(".bivvy").join("workflows");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("ok.yml"), "steps: []").unwrap();
        fs::write(dir.join("README.md"), "# nope").unwrap();
        fs::write(dir.join(".gitkeep"), "").unwrap();

        let d = Discovery::new(temp.path());
        assert_eq!(d.workflow_names(), vec!["ok"]);
    }

    #[test]
    fn workflow_header_parses_legacy_format() {
        let temp = setup(
            &[(
                "ci",
                r#"
description: CI pipeline
steps:
  - install
  - test
"#,
            )],
            &[],
        );
        let d = Discovery::new(temp.path());
        let header = d.workflow_header("ci").unwrap();
        assert_eq!(header.description.as_deref(), Some("CI pipeline"));
        assert_eq!(header.step_names, vec!["install", "test"]);
    }

    #[test]
    fn workflow_header_parses_new_format() {
        let temp = setup(
            &[(
                "release",
                r#"
description: Release prep
steps:
  fetch:
    command: git fetch
workflow:
  steps:
    - fetch
"#,
            )],
            &[],
        );
        let d = Discovery::new(temp.path());
        let header = d.workflow_header("release").unwrap();
        assert_eq!(header.description.as_deref(), Some("Release prep"));
        assert_eq!(header.step_names, vec!["fetch"]);
    }

    #[test]
    fn workflow_header_errors_on_missing_file() {
        let temp = TempDir::new().unwrap();
        let d = Discovery::new(temp.path());
        let err = d.workflow_header("missing").unwrap_err();
        assert!(matches!(err, BivvyError::ConfigNotFound { .. }));
    }

    #[test]
    fn project_config_path_detects_existing_file() {
        let temp = TempDir::new().unwrap();
        let bivvy = temp.path().join(".bivvy");
        fs::create_dir_all(&bivvy).unwrap();
        fs::write(bivvy.join("config.yml"), "app_name: x").unwrap();

        let d = Discovery::new(temp.path());
        assert!(d.project_config_path().is_some());
        assert!(d.local_config_path().is_none());
    }

    #[test]
    fn step_files_discovered() {
        let temp = setup(&[], &[("deps", "command: yarn"), ("db", "command: rake")]);
        let d = Discovery::new(temp.path());
        assert_eq!(d.step_file_names(), vec!["db", "deps"]);
    }

    #[test]
    fn workflow_path_handles_yaml_extension() {
        let temp = TempDir::new().unwrap();
        let dir = temp.path().join(".bivvy").join("workflows");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("ci.yaml"), "steps: []").unwrap();

        let d = Discovery::new(temp.path());
        assert!(d.workflow_path("ci").is_some());
    }
}
