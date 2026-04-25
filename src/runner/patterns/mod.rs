//! Error pattern registry for step failure recovery.
//!
//! Matches step failure output against known patterns to suggest fixes.
//! Patterns have confidence levels: `High` confidence matches produce
//! actionable Fix options in the recovery menu, while `Low` confidence
//! matches produce hints below the error block.
//!
//! Patterns are organized by ecosystem in submodules, each exporting a
//! `patterns()` function. The registry collects them all in specificity
//! order via [`built_in_patterns`].

use regex::Regex;

/// Lazily compile a regex once. Used in ecosystem modules.
macro_rules! lazy_regex {
    ($name:ident, $pattern:expr) => {
        static $name: std::sync::LazyLock<regex::Regex> =
            std::sync::LazyLock::new(|| regex::Regex::new($pattern).unwrap());
    };
}

mod docker;
mod dotnet;
mod elixir;
mod general;
mod go;
mod java;
mod node;
mod postgres;
mod python;
mod rails;
mod redis;
mod ruby;
mod rust_cargo;

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
    /// Supports pipe-separated alternatives: `"pip|python"`.
    CommandContains(&'static str),
    /// Only fires if the step requires one of these.
    RequiresAny(&'static [&'static str]),
}

impl PatternContext {
    fn matches(&self, ctx: &StepContext) -> bool {
        match self {
            PatternContext::Always => true,
            PatternContext::CommandContains(substr) => {
                substr.split('|').any(|s| ctx.command.contains(s))
            }
            PatternContext::RequiresAny(reqs) => {
                reqs.iter().any(|r| ctx.requires.iter().any(|s| s == r))
            }
        }
    }
}

/// Declarative template for generating a [`FixSuggestion`] from regex captures.
///
/// Replaces closure-based pattern definitions with data. The `{1}`, `{2}`, etc.
/// placeholders in `Template` variants are replaced with regex capture groups.
#[derive(Debug, Clone)]
pub enum FixTemplate {
    /// Fixed label, command, and explanation.
    Static {
        label: &'static str,
        command: &'static str,
        explanation: &'static str,
    },
    /// `{1}`, `{2}` placeholders replaced with regex capture groups.
    Template {
        label: &'static str,
        command: &'static str,
        explanation: &'static str,
    },
    /// Hint-only — no runnable command, just advisory text.
    Hint {
        label: &'static str,
        explanation: &'static str,
    },
    /// Different commands for macOS vs Linux.
    PlatformAware {
        macos_label: &'static str,
        macos_command: &'static str,
        linux_label: &'static str,
        linux_command: &'static str,
        explanation: &'static str,
    },
    /// Choose between alternatives based on step command content.
    /// Pairs of `(command_substring, label, command)` tried in order;
    /// falls back to the last entry if none match.
    ContextSwitch {
        alternatives: &'static [(&'static str, &'static str, &'static str)],
        explanation: &'static str,
    },
}

/// Replace `{1}`, `{2}`, etc. with capture groups. Returns `None` if a
/// referenced group did not participate in the match.
fn substitute(template: &str, caps: &regex::Captures) -> Option<String> {
    let mut result = template.to_string();
    let mut i = 1;
    while result.contains(&format!("{{{}}}", i)) {
        let value = caps.get(i)?.as_str();
        result = result.replace(&format!("{{{}}}", i), value);
        i += 1;
    }
    Some(result)
}

impl FixTemplate {
    /// Apply this template to regex captures and step context, producing a
    /// [`FixSuggestion`].
    pub fn apply(
        &self,
        caps: &regex::Captures,
        ctx: &StepContext,
        confidence: Confidence,
    ) -> Option<FixSuggestion> {
        match self {
            FixTemplate::Static {
                label,
                command,
                explanation,
            } => Some(FixSuggestion {
                label: (*label).to_string(),
                command: (*command).to_string(),
                explanation: (*explanation).to_string(),
                confidence,
            }),
            FixTemplate::Template {
                label,
                command,
                explanation,
            } => Some(FixSuggestion {
                label: substitute(label, caps)?,
                command: substitute(command, caps)?,
                explanation: substitute(explanation, caps)?,
                confidence,
            }),
            FixTemplate::Hint { label, explanation } => Some(FixSuggestion {
                label: substitute(label, caps).unwrap_or_else(|| (*label).to_string()),
                command: String::new(),
                explanation: substitute(explanation, caps)
                    .unwrap_or_else(|| (*explanation).to_string()),
                confidence,
            }),
            FixTemplate::PlatformAware {
                macos_label,
                macos_command,
                linux_label,
                linux_command,
                explanation,
            } => {
                let (label, command) = if cfg!(target_os = "macos") {
                    (*macos_label, *macos_command)
                } else {
                    (*linux_label, *linux_command)
                };
                Some(FixSuggestion {
                    label: label.to_string(),
                    command: command.to_string(),
                    explanation: (*explanation).to_string(),
                    confidence,
                })
            }
            FixTemplate::ContextSwitch {
                alternatives,
                explanation,
            } => {
                let (label, command) = alternatives
                    .iter()
                    .find(|(substr, _, _)| ctx.command.contains(substr))
                    .map(|(_, l, c)| (*l, *c))
                    .or_else(|| alternatives.last().map(|(_, l, c)| (*l, *c)))?;
                Some(FixSuggestion {
                    label: label.to_string(),
                    command: command.to_string(),
                    explanation: (*explanation).to_string(),
                    confidence,
                })
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
    /// Declarative fix template.
    pub fix: FixTemplate,
}

/// Return all built-in error patterns, ordered by specificity.
///
/// More specific ecosystem patterns come first; general catch-all patterns
/// (command_not_found, permission_denied) come last.
pub fn built_in_patterns() -> Vec<ErrorPattern> {
    let mut all = Vec::new();
    all.extend(ruby::patterns());
    all.extend(node::patterns());
    all.extend(python::patterns());
    all.extend(postgres::patterns());
    all.extend(redis::patterns());
    all.extend(docker::patterns());
    all.extend(rails::patterns());
    all.extend(java::patterns());
    all.extend(dotnet::patterns());
    all.extend(rust_cargo::patterns());
    all.extend(go::patterns());
    all.extend(elixir::patterns());
    all.extend(general::patterns());
    all
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
            if let Some(fix) = pattern.fix.apply(&caps, context, pattern.confidence) {
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
            if let Some(fix) = pattern.fix.apply(&caps, context, pattern.confidence) {
                return Some(format!("You might try: {}", fix.label));
            }
        }
    }
    None
}

#[cfg(test)]
pub(super) mod test_helpers {
    use super::*;

    pub fn default_context() -> StepContext<'static> {
        StepContext {
            name: "test",
            command: "echo test",
            requires: &[],
            template: None,
        }
    }

    pub fn bundle_context() -> StepContext<'static> {
        StepContext {
            name: "bundler",
            command: "bundle install",
            requires: &[],
            template: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_helpers::*;

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
        assert!(find_fix(error, &ctx).is_none());
    }

    #[test]
    fn find_hint_ignores_high_confidence() {
        let ctx = bundle_context();
        let error = "An error occurred while installing nokogiri (1.14.0)";
        assert!(find_hint(error, &ctx).is_none());
    }

    #[test]
    fn first_match_wins() {
        let ctx = bundle_context();
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
    fn context_filter_excludes_unrelated() {
        let ctx = default_context();
        let error = "PG::ConnectionBad: could not connect to server: Is the server running on host";
        assert!(find_fix(error, &ctx).is_none());
    }

    #[test]
    fn context_command_contains_filters() {
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
    fn substitute_replaces_capture_groups() {
        let re = Regex::new(r"install (\S+) version (\S+)").unwrap();
        let caps = re.captures("install foo version 1.2.3").unwrap();
        assert_eq!(substitute("{1} @ {2}", &caps).unwrap(), "foo @ 1.2.3");
    }

    #[test]
    fn substitute_returns_none_for_missing_group() {
        let re = Regex::new(r"install (\S+)").unwrap();
        let caps = re.captures("install foo").unwrap();
        assert!(substitute("{1} {2}", &caps).is_none());
    }
}
