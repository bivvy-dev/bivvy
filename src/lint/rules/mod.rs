//! Built-in lint rules.
//!
//! This module contains all the built-in validation rules that come with Bivvy.

pub mod app_name;
pub mod check_fields;
pub mod circular_dependency;
pub mod dead_environment;
pub mod deprecated_fields;
pub mod interpolation_syntax;
pub mod required_fields;
pub mod secret_without_handler;
pub mod self_dependency;
pub mod step_name_collision;
pub mod template_inputs;
pub mod undefined_dependency;
pub mod undefined_template;
pub mod undefined_workflow_force;
pub mod unused_step;
pub mod unused_template_source;
pub mod valid_environments;
pub mod valid_requires;
pub mod workflow_references_template;
pub mod workflow_shape_shorthand;
pub mod workflow_singular_typo;

pub use app_name::AppNameRule;
pub use check_fields::CheckFieldsMutualExclusivityRule;
pub use circular_dependency::CircularDependencyRule;
pub use dead_environment::DeadEnvironmentRule;
pub use deprecated_fields::DeprecatedFieldsRule;
pub use interpolation_syntax::InterpolationSyntaxErrorRule;
pub use required_fields::RequiredFieldsRule;
pub use secret_without_handler::SecretWithoutHandlerRule;
pub use self_dependency::SelfDependencyRule;
pub use step_name_collision::StepNameCollisionRule;
pub use template_inputs::TemplateInputsRule;
pub use undefined_dependency::UndefinedDependencyRule;
pub use undefined_template::UndefinedTemplateRule;
pub use undefined_workflow_force::UndefinedWorkflowForceRule;
pub use unused_step::UnusedStepRule;
pub use unused_template_source::UnusedTemplateSourceRule;
pub use valid_environments::{
    CustomEnvironmentShadowsBuiltinRule, EnvironmentCircularDependencyRule,
    EnvironmentDefaultWorkflowMissingRule, RedundantEnvNullRule, RedundantEnvironmentOverrideRule,
    UnknownEnvironmentInOnlyRule, UnknownEnvironmentInStepRule, UnreachableEnvironmentOverrideRule,
};
pub use valid_requires::{
    CircularRequirementDepRule, InstallTemplateMissingRule, ServiceRequirementWithoutHintRule,
    UnknownRequirementRule,
};
pub use workflow_references_template::WorkflowReferencesTemplateNotStepRule;
pub use workflow_shape_shorthand::WorkflowShapeShorthandRule;
pub use workflow_singular_typo::WorkflowSingularTypoRule;
