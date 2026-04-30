# Refactor: Workflow vs Step UI Ownership (Run Path)

## Scope

This refactor is scoped to the **`run` execution path** — the rendering of workflow chrome and step output during a workflow run. It does **not** touch the UI surface used by `init`, `status`, `list`, `lint`, `add`, `templates`, `last`, `history`, or `bivvy` with no args (when not running). Those commands continue to use the existing `UserInterface` trait composition unchanged.

When `init` triggers a run via its "Run setup now?" prompt, it hands off to the run path; the run path constructs its own display types rather than borrowing init's UI.

The refactor follows the same principle the runner already uses: **subsystems own their domain contracts**. Workflows know what a workflow display needs, steps know what a step display needs, and `ui/` provides only generic terminal primitives.

## Context

Today, `TerminalUI` (`src/ui/terminal.rs`) is a single struct that owns the workflow's `MultiProgress` *and* hands out spinners that get inserted into it via `multi.insert_from_back(1, raw_bar)` (`src/ui/spinner.rs:54`). The step's spinner is therefore structurally a child of the workflow's draw region, with no contract that says "when a step yields, its lines are closed."

This produces a visible bug. The step's spinner message — `format!("Running \`{}\`...", command)` (`src/runner/execution.rs:88`) — is multi-line whenever the YAML `command:` block has embedded newlines. `indicatif`'s `MultiProgress` does not reliably track multi-line bar height, so when `finish_and_clear` runs, only one row is cleared. The remaining rows of the spinner, plus the workflow bar that re-rendered between updates, get baked into the scrollback. Single-line commands look fine; multi-line commands leave behind a stale `[██████░░░░░░░░░░] 3/8 steps · 0ms elapsed` next to a stale `⠋ Running …` fragment, right where the step ran.

A second source of multi-line spinner messages exists today by design: `live_output_callback` (`src/ui/spinner.rs:136`) builds the spinner message as `base + "\n" + indented tail lines`, with a ring buffer of 2-3 lines. This is the live-output streaming feature. Any fix for the multi-line bug must keep this UX or replace it intentionally.

The bug is a symptom. The actual problem is that step code can directly mutate workflow draw state. The goal of this refactor is to give the workflow and step layers each their own UI domain, with no shared mutable draw state, so a step cannot corrupt workflow chrome and the workflow cannot interfere with step rendering.

## Current State

`src/ui/mod.rs` already splits the UI into sub-traits: `OutputWriter`, `Prompter`, `SpinnerFactory`, `ProgressDisplay`, `WorkflowDisplay`, `UiState`, with `UserInterface` as a blanket super-trait. The composition is fine. The problems are:

1. `WorkflowDisplay` (the existing one at `mod.rs:131`) lives in `ui/` but is a workflow-runner concern — it has `show_run_header`, `init_workflow_progress`, `show_workflow_progress`, `finish_workflow_progress`, `show_run_summary`. None of those make sense outside a workflow run. Audit confirmed: every production caller is on the run path (`cli/commands/run.rs:363, 628`; `runner/orchestrate.rs:110, 123, 222, 233`). It belongs with the workflow manager.
2. There is no per-step display domain at all. Step rendering happens through ad-hoc `SpinnerFactory` + `OutputWriter` calls scattered through `runner/execution.rs` and `runner/step_manager.rs`. There is no contract that a step's lines are closed before the next step begins.
3. The single struct `TerminalUI` implements every trait *and* owns the `MultiProgress`. Step code can reach into workflow draw state because it must, to attach its spinner to the multi.

## Target Architecture

### Three layers

```
┌────────────────────────────────────────────────────────────────┐
│ Orchestrate (src/runner/orchestrate.rs)                        │
│   Constructs TerminalSurface, WorkflowDisplay,                 │
│   and per-step StepDisplay handles.                            │
│   Owns show_run_header / show_run_summary calls.               │
└───────────┬─────────────────────────────────┬──────────────────┘
            │                                 │
            ▼                                 ▼
┌──────────────────────────┐      ┌──────────────────────────────┐
│ WorkflowDisplay          │      │ StepDisplay (per step)       │
│ src/runner/display/      │      │ src/runner/display/          │
│   Header                 │      │   Step header line           │
│   Persistent progress    │      │   Spinner                    │
│   bar (pinned bottom)    │      │   Live output                │
│   Summary                │      │   Error block                │
│                          │      │   Prompts                    │
│   Owns its pinned-bar    │      │   Final result line          │
│   handle.                │      │                              │
│                          │      │   Owns a transient region    │
│                          │      │   handle scoped to its       │
│                          │      │   lifetime.                  │
└──────────┬───────────────┘      └──────────────┬───────────────┘
           │                                     │
           └──────────────┬──────────────────────┘
                          ▼
            ┌──────────────────────────────┐
            │ TerminalSurface              │
            │ src/ui/surface.rs            │
            │   Owns the MultiProgress.    │
            │   Exposes regions, not bars. │
            │   Nothing outside this       │
            │   struct touches the multi.  │
            └──────────────────────────────┘
```

### Module placement

| Type / trait | Location | Rationale |
|---|---|---|
| `TerminalSurface`, `PinnedBar`, `TransientRegion` | `src/ui/surface.rs` | Generic terminal primitive; no workflow semantics. |
| `WorkflowDisplay` (trait + `TerminalWorkflowDisplay`) | `src/runner/display/workflow.rs` | Workflow concern; owned by the runner. |
| `StepDisplay` (trait + `TerminalStepDisplay`) | `src/runner/display/step.rs` | Step concern; owned by the runner. |
| `RunSummary`, `StepSummary`, `RunHeader` | `src/runner/display/mod.rs` | Workflow-run data, not generic UI. Moved out of `ui/mod.rs:209-238`. |

### Regions, top to bottom

1. **Scrollback** — printed via `surface.println(...)`. Step header lines, status messages, error blocks, summary, anything non-spinner. Both displays write here.
2. **Transient region** — currently-running step's spinner with its bounded live-output tail. Owned by the active `StepDisplay`. Comes and goes between steps.
3. **Pinned region** — workflow progress bar. Owned by `WorkflowDisplay` for the entire run.

### Ownership

`TerminalSurface` is wrapped in `Arc<TerminalSurface>` and shared by both displays. `MultiProgress` is already cheaply cloneable internally, so `Arc` adds little overhead and makes the lifetime story trivial: the surface lives at least as long as any display.

### Trait sketch

```rust
// ───────── src/ui/surface.rs (low-level, dumb) ─────────

pub struct TerminalSurface {
    multi: MultiProgress,
}

impl TerminalSurface {
    pub fn new() -> Arc<Self>;

    /// Print into scrollback (above any pinned/transient bars).
    pub fn println(&self, line: &str);

    /// Pin a bar at the bottom of the live region. Used once, by the
    /// workflow. The handle's Drop clears the bar.
    pub fn pin_bottom(&self, bar: ProgressBar) -> PinnedBar;

    /// Mount a transient region above whatever is pinned. The caller
    /// declares the maximum line count it will ever render. The surface
    /// uses that count to clear the region deterministically on Drop —
    /// not relying on `indicatif`'s height tracking.
    pub fn transient_above_pinned(
        &self,
        bar: ProgressBar,
        max_lines: usize,
    ) -> TransientRegion;

    /// Hide pinned/transient bars for the duration of `f`, then restore.
    /// Used by step prompts. Encapsulates the existing
    /// `set_draw_target(hidden) / set_draw_target(stderr)` pattern from
    /// terminal.rs:158-164.
    pub fn with_cursor_freed<R>(&self, f: impl FnOnce() -> R) -> R;
}

// ───────── src/runner/display/workflow.rs ─────────

pub trait WorkflowDisplay {
    fn show_header(&mut self, hdr: &RunHeader);

    fn start_progress(&mut self, total: usize);
    fn update_progress(&mut self, current: usize, total: usize, elapsed: Duration);
    fn finish_progress(&mut self);

    fn show_summary(&mut self, summary: &RunSummary);

    /// Hand off to the step layer. The returned StepDisplay shares only
    /// the surface (Arc), not any draw state.
    fn begin_step(&mut self, index: usize, total: usize) -> Box<dyn StepDisplay>;
}

// ───────── src/runner/display/step.rs ─────────

pub trait StepDisplay {
    fn show_header(&mut self, step_number: &str, step_name: &str, title: Option<&str>);

    /// Mounts the transient region. `command` is normalized to a single
    /// line internally (newlines collapsed to spaces, truncated). The
    /// declared max_lines reserves room for the live-output tail.
    fn start_running(&mut self, command: &str);
    fn update_live_output(&mut self, line: OutputLine);

    fn message(&mut self, msg: &str);
    fn warning(&mut self, msg: &str);
    fn show_error_block(&mut self, block: &ErrorBlock);

    fn prompt(&mut self, prompt: &Prompt) -> Result<PromptResult>;

    /// Terminal states. Each consumes self so the step's lines are
    /// definitively closed before control returns to the workflow.
    /// The `Drop` impl on the inner TransientRegion is the panic-safety
    /// net only; finish_* is the canonical close path.
    fn finish_success(self: Box<Self>, duration: Duration);
    fn finish_skipped(self: Box<Self>, reason: &str);
    fn finish_failed(self: Box<Self>, duration: Duration);
    fn finish_blocked(self: Box<Self>, reason: &str);
}
```

### Live-output story

The existing UX is a 2-3 line tail under the spinner (`spinner.rs:136-180`, `execution.rs:99-110`). This is preserved:

- `StepDisplay::start_running` mounts the transient region with `max_lines = 1 + tail_capacity` (4 in verbose mode, 3 in normal — matches today's `2-3 lines` plus the spinner row).
- `update_live_output` updates the spinner message in the existing ring-buffer style.
- `TransientRegion::Drop` (and the `finish_*` paths) clear `max_lines` lines deterministically using terminal cursor operations, not indicatif's height tracking. The multi-line bug is fixed because the surface knows the line count and clears all of them.

Verbose-mode non-interactive output keeps streaming through `VerboseStreamSink` to scrollback as today (`execution.rs:111-119`).

### Why this fixes the bug

- Step code cannot reach into the workflow bar's region — there is no API for it. `TransientRegion` is the only way a step gets a bar, and it lives above the pinned bar.
- The surface tracks the transient region's max line count itself; clears are deterministic.
- `finish_*` methods consume `Box<Self>`, so a step has to pick a terminal state. When the workflow loop iterates, the prior step's region is provably closed.
- Prompts go through `surface.with_cursor_freed`, which encapsulates the proven `set_draw_target(hidden)` pattern. No coordination scattered through the codebase.

## Resolved Questions

- **Prompt coordination (was Q1).** Use `TerminalSurface::with_cursor_freed`, which wraps the existing `set_draw_target(hidden) / set_draw_target(stderr)` pattern at `terminal.rs:158-164`. `StepDisplay::prompt` calls it.
- **Header/summary location (was Q5).** Move from `cli/commands/run.rs:363, 628` to `runner/orchestrate.rs`. CLI passes a `RunHeader` (or skip flag — the existing `suppress_header` flag at `run.rs:356` carries through) into the runner; orchestrate calls `display.show_header` and `display.show_summary`.
- **`clear_lines` (was Q6).** The current implementation at `terminal.rs:399-408` admits in its comment that it doesn't work with multi-progress active. With the split, the surface owns line counts and clears deterministically. `clear_lines` and its trait method on `UiState` can be deleted as part of Step 4 (it's only called on the run path; non-run callers verified via grep at planning time).
- **`WorkflowDisplay` collision.** The existing `ui::WorkflowDisplay` is removed entirely as part of Step 2. Audit confirmed every caller is run-path. The new trait lives in `runner/display/`.

## Remaining Open Questions

1. **`OutputMode` (verbose / quiet / silent) plumbing.** Each new display takes the mode at construction. Quiet mode probably skips the pinned bar entirely; silent mode skips everything except errors. Confirm the exact suppression rules by reading current `OutputMode` checks during Step 2/3.

2. **Non-interactive run path.** Today `NonInteractiveUI` (`src/ui/non_interactive.rs`, 420 lines) implements `WorkflowDisplay`. With the trait moving to `runner/`, we need a `NonInteractiveWorkflowDisplay` and `NonInteractiveStepDisplay` that print structured lines and skip the pinned bar entirely. The decision: do we keep these in `non_interactive.rs` (alongside the rest of the non-interactive UI) or co-locate with the new traits in `runner/display/`? Lean toward co-locating in `runner/display/`, since the trait lives there.

3. **Sibling plans.** `runner-architecture-cleanup.md` and `workflow-refactor-plan.md` both touch `orchestrate.rs` and `step_manager.rs` — the same files Step 4 wires. Reconcile order before starting. If runner-architecture-cleanup lands first, Step 4 follows its new structure; if this plan lands first, the cleanup absorbs the new types.

## Migration Plan

Each step is a separate atomic commit per CLAUDE.md. Tests come **first** for every step — write the failing tests, then implement to green.

### Step 0: Reproduce the bug as a failing system test
Write a system test that runs a workflow with a deliberately multi-line `command:` block and asserts the rendered output contains no dangling `[██████░░░░░░░░░░]` characters or stale `Running …` fragments between step boundaries. This test fails today and is the bookend for Step 6.

### Step 1: `TerminalSurface` in `src/ui/surface.rs`
**Tests first:** unit tests for region semantics — `println` writes scrollback, `pin_bottom` returns a working pinned bar that clears on Drop, `transient_above_pinned` returns a region that clears `max_lines` lines on Drop regardless of the bar's actual rendered height, `with_cursor_freed` hides and restores the multi-progress draw target. Test that multi-line input to `start_running` is normalized to one line.

**Implementation:** `TerminalSurface`, `PinnedBar`, `TransientRegion`, `Arc::new` constructor. No callers yet.

### Step 2: `WorkflowDisplay` in `src/runner/display/workflow.rs`
**Tests first:** mock the `Arc<TerminalSurface>` (or use a real one with `ProgressDrawTarget::hidden()`) and assert that `show_header`, `start_progress`, `update_progress`, `finish_progress`, `show_summary`, and `begin_step` produce the expected scrollback writes and pinned-bar lifecycle.

**Implementation:**
- Define the trait and `TerminalWorkflowDisplay`.
- Move `RunSummary`, `StepSummary` from `src/ui/mod.rs:209-238` to `src/runner/display/mod.rs`. Add a `RunHeader` struct that bundles `app_name / workflow / step_count / version / env_name`.
- **Remove the old `ui::WorkflowDisplay` trait** from `src/ui/mod.rs:131` in the same commit.
- Drop `WorkflowDisplay` from the `UserInterface` super-trait composition at `src/ui/mod.rs:198-206`.
- Delete the three impls at `src/ui/terminal.rs:215-361`, `src/ui/non_interactive.rs:149-227` (approximate line range for the impl block), `src/ui/mock.rs:352-380`.
- Delete the now-orphaned tests at `src/ui/mock.rs:790, 873, 897, 919, 930` and `src/ui/non_interactive.rs:411-417`.
- Not yet wired into orchestrate.

Verify nothing else outside the run path implements or calls the old methods (audit done at plan time; re-grep before committing).

### Step 3: `StepDisplay` in `src/runner/display/step.rs`
**Tests first:** assert the transient region mounts on `start_running`, the live-output ring buffer behaves the same as today (2-3 line tail, oldest evicted), `finish_success` consumes self and the region is cleared, prompts route through `with_cursor_freed`, multi-line commands are normalized.

**Implementation:** `StepDisplay` trait, `TerminalStepDisplay`, `Drop` as panic safety net only, `finish_*` consuming `Box<Self>` as the canonical close path.

### Step 4: Wire the run path to the new types
**Tests first:** update existing run-path integration tests (`tests/system_*.rs` that exercise full runs) to match the new rendering. Add tests for the new orchestrate ownership of header/summary.

**Implementation:**
- `runner/orchestrate.rs`: replace `&mut dyn UserInterface` parameters with `&mut dyn WorkflowDisplay` (and let it spawn `Box<dyn StepDisplay>` per step).
- `runner/step_manager.rs`: take `&mut dyn StepDisplay` instead of `&mut dyn UserInterface`.
- `runner/execution.rs`: replace `ui.start_spinner_indented` and the manual `live_output_callback` plumbing (`execution.rs:90-110`) with `step_display.start_running` and `step_display.update_live_output`. The `live_output_callback` helper itself becomes an internal detail of `TerminalStepDisplay` (or moves into `runner/display/step.rs`).
- `cli/commands/run.rs`: stop calling `ui.show_run_header` (`run.rs:363`) and `ui.show_run_summary` (`run.rs:628`). Pass a `RunHeader` (and the `suppress_header` flag) into the runner; orchestrate makes those calls.
- Add a non-interactive `WorkflowDisplay` + `StepDisplay` pair (per Open Q2) so headless / CI runs keep working.
- Delete `clear_lines` from `UiState` (`src/ui/mod.rs:188`) and its impl at `terminal.rs:399-408`. Confirm no remaining callers.

### Step 5: Verify and document the bug is fixed
**Verify** the Step 0 system test now passes. Add a manual smoke checklist to the PR description: run a workflow with a multi-line YAML command, run a workflow with the live-output tail visible, run a workflow that prompts mid-step.

**Document** by updating `src/ui/README.md` with the three-layer model and region semantics, plus a section in `src/runner/display/README.md` (new) covering the `WorkflowDisplay`/`StepDisplay` contracts and why they live in the runner. Rustdoc on every new public item.

## Out of Scope

- **Other commands.** `init`, `status`, `list`, `lint`, `add`, `templates`, `last`, `history`, and standalone `bivvy` continue to use the existing `UserInterface` trait composition. `MockUI` and `NonInteractiveUI` continue to serve them.
- **`UserInterface` god trait.** It stays. The only changes are dropping `WorkflowDisplay` from its super-trait bound and deleting `clear_lines` from `UiState`.
- **Full TUI rewrite** (ratatui, alternate screen).
- **Visual design changes.** Progress bar, header, summary keep their current look; only ownership moves.
- **Decision engine, check evaluator, step execution semantics.** Behavior identical; only rendering changes.