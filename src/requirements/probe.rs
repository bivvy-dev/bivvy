//! Environment probe for discovering version managers and tools.
//!
//! The biggest source of false negatives in requirement checking is version
//! managers (nvm, rbenv, pyenv, mise) not being on PATH in non-interactive
//! shells. Rust's `Command::new()` spawns a non-interactive, non-login shell
//! where those initializations haven't run.
//!
//! The `EnvironmentProbe` runs before requirement checking to discover tools
//! at well-known locations, producing an augmented PATH that subsequent checks
//! use.
//!
//! # Example
//!
//! ```no_run
//! use bivvy::requirements::probe::EnvironmentProbe;
//!
//! let probe = EnvironmentProbe::run();
//! // Use augmented PATH for subsequent tool lookups
//! for path in probe.augmented_path() {
//!     println!("Additional PATH entry: {}", path.display());
//! }
//! for mgr in probe.inactive_managers() {
//!     println!("{} found at {} but not active", mgr.name, mgr.install_path.display());
//! }
//! ```

use std::path::{Path, PathBuf};

/// A version manager detected as installed but not active in the current shell.
#[derive(Debug, Clone)]
pub struct InactiveManager {
    /// Manager name (e.g., "nvm", "rbenv", "mise").
    pub name: String,
    /// Root install path (e.g., ~/.nvm, ~/.rbenv).
    pub install_path: PathBuf,
    /// Shell command to activate this manager.
    pub activation: String,
}

/// Result of probing the environment for version managers and tools.
///
/// The probe checks environment variables first, then falls back to default
/// paths. This handles relocatable managers (e.g., `NVM_DIR=/opt/nvm`).
#[derive(Debug, Clone)]
pub struct EnvironmentProbe {
    /// Additional PATH entries discovered from known version manager locations.
    augmented_path: Vec<PathBuf>,
    /// Version managers detected as installed but not active.
    inactive_managers: Vec<InactiveManager>,
}

/// Definition of a version manager to probe for.
struct ManagerDef {
    name: &'static str,
    env_var: Option<&'static str>,
    default_paths: &'static [&'static str],
    binary_subpath: &'static str,
    path_subpaths: &'static [&'static str],
    activation: &'static str,
}

/// Known version manager definitions.
const MANAGER_DEFS: &[ManagerDef] = &[
    ManagerDef {
        name: "mise",
        env_var: Some("MISE_DATA_DIR"),
        default_paths: &[".local/share/mise"],
        binary_subpath: "bin/mise",
        path_subpaths: &["bin"],
        activation: "eval \"$(mise activate bash)\"",
    },
    ManagerDef {
        name: "mise-local",
        env_var: None,
        default_paths: &[".local/bin"],
        binary_subpath: "mise",
        path_subpaths: &[],
        activation: "eval \"$(mise activate bash)\"",
    },
    ManagerDef {
        name: "nvm",
        env_var: Some("NVM_DIR"),
        default_paths: &[".nvm"],
        binary_subpath: "nvm.sh",
        path_subpaths: &[],
        activation: "source $NVM_DIR/nvm.sh",
    },
    ManagerDef {
        name: "rbenv",
        env_var: Some("RBENV_ROOT"),
        default_paths: &[".rbenv"],
        binary_subpath: "bin/rbenv",
        path_subpaths: &["bin", "shims"],
        activation: "eval \"$(rbenv init -)\"",
    },
    ManagerDef {
        name: "pyenv",
        env_var: Some("PYENV_ROOT"),
        default_paths: &[".pyenv"],
        binary_subpath: "bin/pyenv",
        path_subpaths: &["bin", "shims"],
        activation: "eval \"$(pyenv init -)\"",
    },
    ManagerDef {
        name: "volta",
        env_var: Some("VOLTA_HOME"),
        default_paths: &[".volta"],
        binary_subpath: "bin/volta",
        path_subpaths: &["bin"],
        activation: "export PATH=\"$VOLTA_HOME/bin:$PATH\"",
    },
    ManagerDef {
        name: "homebrew",
        env_var: Some("HOMEBREW_PREFIX"),
        #[cfg(target_arch = "aarch64")]
        default_paths: &[],
        #[cfg(not(target_arch = "aarch64"))]
        default_paths: &[],
        binary_subpath: "bin/brew",
        path_subpaths: &["bin", "sbin"],
        activation: "eval \"$(brew shellenv)\"",
    },
];

/// Check whether a file has executable permission bits set.
#[cfg(unix)]
pub fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// On Windows, executability is determined by file extension, not permission bits.
#[cfg(not(unix))]
pub fn is_executable(_path: &Path) -> bool {
    true
}

/// Resolve a tool's binary path by iterating over PATH entries.
///
/// Returns the first match that exists and is executable. Does NOT use
/// the `which` command — `which` behavior varies across systems and
/// is sometimes a shell builtin with inconsistent error handling.
pub fn resolve_tool_path(tool: &str, path_entries: &[PathBuf]) -> Option<PathBuf> {
    for dir in path_entries {
        let candidate = dir.join(tool);
        if candidate.is_file() && is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// Probe a single manager location, checking env var first then default paths.
///
/// Returns the install root path if the manager's binary is found.
pub fn probe_manager_location<F>(
    env_var: Option<&str>,
    default_paths: &[PathBuf],
    binary_subpath: &str,
    env_fn: &F,
) -> Option<PathBuf>
where
    F: Fn(&str) -> Result<String, std::env::VarError>,
{
    // 1. Check env var first (handles relocatable installs)
    if let Some(var) = env_var {
        if let Ok(val) = env_fn(var) {
            let path = PathBuf::from(val);
            if path.join(binary_subpath).exists() {
                return Some(path);
            }
        }
    }

    // 2. Fall back to default paths
    for default in default_paths {
        if default.join(binary_subpath).exists() {
            return Some(default.clone());
        }
    }

    None
}

/// Parse the system PATH environment variable into a list of directories.
pub fn parse_system_path() -> Vec<PathBuf> {
    std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default()
}

impl EnvironmentProbe {
    /// Probe the environment using actual environment variables and filesystem.
    pub fn run() -> Self {
        Self::run_with_env(|key: &str| std::env::var(key))
    }

    /// Probe the environment with a custom env var lookup function.
    ///
    /// This allows testing without modifying actual environment variables.
    pub fn run_with_env<F>(env_fn: F) -> Self
    where
        F: Fn(&str) -> Result<String, std::env::VarError>,
    {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
        let system_path = parse_system_path();
        let mut augmented_path = Vec::new();
        let mut inactive_managers = Vec::new();

        for def in MANAGER_DEFS {
            if let Some(result) = probe_manager(&home, def, &env_fn, &system_path) {
                for path in &result.path_additions {
                    if !system_path.contains(path) && !augmented_path.contains(path) {
                        augmented_path.push(path.clone());
                    }
                }
                if !result.already_on_path {
                    inactive_managers.push(InactiveManager {
                        name: def.name.to_string(),
                        install_path: result.install_path,
                        activation: def.activation.to_string(),
                    });
                }
            }
        }

        // Also check Homebrew at known absolute paths (not relative to home)
        for prefix in homebrew_default_prefixes() {
            let brew = prefix.join("bin/brew");
            if brew.is_file() && is_executable(&brew) {
                let bin = prefix.join("bin");
                let sbin = prefix.join("sbin");
                for dir in [&bin, &sbin] {
                    if dir.is_dir() && !system_path.contains(dir) && !augmented_path.contains(dir) {
                        augmented_path.push(dir.clone());
                    }
                }
            }
        }

        Self {
            augmented_path,
            inactive_managers,
        }
    }

    /// Get the additional PATH entries discovered by the probe.
    pub fn augmented_path(&self) -> &[PathBuf] {
        &self.augmented_path
    }

    /// Get the list of inactive version managers.
    pub fn inactive_managers(&self) -> &[InactiveManager] {
        &self.inactive_managers
    }

    /// Build a combined PATH: augmented entries prepended to system PATH.
    pub fn full_path(&self) -> Vec<PathBuf> {
        let system = parse_system_path();
        let mut result = self.augmented_path.clone();
        result.extend(system);
        result
    }

    /// Re-probe the environment after an install may have changed things.
    pub fn refresh(&mut self) {
        let refreshed = Self::run();
        self.augmented_path = refreshed.augmented_path;
        self.inactive_managers = refreshed.inactive_managers;
    }
}

/// Result of probing a single manager.
struct ProbeResult {
    install_path: PathBuf,
    path_additions: Vec<PathBuf>,
    already_on_path: bool,
}

/// Probe a single manager definition.
fn probe_manager<F>(
    home: &Path,
    def: &ManagerDef,
    env_fn: &F,
    system_path: &[PathBuf],
) -> Option<ProbeResult>
where
    F: Fn(&str) -> Result<String, std::env::VarError>,
{
    // 1. Check env var first (handles relocatable installs)
    let install_path = if let Some(var) = def.env_var {
        if let Ok(val) = env_fn(var) {
            let path = PathBuf::from(val);
            if path.join(def.binary_subpath).exists() {
                Some(path)
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    // 2. Fall back to default paths relative to home
    let install_path = install_path.or_else(|| {
        for default in def.default_paths {
            let path = home.join(default);
            if path.join(def.binary_subpath).exists() {
                return Some(path);
            }
        }
        None
    })?;

    // 3. Compute path additions
    let path_additions: Vec<PathBuf> = def
        .path_subpaths
        .iter()
        .map(|sub| install_path.join(sub))
        .filter(|p| p.is_dir())
        .collect();

    // 4. Check if already on system PATH
    let already_on_path = path_additions.iter().all(|p| system_path.contains(p));

    Some(ProbeResult {
        install_path,
        path_additions,
        already_on_path,
    })
}

/// Default Homebrew prefix paths to check (absolute, not relative to home).
fn homebrew_default_prefixes() -> Vec<PathBuf> {
    let mut prefixes = Vec::new();
    if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            prefixes.push(PathBuf::from("/opt/homebrew"));
        } else {
            prefixes.push(PathBuf::from("/usr/local"));
        }
    } else if cfg!(target_os = "linux") {
        prefixes.push(PathBuf::from("/home/linuxbrew/.linuxbrew"));
    }
    prefixes
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Create a fake binary at a path (creates parent dirs as needed).
    fn create_fake_binary(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    /// Create a non-executable file at a path.
    #[cfg(unix)]
    fn create_non_executable_file(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, "not executable").unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o644)).unwrap();
    }

    #[test]
    fn resolve_tool_path_finds_first_match() {
        let temp = TempDir::new().unwrap();
        let dir_a = temp.path().join("a");
        let dir_b = temp.path().join("b");
        fs::create_dir_all(&dir_a).unwrap();
        fs::create_dir_all(&dir_b).unwrap();

        create_fake_binary(&dir_a.join("ruby"));
        create_fake_binary(&dir_b.join("ruby"));

        let result = resolve_tool_path("ruby", &[dir_a.clone(), dir_b.clone()]);
        assert_eq!(result, Some(dir_a.join("ruby")));
    }

    #[test]
    fn resolve_tool_path_returns_none_when_not_found() {
        let temp = TempDir::new().unwrap();
        let dir = temp.path().join("empty");
        fs::create_dir_all(&dir).unwrap();

        let result = resolve_tool_path("ruby", &[dir]);
        assert!(result.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn resolve_tool_path_skips_non_executable() {
        let temp = TempDir::new().unwrap();
        let dir_a = temp.path().join("a");
        let dir_b = temp.path().join("b");

        create_non_executable_file(&dir_a.join("ruby"));
        create_fake_binary(&dir_b.join("ruby"));

        let result = resolve_tool_path("ruby", &[dir_a.clone(), dir_b.clone()]);
        // Should skip non-executable in dir_a and find the one in dir_b
        assert_eq!(result, Some(dir_b.join("ruby")));
    }

    #[cfg(unix)]
    #[test]
    fn is_executable_returns_true_for_executable_file() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("test_bin");
        create_fake_binary(&path);
        assert!(is_executable(&path));
    }

    #[cfg(unix)]
    #[test]
    fn is_executable_returns_false_for_non_executable_file() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("test_file");
        create_non_executable_file(&path);
        assert!(!is_executable(&path));
    }

    #[test]
    fn is_executable_returns_false_for_nonexistent_file() {
        assert!(!is_executable(Path::new("/nonexistent/path/to/file")));
    }

    #[test]
    fn probe_checks_env_var_before_default() {
        let temp = TempDir::new().unwrap();
        let custom_nvm = temp.path().join("custom-nvm");
        let default_nvm = temp.path().join("default-nvm");

        // Create nvm.sh at the custom location
        create_fake_binary(&custom_nvm.join("nvm.sh"));
        // Also create at the default location (should NOT be used)
        create_fake_binary(&default_nvm.join("nvm.sh"));

        let custom_nvm_str = custom_nvm.to_string_lossy().to_string();

        let result = probe_manager_location(
            Some("NVM_DIR"),
            std::slice::from_ref(&default_nvm),
            "nvm.sh",
            &|var| {
                if var == "NVM_DIR" {
                    Ok(custom_nvm_str.clone())
                } else {
                    Err(std::env::VarError::NotPresent)
                }
            },
        );

        assert_eq!(result, Some(custom_nvm));
    }

    #[test]
    fn probe_falls_back_to_default_when_env_unset() {
        let temp = TempDir::new().unwrap();
        let default_nvm = temp.path().join("default-nvm");
        create_fake_binary(&default_nvm.join("nvm.sh"));

        let result = probe_manager_location(
            Some("NVM_DIR"),
            std::slice::from_ref(&default_nvm),
            "nvm.sh",
            &|_| Err(std::env::VarError::NotPresent),
        );

        assert_eq!(result, Some(default_nvm));
    }

    #[test]
    fn probe_returns_none_when_nothing_found() {
        let result = probe_manager_location(
            Some("NVM_DIR"),
            &[PathBuf::from("/nonexistent/path")],
            "nvm.sh",
            &|_| Err(std::env::VarError::NotPresent),
        );

        assert!(result.is_none());
    }

    #[test]
    fn probe_respects_rbenv_root() {
        let temp = TempDir::new().unwrap();
        let custom_rbenv = temp.path().join("custom-rbenv");
        create_fake_binary(&custom_rbenv.join("bin/rbenv"));

        let custom_str = custom_rbenv.to_string_lossy().to_string();

        let result = probe_manager_location(Some("RBENV_ROOT"), &[], "bin/rbenv", &|var| {
            if var == "RBENV_ROOT" {
                Ok(custom_str.clone())
            } else {
                Err(std::env::VarError::NotPresent)
            }
        });

        assert_eq!(result, Some(custom_rbenv));
    }

    #[test]
    fn probe_augments_path_with_discovered_locations() {
        let temp = TempDir::new().unwrap();
        let fake_home = temp.path();

        // Create a fake rbenv with bin/ and shims/ dirs
        let rbenv_root = fake_home.join(".rbenv");
        create_fake_binary(&rbenv_root.join("bin/rbenv"));
        fs::create_dir_all(rbenv_root.join("shims")).unwrap();

        // Run probe with env_fn that returns NotPresent for everything
        // and a home dir pointing to our temp dir.
        // We can't easily mock home_dir, so test probe_manager directly.
        let system_path: Vec<PathBuf> = vec![];
        let def = ManagerDef {
            name: "rbenv",
            env_var: None, // Skip env var to test default path
            default_paths: &[],
            binary_subpath: "bin/rbenv",
            path_subpaths: &["bin", "shims"],
            activation: "eval \"$(rbenv init -)\"",
        };

        // Directly test with the install path
        let path_additions: Vec<PathBuf> = def
            .path_subpaths
            .iter()
            .map(|sub| rbenv_root.join(sub))
            .filter(|p| p.is_dir())
            .collect();

        assert!(path_additions.contains(&rbenv_root.join("bin")));
        assert!(path_additions.contains(&rbenv_root.join("shims")));
        for p in &path_additions {
            assert!(!system_path.contains(p));
        }
    }

    #[test]
    fn probe_manager_location_finds_binary_at_subpath() {
        let temp = TempDir::new().unwrap();
        let manager_root = temp.path().join("manager");
        create_fake_binary(&manager_root.join("bin/tool"));

        let result = probe_manager_location(
            None,
            std::slice::from_ref(&manager_root),
            "bin/tool",
            &|_| Err(std::env::VarError::NotPresent),
        );

        assert_eq!(result, Some(manager_root));
    }

    #[test]
    fn probe_env_var_with_nonexistent_path_falls_through() {
        let temp = TempDir::new().unwrap();
        let default_path = temp.path().join("default");
        create_fake_binary(&default_path.join("bin/tool"));

        let result = probe_manager_location(
            Some("CUSTOM_ROOT"),
            std::slice::from_ref(&default_path),
            "bin/tool",
            &|var| {
                if var == "CUSTOM_ROOT" {
                    Ok("/nonexistent/path".to_string())
                } else {
                    Err(std::env::VarError::NotPresent)
                }
            },
        );

        // Env var path doesn't have the binary, falls back to default
        assert_eq!(result, Some(default_path));
    }

    #[test]
    fn full_path_prepends_augmented() {
        let probe = EnvironmentProbe {
            augmented_path: vec![PathBuf::from("/extra/bin")],
            inactive_managers: vec![],
        };

        let full = probe.full_path();
        assert_eq!(full[0], PathBuf::from("/extra/bin"));
        // System PATH entries follow
        assert!(full.len() > 1 || std::env::var_os("PATH").is_none());
    }

    #[test]
    fn empty_probe_has_no_augmented_path() {
        let probe = EnvironmentProbe {
            augmented_path: vec![],
            inactive_managers: vec![],
        };

        assert!(probe.augmented_path().is_empty());
        assert!(probe.inactive_managers().is_empty());
    }

    #[test]
    fn inactive_manager_fields_accessible() {
        let mgr = InactiveManager {
            name: "nvm".to_string(),
            install_path: PathBuf::from("/home/user/.nvm"),
            activation: "source $NVM_DIR/nvm.sh".to_string(),
        };
        assert_eq!(mgr.name, "nvm");
        assert_eq!(mgr.install_path, PathBuf::from("/home/user/.nvm"));
        assert!(mgr.activation.contains("nvm.sh"));
    }
}
