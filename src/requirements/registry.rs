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

        // Bundler (Ruby gem manager, ships with Ruby but may be missing or wrong version)
        requirements.insert(
            "bundler".to_string(),
            Requirement {
                name: "bundler".to_string(),
                checks: vec![RequirementCheck::CommandSucceeds(
                    "bundle --version".to_string(),
                )],
                install_template: None,
                install_hint: Some("Install Bundler: gem install bundler".to_string()),
                depends_on: vec!["ruby".to_string()],
                install_requires: None,
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

        // Java
        requirements.insert(
            "java".to_string(),
            Requirement {
                name: "java".to_string(),
                checks: vec![RequirementCheck::CommandSucceeds(
                    "java -version".to_string(),
                )],
                install_template: None,
                install_hint: Some(
                    "Install a JDK (e.g., brew install openjdk, or https://adoptium.net)"
                        .to_string(),
                ),
                depends_on: vec![],
                install_requires: None,
            },
        );

        // Terraform
        requirements.insert(
            "terraform".to_string(),
            Requirement {
                name: "terraform".to_string(),
                checks: vec![RequirementCheck::CommandSucceeds(
                    "terraform version".to_string(),
                )],
                install_template: None,
                install_hint: Some(
                    "Install Terraform: brew install terraform, or https://terraform.io/downloads"
                        .to_string(),
                ),
                depends_on: vec![],
                install_requires: None,
            },
        );

        // Go
        requirements.insert(
            "go".to_string(),
            Requirement {
                name: "go".to_string(),
                checks: vec![RequirementCheck::CommandSucceeds("go version".to_string())],
                install_template: None,
                install_hint: Some(
                    "Install Go: brew install go, or https://go.dev/dl/".to_string(),
                ),
                depends_on: vec![],
                install_requires: None,
            },
        );

        // Elixir
        requirements.insert(
            "elixir".to_string(),
            Requirement {
                name: "elixir".to_string(),
                checks: vec![RequirementCheck::CommandSucceeds(
                    "elixir --version".to_string(),
                )],
                install_template: None,
                install_hint: Some(
                    "Install Elixir: brew install elixir, or https://elixir-lang.org/install.html"
                        .to_string(),
                ),
                depends_on: vec![],
                install_requires: None,
            },
        );

        // Swift
        requirements.insert(
            "swift".to_string(),
            Requirement {
                name: "swift".to_string(),
                checks: vec![RequirementCheck::CommandSucceeds(
                    "swift --version".to_string(),
                )],
                install_template: None,
                install_hint: Some(
                    "Install Swift: included with Xcode, or https://swift.org/install".to_string(),
                ),
                depends_on: vec![],
                install_requires: None,
            },
        );

        // Dart
        requirements.insert(
            "dart".to_string(),
            Requirement {
                name: "dart".to_string(),
                checks: vec![RequirementCheck::CommandSucceeds(
                    "dart --version".to_string(),
                )],
                install_template: None,
                install_hint: Some(
                    "Install Dart: brew install dart, or https://dart.dev/get-dart".to_string(),
                ),
                depends_on: vec![],
                install_requires: None,
            },
        );

        // Flutter
        requirements.insert(
            "flutter".to_string(),
            Requirement {
                name: "flutter".to_string(),
                checks: vec![RequirementCheck::CommandSucceeds(
                    "flutter --version".to_string(),
                )],
                install_template: None,
                install_hint: Some(
                    "Install Flutter: https://docs.flutter.dev/get-started/install".to_string(),
                ),
                depends_on: vec![],
                install_requires: None,
            },
        );

        // .NET
        requirements.insert(
            "dotnet".to_string(),
            Requirement {
                name: "dotnet".to_string(),
                checks: vec![RequirementCheck::CommandSucceeds(
                    "dotnet --version".to_string(),
                )],
                install_template: None,
                install_hint: Some(
                    "Install .NET: brew install dotnet, or https://dot.net/download".to_string(),
                ),
                depends_on: vec![],
                install_requires: None,
            },
        );

        // Deno
        requirements.insert(
            "deno".to_string(),
            Requirement {
                name: "deno".to_string(),
                checks: vec![RequirementCheck::CommandSucceeds(
                    "deno --version".to_string(),
                )],
                install_template: None,
                install_hint: Some(
                    "Install Deno: brew install deno, or https://deno.land/#installation"
                        .to_string(),
                ),
                depends_on: vec![],
                install_requires: None,
            },
        );

        // PHP
        requirements.insert(
            "php".to_string(),
            Requirement {
                name: "php".to_string(),
                checks: vec![RequirementCheck::CommandSucceeds(
                    "php --version".to_string(),
                )],
                install_template: None,
                install_hint: Some("Install PHP: brew install php, or https://php.net".to_string()),
                depends_on: vec![],
                install_requires: None,
            },
        );

        // Maven
        requirements.insert(
            "mvn".to_string(),
            Requirement {
                name: "mvn".to_string(),
                checks: vec![RequirementCheck::CommandSucceeds(
                    "mvn --version".to_string(),
                )],
                install_template: None,
                install_hint: Some(
                    "Install Maven: brew install maven, or https://maven.apache.org/download.cgi"
                        .to_string(),
                ),
                depends_on: vec!["java".to_string()],
                install_requires: None,
            },
        );

        // Helm
        requirements.insert(
            "helm".to_string(),
            Requirement {
                name: "helm".to_string(),
                checks: vec![RequirementCheck::CommandSucceeds(
                    "helm version".to_string(),
                )],
                install_template: None,
                install_hint: Some(
                    "Install Helm: brew install helm, or https://helm.sh/docs/intro/install/"
                        .to_string(),
                ),
                depends_on: vec![],
                install_requires: None,
            },
        );

        // Ansible
        requirements.insert(
            "ansible".to_string(),
            Requirement {
                name: "ansible".to_string(),
                checks: vec![RequirementCheck::CommandSucceeds(
                    "ansible --version".to_string(),
                )],
                install_template: None,
                install_hint: Some(
                    "Install Ansible: pip install ansible, or brew install ansible".to_string(),
                ),
                depends_on: vec![],
                install_requires: None,
            },
        );

        // Pulumi
        requirements.insert(
            "pulumi".to_string(),
            Requirement {
                name: "pulumi".to_string(),
                checks: vec![RequirementCheck::CommandSucceeds(
                    "pulumi version".to_string(),
                )],
                install_template: None,
                install_hint: Some(
                    "Install Pulumi: brew install pulumi, or https://www.pulumi.com/docs/install/"
                        .to_string(),
                ),
                depends_on: vec![],
                install_requires: None,
            },
        );

        // pre-commit
        requirements.insert(
            "pre-commit".to_string(),
            Requirement {
                name: "pre-commit".to_string(),
                checks: vec![RequirementCheck::CommandSucceeds(
                    "pre-commit --version".to_string(),
                )],
                install_template: None,
                install_hint: Some(
                    "Install pre-commit: pip install pre-commit, or brew install pre-commit"
                        .to_string(),
                ),
                depends_on: vec![],
                install_requires: None,
            },
        );

        // Diesel CLI
        requirements.insert(
            "diesel".to_string(),
            Requirement {
                name: "diesel".to_string(),
                checks: vec![RequirementCheck::CommandSucceeds(
                    "diesel --version".to_string(),
                )],
                install_template: None,
                install_hint: Some("Install Diesel CLI: cargo install diesel_cli".to_string()),
                depends_on: vec!["rust".to_string()],
                install_requires: None,
            },
        );

        // Version managers (used by install templates)

        // fnm (Fast Node Manager)
        requirements.insert(
            "fnm".to_string(),
            Requirement {
                name: "fnm".to_string(),
                checks: vec![RequirementCheck::CommandSucceeds(
                    "fnm --version".to_string(),
                )],
                install_template: None,
                install_hint: Some(
                    "Install fnm: brew install fnm, or https://github.com/Schniz/fnm".to_string(),
                ),
                depends_on: vec![],
                install_requires: None,
            },
        );

        // rbenv
        requirements.insert(
            "rbenv".to_string(),
            Requirement {
                name: "rbenv".to_string(),
                checks: vec![RequirementCheck::CommandSucceeds(
                    "rbenv --version".to_string(),
                )],
                install_template: None,
                install_hint: Some(
                    "Install rbenv: brew install rbenv, or https://github.com/rbenv/rbenv"
                        .to_string(),
                ),
                depends_on: vec![],
                install_requires: None,
            },
        );

        // nvm (Node Version Manager)
        requirements.insert(
            "nvm".to_string(),
            Requirement {
                name: "nvm".to_string(),
                checks: vec![RequirementCheck::CommandSucceeds(
                    "nvm --version".to_string(),
                )],
                install_template: None,
                install_hint: Some(
                    "Install nvm: https://github.com/nvm-sh/nvm#installing-and-updating"
                        .to_string(),
                ),
                depends_on: vec![],
                install_requires: None,
            },
        );

        // Volta
        requirements.insert(
            "volta".to_string(),
            Requirement {
                name: "volta".to_string(),
                checks: vec![RequirementCheck::CommandSucceeds(
                    "volta --version".to_string(),
                )],
                install_template: None,
                install_hint: Some("Install Volta: https://volta.sh".to_string()),
                depends_on: vec![],
                install_requires: None,
            },
        );

        // pyenv
        requirements.insert(
            "pyenv".to_string(),
            Requirement {
                name: "pyenv".to_string(),
                checks: vec![RequirementCheck::CommandSucceeds(
                    "pyenv --version".to_string(),
                )],
                install_template: None,
                install_hint: Some(
                    "Install pyenv: brew install pyenv, or https://github.com/pyenv/pyenv"
                        .to_string(),
                ),
                depends_on: vec![],
                install_requires: None,
            },
        );

        // asdf
        requirements.insert(
            "asdf".to_string(),
            Requirement {
                name: "asdf".to_string(),
                checks: vec![RequirementCheck::CommandSucceeds(
                    "asdf --version".to_string(),
                )],
                install_template: None,
                install_hint: Some(
                    "Install asdf: brew install asdf, or https://asdf-vm.com/guide/getting-started.html"
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
        let expected = [
            "ruby",
            "bundler",
            "node",
            "python",
            "postgres",
            "postgres-server",
            "redis-server",
            "docker",
            "brew",
            "mise",
            "rust",
            "java",
            "terraform",
            "go",
            "elixir",
            "swift",
            "dart",
            "flutter",
            "dotnet",
            "deno",
            "php",
            "mvn",
            "helm",
            "ansible",
            "pulumi",
            "pre-commit",
            "diesel",
            "fnm",
            "rbenv",
            "nvm",
            "volta",
            "pyenv",
            "asdf",
        ];
        for name in &expected {
            assert!(names.contains(name), "registry missing built-in '{}'", name);
        }
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

    /// Helper: assert a requirement uses CommandSucceeds with the given command string.
    fn assert_command_succeeds(req: &Requirement, expected_cmd: &str) {
        assert_eq!(req.checks.len(), 1, "{}: expected 1 check", req.name);
        match &req.checks[0] {
            RequirementCheck::CommandSucceeds(cmd) => {
                assert_eq!(cmd, expected_cmd, "{}: wrong check command", req.name);
            }
            _ => panic!(
                "{}: expected CommandSucceeds, got different check type",
                req.name
            ),
        }
    }

    // --- Comprehensive property tests for all CommandSucceeds requirements ---

    #[test]
    fn command_succeeds_requirements_have_correct_checks() {
        let registry = RequirementRegistry::new();

        // (name, expected_command, expected_depends_on, must_have_install_hint)
        let expectations: &[(&str, &str, &[&str], bool)] = &[
            ("bundler", "bundle --version", &["ruby"], true),
            ("postgres", "psql --version", &[], true),
            ("docker", "docker info", &[], true),
            ("brew", "brew --version", &[], true),
            ("mise", "mise --version", &[], true),
            ("rust", "rustc --version", &[], true),
            ("java", "java -version", &[], true),
            ("terraform", "terraform version", &[], true),
            ("go", "go version", &[], true),
            ("elixir", "elixir --version", &[], true),
            ("swift", "swift --version", &[], true),
            ("dart", "dart --version", &[], true),
            ("flutter", "flutter --version", &[], true),
            ("dotnet", "dotnet --version", &[], true),
            ("deno", "deno --version", &[], true),
            ("php", "php --version", &[], true),
            ("mvn", "mvn --version", &["java"], true),
            ("helm", "helm version", &[], true),
            ("ansible", "ansible --version", &[], true),
            ("pulumi", "pulumi version", &[], true),
            ("pre-commit", "pre-commit --version", &[], true),
            ("diesel", "diesel --version", &["rust"], true),
            ("fnm", "fnm --version", &[], true),
            ("rbenv", "rbenv --version", &[], true),
            ("nvm", "nvm --version", &[], true),
            ("volta", "volta --version", &[], true),
            ("pyenv", "pyenv --version", &[], true),
            ("asdf", "asdf --version", &[], true),
        ];

        for (name, expected_cmd, expected_deps, must_have_hint) in expectations {
            let req = registry
                .get(name)
                .unwrap_or_else(|| panic!("requirement '{}' not in registry", name));

            assert_command_succeeds(req, expected_cmd);

            let expected_deps: Vec<String> = expected_deps.iter().map(|s| s.to_string()).collect();
            assert_eq!(req.depends_on, expected_deps, "{}: wrong depends_on", name);

            if *must_have_hint {
                assert!(req.install_hint.is_some(), "{}: missing install_hint", name);
            }
        }
    }

    // --- ManagedCommand requirements have correct check types ---

    #[test]
    fn managed_command_requirements_have_correct_checks() {
        let registry = RequirementRegistry::new();

        for name in &["ruby", "node"] {
            let req = registry.get(name).unwrap();
            assert_eq!(req.checks.len(), 1, "{}: expected 1 check", name);
            assert!(
                matches!(req.checks[0], RequirementCheck::ManagedCommand { .. }),
                "{}: expected ManagedCommand check type",
                name
            );
        }
    }

    #[test]
    fn python_uses_any_check_for_python3_and_python() {
        let registry = RequirementRegistry::new();
        let python = registry.get("python").unwrap();
        assert_eq!(python.checks.len(), 1);
        assert!(
            matches!(python.checks[0], RequirementCheck::Any(_)),
            "python: expected Any check wrapping python3 and python"
        );
    }

    // --- ServiceReachable requirements ---

    #[test]
    fn service_requirements_have_service_reachable_checks() {
        let registry = RequirementRegistry::new();

        let services: &[(&str, &str)] = &[
            ("postgres-server", "pg_isready -q"),
            ("redis-server", "redis-cli ping"),
        ];

        for (name, expected_cmd) in services {
            let req = registry.get(name).unwrap();
            assert_eq!(req.checks.len(), 1, "{}: expected 1 check", name);
            match &req.checks[0] {
                RequirementCheck::ServiceReachable(cmd) => {
                    assert_eq!(cmd, expected_cmd, "{}: wrong service check command", name);
                }
                _ => panic!("{}: expected ServiceReachable check type", name),
            }
        }
    }

    // --- Dependency chain correctness ---

    #[test]
    fn requirements_with_dependencies_point_to_registered_requirements() {
        let registry = RequirementRegistry::new();
        let names: std::collections::HashSet<&str> = registry.known_names().into_iter().collect();

        for name in registry.known_names() {
            let req = registry.get(name).unwrap();
            for dep in &req.depends_on {
                assert!(
                    names.contains(dep.as_str()),
                    "requirement '{}' depends on '{}' which is not registered",
                    name,
                    dep
                );
            }
        }
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
