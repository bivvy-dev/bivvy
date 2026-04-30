# Run-path display layer

This module owns the rendering contracts for `bivvy run`. It splits what was a single `TerminalUI` god-struct into three concerns that each own their domain:

```
┌──────────────────────────────────────────────────────────────────┐
│ orchestrate.rs                                                   │
│   Constructs WorkflowDisplay; for each step asks it for a        │
│   StepDisplay via begin_step().                                  │
└────────────┬─────────────────────────────────┬───────────────────┘
             │                                 │
             ▼                                 ▼
┌──────────────────────────┐     ┌──────────────────────────────────┐
│ WorkflowDisplay          │     │ StepDisplay  (per step)          │
│ workflow.rs              │     │ step.rs                          │
│   Run header             │     │   [N/M] step_name — title        │
│   Pinned progress bar    │     │   Transient spinner region with  │
│   Run summary            │     │     bounded live-output tail     │
│   Owns its pinned bar    │     │   Error block, prompts           │
│                          │     │   Final result line:             │
│                          │     │     "      ✓ Completed (Xms)"    │
└──────────┬───────────────┘     └──────────────┬───────────────────┘
           │                                    │
           └────────────────┬───────────────────┘
                            ▼
            ┌──────────────────────────────┐
            │ TerminalSurface              │
            │ src/ui/surface.rs            │
            │   Owns the MultiProgress.    │
            │   Hands out regions, never   │
            │   bars. Nothing else touches │
            │   the multi.                 │
            └──────────────────────────────┘
```

## Why this split exists

The original `TerminalUI` owned the workflow's `MultiProgress` *and* handed out spinners that lived inside it. Step code therefore mutated the same draw region as the workflow bar, which made the multi-line spinner clearing bug observable: dangling `[██████░░░░░░░░░░] N/M steps · 0ms elapsed` lines and stale `Running …` fragments would leak into scrollback between steps. The actual problem was that step code could reach into workflow draw state at all.

With the split:

- `TerminalSurface` is the *only* place that touches `MultiProgress`. It hands out region handles (`PinnedBar`, `TransientRegion`) that drop cleanly via `ProgressBar::finish_and_clear`.
- `TerminalWorkflowDisplay` mounts the pinned bar and never reaches into transient regions.
- `TerminalStepDisplay` mounts a transient region above the pinned bar and never reaches into the workflow bar.

A step's rendering is provably finished once `StepDisplay::finish` returns: the transient region is dropped and cleared, and the final result line is written into scrollback.

## Regions, top to bottom

1. **Scrollback** — `surface.println(...)`. Step headers, status messages, error blocks, the run summary, the post-run hint. Both displays write here.
2. **Transient region** — currently-running step's spinner with its bounded live-output tail. Owned by the active `StepDisplay`. Comes and goes between steps.
3. **Pinned region** — workflow progress bar, pinned at the bottom. Owned by `WorkflowDisplay` for the entire run.

## Status labels are sourced from `StepStatus`

The label and icon on the result row (the line under each step header) come from the `StepStatus` enum in `src/steps/executor.rs`:

- `StepStatus::label()` returns the title-case label (`"Completed"`, `"Failed"`, `"Skipped"`, …).
- `StepStatus::display_char()` returns the icon (`'✓'`, `'✗'`, `'⊘'`, …).

`StepDisplay::finish` takes the status enum and derives both. **Never hardcode a label string** — the test `finish_label_comes_from_status_enum` exists to guard against that.

## Implementations

| Type                              | When it's used                                  |
|-----------------------------------|-------------------------------------------------|
| `TerminalWorkflowDisplay`         | Interactive TTY runs                            |
| `NonInteractiveWorkflowDisplay`   | CI / headless / non-interactive runs            |
| `MockWorkflowDisplay`             | Tests; captures every interaction in `MockState` for assertion |
| `TerminalStepDisplay`             | Spawned by `TerminalWorkflowDisplay::begin_step` |
| `NonInteractiveStepDisplay`       | Spawned by `NonInteractiveWorkflowDisplay::begin_step` |
| `MockStepDisplay`                 | Spawned by `MockWorkflowDisplay::begin_step`; shares `Rc<RefCell<MockState>>` with its parent |

## Snapshot tests

`tests/run_rendering_tests.rs` spawns a real `bivvy run` in a PTY, captures every byte (including ANSI escapes), feeds the stream through a `vt100` emulator, and snapshots the rendered screen via `insta`. These tests pin the visual output so refactors here can't silently change what users see. They also act as the regression test for the multi-line spinner bug: a multi-line `command:` block must not leave dangling progress-bar fragments or stale `Running …` lines in scrollback.
