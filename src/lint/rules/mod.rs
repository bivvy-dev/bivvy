//! Built-in lint rules.
//!
//! This module contains all the built-in validation rules that come with Bivvy.

pub mod app_name;
pub mod circular_dependency;
pub mod required_fields;
pub mod self_dependency;
pub mod template_inputs;
pub mod undefined_dependency;
pub mod undefined_template;
pub mod valid_requires;

pub use app_name::AppNameRule;
pub use circular_dependency::CircularDependencyRule;
pub use required_fields::RequiredFieldsRule;
pub use self_dependency::SelfDependencyRule;
pub use template_inputs::TemplateInputsRule;
pub use undefined_dependency::UndefinedDependencyRule;
pub use undefined_template::UndefinedTemplateRule;
pub use valid_requires::{
    CircularRequirementDepRule, InstallTemplateMissingRule, ServiceRequirementWithoutHintRule,
    UnknownRequirementRule,
};
