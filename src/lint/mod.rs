//! Configuration validation and linting.
//!
//! This module provides comprehensive configuration validation through
//! a pluggable rule-based system.
//!
//! # Overview
//!
//! The lint system consists of:
//!
//! - **Rules** - Individual validation checks ([`LintRule`] trait)
//! - **Registry** - Collection of all available rules ([`RuleRegistry`])
//! - **Diagnostics** - Issue reports with severity and suggestions ([`LintDiagnostic`])
//!
//! # Example
//!
//! ```
//! use bivvy::lint::{RuleRegistry, RuleId, Severity};
//!
//! // Create a registry (empty for now, builtins added in later commits)
//! let registry = RuleRegistry::new();
//!
//! // Check if a rule exists
//! assert!(registry.get(&RuleId::new("nonexistent")).is_none());
//!
//! // Severity has ordering
//! assert!(Severity::Hint < Severity::Warning);
//! assert!(Severity::Warning < Severity::Error);
//! ```

pub mod diagnostic;
pub mod fix;
pub mod output;
pub mod registry;
pub mod rule;
pub mod rules;
pub mod schema;
pub mod span;

pub use diagnostic::{parse_error_to_diagnostic, LintDiagnostic, RelatedInfo};
pub use fix::{Fix, FixEngine, FixResult};
pub use output::{
    display_path, FileCard, HumanFormatter, HumanReport, JsonFormatter, LintFormatter,
    OutputFormat, SarifFormatter,
};
pub use registry::RuleRegistry;
pub use rule::{LintRule, RuleId, Severity};
pub use rules::{
    AppNameRule, CircularDependencyRule, CircularRequirementDepRule, DeadEnvironmentRule,
    DeprecatedFieldsRule, InstallTemplateMissingRule, InterpolationSyntaxErrorRule,
    LocalConfigOverridesSecretRule, RequiredFieldsRule, SecretWithoutHandlerRule,
    SelfDependencyRule, ServiceRequirementWithoutHintRule, StepNameCollisionRule,
    TemplateInputsRule, UndefinedDependencyRule, UndefinedTemplateRule, UnknownRequirementRule,
    UnusedStepRule, UnusedTemplateSourceRule, WorkflowReferencesTemplateNotStepRule,
    WorkflowShapeShorthandRule, WorkflowSingularTypoRule,
};
pub use schema::{schema_json, schema_value, SchemaGenerator};
pub use span::Span;
