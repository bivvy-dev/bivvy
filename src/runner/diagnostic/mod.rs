//! Diagnostic funnel for step failure recovery.
//!
//! Replaces the explicit error pattern registry with a pipeline of stages
//! that progressively narrow from raw output to ranked resolution candidates.
//! Each stage adds confidence. The system degrades gracefully — a rewording
//! in a tool's output might change which stage contributes the most, but the
//! pipeline still produces useful results.
//!
//! ## Pipeline Stages
//!
//! 1. **Normalize** — strip ANSI, normalize line endings, collapse blanks
//! 2. **Segment** — tag lines as error signal, resolution candidate, or noise
//! 3. **Classify** — detect error categories with confidence scoring
//! 4. **Contextualize** — refine using step context and workflow state
//! 5. **Extract** — pull resolution candidates from tool output
//! 6. **Deduce** — generate heuristic resolutions from diagnosis + context

mod classify;
mod contextualize;
mod deduce;
mod extract;
mod normalize;
mod segment;

use std::collections::HashMap;

use crate::steps::{ResolvedStep, StepStatus};

pub use classify::ErrorCategory;
pub use segment::LineTag;

/// Context about the step that failed, used for diagnostic filtering.
pub struct StepContext<'a> {
    /// Step name.
    pub name: &'a str,
    /// Step command.
    pub command: &'a str,
    /// Step requirements (e.g., `["postgres-server", "ruby"]`).
    pub requires: &'a [String],
    /// Template name, if any.
    pub template: Option<&'a str>,
}

/// Workflow state threaded from the orchestrator for Stage 4 context.
pub struct WorkflowState<'a> {
    /// The full workflow definition — Stage 4 can look up any step by name.
    pub steps: &'a [(&'a str, &'a ResolvedStep)],
    /// Step execution outcomes so far (name → status).
    pub outcomes: &'a HashMap<String, StepStatus>,
}

/// A category match with confidence score.
#[derive(Debug, Clone)]
pub struct CategoryMatch {
    /// The error category identified.
    pub category: ErrorCategory,
    /// Confidence score (0.0–1.0).
    pub confidence: f32,
}

/// Structured details extracted from the error output.
#[derive(Debug, Clone, Default)]
pub struct DiagnosticDetails {
    /// What's missing, conflicting, or broken.
    pub target: Option<String>,
    /// Version the user has, if relevant.
    pub version_have: Option<String>,
    /// Version needed, if relevant.
    pub version_need: Option<String>,
    /// Port number if relevant.
    pub port: Option<u16>,
    /// Host if relevant.
    pub host: Option<String>,
    /// Service name if relevant.
    pub service: Option<String>,
}

/// Where a resolution candidate came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolutionSource {
    /// Extracted from the tool's own output.
    Extracted,
    /// Deduced from diagnosis + context.
    Deduced,
}

/// Platform constraint for a resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacOS,
    Linux,
    Windows,
    Any,
}

/// A ranked resolution candidate.
#[derive(Debug, Clone)]
pub struct ResolutionCandidate {
    /// Human-readable label shown in recovery menu.
    pub label: String,
    /// Runnable command, if we have one. `None` for advisory-only.
    pub command: Option<String>,
    /// Why we think this will help.
    pub explanation: String,
    /// How confident we are in this resolution (0.0–1.0).
    pub confidence: f32,
    /// Where the resolution came from.
    pub source: ResolutionSource,
    /// Platform constraint, if any.
    pub platform: Option<Platform>,
}

/// The full diagnosis produced by the funnel.
#[derive(Debug, Clone)]
pub struct Diagnosis {
    /// Primary error categories identified, sorted by confidence.
    pub categories: Vec<CategoryMatch>,
    /// Overall diagnostic confidence (0.0–1.0).
    pub confidence: f32,
    /// Structured details extracted from the output.
    pub details: DiagnosticDetails,
    /// Ranked resolution candidates.
    pub resolutions: Vec<ResolutionCandidate>,
}

/// Run the full diagnostic funnel pipeline.
///
/// Single entry point replacing `find_fix()` + `find_hint()`. Returns a
/// [`Diagnosis`] with ranked resolution candidates. The caller maps this
/// to recovery menu options using confidence thresholds.
pub fn diagnose(
    error_output: &str,
    step_context: &StepContext<'_>,
    workflow_state: &WorkflowState<'_>,
) -> Diagnosis {
    // Stage 1: Normalize
    let normalized = normalize::normalize(error_output);

    // Stage 2: Segment
    let tagged_lines = segment::segment(&normalized);

    // Stage 3: Classify
    let (mut categories, mut details) = classify::classify(&tagged_lines);

    // Stage 4: Contextualize
    contextualize::contextualize(&mut categories, &mut details, step_context, workflow_state);

    // Stage 5: Extract resolutions from output
    let mut resolutions = extract::extract_resolutions(&tagged_lines, &categories, step_context);

    // Stage 6: Deduce resolutions
    let deduced = deduce::deduce_resolutions(&categories, &details, step_context, workflow_state);

    // Merge and deduplicate
    deduce::merge_resolutions(&mut resolutions, deduced);

    // Sort by confidence descending
    resolutions.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Overall confidence is the max category confidence
    let confidence = categories.first().map(|c| c.confidence).unwrap_or(0.0);

    Diagnosis {
        categories,
        confidence,
        details,
        resolutions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnose_pg_dump_version_mismatch() {
        let error = "\
[online_migrations] DANGER: No lock timeout set
pg_dump: error: server version: 16.13 (Homebrew); pg_dump version: 14.21 (Homebrew)
pg_dump: error: aborting because of server version mismatch
bin/rails aborted!
failed to execute:
pg_dump --schema-only --no-privileges --no-owner --file db/structure.sql myapp_development

Please check the output above for any errors and make sure that `pg_dump` is installed in your PATH and has proper permissions.

Tasks: TOP => db:prepare
(See full trace by running task with --trace)";

        let ctx = StepContext {
            name: "db_prepare",
            command: "rails db:prepare",
            requires: &[],
            template: None,
        };

        let outcomes = HashMap::new();
        let ws = WorkflowState {
            steps: &[],
            outcomes: &outcomes,
        };

        let diag = diagnose(error, &ctx, &ws);

        // Primary category should be version_mismatch with high confidence.
        // The version pair "16.13 / 14.21" fires +0.3, the phrase "version mismatch"
        // adds +0.15 (diminishing), and extracted version details boost +0.15.
        assert!(!diag.categories.is_empty());
        assert_eq!(diag.categories[0].category, ErrorCategory::VersionMismatch);
        assert!(
            diag.categories[0].confidence >= 0.6,
            "Expected >= 0.6, got {}",
            diag.categories[0].confidence
        );

        // Should have version details
        assert!(diag.details.version_have.is_some());
        assert!(diag.details.version_need.is_some());

        // Should have resolutions
        assert!(!diag.resolutions.is_empty());
    }

    #[test]
    fn diagnose_connection_refused() {
        let error = "PG::ConnectionBad: could not connect to server: Connection refused\nIs the server running on host \"localhost\" (::1) and accepting TCP/IP connections on port 5432?";

        let requires = vec!["postgres-server".to_string()];
        let ctx = StepContext {
            name: "db_setup",
            command: "rails db:create",
            requires: &requires,
            template: None,
        };

        let outcomes = HashMap::new();
        let ws = WorkflowState {
            steps: &[],
            outcomes: &outcomes,
        };

        let diag = diagnose(error, &ctx, &ws);

        assert!(!diag.categories.is_empty());
        assert_eq!(
            diag.categories[0].category,
            ErrorCategory::ConnectionRefused
        );

        // Should have resolutions to start the service
        assert!(!diag.resolutions.is_empty());
    }

    #[test]
    fn diagnose_module_not_found() {
        let error = "Traceback (most recent call last):\n  File \"app.py\", line 1, in <module>\n    import flask\nModuleNotFoundError: No module named 'flask'";

        let ctx = StepContext {
            name: "deps",
            command: "pip install -r requirements.txt",
            requires: &[],
            template: None,
        };

        let outcomes = HashMap::new();
        let ws = WorkflowState {
            steps: &[],
            outcomes: &outcomes,
        };

        let diag = diagnose(error, &ctx, &ws);

        assert!(!diag.categories.is_empty());
        assert_eq!(diag.categories[0].category, ErrorCategory::NotFound);
        assert_eq!(diag.details.target.as_deref(), Some("flask"));
    }

    #[test]
    fn diagnose_permission_denied() {
        let error = "bash: ./gradlew: Permission denied";

        let ctx = StepContext {
            name: "build",
            command: "./gradlew build",
            requires: &[],
            template: None,
        };

        let outcomes = HashMap::new();
        let ws = WorkflowState {
            steps: &[],
            outcomes: &outcomes,
        };

        let diag = diagnose(error, &ctx, &ws);

        assert!(!diag.categories.is_empty());
        assert_eq!(diag.categories[0].category, ErrorCategory::PermissionDenied);

        // Should suggest chmod +x
        let has_chmod = diag.resolutions.iter().any(|r| {
            r.command
                .as_deref()
                .map(|c| c.contains("chmod"))
                .unwrap_or(false)
        });
        assert!(has_chmod);
    }

    #[test]
    fn diagnose_empty_output_produces_empty_diagnosis() {
        let ctx = StepContext {
            name: "test",
            command: "echo test",
            requires: &[],
            template: None,
        };

        let outcomes = HashMap::new();
        let ws = WorkflowState {
            steps: &[],
            outcomes: &outcomes,
        };

        let diag = diagnose("", &ctx, &ws);
        assert!(diag.categories.is_empty());
        assert!(diag.resolutions.is_empty());
        assert_eq!(diag.confidence, 0.0);
    }

    #[test]
    fn diagnose_externally_managed_python() {
        let error = "error: externally-managed-environment\n\n× This environment is externally managed\n╰─> To install Python packages system-wide, try apt install python3-xyz\n\nnote: If you wish to install a Python package, use a virtual environment.";

        let ctx = StepContext {
            name: "deps",
            command: "pip install flask",
            requires: &[],
            template: None,
        };

        let outcomes = HashMap::new();
        let ws = WorkflowState {
            steps: &[],
            outcomes: &outcomes,
        };

        let diag = diagnose(error, &ctx, &ws);

        assert!(!diag.categories.is_empty());
        assert_eq!(diag.categories[0].category, ErrorCategory::SystemConstraint);
    }

    #[test]
    fn diagnose_port_conflict() {
        let error = "Error: listen EADDRINUSE: address already in use :::3000";

        let ctx = StepContext {
            name: "server",
            command: "npm start",
            requires: &[],
            template: None,
        };

        let outcomes = HashMap::new();
        let ws = WorkflowState {
            steps: &[],
            outcomes: &outcomes,
        };

        let diag = diagnose(error, &ctx, &ws);

        assert!(!diag.categories.is_empty());
        assert_eq!(diag.categories[0].category, ErrorCategory::PortConflict);
        assert_eq!(diag.details.port, Some(3000));
    }
}
