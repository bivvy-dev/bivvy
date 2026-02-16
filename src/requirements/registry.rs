//! Requirement registry and definitions.
//!
//! Defines what requirements exist, how to check them, and how to install them.
//! The registry holds both built-in requirements (ruby, node, etc.) and
//! custom project-specific requirements from config.

use crate::config::CustomRequirement;
use std::collections::HashMap;

/// Context for resolving dynamic install dependencies.
pub struct InstallContext {
    /// Version managers detected on the system
    pub detected_managers: Vec<String>,
    /// Current platform
    pub platform: Platform,
}

/// Platform for requirement resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacOS,
    Linux,
    Windows,
}

impl Platform {
    /// Detect the current platform.
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Platform::MacOS
        } else if cfg!(target_os = "windows") {
            Platform::Windows
        } else {
            Platform::Linux
        }
    }
}

/// A requirement definition.
pub struct Requirement {
    /// Requirement name (e.g., "ruby", "node", "postgres-server")
    pub name: String,
    /// Ordered checks — first match determines status
    pub checks: Vec<RequirementCheck>,
    /// Template to install this requirement
    pub install_template: Option<String>,
    /// Human-readable install instructions
    pub install_hint: Option<String>,
    /// Static dependencies (always required before this one)
    pub depends_on: Vec<String>,
    /// Dynamic dependency resolver, called when install is selected
    pub install_requires: Option<fn(&InstallContext) -> Vec<String>>,
}

/// How to check if a requirement is met.
pub enum RequirementCheck {
    /// Run a command and check exit code 0
    CommandSucceeds(String),

    /// Check if a file or directory exists
    FileExists(String),

    /// Check if a service is reachable (run command, check exit 0)
    ServiceReachable(String),

    /// Check for a managed command (tool binary) with path classification
    ManagedCommand {
        /// The tool binary name (e.g., "ruby", "node")
        command: String,
        /// Path patterns indicating a version-managed install
        managed_path_patterns: Vec<String>,
        /// Path patterns indicating a system/default install
        system_path_patterns: Vec<String>,
        /// Version file that indicates a manager should provide this tool
        version_file: Option<String>,
    },

    /// Any of the sub-checks passing is sufficient
    Any(Vec<RequirementCheck>),
}

/// Registry of all known requirements.
pub struct RequirementRegistry {
    requirements: HashMap<String, Requirement>,
}

impl RequirementRegistry {
    /// Create a registry with built-in requirements.
    pub fn new() -> Self {
        let mut requirements = HashMap::new();

        // Ruby
        requirements.insert(
            "ruby".to_string(),
            Requirement {
                name: "ruby".to_string(),
                checks: vec![RequirementCheck::ManagedCommand {
                    command: "ruby".to_string(),
                    managed_path_patterns: vec![
                        "mise/".to_string(),
                        "rbenv/".to_string(),
                        "asdf/".to_string(),
                        "chruby/".to_string(),
                        ".rubies/".to_string(),
                    ],
                    system_path_patterns: vec![
                        "/usr/bin/ruby".to_string(),
                        "/System/".to_string(),
                        "/Library/".to_string(),
                    ],
                    version_file: Some(".ruby-version".to_string()),
                }],
                install_template: Some("mise-ruby".to_string()),
                install_hint: Some("Install Ruby via a version manager (mise, rbenv)".to_string()),
                depends_on: vec![],
                install_requires: Some(ruby_install_requires),
            },
        );

        // Node
        requirements.insert(
            "node".to_string(),
            Requirement {
                name: "node".to_string(),
                checks: vec![RequirementCheck::ManagedCommand {
                    command: "node".to_string(),
                    managed_path_patterns: vec![
                        "volta/".to_string(),
                        "nvm/".to_string(),
                        "fnm/".to_string(),
                        "mise/".to_string(),
                    ],
                    system_path_patterns: vec!["/usr/bin/node".to_string()],
                    version_file: Some(".node-version".to_string()),
                }],
                install_template: Some("mise-node".to_string()),
                install_hint: Some(
                    "Install Node.js via a version manager (mise, nvm, volta)".to_string(),
                ),
                depends_on: vec![],
                install_requires: Some(node_install_requires),
            },
        );

        // Python
        requirements.insert(
            "python".to_string(),
            Requirement {
                name: "python".to_string(),
                checks: vec![RequirementCheck::Any(vec![
                    RequirementCheck::ManagedCommand {
                        command: "python3".to_string(),
                        managed_path_patterns: vec![
                            "mise/".to_string(),
                            "pyenv/".to_string(),
                            "asdf/".to_string(),
                        ],
                        system_path_patterns: vec!["/usr/bin/python3".to_string()],
                        version_file: Some(".python-version".to_string()),
                    },
                    RequirementCheck::ManagedCommand {
                        command: "python".to_string(),
                        managed_path_patterns: vec![
                            "mise/".to_string(),
                            "pyenv/".to_string(),
                            "asdf/".to_string(),
                        ],
                        system_path_patterns: vec!["/usr/bin/python".to_string()],
                        version_file: Some(".python-version".to_string()),
                    },
                ])],
                install_template: Some("mise-python".to_string()),
                install_hint: Some(
                    "Install Python via a version manager (mise, pyenv)".to_string(),
                ),
                depends_on: vec![],
                install_requires: Some(python_install_requires),
            },
        );

        // Postgres (client tools only)
        requirements.insert(
            "postgres".to_string(),
            Requirement {
                name: "postgres".to_string(),
                checks: vec![RequirementCheck::CommandSucceeds(
                    "psql --version".to_string(),
                )],
                install_template: Some("postgres-install".to_string()),
                install_hint: Some("Install PostgreSQL client tools".to_string()),
                depends_on: vec![],
                install_requires: None,
            },
        );

        // Postgres server (running service)
        requirements.insert(
            "postgres-server".to_string(),
            Requirement {
                name: "postgres-server".to_string(),
                checks: vec![RequirementCheck::ServiceReachable(
                    "pg_isready -q".to_string(),
                )],
                install_template: Some("postgres-install".to_string()),
                install_hint: Some(
                    "Install and start PostgreSQL: brew install postgresql@16 && brew services start postgresql@16".to_string(),
                ),
                depends_on: vec![],
                install_requires: None,
            },
        );

        // Redis server
        requirements.insert(
            "redis-server".to_string(),
            Requirement {
                name: "redis-server".to_string(),
                checks: vec![RequirementCheck::ServiceReachable(
                    "redis-cli ping".to_string(),
                )],
                install_template: Some("redis-install".to_string()),
                install_hint: Some(
                    "Install and start Redis: brew install redis && brew services start redis"
                        .to_string(),
                ),
                depends_on: vec![],
                install_requires: None,
            },
        );

        // Docker
        requirements.insert(
            "docker".to_string(),
            Requirement {
                name: "docker".to_string(),
                checks: vec![RequirementCheck::CommandSucceeds("docker info".to_string())],
                install_template: Some("docker-install".to_string()),
                install_hint: Some("Install Docker Desktop from https://docker.com".to_string()),
                depends_on: vec![],
                install_requires: None,
            },
        );

        // Homebrew
        requirements.insert(
            "brew".to_string(),
            Requirement {
                name: "brew".to_string(),
                checks: vec![RequirementCheck::CommandSucceeds(
                    "brew --version".to_string(),
                )],
                install_template: Some("brew-install".to_string()),
                install_hint: Some(
                    "Install Homebrew: /bin/bash -c \"$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)\"".to_string(),
                ),
                depends_on: vec![],
                install_requires: None,
            },
        );

        // mise
        requirements.insert(
            "mise".to_string(),
            Requirement {
                name: "mise".to_string(),
                checks: vec![RequirementCheck::CommandSucceeds(
                    "mise --version".to_string(),
                )],
                install_template: Some("mise-install".to_string()),
                install_hint: Some("Install mise: https://mise.jdx.dev".to_string()),
                depends_on: vec![],
                install_requires: None,
            },
        );

        // Rust
        requirements.insert(
            "rust".to_string(),
            Requirement {
                name: "rust".to_string(),
                checks: vec![RequirementCheck::CommandSucceeds(
                    "rustc --version".to_string(),
                )],
                install_template: Some("rust-install".to_string()),
                install_hint: Some(
                    "Install Rust: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
                        .to_string(),
                ),
                depends_on: vec![],
                install_requires: None,
            },
        );

        Self { requirements }
    }

    /// Add custom requirements from project config.
    pub fn with_custom(mut self, custom: &HashMap<String, CustomRequirement>) -> Self {
        for (name, req) in custom {
            let checks = vec![match &req.check {
                crate::config::CustomRequirementCheck::CommandSucceeds { command } => {
                    RequirementCheck::CommandSucceeds(command.clone())
                }
                crate::config::CustomRequirementCheck::FileExists { path } => {
                    RequirementCheck::FileExists(path.clone())
                }
                crate::config::CustomRequirementCheck::ServiceReachable { command } => {
                    RequirementCheck::ServiceReachable(command.clone())
                }
            }];
            self.requirements.insert(
                name.clone(),
                Requirement {
                    name: name.clone(),
                    checks,
                    install_template: req.install_template.clone(),
                    install_hint: req.install_hint.clone(),
                    depends_on: vec![],
                    install_requires: None,
                },
            );
        }
        self
    }

    /// Insert a requirement directly (test-only).
    #[cfg(test)]
    pub(crate) fn insert(&mut self, name: String, requirement: Requirement) {
        self.requirements.insert(name, requirement);
    }

    /// Look up a requirement by name.
    pub fn get(&self, name: &str) -> Option<&Requirement> {
        self.requirements.get(name)
    }

    /// Get all known requirement names.
    pub fn known_names(&self) -> Vec<&str> {
        self.requirements.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for RequirementRegistry {
    fn default() -> Self {
        Self::new()
    }
}

fn ruby_install_requires(ctx: &InstallContext) -> Vec<String> {
    if ctx.detected_managers.contains(&"rbenv".to_string()) {
        vec!["rbenv".to_string()]
    } else if ctx.detected_managers.contains(&"mise".to_string()) {
        vec!["mise".to_string()]
    } else {
        // Default to mise
        vec!["mise".to_string()]
    }
}

fn node_install_requires(ctx: &InstallContext) -> Vec<String> {
    if ctx.detected_managers.contains(&"volta".to_string()) {
        vec!["volta".to_string()]
    } else if ctx.detected_managers.contains(&"nvm".to_string()) {
        vec!["nvm".to_string()]
    } else {
        // Default to mise (also used when mise is explicitly detected)
        vec!["mise".to_string()]
    }
}

fn python_install_requires(ctx: &InstallContext) -> Vec<String> {
    if ctx.detected_managers.contains(&"pyenv".to_string()) {
        vec!["pyenv".to_string()]
    } else {
        // Default to mise (also used when mise is explicitly detected)
        vec!["mise".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_new_has_builtins() {
        let registry = RequirementRegistry::new();
        let names = registry.known_names();
        assert!(names.contains(&"ruby"));
        assert!(names.contains(&"node"));
        assert!(names.contains(&"python"));
        assert!(names.contains(&"postgres"));
        assert!(names.contains(&"postgres-server"));
        assert!(names.contains(&"redis-server"));
        assert!(names.contains(&"docker"));
        assert!(names.contains(&"brew"));
        assert!(names.contains(&"mise"));
        assert!(names.contains(&"rust"));
    }

    #[test]
    fn registry_get_known_returns_some() {
        let registry = RequirementRegistry::new();
        assert!(registry.get("ruby").is_some());
        assert!(registry.get("node").is_some());
        assert!(registry.get("postgres-server").is_some());
    }

    #[test]
    fn registry_get_unknown_returns_none() {
        let registry = RequirementRegistry::new();
        assert!(registry.get("nonexistent-tool").is_none());
    }

    #[test]
    fn registry_with_custom_adds_custom() {
        let mut custom = HashMap::new();
        custom.insert(
            "internal-cli".to_string(),
            CustomRequirement {
                check: crate::config::CustomRequirementCheck::CommandSucceeds {
                    command: "internal-cli --version".to_string(),
                },
                install_template: None,
                install_hint: Some("Download from intranet".to_string()),
            },
        );

        let registry = RequirementRegistry::new().with_custom(&custom);
        let req = registry.get("internal-cli").unwrap();
        assert_eq!(req.name, "internal-cli");
        assert_eq!(req.install_hint, Some("Download from intranet".to_string()));
    }

    #[test]
    fn registry_known_names_includes_builtins() {
        let registry = RequirementRegistry::new();
        let names = registry.known_names();
        // At least 10 built-in requirements
        assert!(names.len() >= 10);
    }

    #[test]
    fn ruby_requirement_has_install_requires() {
        let registry = RequirementRegistry::new();
        let ruby = registry.get("ruby").unwrap();
        assert!(ruby.install_requires.is_some());
    }

    #[test]
    fn ruby_install_requires_defaults_to_mise() {
        let ctx = InstallContext {
            detected_managers: vec![],
            platform: Platform::MacOS,
        };
        let deps = ruby_install_requires(&ctx);
        assert_eq!(deps, vec!["mise"]);
    }

    #[test]
    fn ruby_install_requires_uses_rbenv_when_detected() {
        let ctx = InstallContext {
            detected_managers: vec!["rbenv".to_string()],
            platform: Platform::MacOS,
        };
        let deps = ruby_install_requires(&ctx);
        assert_eq!(deps, vec!["rbenv"]);
    }

    #[test]
    fn custom_requirement_overrides_builtin() {
        let mut custom = HashMap::new();
        custom.insert(
            "ruby".to_string(),
            CustomRequirement {
                check: crate::config::CustomRequirementCheck::CommandSucceeds {
                    command: "my-ruby --version".to_string(),
                },
                install_template: None,
                install_hint: Some("Use our custom Ruby".to_string()),
            },
        );

        let registry = RequirementRegistry::new().with_custom(&custom);
        let ruby = registry.get("ruby").unwrap();
        assert_eq!(ruby.install_hint, Some("Use our custom Ruby".to_string()));
    }

    #[test]
    fn platform_current_returns_valid() {
        let platform = Platform::current();
        // Should return one of the valid platforms
        assert!(matches!(
            platform,
            Platform::MacOS | Platform::Linux | Platform::Windows
        ));
    }

    #[test]
    fn ruby_depends_on_rbenv_when_rbenv_detected() {
        // Verify the dynamic install_requires on the registry Requirement
        // switches to rbenv when it's in detected_managers.
        let registry = RequirementRegistry::new();
        let ruby = registry.get("ruby").unwrap();
        let install_requires_fn = ruby
            .install_requires
            .expect("ruby should have install_requires");
        let ctx = InstallContext {
            detected_managers: vec!["rbenv".to_string()],
            platform: Platform::MacOS,
        };
        let deps = install_requires_fn(&ctx);
        assert_eq!(deps, vec!["rbenv"]);
    }

    #[test]
    fn node_depends_on_nvm_when_nvm_detected() {
        // Verify the dynamic install_requires on the registry Requirement
        // switches to nvm when it's in detected_managers.
        let registry = RequirementRegistry::new();
        let node = registry.get("node").unwrap();
        let install_requires_fn = node
            .install_requires
            .expect("node should have install_requires");
        let ctx = InstallContext {
            detected_managers: vec!["nvm".to_string()],
            platform: Platform::MacOS,
        };
        let deps = install_requires_fn(&ctx);
        assert_eq!(deps, vec!["nvm"]);
    }
}
