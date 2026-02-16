//! Template registry for Bivvy.
//!
//! This module handles loading and resolving templates from multiple sources:
//! - Built-in templates (embedded in binary)
//! - User templates (~/.bivvy/templates/)
//! - Project templates (.bivvy/templates/)
//! - Remote templates (coming in M11)
//!
//! # Resolution Order
//!
//! Templates are resolved in this order (first match wins):
//! 1. Project-local
//! 2. User-local
//! 3. Remote (by priority)
//! 4. Built-in
//!
//! # Example
//!
//! ```
//! use bivvy::registry::Registry;
//!
//! // Load registry with built-in templates only
//! let registry = Registry::new(None).unwrap();
//!
//! // Get a built-in template
//! if let Some(template) = registry.get("yarn") {
//!     println!("Template: {}", template.name);
//! }
//! ```

pub mod builtin;
pub mod fetch;
pub mod local;
pub mod manifest;
pub mod remote;
pub mod resolver;
pub mod source;
pub mod template;

// Re-exports
pub use builtin::BuiltinLoader;
pub use fetch::{FetchResponse, GitFetchResult, GitFetcher, HttpFetcher};
pub use local::LocalLoader;
pub use manifest::{Category, RegistryManifest};
pub use remote::RemoteLoader;
pub use resolver::Registry;
pub use source::{RemoteCacheConfig, RemoteCacheStrategy, RemoteSource};
pub use template::{
    Detection, EnvironmentImpact, InputType, Platform, Template, TemplateInput, TemplateSource,
    TemplateStep,
};
