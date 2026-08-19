//! Lint command implementation.
//!
//! `bivvy lint` validates configuration files using the lint rule system
//! and prints a per-file report. Each card carries a few summary stats
//! (steps, workflows, templates, environments, vars) plus an `Errors:`
//! row that prefixes any rustc-style diagnostic blocks.
//!
//! Parse failures are surfaced as `parse-error/*` diagnostics rather than
//! a flat error string, so they appear in the same report shape as any
//! other lint finding.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::cli::args::LintArgs;
use crate::config::{
    load_config, load_config_file, load_merged_config, load_project_config, load_single_step_file,
    load_single_workflow_file, BivvyConfig, ConfigPaths, Discovery, StepConfig, WorkflowFile,
};
use crate::error::{BivvyError, Result};
use crate::lint::{
    parse_error_to_diagnostic, CircularRequirementDepRule, FileCard, Fix, FixEngine,
    HumanFormatter, HumanReport, InstallTemplateMissingRule, JsonFormatter, LintDiagnostic,
    LintFormatter, RuleId, RuleRegistry, SarifFormatter, ServiceRequirementWithoutHintRule,
    Severity, TemplateInputsRule, UndefinedTemplateRule, UnknownRequirementRule,
};
use crate::registry::Registry;
use crate::requirements::registry::RequirementRegistry;
use crate::ui::{OutputMode, UserInterface};

use super::dispatcher::{Command, CommandResult};

/// What the user asked to lint.
enum LintTarget {
    /// Bare invocation or `--config`: lint `.bivvy/config.yml` only.
    ProjectConfig,
    /// `--workflow <name>` or positional that resolved to a workflow file.
    WorkflowFile(String),
    /// `--step <name>` or positional that resolved to a step file.
    StepFile(String),
    /// `--all`: full merged config (legacy behavior).
    All,
}

/// The lint command implementation.
pub struct LintCommand {
    project_root: PathBuf,
    args: LintArgs,
    config_override: Option<PathBuf>,
}

impl LintCommand {
    /// Create a new lint command.
    pub fn new(project_root: &Path, args: LintArgs) -> Self {
        Self {
            project_root: project_root.to_path_buf(),
            args,
            config_override: None,
        }
    }

    /// Set an override config path.
    pub fn with_config_override(mut self, config_override: Option<PathBuf>) -> Self {
        self.config_override = config_override;
        self
    }

    /// Get the project root path.
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    /// Get the command arguments.
    pub fn args(&self) -> &LintArgs {
        &self.args
    }

    /// Run all lint rules and collect diagnostics, applying any
    /// `--rule` / `--no-rule` filters from the args.
    fn run_rules(
        &self,
        registry: &RuleRegistry,
        config: &crate::config::BivvyConfig,
    ) -> Vec<LintDiagnostic> {
        let allow: Option<HashSet<String>> = if self.args.rule.is_empty() {
            None
        } else {
            Some(self.args.rule.iter().cloned().collect())
        };
        let deny: HashSet<String> = self.args.no_rule.iter().cloned().collect();

        let mut diagnostics = Vec::new();
        for rule in registry.iter() {
            let id = rule.id().0;
            if let Some(ref a) = allow {
                if !a.contains(&id) {
                    continue;
                }
            }
            if deny.contains(&id) {
                continue;
            }
            diagnostics.extend(rule.check(config));
        }
        diagnostics
    }

    /// Resolve which target the user asked to lint.
    fn resolve_target(&self) -> Result<LintTarget> {
        if self.config_override.is_some() {
            // Explicit config file override: lint just that file via load_config.
            return Ok(LintTarget::ProjectConfig);
        }
        if let Some(ref name) = self.args.workflow {
            return Ok(LintTarget::WorkflowFile(name.clone()));
        }
        if let Some(ref name) = self.args.step {
            return Ok(LintTarget::StepFile(name.clone()));
        }
        if self.args.all {
            return Ok(LintTarget::All);
        }
        if self.args.config_only {
            return Ok(LintTarget::ProjectConfig);
        }
        if let Some(ref name) = self.args.target {
            let discovery = Discovery::new(&self.project_root);
            if discovery.workflow_path(name).is_some() {
                if discovery.step_path(name).is_some() {
                    // Both exist (rare) — pick workflow but surface a hint.
                    return Ok(LintTarget::WorkflowFile(name.clone()));
                }
                return Ok(LintTarget::WorkflowFile(name.clone()));
            }
            if discovery.step_path(name).is_some() {
                return Ok(LintTarget::StepFile(name.clone()));
            }
            return Err(BivvyError::ConfigValidationError {
                message: format!(
                    "Unknown lint target: {name}. No file found at \
                     .bivvy/workflows/{name}.yml or .bivvy/steps/{name}.yml"
                ),
            });
        }
        Ok(LintTarget::ProjectConfig)
    }

    /// Build the [`BivvyConfig`] view to lint plus the source paths it draws from.
    fn build_target_config(&self, target: &LintTarget) -> Result<(BivvyConfig, Vec<PathBuf>)> {
        if let Some(ref override_path) = self.config_override {
            // Explicit override: just load that file in isolation.
            let cfg = load_config(&self.project_root, Some(override_path))?;
            return Ok((cfg, vec![override_path.clone()]));
        }

        match target {
            LintTarget::ProjectConfig => {
                let cfg = load_project_config(&self.project_root)?;
                let path = self.project_root.join(".bivvy").join("config.yml");
                Ok((cfg, vec![path]))
            }
            LintTarget::WorkflowFile(name) => {
                let discovery = Discovery::new(&self.project_root);
                let workflow_path =
                    discovery
                        .workflow_path(name)
                        .ok_or_else(|| BivvyError::ConfigNotFound {
                            path: self
                                .project_root
                                .join(".bivvy")
                                .join("workflows")
                                .join(format!("{name}.yml")),
                        })?;

                // Project file gives us context (settings, templates, custom
                // requirements). Missing project file is fine — fall back to
                // a default config so we can still lint the workflow file.
                let mut cfg = match load_project_config(&self.project_root) {
                    Ok(c) => c,
                    Err(BivvyError::ConfigNotFound { .. }) => BivvyConfig::default(),
                    Err(e) => return Err(e),
                };

                let workflow_file = load_single_workflow_file(&workflow_path)?;

                // Replace workflows with just the named one so cross-workflow
                // rules don't fire on workflows we aren't targeting.
                let mut workflow = workflow_file.workflow.clone();
                if workflow.description.is_none() {
                    workflow.description = workflow_file.description.clone();
                }
                cfg.workflows = HashMap::new();
                cfg.workflows.insert(name.clone(), workflow);

                // Splice in steps and vars from the workflow file.
                for (step_name, step_config) in workflow_file.steps {
                    cfg.steps.insert(step_name, step_config);
                }
                for (var_name, var_def) in workflow_file.vars {
                    cfg.vars.insert(var_name, var_def);
                }
                cfg.migrate_deprecated_fields();

                let mut paths = vec![workflow_path];
                let project_path = self.project_root.join(".bivvy").join("config.yml");
                if project_path.exists() {
                    paths.push(project_path);
                }
                Ok((cfg, paths))
            }
            LintTarget::StepFile(name) => {
                let discovery = Discovery::new(&self.project_root);
                let step_path =
                    discovery
                        .step_path(name)
                        .ok_or_else(|| BivvyError::ConfigNotFound {
                            path: self
                                .project_root
                                .join(".bivvy")
                                .join("steps")
                                .join(format!("{name}.yml")),
                        })?;

                let mut cfg = match load_project_config(&self.project_root) {
                    Ok(c) => c,
                    Err(BivvyError::ConfigNotFound { .. }) => BivvyConfig::default(),
                    Err(e) => return Err(e),
                };

                let step_config = load_single_step_file(&step_path)?;
                cfg.steps.insert(name.clone(), step_config);
                cfg.migrate_deprecated_fields();

                let mut paths = vec![step_path];
                let project_path = self.project_root.join(".bivvy").join("config.yml");
                if project_path.exists() {
                    paths.push(project_path);
                }
                Ok((cfg, paths))
            }
            LintTarget::All => {
                let cfg = load_merged_config(&self.project_root)?;
                let discovered = ConfigPaths::discover(&self.project_root);
                let mut paths: Vec<PathBuf> = discovered
                    .all_existing()
                    .iter()
                    .map(|p| (*p).clone())
                    .collect();
                paths.extend(discovered.split_steps.iter().cloned());
                paths.extend(discovered.split_workflows.iter().cloned());
                Ok((cfg, paths))
            }
        }
    }

    /// Build the per-file report shown for the human formatter. Cards are
    /// emitted in canonical order (system → project → local → workflows
    /// alphabetically → steps alphabetically → extends URLs).
    fn build_report(
        &self,
        target: &LintTarget,
        config: Option<&BivvyConfig>,
        diagnostics: &[LintDiagnostic],
    ) -> HumanReport {
        let home = crate::sys::home_dir();
        let home_ref = home.as_deref();
        let proj = &self.project_root;

        // Group diagnostics by canonical absolute path string.
        let mut by_path: HashMap<String, Vec<LintDiagnostic>> = HashMap::new();
        let mut no_path: Vec<LintDiagnostic> = Vec::new();
        for d in diagnostics {
            if let Some(ref span) = d.span {
                by_path
                    .entry(span.file.to_string_lossy().to_string())
                    .or_default()
                    .push(d.clone());
            } else {
                no_path.push(d.clone());
            }
        }

        let mut report = HumanReport::new();
        let discovered = ConfigPaths::discover(proj);

        // Determine which file cards to emit and which to mark as context.
        let (primary_paths, context_paths) = self.report_file_set(target, &discovered);

        for path in &primary_paths {
            let display = crate::lint::display_path(path, proj, home_ref);
            let label = label_for_path(path, proj);
            let stats = compute_stats_for_card(path, &label, config);
            let diags = by_path
                .remove(&path.to_string_lossy().to_string())
                .unwrap_or_default();
            report.push_card(FileCard {
                path: path.clone(),
                display,
                label,
                stats,
                diagnostics: diags,
            });
        }

        for path in context_paths {
            let display = crate::lint::display_path(&path, proj, home_ref);
            report.push_context_file(display);
        }

        // Any diagnostics we didn't place in a card go in the no-file bucket
        // (e.g. rules that fire without a span, or spans pointing at files
        // outside our card set).
        for (_, mut group) in by_path {
            no_path.append(&mut group);
        }
        report.no_file_diagnostics = no_path;

        report
    }

    /// Return `(primary_paths, context_paths)` for the report.
    ///
    /// `primary_paths` get a full card with stats + any diagnostics. `context_paths`
    /// render as a one-line "Loaded for context" trailing note.
    fn report_file_set(
        &self,
        target: &LintTarget,
        discovered: &ConfigPaths,
    ) -> (Vec<PathBuf>, Vec<PathBuf>) {
        let proj = &self.project_root;
        let mut primary: Vec<PathBuf> = Vec::new();
        let mut context: Vec<PathBuf> = Vec::new();

        match target {
            LintTarget::ProjectConfig => {
                let p = self
                    .config_override
                    .clone()
                    .unwrap_or_else(|| proj.join(".bivvy").join("config.yml"));
                primary.push(p);
            }
            LintTarget::WorkflowFile(name) => {
                let path = Discovery::new(proj).workflow_path(name).unwrap_or_else(|| {
                    proj.join(".bivvy")
                        .join("workflows")
                        .join(format!("{name}.yml"))
                });
                primary.push(path);
                let proj_cfg = proj.join(".bivvy").join("config.yml");
                if proj_cfg.exists() {
                    context.push(proj_cfg);
                }
            }
            LintTarget::StepFile(name) => {
                let path = Discovery::new(proj).step_path(name).unwrap_or_else(|| {
                    proj.join(".bivvy")
                        .join("steps")
                        .join(format!("{name}.yml"))
                });
                primary.push(path);
                let proj_cfg = proj.join(".bivvy").join("config.yml");
                if proj_cfg.exists() {
                    context.push(proj_cfg);
                }
            }
            LintTarget::All => {
                if let Some(p) = &discovered.user_global {
                    primary.push(p.clone());
                }
                if let Some(p) = &discovered.project {
                    primary.push(p.clone());
                }
                if let Some(p) = &discovered.project_local {
                    primary.push(p.clone());
                }
                let mut wfs: Vec<PathBuf> = discovered.split_workflows.clone();
                wfs.sort();
                primary.extend(wfs);
                let mut steps: Vec<PathBuf> = discovered.split_steps.clone();
                steps.sort();
                primary.extend(steps);
                primary.extend(discovered.extends.iter().cloned());
            }
        }

        (primary, context)
    }

    /// Format diagnostics using the appropriate formatter for non-human modes.
    fn format_machine_output(&self, diagnostics: &[LintDiagnostic]) -> String {
        let mut output = Vec::new();
        match self.args.format.as_str() {
            "json" => {
                let formatter = JsonFormatter::new();
                let _ = formatter.format(diagnostics, &mut output);
            }
            "sarif" => {
                let formatter = SarifFormatter::new("bivvy", env!("CARGO_PKG_VERSION"));
                let _ = formatter.format(diagnostics, &mut output);
            }
            _ => {
                let formatter = HumanFormatter::new(false);
                let _ = formatter.format(diagnostics, &mut output);
            }
        }
        String::from_utf8(output).unwrap_or_default()
    }

    /// Emit the human-formatted report to the UI, line by line.
    fn emit_report_to_ui(&self, report: &HumanReport, ui: &mut dyn UserInterface) {
        let formatter = HumanFormatter::new(false)
            .with_path_display(Some(self.project_root.clone()), crate::sys::home_dir());
        let mut buf = Vec::new();
        let _ = formatter.format_report(report, &mut buf);
        let text = String::from_utf8(buf).unwrap_or_default();
        for line in text.lines() {
            ui.message(line);
        }
    }
}

/// Pick the label string shown after the file path in a card header,
/// based on what kind of file the path points to.
fn label_for_path(path: &Path, project_root: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    let parent_name = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("");

    let bivvy_dir = project_root.join(".bivvy");
    if path == bivvy_dir.join("config.yml") {
        return "project config".to_string();
    }
    if path == bivvy_dir.join("config.local.yml") {
        return "local config".to_string();
    }
    if parent_name == "workflows" {
        return format!("workflow file: {stem}");
    }
    if parent_name == "steps" {
        return format!("step file: {stem}");
    }
    if let Some(home) = crate::sys::home_dir() {
        if path == home.join(".bivvy").join("config.yml") {
            return "system config".to_string();
        }
    }
    "config".to_string()
}

/// Build the stats rows shown inside a card. Each card reports stats for
/// the contents of its own file — never the merged config — so a near-empty
/// system config doesn't render the project's numbers next to it.
///
/// `merged_fallback` is consulted only as a last-resort fallback for files
/// the per-file loader can't parse on its own (e.g. workflow files with
/// shorthand shapes that still merge cleanly into the project picture).
fn compute_stats_for_card(
    path: &Path,
    label: &str,
    merged_fallback: Option<&BivvyConfig>,
) -> Vec<(String, String)> {
    let is_config_shape = matches!(label, "project config" | "system config" | "local config");

    if is_config_shape {
        if let Ok(cfg) = load_config_file(path) {
            return stats_from_bivvy_config(&cfg);
        }
        return Vec::new();
    }

    if let Some(name) = label.strip_prefix("workflow file: ") {
        if let Ok(wf_file) = load_single_workflow_file(path) {
            return stats_from_workflow_file(&wf_file, name);
        }
        if let Some(cfg) = merged_fallback {
            return stats_from_workflow_in_merged(cfg, name);
        }
        return Vec::new();
    }

    if let Some(name) = label.strip_prefix("step file: ") {
        if let Ok(step) = load_single_step_file(path) {
            return stats_from_step_file(&step, name);
        }
    }

    Vec::new()
}

/// Stats rows derived from a `BivvyConfig` parsed from a single file
/// (project, system, or local config).
fn stats_from_bivvy_config(cfg: &BivvyConfig) -> Vec<(String, String)> {
    let mut rows: Vec<(String, String)> = Vec::new();

    let step_count = cfg.steps.len();
    let referenced: HashSet<&String> = cfg
        .workflows
        .values()
        .flat_map(|w| w.steps.iter())
        .collect();
    let referenced_count = referenced.len();
    if step_count > 0 {
        let value = if referenced_count > 0 {
            format!("{step_count} defined, {referenced_count} referenced from workflows")
        } else {
            format!("{step_count} defined")
        };
        rows.push(("Steps".to_string(), value));
    }

    let wf_count = cfg.workflows.len();
    if wf_count > 0 {
        let mut names: Vec<&String> = cfg.workflows.keys().collect();
        names.sort();
        let value = if wf_count <= 5 {
            let joined = names
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            format!("{wf_count} ({joined})")
        } else {
            wf_count.to_string()
        };
        rows.push(("Workflows".to_string(), value));
    }

    let templates: HashSet<&String> = cfg
        .steps
        .values()
        .filter_map(|s| s.template.as_ref())
        .collect();
    if !templates.is_empty() {
        let value = if templates.len() <= 3 {
            let mut names: Vec<&str> = templates.iter().map(|s| s.as_str()).collect();
            names.sort();
            format!("{} ({})", templates.len(), names.join(", "))
        } else {
            templates.len().to_string()
        };
        rows.push(("Templates".to_string(), value));
    }

    let envs = &cfg.settings.environment_profiles.environments;
    if !envs.is_empty() {
        let mut names: Vec<&str> = envs.keys().map(|s| s.as_str()).collect();
        names.sort();
        let value = if envs.len() <= 5 {
            format!("{} ({})", envs.len(), names.join(", "))
        } else {
            envs.len().to_string()
        };
        rows.push(("Environments".to_string(), value));
    }

    if !cfg.vars.is_empty() {
        rows.push(("Vars".to_string(), cfg.vars.len().to_string()));
    }

    rows
}

/// Stats rows for a single workflow file. The card reports what the FILE
/// declares — its own embedded steps, vars, and the workflow body —
/// not the merged-config picture.
fn stats_from_workflow_file(wf_file: &WorkflowFile, name: &str) -> Vec<(String, String)> {
    let mut rows: Vec<(String, String)> = Vec::new();

    let wf = &wf_file.workflow;
    let mut conditional = 0usize;
    if !wf.env.is_empty() {
        conditional += 1;
    }
    if !wf.force.is_empty() || wf.force_all {
        conditional += 1;
    }
    if wf.settings.is_some() {
        conditional += 1;
    }
    rows.push((
        "Workflow".to_string(),
        format!(
            "{name} ({} steps, {conditional} conditionals)",
            wf.steps.len()
        ),
    ));

    if !wf_file.steps.is_empty() {
        rows.push((
            "Steps".to_string(),
            format!("{} defined", wf_file.steps.len()),
        ));
    }

    let templates: HashSet<&String> = wf_file
        .steps
        .values()
        .filter_map(|s| s.template.as_ref())
        .collect();
    if !templates.is_empty() {
        let value = if templates.len() <= 3 {
            let mut names: Vec<&str> = templates.iter().map(|s| s.as_str()).collect();
            names.sort();
            format!("{} ({})", templates.len(), names.join(", "))
        } else {
            templates.len().to_string()
        };
        rows.push(("Templates".to_string(), value));
    }

    if !wf_file.vars.is_empty() {
        rows.push(("Vars".to_string(), wf_file.vars.len().to_string()));
    }

    rows
}

/// Last-resort stats for a workflow file that didn't parse on its own —
/// fall back to whatever the merged config knows about that workflow.
fn stats_from_workflow_in_merged(cfg: &BivvyConfig, name: &str) -> Vec<(String, String)> {
    let Some(wf) = cfg.workflows.get(name) else {
        return Vec::new();
    };
    let mut rows: Vec<(String, String)> = Vec::new();
    rows.push((
        "Workflow".to_string(),
        format!("{name} ({} steps, ? conditionals)", wf.steps.len()),
    ));
    rows
}

/// Stats rows for a single step file.
fn stats_from_step_file(step: &StepConfig, name: &str) -> Vec<(String, String)> {
    let mut rows: Vec<(String, String)> = Vec::new();
    rows.push(("Step".to_string(), name.to_string()));
    if let Some(t) = &step.template {
        rows.push(("Template".to_string(), t.clone()));
    }
    if !step.depends_on.is_empty() {
        rows.push(("Depends on".to_string(), step.depends_on.join(", ")));
    }
    rows
}

/// Emit the `--list-rules` table.
pub fn print_rule_list(ui: &mut dyn UserInterface) {
    let registry = RuleRegistry::with_builtins();
    let mut rows: Vec<(String, String, String)> = registry
        .iter()
        .map(|r| {
            (
                r.id().0,
                r.default_severity().to_string(),
                r.name().to_string(),
            )
        })
        .collect();
    // Add the synthetic parse-error rules so users can `--explain` them.
    rows.push((
        "parse-error".to_string(),
        Severity::Error.to_string(),
        "Configuration parse error".to_string(),
    ));
    rows.push((
        "parse-error/unknown-field".to_string(),
        Severity::Error.to_string(),
        "Unrecognized top-level field".to_string(),
    ));
    rows.push((
        "parse-error/invalid-type".to_string(),
        Severity::Error.to_string(),
        "Value has the wrong type".to_string(),
    ));
    rows.push((
        "parse-error/missing-field".to_string(),
        Severity::Error.to_string(),
        "Required field missing".to_string(),
    ));
    rows.push((
        "parse-error/duplicate-key".to_string(),
        Severity::Error.to_string(),
        "Duplicate mapping key".to_string(),
    ));

    rows.sort_by(|a, b| a.0.cmp(&b.0));

    let id_w = rows.iter().map(|r| r.0.len()).max().unwrap_or(0).max(2);
    let sev_w = rows.iter().map(|r| r.1.len()).max().unwrap_or(0).max(8);

    ui.message("Bivvy Lint Rules");
    ui.message("");
    let header = format!(
        "  {:id_w$}  {:sev_w$}  {}",
        "ID",
        "Severity",
        "Name",
        id_w = id_w,
        sev_w = sev_w,
    );
    ui.message(&header);
    for (id, sev, name) in &rows {
        ui.message(&format!(
            "  {:id_w$}  {:sev_w$}  {name}",
            id,
            sev,
            id_w = id_w,
            sev_w = sev_w,
        ));
    }
}

/// Emit the body of `--explain <RULE>`.
///
/// Returns `false` if the rule is unknown — the caller should exit 1 in
/// that case.
pub fn print_rule_explanation(ui: &mut dyn UserInterface, rule_id: &str) -> bool {
    if let Some((sev, name, desc)) = builtin_rule_info(rule_id) {
        ui.message(rule_id);
        ui.message("");
        ui.message(&format!("  Severity:    {sev}"));
        ui.message(&format!("  Name:        {name}"));
        let mut first = true;
        for line in wrap_text(&desc, 64) {
            if first {
                ui.message(&format!("  Description: {line}"));
                first = false;
            } else {
                ui.message(&format!("               {line}"));
            }
        }
        true
    } else {
        ui.error(&format!("error[explain]: no such rule: '{rule_id}'"));
        ui.message("   = help: run `bivvy lint --list-rules` to see all available rules");
        false
    }
}

/// Look up the description for a rule by id, including the synthetic
/// `parse-error/*` rules. Returns `(severity, name, description)`.
fn builtin_rule_info(rule_id: &str) -> Option<(String, String, String)> {
    // First try the live registry.
    let registry = RuleRegistry::with_builtins();
    if let Some(rule) = registry.get(&RuleId::new(rule_id)) {
        return Some((
            rule.default_severity().to_string(),
            rule.name().to_string(),
            rule.description().to_string(),
        ));
    }
    // Synthetic parse-error rules.
    let entry = match rule_id {
        "parse-error" => Some((
            "Configuration parse error",
            "The YAML in a config file could not be parsed. The diagnostic carries a span pointing at the offending location.",
        )),
        "parse-error/unknown-field" => Some((
            "Unrecognized top-level field",
            "A field appears at the top level of a config file that the schema doesn't recognize. Often a typo (e.g. `workflow:` for `workflows:`) or a stale field name. The diagnostic includes a \"did you mean?\" suggestion when one exists.",
        )),
        "parse-error/invalid-type" => Some((
            "Value has the wrong type",
            "A field's value is the wrong YAML type — for example, a list where a string was expected, or a number where a boolean was required.",
        )),
        "parse-error/missing-field" => Some((
            "Required field missing",
            "A struct in the config schema is missing a field that the schema marks as required.",
        )),
        "parse-error/duplicate-key" => Some((
            "Duplicate mapping key",
            "A YAML mapping has the same key twice. YAML treats this as ambiguous and the parser rejects it.",
        )),
        _ => None,
    };
    entry.map(|(name, desc)| (Severity::Error.to_string(), name.into(), desc.into()))
}

/// Word-wrap `text` to lines of at most `max` characters, breaking on spaces.
fn wrap_text(text: &str, max: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if current.is_empty() {
            current.push_str(word);
        } else if current.len() + 1 + word.len() > max {
            lines.push(std::mem::take(&mut current));
            current.push_str(word);
        } else {
            current.push(' ');
            current.push_str(word);
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

impl Command for LintCommand {
    fn execute(&self, ui: &mut dyn UserInterface) -> Result<CommandResult> {
        // Standalone modes that don't need to load config.
        if self.args.list_rules {
            print_rule_list(ui);
            return Ok(CommandResult::success());
        }
        if let Some(ref rule_id) = self.args.explain {
            let ok = print_rule_explanation(ui, rule_id);
            return Ok(if ok {
                CommandResult::success()
            } else {
                CommandResult::failure(1)
            });
        }

        // Create event bus for structured logging
        let mut event_bus = crate::logging::EventBus::new();
        if let Ok(logger) = crate::logging::EventLogger::new(
            crate::logging::default_log_dir(),
            &format!("sess_{}_lint", chrono::Utc::now().format("%Y%m%d%H%M%S"),),
            crate::logging::RetentionPolicy::default(),
        ) {
            event_bus.add_consumer(Box::new(logger));
        }
        let start = std::time::Instant::now();

        event_bus.emit(&crate::logging::BivvyEvent::SessionStarted {
            command: "lint".to_string(),
            args: vec![
                format!("--format={}", self.args.format),
                if self.args.strict {
                    "--strict".to_string()
                } else {
                    String::new()
                },
                if self.args.fix {
                    "--fix".to_string()
                } else {
                    String::new()
                },
            ]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            os: Some(std::env::consts::OS.to_string()),
            working_directory: Some(self.project_root.display().to_string()),
        });

        // Check if config exists (skip check when override is provided)
        if self.config_override.is_none() {
            let paths = ConfigPaths::discover(&self.project_root);
            if !paths.has_project_config() {
                ui.error("No configuration found. Run 'bivvy init' first.");
                event_bus.emit(&crate::logging::BivvyEvent::SessionEnded {
                    exit_code: 2,
                    duration_ms: start.elapsed().as_millis() as u64,
                });
                return Ok(CommandResult::failure(2));
            }
        }

        // Resolve which target the user asked to lint.
        let target = match self.resolve_target() {
            Ok(t) => t,
            Err(BivvyError::ConfigValidationError { message }) => {
                ui.error(&message);
                let discovery = Discovery::new(&self.project_root);
                let workflows = discovery.workflow_names();
                let steps = discovery.step_file_names();
                if !workflows.is_empty() {
                    ui.message(&format!("Available workflows: {}", workflows.join(", ")));
                }
                if !steps.is_empty() {
                    ui.message(&format!("Available steps: {}", steps.join(", ")));
                }
                event_bus.emit(&crate::logging::BivvyEvent::SessionEnded {
                    exit_code: 1,
                    duration_ms: start.elapsed().as_millis() as u64,
                });
                return Ok(CommandResult::failure(1));
            }
            Err(e) => return Err(e),
        };

        // Build the BivvyConfig view to lint along with the file paths
        // we actually consulted (used for raw-YAML deprecation scanning).
        let (config_opt, lint_file_paths, parse_error_diag) =
            match self.build_target_config(&target) {
                Ok((cfg, paths)) => (Some(cfg), paths, None),
                Err(BivvyError::ConfigParseError { path, message }) => {
                    let diag = parse_error_to_diagnostic(&path, &message);
                    (None, vec![path.clone()], Some(diag))
                }
                Err(BivvyError::ConfigNotFound { path }) => {
                    ui.error(&format!("File not found: {}", path.display()));
                    event_bus.emit(&crate::logging::BivvyEvent::SessionEnded {
                        exit_code: 1,
                        duration_ms: start.elapsed().as_millis() as u64,
                    });
                    return Ok(CommandResult::failure(1));
                }
                Err(e) => return Err(e),
            };

        // If we couldn't parse, render a single-card report and exit 1.
        if let Some(diag) = parse_error_diag {
            let report = build_parse_error_report(&self.project_root, &lint_file_paths[0], diag);
            if self.args.format.is_empty() || self.args.format == "human" {
                self.emit_report_to_ui(&report, ui);
            } else {
                let diags: Vec<LintDiagnostic> = report
                    .cards
                    .iter()
                    .flat_map(|c| c.diagnostics.iter().cloned())
                    .collect();
                ui.message(&self.format_machine_output(&diags));
            }
            event_bus.emit(&crate::logging::BivvyEvent::SessionEnded {
                exit_code: 1,
                duration_ms: start.elapsed().as_millis() as u64,
            });
            return Ok(CommandResult::failure(1));
        }

        let config = config_opt.expect("config must be Some on success path");

        let mut deprecation_warnings =
            crate::lint::rules::deprecated_fields::collect_deprecation_warnings(&config);

        // Scan raw YAML for alias-based deprecations (e.g., old field names)
        {
            let refs: Vec<&std::path::Path> = lint_file_paths.iter().map(|p| p.as_path()).collect();
            deprecation_warnings.extend(
                crate::lint::rules::deprecated_fields::collect_raw_yaml_deprecation_warnings(&refs),
            );
        }

        // Surface nested config typos (e.g. `paralel:`, `comand:`) that serde
        // silently drops because `deny_unknown_fields` is incompatible with the
        // flattened `settings`/step sub-structs. These are display-only: unknown
        // fields are not deprecations, so they stay out of the `ConfigLoaded`
        // event's `deprecation_warnings` vector.
        let unknown_field_warnings = config.unknown_field_warnings();

        // Display deprecation warnings and unknown-field warnings to the user
        for warning in deprecation_warnings.iter().chain(&unknown_field_warnings) {
            ui.warning(warning);
        }

        event_bus.emit(&crate::logging::BivvyEvent::ConfigLoaded {
            config_path: self
                .config_override
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| ".bivvy/config.yml".to_string()),
            parse_duration_ms: None,
            deprecation_warnings,
        });

        // Apply config default_output when no CLI flag was explicitly set
        if ui.output_mode() == OutputMode::Normal {
            ui.set_output_mode(config.settings.defaults.output.into());
        }

        // Create rule registry with built-in rules
        let mut rule_registry = RuleRegistry::with_builtins();

        // Add template-related rules if we can load the template registry
        let template_registry_result = if config.template_sources.is_empty() {
            Registry::new(Some(&self.project_root))
        } else {
            Registry::with_remote_sources(Some(&self.project_root), &config.template_sources)
        };
        if let Ok(template_registry) = template_registry_result {
            rule_registry.register(Box::new(UndefinedTemplateRule::new(
                template_registry.clone(),
            )));
            rule_registry.register(Box::new(TemplateInputsRule::new(template_registry)));
        }

        // Add requirement-related rules
        // Each rule takes ownership of its own RequirementRegistry instance
        let make_req_registry = || RequirementRegistry::new().with_custom(&config.requirements);
        rule_registry.register(Box::new(UnknownRequirementRule::new(make_req_registry())));
        rule_registry.register(Box::new(CircularRequirementDepRule::new(
            make_req_registry(),
        )));
        rule_registry.register(Box::new(InstallTemplateMissingRule::new(
            make_req_registry(),
        )));
        rule_registry.register(Box::new(ServiceRequirementWithoutHintRule::new(
            make_req_registry(),
        )));

        // Run all lint rules
        let mut diagnostics = self.run_rules(&rule_registry, &config);

        // Apply fixes if requested
        if self.args.fix {
            let fixes: Vec<Fix> = diagnostics
                .iter()
                .filter_map(|d| {
                    // Only create fixes for diagnostics that have suggestions and spans
                    match (&d.suggestion, &d.span) {
                        (Some(suggestion), Some(span)) => Some(Fix {
                            file: span.file.clone(),
                            start: 0, // Would need actual byte offsets from marked_yaml
                            end: 0,
                            replacement: suggestion.clone(),
                        }),
                        _ => None,
                    }
                })
                .collect();

            if !fixes.is_empty() {
                let engine = FixEngine::new();
                let result = engine.apply_fixes(&diagnostics, &fixes);
                if result.applied > 0 {
                    ui.success(&format!("Applied {} fix(es)", result.applied));
                    // Re-run rules after fixes
                    diagnostics = self.run_rules(&rule_registry, &config);
                }
            }
        }

        // Evaluate checks defined in config and emit CheckEvaluated events
        {
            let ctx = crate::config::interpolation::InterpolationContext::default();
            let mut snapshot_store = crate::snapshots::SnapshotStore::empty();
            for (step_name, step_config) in &config.steps {
                if let Some(ref check) = step_config.execution.check {
                    let mut evaluator = crate::checks::evaluator::CheckEvaluator::new(
                        &self.project_root,
                        &ctx,
                        &mut snapshot_store,
                    );
                    let result = evaluator.evaluate(check);
                    event_bus.emit(&crate::logging::BivvyEvent::CheckEvaluated {
                        step: step_name.clone(),
                        check_name: check.name().map(|s| s.to_string()),
                        check_type: check.type_name().to_string(),
                        outcome: result.outcome.as_str().to_string(),
                        description: result.description.clone(),
                        details: result.details.clone(),
                        duration_ms: None,
                    });
                }
            }
        }

        // Check for errors based on strict mode
        let has_errors = diagnostics.iter().any(|d| d.severity == Severity::Error);
        let has_warnings = diagnostics.iter().any(|d| d.severity == Severity::Warning);
        let should_fail = has_errors || (self.args.strict && has_warnings);

        if self.args.format.is_empty() || self.args.format == "human" {
            let report = self.build_report(&target, Some(&config), &diagnostics);
            self.emit_report_to_ui(&report, ui);
        } else {
            let output = self.format_machine_output(&diagnostics);
            ui.message(&output);
        }

        let (exit_code, result) = if should_fail {
            (1, CommandResult::failure(1))
        } else {
            (0, CommandResult::success())
        };
        event_bus.emit(&crate::logging::BivvyEvent::SessionEnded {
            exit_code,
            duration_ms: start.elapsed().as_millis() as u64,
        });
        Ok(result)
    }
}

/// Build a single-card report for an unparseable file.
fn build_parse_error_report(project_root: &Path, path: &Path, diag: LintDiagnostic) -> HumanReport {
    let home = crate::sys::home_dir();
    let display = crate::lint::display_path(path, project_root, home.as_deref());
    let label = label_for_path(path, project_root);
    let card = FileCard {
        path: path.to_path_buf(),
        display,
        label,
        stats: Vec::new(),
        diagnostics: vec![diag],
    };
    let mut report = HumanReport::new();
    report.push_card(card);
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{MockUI, UiState};
    use std::fs;
    use tempfile::TempDir;

    fn setup_project(config: &str) -> TempDir {
        let temp = TempDir::new().unwrap();
        let bivvy_dir = temp.path().join(".bivvy");
        fs::create_dir_all(&bivvy_dir).unwrap();
        fs::write(bivvy_dir.join("config.yml"), config).unwrap();
        temp
    }

    fn collect_output(ui: &MockUI) -> String {
        let mut s = String::new();
        for m in ui.messages() {
            s.push_str(m);
            s.push('\n');
        }
        for w in ui.warnings() {
            s.push_str(w);
            s.push('\n');
        }
        for e in ui.errors() {
            s.push_str(e);
            s.push('\n');
        }
        s
    }

    #[test]
    fn lint_command_creation() {
        let temp = TempDir::new().unwrap();
        let args = LintArgs::default();
        let cmd = LintCommand::new(temp.path(), args);

        assert_eq!(cmd.project_root(), temp.path());
    }

    #[test]
    fn lint_no_config() {
        let temp = TempDir::new().unwrap();
        let args = LintArgs::default();
        let cmd = LintCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(!result.success);
        assert_eq!(result.exit_code, 2);
    }

    #[test]
    fn lint_warns_on_unknown_nested_fields() {
        // Typos under `settings:` and inside a step are dropped by serde
        // (flatten + no deny_unknown_fields), so lint must surface them. This
        // is the check the `bivvy run` warning points users to.
        let config = r#"
app_name: test-app
settings:
  paralel: true
steps:
  hello:
    command: echo hello
    comand: echo hello
workflows:
  default:
    steps: [hello]
"#;
        let temp = setup_project(config);
        let args = LintArgs::default();
        let cmd = LintCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        let output = collect_output(&ui);
        assert!(
            output.contains(
                "Unknown field 'paralel' in settings will be ignored. \
                 Run 'bivvy lint' to check your config."
            ),
            "expected settings typo warning, got:\n{output}"
        );
        assert!(
            output.contains(
                "Unknown field 'comand' in step 'hello' will be ignored. \
                 Run 'bivvy lint' to check your config."
            ),
            "expected step typo warning, got:\n{output}"
        );
    }

    #[test]
    fn lint_valid_config() {
        let config = r#"
app_name: test-app
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;
        let temp = setup_project(config);
        let args = LintArgs::default();
        let cmd = LintCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
        let out = collect_output(&ui);
        assert!(out.contains("(project config)"), "got:\n{out}");
        assert!(out.contains("Errors:"), "got:\n{out}");
        // Stable invariant: zero-error card carries "Errors: ... 0" — find a
        // line that starts with whitespace + "Errors" + colon + spaces + "0".
        assert!(
            out.lines().any(|l| {
                let t = l.trim_start();
                t.starts_with("Errors:") && t.trim_end().ends_with(" 0")
            }),
            "got:\n{out}"
        );
    }

    #[test]
    fn lint_applies_config_default_output() {
        let config = r#"
app_name: test-app
settings:
  defaults:
    output: quiet
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;
        let temp = setup_project(config);
        let args = LintArgs::default();
        let cmd = LintCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        assert_eq!(ui.output_mode(), crate::ui::OutputMode::Quiet);
    }

    #[test]
    fn lint_invalid_config_circular_dependency() {
        let config = r#"
app_name: test-app
steps:
  a:
    command: echo a
    depends_on: [b]
  b:
    command: echo b
    depends_on: [a]
workflows:
  default:
    steps: [a, b]
"#;
        let temp = setup_project(config);
        let args = LintArgs::default();
        let cmd = LintCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(!result.success);
    }

    #[test]
    fn lint_detects_self_dependency() {
        let config = r#"
app_name: test-app
steps:
  a:
    command: echo a
    depends_on: [a]
workflows:
  default:
    steps: [a]
"#;
        let temp = setup_project(config);
        let args = LintArgs::default();
        let cmd = LintCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(!result.success);
    }

    #[test]
    fn lint_detects_undefined_dependency() {
        let config = r#"
app_name: test-app
steps:
  a:
    command: echo a
    depends_on: [nonexistent]
workflows:
  default:
    steps: [a]
"#;
        let temp = setup_project(config);
        let args = LintArgs::default();
        let cmd = LintCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(!result.success);
    }

    #[test]
    fn lint_json_format() {
        let config = r#"
app_name: test-app
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;
        let temp = setup_project(config);
        let args = LintArgs {
            format: "json".to_string(),
            ..Default::default()
        };
        let cmd = LintCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
    }

    #[test]
    fn lint_sarif_format() {
        let config = r#"
app_name: test-app
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;
        let temp = setup_project(config);
        let args = LintArgs {
            format: "sarif".to_string(),
            ..Default::default()
        };
        let cmd = LintCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
    }

    #[test]
    fn lint_strict_mode_fails_on_warnings() {
        let config = r#"
app_name: My App With Spaces
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;
        let temp = setup_project(config);
        let args = LintArgs {
            strict: true,
            ..Default::default()
        };
        let cmd = LintCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        // App name with spaces produces a warning
        assert!(!result.success);
    }

    #[test]
    fn lint_without_strict_passes_on_warnings() {
        let config = r#"
app_name: My App With Spaces
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;
        let temp = setup_project(config);
        let args = LintArgs::default();
        let cmd = LintCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        // Without strict mode, warnings don't cause failure
        assert!(result.success);
    }

    #[test]
    fn lint_targeted_workflow_does_not_parse_other_workflow_files() {
        // A malformed sibling workflow file must NOT block targeted lint of
        // a different workflow.
        let temp = setup_project("app_name: Test\n");
        let workflows_dir = temp.path().join(".bivvy").join("workflows");
        fs::create_dir_all(&workflows_dir).unwrap();
        fs::write(
            workflows_dir.join("good.yml"),
            r#"
steps:
  hello:
    command: echo hello
workflow:
  steps:
    - hello
"#,
        )
        .unwrap();
        fs::write(
            workflows_dir.join("broken.yml"),
            "this: is: not: valid: yaml: at all",
        )
        .unwrap();

        let args = LintArgs {
            target: Some("good".to_string()),
            ..Default::default()
        };
        let cmd = LintCommand::new(temp.path(), args);
        let mut ui = MockUI::new();
        let result = cmd.execute(&mut ui).unwrap();
        assert!(result.success);
    }

    #[test]
    fn lint_unknown_target_errors_with_available_list() {
        let temp = setup_project("app_name: Test\n");
        let workflows_dir = temp.path().join(".bivvy").join("workflows");
        fs::create_dir_all(&workflows_dir).unwrap();
        fs::write(workflows_dir.join("ci.yml"), "steps: []").unwrap();

        let args = LintArgs {
            target: Some("nonexistent".to_string()),
            ..Default::default()
        };
        let cmd = LintCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();
        assert!(!result.success);
        assert!(ui
            .messages()
            .iter()
            .chain(ui.errors().iter())
            .any(|m| m.contains("nonexistent")));
    }

    #[test]
    fn lint_step_target_loads_step_file() {
        let temp = setup_project(
            r#"
app_name: Test
steps:
  other:
    command: "echo other"
"#,
        );
        let steps_dir = temp.path().join(".bivvy").join("steps");
        fs::create_dir_all(&steps_dir).unwrap();
        fs::write(
            steps_dir.join("deps.yml"),
            "command: yarn install\ntitle: Install deps\n",
        )
        .unwrap();

        let args = LintArgs {
            target: Some("deps".to_string()),
            ..Default::default()
        };
        let cmd = LintCommand::new(temp.path(), args);
        let mut ui = MockUI::new();
        let result = cmd.execute(&mut ui).unwrap();
        assert!(result.success);
    }

    #[test]
    fn lint_all_flag_uses_full_merge() {
        let temp = setup_project(
            r#"
app_name: Test
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#,
        );
        let workflows_dir = temp.path().join(".bivvy").join("workflows");
        fs::create_dir_all(&workflows_dir).unwrap();
        fs::write(
            workflows_dir.join("ci.yml"),
            "description: CI\nsteps: [hello]\n",
        )
        .unwrap();

        let args = LintArgs {
            all: true,
            ..Default::default()
        };
        let cmd = LintCommand::new(temp.path(), args);
        let mut ui = MockUI::new();
        let result = cmd.execute(&mut ui).unwrap();
        assert!(result.success);

        let out = collect_output(&ui);
        // We should see both the project card AND the workflow card.
        assert!(out.contains("(project config)"), "got:\n{out}");
        assert!(out.contains("workflow file: ci"), "got:\n{out}");
    }

    #[test]
    fn lint_parse_error_renders_card_with_diagnostic() {
        let temp = setup_project("my-settings:\n  foo: bar\n");
        let args = LintArgs::default();
        let cmd = LintCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();
        assert!(!result.success);
        assert_eq!(result.exit_code, 1);

        let out = collect_output(&ui);
        assert!(out.contains("(project config)"), "got:\n{out}");
        assert!(
            out.contains("error[parse-error/unknown-field]"),
            "got:\n{out}"
        );
        assert!(
            out.contains("did you mean") || out.contains("`my-settings`"),
            "got:\n{out}"
        );
    }

    #[test]
    fn lint_list_rules_includes_known_rule() {
        let temp = setup_project("app_name: t\n");
        let args = LintArgs {
            list_rules: true,
            ..Default::default()
        };
        let cmd = LintCommand::new(temp.path(), args);
        let mut ui = MockUI::new();
        let result = cmd.execute(&mut ui).unwrap();
        assert!(result.success);
        let out = collect_output(&ui);
        assert!(out.contains("Bivvy Lint Rules"), "got:\n{out}");
        assert!(out.contains("app-name-format"), "got:\n{out}");
        assert!(out.contains("parse-error/unknown-field"), "got:\n{out}");
    }

    #[test]
    fn lint_list_rules_does_not_require_config() {
        // Note: no .bivvy directory at all.
        let temp = TempDir::new().unwrap();
        let args = LintArgs {
            list_rules: true,
            ..Default::default()
        };
        let cmd = LintCommand::new(temp.path(), args);
        let mut ui = MockUI::new();
        let result = cmd.execute(&mut ui).unwrap();
        assert!(result.success);
    }

    #[test]
    fn lint_explain_known_rule_shows_severity() {
        let temp = TempDir::new().unwrap();
        let args = LintArgs {
            explain: Some("app-name-format".to_string()),
            ..Default::default()
        };
        let cmd = LintCommand::new(temp.path(), args);
        let mut ui = MockUI::new();
        let result = cmd.execute(&mut ui).unwrap();
        assert!(result.success);
        let out = collect_output(&ui);
        assert!(out.contains("app-name-format"), "got:\n{out}");
        assert!(out.contains("Severity:"), "got:\n{out}");
    }

    #[test]
    fn lint_explain_unknown_rule_exits_one() {
        let temp = TempDir::new().unwrap();
        let args = LintArgs {
            explain: Some("nope-not-a-rule".to_string()),
            ..Default::default()
        };
        let cmd = LintCommand::new(temp.path(), args);
        let mut ui = MockUI::new();
        let result = cmd.execute(&mut ui).unwrap();
        assert!(!result.success);
        assert_eq!(result.exit_code, 1);
        let out = collect_output(&ui);
        assert!(out.contains("no such rule"), "got:\n{out}");
    }

    #[test]
    fn lint_explain_includes_parse_error_synthetic_rule() {
        let temp = TempDir::new().unwrap();
        let args = LintArgs {
            explain: Some("parse-error/unknown-field".to_string()),
            ..Default::default()
        };
        let cmd = LintCommand::new(temp.path(), args);
        let mut ui = MockUI::new();
        let result = cmd.execute(&mut ui).unwrap();
        assert!(result.success);
        let out = collect_output(&ui);
        assert!(out.contains("parse-error/unknown-field"));
        assert!(out.contains("Severity:    error"));
    }

    #[test]
    fn lint_no_rule_disables_specific_rule() {
        let config = r#"
app_name: My App With Spaces
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;
        let temp = setup_project(config);
        let args = LintArgs {
            no_rule: vec!["app-name-format".to_string()],
            strict: true,
            ..Default::default()
        };
        let cmd = LintCommand::new(temp.path(), args);
        let mut ui = MockUI::new();
        let result = cmd.execute(&mut ui).unwrap();
        // app-name-format would fire a warning on "My App With Spaces", but
        // we disabled it — strict mode therefore passes.
        assert!(result.success);
    }

    #[test]
    fn lint_only_runs_explicit_rule() {
        let config = r#"
app_name: My App With Spaces
steps:
  alpha:
    command: echo alpha
    depends_on: [alpha]
workflows:
  default:
    steps: [alpha]
"#;
        let temp = setup_project(config);
        // With --rule self-dependency only, the run should still fail (the
        // self-dep rule fires) and the report should contain the self-dep
        // diagnostic.
        let args = LintArgs {
            rule: vec!["self-dependency".to_string()],
            ..Default::default()
        };
        let cmd = LintCommand::new(temp.path(), args);
        let mut ui = MockUI::new();
        let result = cmd.execute(&mut ui).unwrap();
        assert!(!result.success);
        let out = collect_output(&ui);
        assert!(out.contains("self-dependency"), "got:\n{out}");
        // app-name-format would normally also fire (warning), but the rule
        // filter excluded it. With strict off, that doesn't affect exit code,
        // but the diagnostic should not appear in the output either.
        assert!(!out.contains("app-name-format"), "got:\n{out}");
    }

    #[test]
    fn label_for_path_classifies_well_known_paths() {
        let proj = PathBuf::from("/proj");
        assert_eq!(
            label_for_path(&proj.join(".bivvy/config.yml"), &proj),
            "project config"
        );
        assert_eq!(
            label_for_path(&proj.join(".bivvy/config.local.yml"), &proj),
            "local config"
        );
        assert_eq!(
            label_for_path(&proj.join(".bivvy/workflows/release.yml"), &proj),
            "workflow file: release"
        );
        assert_eq!(
            label_for_path(&proj.join(".bivvy/steps/install.yml"), &proj),
            "step file: install"
        );
    }

    #[test]
    fn wrap_text_breaks_on_word_boundaries() {
        let lines = wrap_text("a quick brown fox jumps", 10);
        assert!(lines.iter().all(|l| l.len() <= 10));
        assert_eq!(lines.join(" "), "a quick brown fox jumps");
    }
}
