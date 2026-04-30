//! Run-path display layer.
//!
//! This module provides the rendering contracts for a workflow run:
//!
//! - [`WorkflowDisplay`] — workflow chrome (run header, pinned progress
//!   bar, run summary).
//! - [`StepDisplay`] — per-step rendering (step header, transient
//!   spinner with live-output tail, error block, prompts, final result
//!   line).
//!
//! Both contracts live here, not in `ui/`, because they are concerns of
//! the workflow runner and depend on workflow types
//! ([`RunSummary`], [`StepSummary`], [`RunHeader`]). The `ui/` crate
//! provides only generic primitives — most importantly
//! [`crate::ui::surface::TerminalSurface`].
//!
//! ## Why split workflow vs step?
//!
//! The original `TerminalUI` owned the workflow's `MultiProgress` and
//! also handed out spinners that lived inside it. Step code could
//! therefore mutate the same draw region as the workflow bar, which
//! made the multi-line spinner clearing bug observable: a step's
//! lingering rows would corrupt the workflow chrome.
//!
//! With this split, each layer owns its own region:
//!
//! - `TerminalWorkflowDisplay` owns the pinned bar.
//! - `TerminalStepDisplay` owns a transient region that lives above
//!   the pinned bar.
//! - Both share an `Arc<TerminalSurface>`; only the surface touches the
//!   underlying `MultiProgress`.

pub mod step;
pub mod workflow;

use std::time::Duration;

use crate::ui::StatusKind;

pub use step::{NonInteractiveStepDisplay, StepDisplay, TerminalStepDisplay};
pub use workflow::{
    MockState, MockStepDisplay, MockWorkflowDisplay, NonInteractiveWorkflowDisplay,
    TerminalWorkflowDisplay, WorkflowDisplay,
};

/// Header shown at the top of a workflow run.
#[derive(Debug, Clone)]
pub struct RunHeader {
    /// Application name shown alongside the lodge icon.
    pub app_name: String,
    /// Bivvy version string.
    pub version: String,
    /// Workflow name (e.g. `default`).
    pub workflow: String,
    /// Total step count to run.
    pub step_count: usize,
    /// Active environment label (e.g. `development`).
    pub env_name: String,
}

/// Summary of a workflow run, used by [`WorkflowDisplay::show_run_summary`].
#[derive(Debug, Clone)]
pub struct RunSummary {
    /// Per-step results in execution order.
    pub step_results: Vec<StepSummary>,
    /// Total run duration.
    pub total_duration: Duration,
    /// Number of steps that ran.
    pub steps_run: usize,
    /// Number of steps skipped.
    pub steps_skipped: usize,
    /// Number of steps auto-skipped because already satisfied.
    pub steps_satisfied: usize,
    /// Whether all steps succeeded.
    pub success: bool,
    /// Names of failed steps.
    pub failed_steps: Vec<String>,
}

/// Summary of a single step's result.
#[derive(Debug, Clone)]
pub struct StepSummary {
    /// Step name.
    pub name: String,
    /// Step status.
    pub status: StatusKind,
    /// How long the step took.
    pub duration: Option<Duration>,
    /// Additional context (e.g., check description like "rustc --version").
    pub detail: Option<String>,
}
