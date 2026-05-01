//! Detects template_sources entries unused by any step.
//!
//! Template sources are remote registries declared in `template_sources:`.
//! A source is "used" when at least one step's `template:` field begins
//! with the source's URL host or otherwise references templates obtained
//! through that source. Because Bivvy resolves templates by name (not by
//! source URL) at runtime, the cleanest heuristic is: a source is used
//! when at least one step has a `template:` whose top-level segment
//! (before the first `/`) matches the source's path or short name.
//!
//! The rule keeps the heuristic deliberately permissive — a hint, not
//! an error — so that mis-detection only nudges users to clean up rather
//! than blocking work.

use crate::config::BivvyConfig;
use crate::lint::{LintDiagnostic, LintRule, RuleId, Severity};

/// Detects `template_sources:` entries unused by any step.
pub struct UnusedTemplateSourceRule;

impl LintRule for UnusedTemplateSourceRule {
    fn id(&self) -> RuleId {
        RuleId::new("unused-template-source")
    }

    fn name(&self) -> &str {
        "Unused Template Source"
    }

    fn description(&self) -> &str {
        "Detects template_sources entries that are never referenced by any step's template field"
    }

    fn default_severity(&self) -> Severity {
        Severity::Hint
    }

    fn check(&self, config: &BivvyConfig) -> Vec<LintDiagnostic> {
        if config.template_sources.is_empty() {
            return Vec::new();
        }

        // Collect the prefixes of every step's `template:` field. We use
        // the part before the first '/' (the source-name prefix) so a
        // step with template `acme/postgres` is recognized as using a
        // source whose name starts with `acme`.
        let used_prefixes: std::collections::HashSet<String> = config
            .steps
            .values()
            .filter_map(|s| s.template.as_deref())
            .map(|t| {
                let prefix = t.split('/').next().unwrap_or(t);
                prefix.to_string()
            })
            .collect();

        let mut diagnostics = Vec::new();
        for source in &config.template_sources {
            let name = derive_source_name(&source.url);
            if !used_prefixes.contains(&name) {
                diagnostics.push(
                    LintDiagnostic::new(
                        self.id(),
                        self.default_severity(),
                        format!(
                            "Template source '{}' (url: {}) is not referenced by any step",
                            name, source.url
                        ),
                    )
                    .with_suggestion(format!(
                        "Reference '{}/<template>' from a step or remove the source from template_sources",
                        name
                    )),
                );
            }
        }

        diagnostics.sort_by(|a, b| a.message.cmp(&b.message));
        diagnostics
    }
}

/// Derive a short name for a template source from its URL.
///
/// Picks the last path segment, stripping trailing `.git` if present.
/// For URLs without a path (e.g. `https://example.com`) returns the host.
fn derive_source_name(url: &str) -> String {
    // Strip scheme.
    let without_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    // Drop everything before host (handle `git@host:path` forms too).
    let after_host = without_scheme
        .split_once(':')
        .map(|(_, rest)| rest)
        .or_else(|| without_scheme.split_once('/').map(|(_, rest)| rest))
        .unwrap_or(without_scheme);

    let mut last = after_host
        .rsplit('/')
        .find(|s| !s.is_empty())
        .unwrap_or(without_scheme)
        .to_string();
    if let Some(stripped) = last.strip_suffix(".git") {
        last = stripped.to_string();
    }
    last
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ExecutionConfig, StepConfig, TemplateSource};
    use std::collections::HashMap;

    fn template_step(template: &str) -> StepConfig {
        StepConfig {
            template: Some(template.to_string()),
            execution: ExecutionConfig::default(),
            ..Default::default()
        }
    }

    fn source(url: &str) -> TemplateSource {
        TemplateSource {
            kind: None,
            url: url.to_string(),
            git_ref: None,
            path: None,
            priority: 100,
            timeout: 10,
            cache: None,
            auth: None,
        }
    }

    #[test]
    fn flags_unused_source() {
        let rule = UnusedTemplateSourceRule;
        let mut steps = HashMap::new();
        steps.insert("install".to_string(), template_step("postgres-tpl/server"));

        let config = BivvyConfig {
            template_sources: vec![
                source("https://github.com/acme/postgres-tpl.git"),
                source("https://github.com/unused/redis-tpl.git"),
            ],
            steps,
            ..Default::default()
        };

        let diags = rule.check(&config);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, RuleId::new("unused-template-source"));
        assert_eq!(diags[0].severity, Severity::Hint);
        assert!(diags[0].message.contains("redis-tpl"));
        assert!(diags[0].suggestion.is_some());
    }

    #[test]
    fn passes_when_source_is_referenced() {
        let rule = UnusedTemplateSourceRule;
        let mut steps = HashMap::new();
        steps.insert("install".to_string(), template_step("templates/postgres"));

        let config = BivvyConfig {
            template_sources: vec![source("https://github.com/acme/templates.git")],
            steps,
            ..Default::default()
        };

        let diags = rule.check(&config);
        assert!(diags.is_empty());
    }

    #[test]
    fn skips_when_no_sources_defined() {
        let rule = UnusedTemplateSourceRule;
        let config = BivvyConfig::default();
        let diags = rule.check(&config);
        assert!(diags.is_empty());
    }

    #[test]
    fn derive_source_name_from_git_url() {
        assert_eq!(
            derive_source_name("https://github.com/acme/templates.git"),
            "templates"
        );
    }

    #[test]
    fn derive_source_name_from_scp_form() {
        assert_eq!(
            derive_source_name("git@github.com:acme/templates.git"),
            "templates"
        );
    }

    #[test]
    fn suggestion_proposes_action() {
        let rule = UnusedTemplateSourceRule;
        let config = BivvyConfig {
            template_sources: vec![source("https://github.com/acme/lib.git")],
            ..Default::default()
        };

        let diags = rule.check(&config);
        let suggestion = diags[0].suggestion.as_ref().unwrap();
        assert!(suggestion.contains("lib"));
        assert!(suggestion.contains("template_sources") || suggestion.contains("Reference"));
    }
}
