//! Error pattern registry for step failure recovery.
//!
//! Matches step failure output against known patterns to suggest fixes.
//! Patterns have confidence levels: `High` confidence matches produce
//! actionable Fix options in the recovery menu, while `Low` confidence
//! matches produce hints below the error block.

use regex::Regex;
use std::sync::LazyLock;

/// Confidence level for a pattern match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    /// Strong pattern match — show as an actionable Fix option in the menu.
    High,
    /// Weaker match — show as a hint below the error block only.
    Low,
}

/// A suggested fix command for a recognized error.
#[derive(Debug, Clone)]
pub struct FixSuggestion {
    /// Shown in menu (e.g., "bundle update nokogiri").
    pub label: String,
    /// Command to actually run.
    pub command: String,
    /// Why this fix is suggested.
    pub explanation: String,
    /// How confident the match is.
    pub confidence: Confidence,
}

/// Context about the step that failed, used for pattern filtering.
pub struct StepContext<'a> {
    /// Step name.
    pub name: &'a str,
    /// Step command.
    pub command: &'a str,
    /// Step requirements (e.g., ["postgres-server", "ruby"]).
    pub requires: &'a [String],
    /// Template name, if any.
    pub template: Option<&'a str>,
}

/// Controls when a pattern fires.
#[derive(Debug)]
pub enum PatternContext {
    /// Fires for any step.
    Always,
    /// Only fires if the step command contains this substring.
    CommandContains(&'static str),
    /// Only fires if the step requires one of these.
    RequiresAny(&'static [&'static str]),
}

impl PatternContext {
    fn matches(&self, ctx: &StepContext) -> bool {
        match self {
            PatternContext::Always => true,
            PatternContext::CommandContains(substr) => {
                // Support pipe-separated alternatives: "pip|python"
                substr.split('|').any(|s| ctx.command.contains(s))
            }
            PatternContext::RequiresAny(reqs) => {
                reqs.iter().any(|r| ctx.requires.iter().any(|s| s == r))
            }
        }
    }
}

/// A registered error pattern.
pub struct ErrorPattern {
    /// Pattern name (for debugging).
    pub name: &'static str,
    /// Regex to match against error output.
    pub regex: &'static str,
    /// When this pattern fires.
    pub context: PatternContext,
    /// Confidence level.
    pub confidence: Confidence,
    /// Build a fix suggestion from captures and context. Returns None if
    /// captures don't contain enough info.
    pub suggest: fn(&regex::Captures, &StepContext) -> Option<FixSuggestion>,
}

// --- Compiled regexes (one-time via LazyLock) ---

macro_rules! lazy_regex {
    ($name:ident, $pattern:expr) => {
        static $name: LazyLock<Regex> = LazyLock::new(|| Regex::new($pattern).unwrap());
    };
}

lazy_regex!(
    RE_BUNDLER_NATIVE_EXT,
    r"error occurred while installing (\S+)"
);
lazy_regex!(
    RE_BUNDLER_VERSION_CONFLICT,
    r"Bundler could not find compatible versions"
);
lazy_regex!(RE_BUNDLER_GEM_NOT_FOUND, r"Could not find gem '([^']+)'");
lazy_regex!(
    RE_NPM_MODULE_NOT_FOUND,
    r"Cannot find module|MODULE_NOT_FOUND"
);
lazy_regex!(RE_YARN_INTEGRITY, r"integrity check failed");
lazy_regex!(
    RE_PIP_MODULE_NOT_FOUND,
    r"ModuleNotFoundError: No module named '([^']+)'"
);
lazy_regex!(
    RE_POSTGRES_CONN_REFUSED,
    r"could not connect to server.*Is the server running"
);
lazy_regex!(
    RE_POSTGRES_DB_NOT_EXIST,
    r#"FATAL:.*database "([^"]+)" does not exist"#
);
lazy_regex!(
    RE_POSTGRES_ROLE_NOT_EXIST,
    r#"FATAL:.*role "([^"]+)" does not exist"#
);
lazy_regex!(
    RE_REDIS_CONN_REFUSED,
    r"Connection refused.*6379|Error connecting to Redis"
);
lazy_regex!(RE_DOCKER_DAEMON, r"Cannot connect to the Docker daemon");
lazy_regex!(RE_COMMAND_NOT_FOUND, r"command not found: (\S+)");
lazy_regex!(RE_PERMISSION_DENIED, r"Permission denied");
lazy_regex!(
    RE_RUBY_VERSION_MISMATCH,
    r"Your Ruby version is .+ but your Gemfile specified"
);

/// Platform-aware service start command.
fn service_start_command(service: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("brew services start {}", service)
    } else {
        format!("systemctl start {}", service)
    }
}

/// Return all built-in error patterns, ordered by specificity.
pub fn built_in_patterns() -> Vec<ErrorPattern> {
    vec![
        ErrorPattern {
            name: "bundler_native_ext",
            regex: RE_BUNDLER_NATIVE_EXT.as_str(),
            context: PatternContext::CommandContains("bundle"),
            confidence: Confidence::High,
            suggest: |caps, _ctx| {
                let gem = caps.get(1)?.as_str();
                Some(FixSuggestion {
                    label: format!("bundle update {}", gem),
                    command: format!("bundle update {}", gem),
                    explanation: format!("{} failed to build native extensions", gem),
                    confidence: Confidence::High,
                })
            },
        },
        ErrorPattern {
            name: "bundler_version_conflict",
            regex: RE_BUNDLER_VERSION_CONFLICT.as_str(),
            context: PatternContext::CommandContains("bundle"),
            confidence: Confidence::High,
            suggest: |_caps, _ctx| {
                Some(FixSuggestion {
                    label: "bundle update".to_string(),
                    command: "bundle update".to_string(),
                    explanation: "Bundler version conflict detected".to_string(),
                    confidence: Confidence::High,
                })
            },
        },
        ErrorPattern {
            name: "bundler_gem_not_found",
            regex: RE_BUNDLER_GEM_NOT_FOUND.as_str(),
            context: PatternContext::CommandContains("bundle"),
            confidence: Confidence::High,
            suggest: |_caps, _ctx| {
                Some(FixSuggestion {
                    label: "bundle install".to_string(),
                    command: "bundle install".to_string(),
                    explanation: "Required gem not found".to_string(),
                    confidence: Confidence::High,
                })
            },
        },
        ErrorPattern {
            name: "npm_module_not_found",
            regex: RE_NPM_MODULE_NOT_FOUND.as_str(),
            context: PatternContext::CommandContains("npm"),
            confidence: Confidence::High,
            suggest: |_caps, _ctx| {
                Some(FixSuggestion {
                    label: "npm install".to_string(),
                    command: "npm install".to_string(),
                    explanation: "Node module not found".to_string(),
                    confidence: Confidence::High,
                })
            },
        },
        ErrorPattern {
            name: "yarn_integrity",
            regex: RE_YARN_INTEGRITY.as_str(),
            context: PatternContext::CommandContains("yarn"),
            confidence: Confidence::High,
            suggest: |_caps, _ctx| {
                Some(FixSuggestion {
                    label: "yarn install --check-files".to_string(),
                    command: "yarn install --check-files".to_string(),
                    explanation: "Yarn integrity check failed".to_string(),
                    confidence: Confidence::High,
                })
            },
        },
        ErrorPattern {
            name: "pip_module_not_found",
            regex: RE_PIP_MODULE_NOT_FOUND.as_str(),
            context: PatternContext::CommandContains("pip|python"),
            confidence: Confidence::High,
            suggest: |caps, _ctx| {
                let module = caps.get(1)?.as_str();
                Some(FixSuggestion {
                    label: format!("pip install {}", module),
                    command: format!("pip install {}", module),
                    explanation: format!("Python module '{}' not found", module),
                    confidence: Confidence::High,
                })
            },
        },
        ErrorPattern {
            name: "postgres_conn_refused",
            regex: RE_POSTGRES_CONN_REFUSED.as_str(),
            context: PatternContext::RequiresAny(&["postgres-server"]),
            confidence: Confidence::High,
            suggest: |_caps, _ctx| {
                let cmd = service_start_command("postgresql");
                Some(FixSuggestion {
                    label: cmd.clone(),
                    command: cmd,
                    explanation: "PostgreSQL server is not running".to_string(),
                    confidence: Confidence::High,
                })
            },
        },
        ErrorPattern {
            name: "postgres_db_not_exist",
            regex: RE_POSTGRES_DB_NOT_EXIST.as_str(),
            context: PatternContext::RequiresAny(&["postgres-server"]),
            confidence: Confidence::High,
            suggest: |caps, _ctx| {
                let db = caps.get(1)?.as_str();
                Some(FixSuggestion {
                    label: format!("createdb {}", db),
                    command: format!("createdb {}", db),
                    explanation: format!("Database '{}' does not exist", db),
                    confidence: Confidence::High,
                })
            },
        },
        ErrorPattern {
            name: "postgres_role_not_exist",
            regex: RE_POSTGRES_ROLE_NOT_EXIST.as_str(),
            context: PatternContext::RequiresAny(&["postgres-server"]),
            confidence: Confidence::High,
            suggest: |caps, _ctx| {
                let role = caps.get(1)?.as_str();
                Some(FixSuggestion {
                    label: format!("createuser {}", role),
                    command: format!("createuser {}", role),
                    explanation: format!("PostgreSQL role '{}' does not exist", role),
                    confidence: Confidence::High,
                })
            },
        },
        ErrorPattern {
            name: "redis_conn_refused",
            regex: RE_REDIS_CONN_REFUSED.as_str(),
            context: PatternContext::RequiresAny(&["redis-server"]),
            confidence: Confidence::High,
            suggest: |_caps, _ctx| {
                let cmd = service_start_command("redis");
                Some(FixSuggestion {
                    label: cmd.clone(),
                    command: cmd,
                    explanation: "Redis server is not running".to_string(),
                    confidence: Confidence::High,
                })
            },
        },
        ErrorPattern {
            name: "docker_daemon",
            regex: RE_DOCKER_DAEMON.as_str(),
            context: PatternContext::Always,
            confidence: Confidence::High,
            suggest: |_caps, _ctx| {
                let cmd = if cfg!(target_os = "macos") {
                    "open -a Docker".to_string()
                } else {
                    "systemctl start docker".to_string()
                };
                Some(FixSuggestion {
                    label: cmd.clone(),
                    command: cmd,
                    explanation: "Docker daemon is not running".to_string(),
                    confidence: Confidence::High,
                })
            },
        },
        ErrorPattern {
            name: "command_not_found",
            regex: RE_COMMAND_NOT_FOUND.as_str(),
            context: PatternContext::Always,
            confidence: Confidence::Low,
            suggest: |caps, _ctx| {
                let cmd = caps.get(1)?.as_str();
                Some(FixSuggestion {
                    label: format!("install {}", cmd),
                    command: String::new(),
                    explanation: format!("'{}' is not installed", cmd),
                    confidence: Confidence::Low,
                })
            },
        },
        ErrorPattern {
            name: "permission_denied",
            regex: RE_PERMISSION_DENIED.as_str(),
            context: PatternContext::Always,
            confidence: Confidence::Low,
            suggest: |_caps, _ctx| {
                Some(FixSuggestion {
                    label: "check file permissions".to_string(),
                    command: String::new(),
                    explanation: "Permission denied".to_string(),
                    confidence: Confidence::Low,
                })
            },
        },
        ErrorPattern {
            name: "ruby_version_mismatch",
            regex: RE_RUBY_VERSION_MISMATCH.as_str(),
            context: PatternContext::CommandContains("bundle"),
            confidence: Confidence::Low,
            suggest: |_caps, _ctx| {
                Some(FixSuggestion {
                    label: "check .ruby-version".to_string(),
                    command: String::new(),
                    explanation: "Ruby version doesn't match Gemfile requirement".to_string(),
                    confidence: Confidence::Low,
                })
            },
        },
    ]
}

/// Find the first high-confidence fix for the given error output and context.
pub fn find_fix(error_output: &str, context: &StepContext) -> Option<FixSuggestion> {
    for pattern in built_in_patterns() {
        if pattern.confidence != Confidence::High {
            continue;
        }
        if !pattern.context.matches(context) {
            continue;
        }
        let re = Regex::new(pattern.regex).ok()?;
        if let Some(caps) = re.captures(error_output) {
            if let Some(fix) = (pattern.suggest)(&caps, context) {
                return Some(fix);
            }
        }
    }
    None
}

/// Find the first low-confidence hint for the given error output and context.
pub fn find_hint(error_output: &str, context: &StepContext) -> Option<String> {
    for pattern in built_in_patterns() {
        if pattern.confidence != Confidence::Low {
            continue;
        }
        if !pattern.context.matches(context) {
            continue;
        }
        let re = Regex::new(pattern.regex).ok()?;
        if let Some(caps) = re.captures(error_output) {
            if let Some(fix) = (pattern.suggest)(&caps, context) {
                return Some(format!("You might try: {}", fix.label));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_context() -> StepContext<'static> {
        StepContext {
            name: "test",
            command: "echo test",
            requires: &[],
            template: None,
        }
    }

    fn bundle_context() -> StepContext<'static> {
        StepContext {
            name: "bundler",
            command: "bundle install",
            requires: &[],
            template: None,
        }
    }

    #[test]
    fn all_patterns_compile() {
        for pattern in built_in_patterns() {
            Regex::new(pattern.regex).unwrap_or_else(|e| {
                panic!("Pattern '{}' failed to compile: {}", pattern.name, e);
            });
        }
    }

    #[test]
    fn no_duplicate_pattern_names() {
        let patterns = built_in_patterns();
        let mut seen = std::collections::HashSet::new();
        for p in &patterns {
            assert!(seen.insert(p.name), "Duplicate pattern name: {}", p.name);
        }
    }

    #[test]
    fn bundler_native_ext_matches() {
        let ctx = bundle_context();
        let error = "An error occurred while installing nokogiri (1.14.0)";
        let fix = find_fix(error, &ctx).unwrap();
        assert_eq!(fix.command, "bundle update nokogiri");
    }

    #[test]
    fn bundler_native_ext_extracts_gem_name() {
        let ctx = bundle_context();
        let error = "An error occurred while installing pg (1.5.0)";
        let fix = find_fix(error, &ctx).unwrap();
        assert!(fix.command.contains("pg"));
        assert!(fix.label.contains("pg"));
    }

    #[test]
    fn bundler_version_conflict_matches() {
        let ctx = bundle_context();
        let error = "Bundler could not find compatible versions for gem \"rails\"";
        let fix = find_fix(error, &ctx).unwrap();
        assert_eq!(fix.command, "bundle update");
    }

    #[test]
    fn npm_module_not_found_matches() {
        let ctx = StepContext {
            name: "npm",
            command: "npm run build",
            requires: &[],
            template: None,
        };
        let error = "Error: Cannot find module 'webpack'";
        let fix = find_fix(error, &ctx).unwrap();
        assert_eq!(fix.command, "npm install");
    }

    #[test]
    fn postgres_conn_refused_matches() {
        let requires = vec!["postgres-server".to_string()];
        let ctx = StepContext {
            name: "db_setup",
            command: "rails db:create",
            requires: &requires,
            template: None,
        };
        let error = "PG::ConnectionBad: could not connect to server: Is the server running on host";
        let fix = find_fix(error, &ctx).unwrap();
        assert!(fix.command.contains("postgresql"));
    }

    #[test]
    fn postgres_db_not_exist_extracts_name() {
        let requires = vec!["postgres-server".to_string()];
        let ctx = StepContext {
            name: "db_setup",
            command: "rails db:create",
            requires: &requires,
            template: None,
        };
        let error = "FATAL:  database \"myapp_dev\" does not exist";
        let fix = find_fix(error, &ctx).unwrap();
        assert_eq!(fix.command, "createdb myapp_dev");
    }

    #[test]
    fn context_filter_excludes_unrelated() {
        // Postgres pattern shouldn't fire for a step without postgres-server requirement
        let ctx = default_context();
        let error = "PG::ConnectionBad: could not connect to server: Is the server running on host";
        assert!(find_fix(error, &ctx).is_none());
    }

    #[test]
    fn context_command_contains_filters() {
        // Bundle pattern shouldn't fire for non-bundle commands
        let ctx = StepContext {
            name: "test",
            command: "python setup.py",
            requires: &[],
            template: None,
        };
        let error = "An error occurred while installing nokogiri (1.14.0)";
        assert!(find_fix(error, &ctx).is_none());
    }

    #[test]
    fn high_confidence_returned_by_find_fix() {
        let ctx = bundle_context();
        let error = "An error occurred while installing nokogiri (1.14.0)";
        let fix = find_fix(error, &ctx).unwrap();
        assert_eq!(fix.confidence, Confidence::High);
    }

    #[test]
    fn low_confidence_returned_by_find_hint() {
        let ctx = default_context();
        let error = "bash: command not found: jq";
        let hint = find_hint(error, &ctx).unwrap();
        assert!(hint.contains("install jq"));
    }

    #[test]
    fn find_fix_ignores_low_confidence() {
        let ctx = default_context();
        let error = "bash: command not found: jq";
        // find_fix should NOT return low-confidence matches
        assert!(find_fix(error, &ctx).is_none());
    }

    #[test]
    fn find_hint_ignores_high_confidence() {
        let ctx = bundle_context();
        let error = "An error occurred while installing nokogiri (1.14.0)";
        // find_hint should NOT return high-confidence matches
        assert!(find_hint(error, &ctx).is_none());
    }

    #[test]
    fn first_match_wins() {
        let ctx = bundle_context();
        // This matches bundler_native_ext first (more specific than version_conflict)
        let error = "An error occurred while installing nokogiri (1.14.0)";
        let fix = find_fix(error, &ctx).unwrap();
        assert_eq!(fix.command, "bundle update nokogiri");
    }

    #[test]
    fn no_match_returns_none() {
        let ctx = default_context();
        let error = "Some completely unrecognized error output";
        assert!(find_fix(error, &ctx).is_none());
        assert!(find_hint(error, &ctx).is_none());
    }

    #[test]
    fn docker_daemon_matches_any_step() {
        let ctx = default_context();
        let error = "Cannot connect to the Docker daemon at unix:///var/run/docker.sock";
        let fix = find_fix(error, &ctx).unwrap();
        assert!(fix.command.contains("docker") || fix.command.contains("Docker"));
    }

    #[test]
    fn pip_module_not_found_matches() {
        let ctx = StepContext {
            name: "deps",
            command: "pip install -r requirements.txt",
            requires: &[],
            template: None,
        };
        let error = "ModuleNotFoundError: No module named 'flask'";
        let fix = find_fix(error, &ctx).unwrap();
        assert_eq!(fix.command, "pip install flask");
    }

    #[test]
    fn permission_denied_returns_hint() {
        let ctx = default_context();
        let error = "Permission denied (os error 13)";
        let hint = find_hint(error, &ctx).unwrap();
        assert!(hint.contains("permissions"));
    }

    #[test]
    fn ruby_version_mismatch_returns_hint() {
        let ctx = bundle_context();
        let error = "Your Ruby version is 3.2.0 but your Gemfile specified 3.3.0";
        let hint = find_hint(error, &ctx).unwrap();
        assert!(hint.contains(".ruby-version"));
    }
}
