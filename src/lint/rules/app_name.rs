//! App name format validation.
//!
//! This rule validates the format and conventions of the app_name field.

use crate::config::BivvyConfig;
use crate::lint::{LintDiagnostic, LintRule, RuleId, Severity};

/// Validates app_name format and conventions.
pub struct AppNameRule;

impl LintRule for AppNameRule {
    fn id(&self) -> RuleId {
        RuleId::new("app-name-format")
    }

    fn name(&self) -> &str {
        "App Name Format"
    }

    fn description(&self) -> &str {
        "Validates app_name follows naming conventions"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, config: &BivvyConfig) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();

        if let Some(ref name) = config.app_name {
            if name.is_empty() {
                diagnostics.push(LintDiagnostic::new(
                    self.id(),
                    Severity::Error,
                    "app_name cannot be empty",
                ));
            } else if name.contains(' ') {
                diagnostics.push(
                    LintDiagnostic::new(
                        self.id(),
                        self.default_severity(),
                        "app_name contains spaces; consider using kebab-case",
                    )
                    .with_suggestion(format!(
                        "Use \"{}\" instead",
                        name.to_lowercase().replace(' ', "-")
                    )),
                );
            }
        }

        diagnostics
    }

    fn supports_fix(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warns_on_spaces_in_name() {
        let rule = AppNameRule;
        let config = BivvyConfig {
            app_name: Some("My App Name".to_string()),
            ..Default::default()
        };

        let diagnostics = rule.check(&config);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].suggestion.is_some());
        assert!(diagnostics[0]
            .suggestion
            .as_ref()
            .unwrap()
            .contains("my-app-name"));
    }

    #[test]
    fn errors_on_empty_name() {
        let rule = AppNameRule;
        let config = BivvyConfig {
            app_name: Some(String::new()),
            ..Default::default()
        };

        let diagnostics = rule.check(&config);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Severity::Error);
    }

    #[test]
    fn passes_valid_name() {
        let rule = AppNameRule;
        let config = BivvyConfig {
            app_name: Some("my-app".to_string()),
            ..Default::default()
        };

        let diagnostics = rule.check(&config);

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn passes_none_app_name() {
        let rule = AppNameRule;
        let config = BivvyConfig::default();

        let diagnostics = rule.check(&config);

        // This rule doesn't check for missing app_name (that's RequiredFieldsRule)
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn supports_fix_returns_true() {
        let rule = AppNameRule;
        assert!(rule.supports_fix());
    }
}
