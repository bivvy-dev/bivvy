//! Detector registry for shared, cacheable detection checks.
//!
//! Detectors are named, structured check definitions with addressable facets.
//! They are defined once in `templates/detectors.yml` and referenced by
//! templates via dot notation (e.g., `rails.commands.rails_version` or
//! `node.file`).
//!
//! # Reference Syntax
//!
//! - `rails` — bare name: all commands pass AND any file matches
//! - `rails.commands` — run all commands (AND semantics)
//! - `rails.files` — check all files (OR semantics)
//! - `rails.commands.rails_version` — run a specific named command
//! - `rails.files.application_file` — check a specific named file
//! - `node.command` — singular command shorthand
//! - `node.file` — singular file shorthand
//!
//! # Evaluation Semantics
//!
//! - **commands group** (plural): ALL must succeed (AND)
//! - **files group** (plural): ANY must exist (OR)
//! - **single command**: must succeed
//! - **single file**: must exist
//! - **bare detector name**: commands pass AND files pass

use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;

use crate::detection::command_detection::command_succeeds;
use crate::detection::file_detection::file_exists;
use crate::error::{BivvyError, Result};

/// Top-level detectors file structure.
///
/// Corresponds to the `templates/detectors.yml` file format.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DetectorFile {
    /// Map of detector name to definition.
    pub detectors: HashMap<String, DetectorDef>,
}

/// A single detector definition.
///
/// Supports both singular (`command`/`file`) and plural (`commands`/`files`)
/// forms. Singular values are merged into the plural maps with the key
/// `"default"` when accessed via [`all_commands`](DetectorDef::all_commands)
/// or [`all_files`](DetectorDef::all_files).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DetectorDef {
    /// Singular command shorthand.
    #[serde(default)]
    pub command: Option<String>,

    /// Named commands map.
    #[serde(default)]
    pub commands: HashMap<String, String>,

    /// Singular file shorthand.
    #[serde(default)]
    pub file: Option<String>,

    /// Named files map.
    #[serde(default)]
    pub files: HashMap<String, String>,
}

impl DetectorDef {
    /// Get all commands, merging the singular `command` field into the map
    /// with key `"default"` if not already present.
    pub fn all_commands(&self) -> HashMap<String, String> {
        let mut cmds = self.commands.clone();
        if let Some(ref cmd) = self.command {
            cmds.entry("default".to_string())
                .or_insert_with(|| cmd.clone());
        }
        cmds
    }

    /// Get all files, merging the singular `file` field into the map
    /// with key `"default"` if not already present.
    pub fn all_files(&self) -> HashMap<String, String> {
        let mut files = self.files.clone();
        if let Some(ref f) = self.file {
            files
                .entry("default".to_string())
                .or_insert_with(|| f.clone());
        }
        files
    }

    /// Check if this detector has any checks defined.
    pub fn has_checks(&self) -> bool {
        self.command.is_some()
            || !self.commands.is_empty()
            || self.file.is_some()
            || !self.files.is_empty()
    }
}

/// Parsed reference to a detector facet.
///
/// Created by [`DetectorRef::parse`] from a dot-notation string.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DetectorRef {
    /// The detector name (first segment).
    pub detector: String,
    /// The group being referenced (second segment).
    pub group: Option<DetectorGroup>,
    /// A specific named entry within the group (third segment).
    pub specific: Option<String>,
}

/// Which group of checks a detector reference targets.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DetectorGroup {
    /// Plural commands group (AND semantics).
    Commands,
    /// Plural files group (OR semantics).
    Files,
    /// Singular command shorthand.
    Command,
    /// Singular file shorthand.
    File,
}

impl DetectorRef {
    /// Parse a detector reference string like `"rails.commands.rails_version"`.
    ///
    /// # Errors
    ///
    /// Returns an error if the group segment is unrecognized or if a specific
    /// name is used with a singular group (`command` or `file`).
    pub fn parse(s: &str) -> Result<Self> {
        let parts: Vec<&str> = s.splitn(3, '.').collect();
        match parts.len() {
            1 => Ok(DetectorRef {
                detector: parts[0].to_string(),
                group: None,
                specific: None,
            }),
            2 => {
                let group = match parts[1] {
                    "commands" => DetectorGroup::Commands,
                    "files" => DetectorGroup::Files,
                    "command" => DetectorGroup::Command,
                    "file" => DetectorGroup::File,
                    other => {
                        return Err(BivvyError::ConfigValidationError {
                            message: format!(
                                "Invalid detector group '{}' in '{}'. Expected: commands, files, command, file",
                                other, s
                            ),
                        })
                    }
                };
                Ok(DetectorRef {
                    detector: parts[0].to_string(),
                    group: Some(group),
                    specific: None,
                })
            }
            3 => {
                let group = match parts[1] {
                    "commands" => DetectorGroup::Commands,
                    "files" => DetectorGroup::Files,
                    other => {
                        return Err(BivvyError::ConfigValidationError {
                            message: format!(
                                "Invalid detector group '{}' in '{}'. Specific references require 'commands' or 'files'",
                                other, s
                            ),
                        })
                    }
                };
                Ok(DetectorRef {
                    detector: parts[0].to_string(),
                    group: Some(group),
                    specific: Some(parts[2].to_string()),
                })
            }
            _ => Err(BivvyError::ConfigValidationError {
                message: format!("Invalid detector reference: '{}'", s),
            }),
        }
    }
}

/// Result of evaluating a detector reference.
#[derive(Debug, Clone)]
pub struct DetectorResult {
    /// Whether the detector check passed.
    pub passed: bool,
    /// The original reference string.
    pub reference: String,
    /// Human-readable detail lines describing each sub-check.
    pub details: Vec<String>,
}

/// Registry that holds detector definitions and evaluates references with caching.
///
/// Results are cached within a session to avoid redundant command executions
/// and file-system checks.
pub struct DetectorRegistry {
    detectors: HashMap<String, DetectorDef>,
    cache: RefCell<HashMap<String, bool>>,
}

impl DetectorRegistry {
    /// Create a new registry from a parsed detector file.
    pub fn new(file: DetectorFile) -> Self {
        Self {
            detectors: file.detectors,
            cache: RefCell::new(HashMap::new()),
        }
    }

    /// Get a detector definition by name.
    pub fn get(&self, name: &str) -> Option<&DetectorDef> {
        self.detectors.get(name)
    }

    /// List all detector names.
    pub fn names(&self) -> Vec<&String> {
        self.detectors.keys().collect()
    }

    /// Clear the evaluation cache.
    pub fn clear_cache(&self) {
        self.cache.borrow_mut().clear();
    }

    /// Evaluate a single detector reference.
    ///
    /// # Errors
    ///
    /// Returns an error if the reference is malformed or references an
    /// unknown detector or facet.
    pub fn evaluate(&self, reference: &str, project_root: &Path) -> Result<DetectorResult> {
        let parsed = DetectorRef::parse(reference)?;

        let def = self.detectors.get(&parsed.detector).ok_or_else(|| {
            BivvyError::ConfigValidationError {
                message: format!("Unknown detector: '{}'", parsed.detector),
            }
        })?;

        let (passed, details) = match (&parsed.group, &parsed.specific) {
            // Bare name: commands AND files
            (None, None) => {
                let (cmds_pass, mut details) = self.eval_commands_group(def);
                let (files_pass, file_details) = self.eval_files_group(def, project_root);
                details.extend(file_details);
                (cmds_pass && files_pass, details)
            }

            // commands group (all must pass)
            (Some(DetectorGroup::Commands), None) => self.eval_commands_group(def),

            // files group (any must exist)
            (Some(DetectorGroup::Files), None) => self.eval_files_group(def, project_root),

            // command singular shorthand
            (Some(DetectorGroup::Command), None) => {
                self.eval_command_singular(def, &parsed.detector)?
            }

            // file singular shorthand
            (Some(DetectorGroup::File), None) => {
                self.eval_file_singular(def, &parsed.detector, project_root)?
            }

            // specific command
            (Some(DetectorGroup::Commands), Some(name)) => {
                self.eval_specific_command(def, &parsed.detector, name)?
            }

            // specific file
            (Some(DetectorGroup::Files), Some(name)) => {
                self.eval_specific_file(def, &parsed.detector, name, project_root)?
            }

            // singular shorthands can't have specifics
            (Some(DetectorGroup::Command), Some(_)) | (Some(DetectorGroup::File), Some(_)) => {
                return Err(BivvyError::ConfigValidationError {
                    message: format!(
                        "Cannot use specific name with singular 'command' or 'file' in '{}'",
                        reference
                    ),
                });
            }

            // bare name with specific but no group -- unreachable via parse but needed for exhaustiveness
            (None, Some(_)) => {
                return Err(BivvyError::ConfigValidationError {
                    message: format!("Invalid detector reference: '{}'", reference),
                });
            }
        };

        Ok(DetectorResult {
            passed,
            reference: reference.to_string(),
            details,
        })
    }

    /// Evaluate multiple detector references. Returns true if ALL pass.
    pub fn evaluate_all(&self, references: &[String], project_root: &Path) -> Result<bool> {
        for reference in references {
            let result = self.evaluate(reference, project_root)?;
            if !result.passed {
                return Ok(false);
            }
        }
        Ok(true)
    }

    // --- Private evaluation helpers ---

    fn eval_commands_group(&self, def: &DetectorDef) -> (bool, Vec<String>) {
        let cmds = def.all_commands();
        let mut details = Vec::new();
        let mut all_pass = true;
        for (name, cmd) in &cmds {
            let pass = self.eval_command_cached(cmd);
            if !pass {
                all_pass = false;
            }
            details.push(format!(
                "{}: {} ({})",
                name,
                if pass { "passed" } else { "failed" },
                cmd
            ));
        }
        (all_pass, details)
    }

    fn eval_files_group(&self, def: &DetectorDef, project_root: &Path) -> (bool, Vec<String>) {
        let files = def.all_files();
        if files.is_empty() {
            return (true, Vec::new());
        }
        let mut details = Vec::new();
        let mut any_pass = false;
        for (name, path) in &files {
            let pass = self.eval_file_cached(project_root, path);
            if pass {
                any_pass = true;
            }
            details.push(format!(
                "{}: {} ({})",
                name,
                if pass { "found" } else { "missing" },
                path
            ));
        }
        (any_pass, details)
    }

    fn eval_command_singular(
        &self,
        def: &DetectorDef,
        detector_name: &str,
    ) -> Result<(bool, Vec<String>)> {
        let cmd = def
            .command
            .as_ref()
            .or_else(|| def.commands.values().next())
            .ok_or_else(|| BivvyError::ConfigValidationError {
                message: format!("Detector '{}' has no command defined", detector_name),
            })?;
        let pass = self.eval_command_cached(cmd);
        Ok((
            pass,
            vec![format!(
                "{} ({})",
                if pass { "passed" } else { "failed" },
                cmd
            )],
        ))
    }

    fn eval_file_singular(
        &self,
        def: &DetectorDef,
        detector_name: &str,
        project_root: &Path,
    ) -> Result<(bool, Vec<String>)> {
        let f = def
            .file
            .as_ref()
            .or_else(|| def.files.values().next())
            .ok_or_else(|| BivvyError::ConfigValidationError {
                message: format!("Detector '{}' has no file defined", detector_name),
            })?;
        let pass = self.eval_file_cached(project_root, f);
        Ok((
            pass,
            vec![format!(
                "{} ({})",
                if pass { "found" } else { "missing" },
                f
            )],
        ))
    }

    fn eval_specific_command(
        &self,
        def: &DetectorDef,
        detector_name: &str,
        name: &str,
    ) -> Result<(bool, Vec<String>)> {
        let cmd = def.all_commands().get(name).cloned().ok_or_else(|| {
            BivvyError::ConfigValidationError {
                message: format!(
                    "Detector '{}' has no command named '{}'",
                    detector_name, name
                ),
            }
        })?;
        let pass = self.eval_command_cached(&cmd);
        Ok((
            pass,
            vec![format!(
                "{}: {} ({})",
                name,
                if pass { "passed" } else { "failed" },
                cmd
            )],
        ))
    }

    fn eval_specific_file(
        &self,
        def: &DetectorDef,
        detector_name: &str,
        name: &str,
        project_root: &Path,
    ) -> Result<(bool, Vec<String>)> {
        let f = def.all_files().get(name).cloned().ok_or_else(|| {
            BivvyError::ConfigValidationError {
                message: format!("Detector '{}' has no file named '{}'", detector_name, name),
            }
        })?;
        let pass = self.eval_file_cached(project_root, &f);
        Ok((
            pass,
            vec![format!(
                "{}: {} ({})",
                name,
                if pass { "found" } else { "missing" },
                f
            )],
        ))
    }

    fn eval_command_cached(&self, cmd: &str) -> bool {
        let key = format!("cmd:{}", cmd);
        if let Some(&cached) = self.cache.borrow().get(&key) {
            return cached;
        }
        let result = command_succeeds(cmd);
        self.cache.borrow_mut().insert(key, result);
        result
    }

    fn eval_file_cached(&self, project_root: &Path, path: &str) -> bool {
        let key = format!("file:{}:{}", project_root.display(), path);
        if let Some(&cached) = self.cache.borrow().get(&key) {
            return cached;
        }
        let result = file_exists(project_root, path);
        self.cache.borrow_mut().insert(key, result);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn sample_detectors() -> DetectorFile {
        serde_yaml::from_str(
            r#"
detectors:
  rails:
    commands:
      rails_version: "true"
      has_gemfile: "true"
    files:
      application_file: config/application.rb
      database_file: config/database.yml

  node:
    command: "true"
    file: package.json

  missing_tool:
    command: "false"
    file: nonexistent.txt

  file_only:
    file: Cargo.toml

  command_only:
    command: "true"
"#,
        )
        .unwrap()
    }

    // --- DetectorRef parsing tests ---

    #[test]
    fn parse_bare_name() {
        let r = DetectorRef::parse("rails").unwrap();
        assert_eq!(r.detector, "rails");
        assert!(r.group.is_none());
        assert!(r.specific.is_none());
    }

    #[test]
    fn parse_commands_group() {
        let r = DetectorRef::parse("rails.commands").unwrap();
        assert_eq!(r.detector, "rails");
        assert_eq!(r.group, Some(DetectorGroup::Commands));
        assert!(r.specific.is_none());
    }

    #[test]
    fn parse_files_group() {
        let r = DetectorRef::parse("rails.files").unwrap();
        assert_eq!(r.group, Some(DetectorGroup::Files));
    }

    #[test]
    fn parse_command_singular() {
        let r = DetectorRef::parse("node.command").unwrap();
        assert_eq!(r.group, Some(DetectorGroup::Command));
    }

    #[test]
    fn parse_file_singular() {
        let r = DetectorRef::parse("node.file").unwrap();
        assert_eq!(r.group, Some(DetectorGroup::File));
    }

    #[test]
    fn parse_specific_command() {
        let r = DetectorRef::parse("rails.commands.rails_version").unwrap();
        assert_eq!(r.detector, "rails");
        assert_eq!(r.group, Some(DetectorGroup::Commands));
        assert_eq!(r.specific, Some("rails_version".to_string()));
    }

    #[test]
    fn parse_specific_file() {
        let r = DetectorRef::parse("rails.files.application_file").unwrap();
        assert_eq!(r.specific, Some("application_file".to_string()));
    }

    #[test]
    fn parse_invalid_group_errors() {
        assert!(DetectorRef::parse("rails.foobar").is_err());
    }

    #[test]
    fn parse_specific_on_singular_errors_at_parse() {
        // "node.command.something" errors because "command" isn't valid for 3-part refs
        assert!(DetectorRef::parse("node.command.something").is_err());
    }

    // --- DetectorDef tests ---

    #[test]
    fn all_commands_merges_singular() {
        let def = DetectorDef {
            command: Some("node -v".to_string()),
            commands: HashMap::new(),
            file: None,
            files: HashMap::new(),
        };
        let cmds = def.all_commands();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds["default"], "node -v");
    }

    #[test]
    fn all_commands_preserves_named() {
        let mut commands = HashMap::new();
        commands.insert("check".to_string(), "rails -v".to_string());
        let def = DetectorDef {
            command: None,
            commands,
            file: None,
            files: HashMap::new(),
        };
        let cmds = def.all_commands();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds["check"], "rails -v");
    }

    #[test]
    fn all_files_merges_singular() {
        let def = DetectorDef {
            command: None,
            commands: HashMap::new(),
            file: Some("package.json".to_string()),
            files: HashMap::new(),
        };
        let files = def.all_files();
        assert_eq!(files.len(), 1);
        assert_eq!(files["default"], "package.json");
    }

    #[test]
    fn has_checks_true_with_command() {
        let def = DetectorDef {
            command: Some("x".into()),
            commands: HashMap::new(),
            file: None,
            files: HashMap::new(),
        };
        assert!(def.has_checks());
    }

    #[test]
    fn has_checks_false_when_empty() {
        let def = DetectorDef {
            command: None,
            commands: HashMap::new(),
            file: None,
            files: HashMap::new(),
        };
        assert!(!def.has_checks());
    }

    // --- DetectorRegistry evaluation tests ---

    #[test]
    fn evaluate_file_singular_found() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("package.json"), "{}").unwrap();

        let registry = DetectorRegistry::new(sample_detectors());
        let result = registry.evaluate("node.file", temp.path()).unwrap();
        assert!(result.passed);
    }

    #[test]
    fn evaluate_file_singular_missing() {
        let temp = TempDir::new().unwrap();

        let registry = DetectorRegistry::new(sample_detectors());
        let result = registry.evaluate("node.file", temp.path()).unwrap();
        assert!(!result.passed);
    }

    #[test]
    fn evaluate_command_singular() {
        let temp = TempDir::new().unwrap();
        let registry = DetectorRegistry::new(sample_detectors());
        let result = registry.evaluate("node.command", temp.path()).unwrap();
        assert!(result.passed); // "true" succeeds
    }

    #[test]
    fn evaluate_commands_group_all_must_pass() {
        let temp = TempDir::new().unwrap();
        let registry = DetectorRegistry::new(sample_detectors());
        let result = registry.evaluate("rails.commands", temp.path()).unwrap();
        assert!(result.passed); // both "true"
    }

    #[test]
    fn evaluate_files_group_any_match() {
        let temp = TempDir::new().unwrap();
        // Only create one of the two files
        fs::create_dir_all(temp.path().join("config")).unwrap();
        fs::write(temp.path().join("config/application.rb"), "").unwrap();

        let registry = DetectorRegistry::new(sample_detectors());
        let result = registry.evaluate("rails.files", temp.path()).unwrap();
        assert!(result.passed); // one file found is enough
    }

    #[test]
    fn evaluate_files_group_none_match() {
        let temp = TempDir::new().unwrap();
        let registry = DetectorRegistry::new(sample_detectors());
        let result = registry.evaluate("rails.files", temp.path()).unwrap();
        assert!(!result.passed); // no files found
    }

    #[test]
    fn evaluate_specific_command() {
        let temp = TempDir::new().unwrap();
        let registry = DetectorRegistry::new(sample_detectors());
        let result = registry
            .evaluate("rails.commands.rails_version", temp.path())
            .unwrap();
        assert!(result.passed);
    }

    #[test]
    fn evaluate_specific_file() {
        let temp = TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join("config")).unwrap();
        fs::write(temp.path().join("config/application.rb"), "").unwrap();

        let registry = DetectorRegistry::new(sample_detectors());
        let result = registry
            .evaluate("rails.files.application_file", temp.path())
            .unwrap();
        assert!(result.passed);
    }

    #[test]
    fn evaluate_specific_file_missing() {
        let temp = TempDir::new().unwrap();
        let registry = DetectorRegistry::new(sample_detectors());
        let result = registry
            .evaluate("rails.files.database_file", temp.path())
            .unwrap();
        assert!(!result.passed);
    }

    #[test]
    fn evaluate_bare_name_requires_both() {
        let temp = TempDir::new().unwrap();
        // commands pass (exit 0) but no files exist
        let registry = DetectorRegistry::new(sample_detectors());
        let result = registry.evaluate("rails", temp.path()).unwrap();
        assert!(!result.passed); // files not found
    }

    #[test]
    fn evaluate_bare_name_passes_when_all_match() {
        let temp = TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join("config")).unwrap();
        fs::write(temp.path().join("config/application.rb"), "").unwrap();
        // commands pass (exit 0) and at least one file exists
        let registry = DetectorRegistry::new(sample_detectors());
        let result = registry.evaluate("rails", temp.path()).unwrap();
        assert!(result.passed);
    }

    #[test]
    fn evaluate_command_only_detector() {
        let temp = TempDir::new().unwrap();
        let registry = DetectorRegistry::new(sample_detectors());
        let result = registry.evaluate("command_only", temp.path()).unwrap();
        assert!(result.passed); // "true" + no files = pass
    }

    #[test]
    fn evaluate_file_only_detector() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("Cargo.toml"), "").unwrap();
        let registry = DetectorRegistry::new(sample_detectors());
        let result = registry.evaluate("file_only", temp.path()).unwrap();
        assert!(result.passed); // no commands + file found = pass
    }

    #[test]
    fn evaluate_unknown_detector_errors() {
        let temp = TempDir::new().unwrap();
        let registry = DetectorRegistry::new(sample_detectors());
        assert!(registry.evaluate("nonexistent", temp.path()).is_err());
    }

    #[test]
    fn evaluate_unknown_specific_command_errors() {
        let temp = TempDir::new().unwrap();
        let registry = DetectorRegistry::new(sample_detectors());
        assert!(registry
            .evaluate("rails.commands.nonexistent", temp.path())
            .is_err());
    }

    #[test]
    fn evaluate_all_returns_true_when_all_pass() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("package.json"), "{}").unwrap();

        let registry = DetectorRegistry::new(sample_detectors());
        let refs = vec!["node.command".to_string(), "node.file".to_string()];
        assert!(registry.evaluate_all(&refs, temp.path()).unwrap());
    }

    #[test]
    fn evaluate_all_returns_false_when_any_fails() {
        let temp = TempDir::new().unwrap();
        // node.command passes but node.file fails (no package.json)

        let registry = DetectorRegistry::new(sample_detectors());
        let refs = vec!["node.command".to_string(), "node.file".to_string()];
        assert!(!registry.evaluate_all(&refs, temp.path()).unwrap());
    }

    // --- Caching tests ---

    #[test]
    fn cache_prevents_redundant_evaluation() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("package.json"), "{}").unwrap();

        let registry = DetectorRegistry::new(sample_detectors());

        // Evaluate twice
        let r1 = registry.evaluate("node.file", temp.path()).unwrap();
        let r2 = registry.evaluate("node.file", temp.path()).unwrap();
        assert_eq!(r1.passed, r2.passed);

        // Cache should have an entry
        assert!(!registry.cache.borrow().is_empty());
    }

    #[test]
    fn clear_cache_empties_cache() {
        let temp = TempDir::new().unwrap();
        let registry = DetectorRegistry::new(sample_detectors());
        registry.evaluate("node.command", temp.path()).unwrap();
        assert!(!registry.cache.borrow().is_empty());

        registry.clear_cache();
        assert!(registry.cache.borrow().is_empty());
    }

    // --- YAML deserialization tests ---

    #[test]
    fn deserialize_detector_file() {
        let yaml = r#"
detectors:
  simple:
    command: "echo hi"
    file: README.md
"#;
        let file: DetectorFile = serde_yaml::from_str(yaml).unwrap();
        assert!(file.detectors.contains_key("simple"));
        assert_eq!(
            file.detectors["simple"].command,
            Some("echo hi".to_string())
        );
        assert_eq!(file.detectors["simple"].file, Some("README.md".to_string()));
    }

    #[test]
    fn deserialize_detector_with_maps() {
        let yaml = r#"
detectors:
  complex:
    commands:
      check_a: "true"
      check_b: "true"
    files:
      file_a: a.txt
      file_b: b.txt
"#;
        let file: DetectorFile = serde_yaml::from_str(yaml).unwrap();
        let d = &file.detectors["complex"];
        assert_eq!(d.commands.len(), 2);
        assert_eq!(d.files.len(), 2);
    }
}
