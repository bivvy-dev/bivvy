//! Conflict detection.

use std::path::Path;

use super::file_detection::file_exists;

/// A detected conflict.
#[derive(Debug, Clone)]
pub struct Conflict {
    pub kind: ConflictKind,
    pub message: String,
    pub files: Vec<String>,
    pub suggestion: String,
}

/// Type of conflict.
#[derive(Debug, Clone, PartialEq)]
pub enum ConflictKind {
    /// Multiple Node package manager lockfiles.
    NodeLockfiles,
    /// Multiple version manager configs.
    VersionManagers,
    /// Multiple Python package managers.
    PythonPackageManagers,
}

/// Detects conflicts in project configuration.
pub struct ConflictDetector;

impl ConflictDetector {
    /// Detect all conflicts in a project.
    pub fn detect(project_root: &Path) -> Vec<Conflict> {
        let mut conflicts = Vec::new();

        if let Some(conflict) = Self::detect_node_lockfile_conflict(project_root) {
            conflicts.push(conflict);
        }

        if let Some(conflict) = Self::detect_version_manager_conflict(project_root) {
            conflicts.push(conflict);
        }

        if let Some(conflict) = Self::detect_python_conflict(project_root) {
            conflicts.push(conflict);
        }

        conflicts
    }

    fn detect_node_lockfile_conflict(project_root: &Path) -> Option<Conflict> {
        let lockfiles = [
            ("package-lock.json", "npm"),
            ("yarn.lock", "yarn"),
            ("pnpm-lock.yaml", "pnpm"),
            ("bun.lockb", "bun"),
        ];

        let found: Vec<_> = lockfiles
            .iter()
            .filter(|(file, _)| file_exists(project_root, file))
            .collect();

        if found.len() > 1 {
            let files: Vec<String> = found.iter().map(|(f, _)| f.to_string()).collect();
            let managers: Vec<_> = found.iter().map(|(_, m)| *m).collect();

            Some(Conflict {
                kind: ConflictKind::NodeLockfiles,
                message: format!("Multiple Node.js lockfiles detected: {}", files.join(", ")),
                files,
                suggestion: format!(
                    "Choose one package manager ({}). Delete the other lockfiles.",
                    managers.join(" or ")
                ),
            })
        } else {
            None
        }
    }

    fn detect_version_manager_conflict(project_root: &Path) -> Option<Conflict> {
        let configs = [
            (".mise.toml", "mise"),
            ("mise.toml", "mise"),
            (".tool-versions", "asdf"),
            (".nvmrc", "nvm"),
            (".node-version", "nodenv/volta"),
            (".ruby-version", "rbenv/rvm"),
            (".python-version", "pyenv"),
        ];

        let found: Vec<_> = configs
            .iter()
            .filter(|(file, _)| file_exists(project_root, file))
            .collect();

        let has_mise = found.iter().any(|(_, t)| *t == "mise");
        let has_asdf = found.iter().any(|(_, t)| *t == "asdf");

        if has_mise && has_asdf {
            Some(Conflict {
                kind: ConflictKind::VersionManagers,
                message: "Multiple version manager configs detected (mise and asdf)".to_string(),
                files: found.iter().map(|(f, _)| f.to_string()).collect(),
                suggestion: "Choose one version manager. mise can read .tool-versions.".to_string(),
            })
        } else {
            None
        }
    }

    fn detect_python_conflict(project_root: &Path) -> Option<Conflict> {
        let configs = [
            ("poetry.lock", "poetry"),
            ("Pipfile.lock", "pipenv"),
            ("uv.lock", "uv"),
        ];

        let found: Vec<_> = configs
            .iter()
            .filter(|(file, _)| file_exists(project_root, file))
            .collect();

        if found.len() > 1 {
            Some(Conflict {
                kind: ConflictKind::PythonPackageManagers,
                message: format!(
                    "Multiple Python lockfiles detected: {}",
                    found.iter().map(|(f, _)| *f).collect::<Vec<_>>().join(", ")
                ),
                files: found.iter().map(|(f, _)| f.to_string()).collect(),
                suggestion: "Choose one package manager for consistency.".to_string(),
            })
        } else {
            None
        }
    }

    /// Check if a specific conflict exists.
    pub fn has_conflict(project_root: &Path, kind: ConflictKind) -> bool {
        Self::detect(project_root).iter().any(|c| c.kind == kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn detect_node_lockfile_conflict() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("package.json"), "{}").unwrap();
        fs::write(temp.path().join("package-lock.json"), "").unwrap();
        fs::write(temp.path().join("yarn.lock"), "").unwrap();

        let conflicts = ConflictDetector::detect(temp.path());

        assert!(conflicts
            .iter()
            .any(|c| c.kind == ConflictKind::NodeLockfiles));
    }

    #[test]
    fn no_conflict_with_single_lockfile() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("package.json"), "{}").unwrap();
        fs::write(temp.path().join("yarn.lock"), "").unwrap();

        let conflicts = ConflictDetector::detect(temp.path());

        assert!(conflicts.is_empty());
    }

    #[test]
    fn detect_version_manager_conflict() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join(".mise.toml"), "").unwrap();
        fs::write(temp.path().join(".tool-versions"), "").unwrap();

        let conflicts = ConflictDetector::detect(temp.path());

        assert!(conflicts
            .iter()
            .any(|c| c.kind == ConflictKind::VersionManagers));
    }

    #[test]
    fn detect_python_conflict() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("poetry.lock"), "").unwrap();
        fs::write(temp.path().join("uv.lock"), "").unwrap();

        let conflicts = ConflictDetector::detect(temp.path());

        assert!(conflicts
            .iter()
            .any(|c| c.kind == ConflictKind::PythonPackageManagers));
    }

    #[test]
    fn has_conflict_helper() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("package-lock.json"), "").unwrap();
        fs::write(temp.path().join("yarn.lock"), "").unwrap();

        assert!(ConflictDetector::has_conflict(
            temp.path(),
            ConflictKind::NodeLockfiles
        ));
    }

    #[test]
    fn no_conflicts_empty_project() {
        let temp = TempDir::new().unwrap();

        let conflicts = ConflictDetector::detect(temp.path());

        assert!(conflicts.is_empty());
    }
}
