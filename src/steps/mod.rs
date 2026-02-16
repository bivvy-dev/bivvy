//! Step resolution and execution.
//!
//! This module provides the core step execution engine for Bivvy:
//!
//! - [`ResolvedStep`] - A fully resolved step ready for execution
//! - [`execute_step`] - Execute a step with environment and hooks
//! - [`run_check`] - Run completed checks for idempotency
//! - [`StepStatus`] - Track step execution state
//! - [`StepResult`] - Capture execution results
//!
//! # Example
//!
//! ```no_run
//! use bivvy::steps::{ResolvedStep, execute_step, ExecutionOptions, StepStatus};
//! use bivvy::config::InterpolationContext;
//! use std::collections::HashMap;
//! use std::path::Path;
//!
//! // Create a resolved step
//! let step = ResolvedStep::from_config("install", &Default::default(), None);
//!
//! // Execute with options
//! let options = ExecutionOptions {
//!     force: false,
//!     dry_run: true,
//!     capture_output: true,
//!     ..Default::default()
//! };
//!
//! let ctx = InterpolationContext::new();
//! let result = execute_step(
//!     &step,
//!     Path::new("."),
//!     &ctx,
//!     &HashMap::new(),
//!     &options,
//!     None,
//! ).unwrap();
//!
//! match result.status() {
//!     StepStatus::Completed => println!("Step completed successfully"),
//!     StepStatus::Skipped => println!("Step was already complete"),
//!     StepStatus::Failed => println!("Step failed: {}", result.error.unwrap_or_default()),
//!     _ => {}
//! }
//! ```

pub mod completed_check;
pub mod executor;
pub mod resolved;
pub mod sensitive;

pub use completed_check::{run_check, CheckResult};
pub use executor::{execute_step, ExecutionOptions, StepResult, StepStatus};
pub use resolved::ResolvedStep;
pub use sensitive::SensitiveStepHandler;
