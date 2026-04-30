# Bivvy CLX (CLI Experience) Design Norms

Developer-facing guide for building consistent, polished CLI output.

## Color Assignments

Colors are **semantic** — each color maps to a specific meaning across the entire CLI.

| Purpose | Color | Style | Example |
|---------|-------|-------|---------|
| Success / completed | Green | normal | `✓ install_deps (1.2s)` |
| Error / failure | Red | **bold** | `✗ build failed` |
| Warning / caution / blocked | Orange (256: 208) | normal | `⚠ Node version mismatch`, `⊘ blocked` |
| Info / running / hints / progress / divider | Fuchsia/Magenta | normal | Spinners, `Run bivvy status to verify` |
| Secondary / metadata | — | dim | Timestamps, durations, borders |
| Primary emphasis | — | **bold** | Step names in headings, app name |

The `console` crate supports these via:
- `Style::new().color256(208)` for orange
- `Style::new().magenta()` for fuchsia
- `Style::new().green()`, `.red().bold()`, `.dim()`, `.bold()`

## Typography Norms

### Text separation

Use ` · ` for text separation between distinct elements in things like headers. Never use `:`, `;`, `,`, or `-`.
Color: teal
Text: dim

- `bivvy · default workflow · 7 steps`

Use ` | ` to notate optionality in things like configuration options.

- `--foo | -f`

Use `: ` to separate labels from values in key-value displays.

- `Workflow: default`

### Bold

Use bold for elements the user's eye should land on first:

- App name in header: `⛺ **bivvy** · default workflow · 7 steps`
- Step names in step headings: `[2/7] **install_deps** · Install dependencies`
- Key labels in key-value displays: `**Workflow:** default`
- Error icon `✗` is red bold (the only bold icon)

### Dim

Use dim for supporting context the user can skip:

- Descriptions after step names: ` Install dependencies`
- Timestamps and durations: `1.2s`, `2 minutes ago`
- Box-drawing borders: `┌ │ └ ├`
- Progress counters: `[2/7]`
- Commands shown in error blocks

### Normal (no style)

Default for body text and values in key-value pairs.

## Status Vocabulary (`StatusKind`)

One canonical set of status icons used everywhere:

| StatusKind | Icon | Color | Non-TTY |
|-----------|------|-------|---------|
| Success | `✓` | green | `[ok]` |
| Failed | `✗` | red bold | `[FAIL]` |
| Skipped | `` | dim | `[skip]` |
| Pending | `◌` | dim | `[pending]` |
| Running | `⠋` | fuchsia | `[run]` |
| Blocked | `⊘` | orange | `[blocked]` |
| Warning | `⚠` | orange | `[warn]` |

Never invent ad-hoc status symbols. Always use `StatusKind`.

## Output Modes

| Mode | Status | Spinners | Live Output | Errors |
|------|--------|----------|-------------|--------|
| Verbose | yes | yes | 3 lines | yes |
| Normal | yes | yes | 2 lines | yes |
| Quiet | no | no | no | yes |
| Silent | no | no | no | no |

### Which method to use

- `ui.message(msg)` — General informational text. Hidden in Quiet/Silent.
- `ui.success(msg)` — Operation completed successfully. Hidden in Quiet/Silent.
- `ui.warning(msg)` — Recoverable issue or caution. Hidden in Quiet/Silent.
- `ui.error(msg)` — Something failed. Always shown (except Silent).
- `ui.show_hint(hint)` — Contextual next-step suggestion. Hidden in Quiet/Silent.

## Spinner Lifecycle

```
let mut spinner = ui.start_spinner("Running install_deps...");
// optionally update:
spinner.set_message("Installing packages...");
// finish with exactly ONE of:
spinner.finish_success("install_deps (1.2s)");
spinner.finish_error("install_deps failed");
spinner.finish_skipped("install_deps (bundle exec --version)");
```

Spinners are hidden in Quiet/Silent mode (they create a `ProgressBar::hidden()`).

## Live Step Output

During step execution, the last 2-3 lines of command output appear below the spinner (dim, indented). This gives users confidence that work is happening:

```
    ⠋ Running install_deps...
      yarn install v1.22.19
        Resolving packages...
```

On finish, the live output lines are cleared and replaced with the final status:

```
    ✓ install_deps (1.2s)
```

## Prompts

All interactive prompts use selectable options with arrow keys + keyboard shortcuts.
Can be configured to support keyboard shortcuts.
Can be configured to be single or multi-select.
No inline `[Y/n]` style prompts.

```
  Run setup now?
  › Yes (y)
    No  (n)
```

```
  Run setup now?
  › Yes (y)
    No  (n)
```

In non-interactive mode, prompts use defaults or skip entirely.

## Non-Interactive / Non-TTY

- No spinners (hidden)
- No color (plain theme)
- Status uses bracketed text: `[ok]`, `[FAIL]`, `[skip]`, etc.
- No prompts — uses defaults or skips
- Respects `NO_COLOR` env var

### CI Output

When `is_ci()` detects a CI environment (via `CI`, `GITHUB_ACTIONS`, etc.):

- Non-interactive mode is forced automatically (no need for `--non-interactive`)
- The workflow progress bar is suppressed (noisy in log-based CI output)
- Headers, step output, summaries, and errors are preserved
- Version is shown in the run header: `⛺ bivvy v1.6.1 · ci workflow · 9 steps`

## Adding New UI Methods

1. Add the method to the `UserInterface` trait in `src/ui/mod.rs` with a default implementation (so existing impls don't break)
2. Override in `TerminalUI` (`src/ui/terminal.rs`) with the real implementation
3. Override in `NonInteractiveUI` (`src/ui/non_interactive.rs`) for non-TTY behavior
4. Add capture support in `MockUI` (`src/ui/mock.rs`) for testing

### Where new UI lives: generic vs run-path

`UserInterface` is the *generic* UI used by every command (`init`, `status`, `list`, `lint`, `add`, `templates`, `last`, `history`, …). It owns scrollback writes (`message`, `success`, `warning`, `error`), prompts, ad-hoc spinners, headers, and progress lines.

The **`run` command** has its own narrower contracts because workflow chrome and per-step rendering must coordinate around a single pinned progress bar:

- **`crate::ui::surface::TerminalSurface`** — the only thing that owns the `MultiProgress`. Exposes regions (pinned bar, transient region above it) and a `with_cursor_freed` helper for prompts. See the `surface.rs` rustdoc.
- **`crate::runner::display::WorkflowDisplay`** — workflow header, persistent progress bar, run summary. Lives in the *runner*, not in `ui/`, because it's a workflow-runner concern.
- **`crate::runner::display::StepDisplay`** — per-step header, transient spinner with bounded live-output tail, error block, prompts, final result line. The result label and icon come from `StepStatus::label()` / `StepStatus::display_char()` — never hardcode them.

If the new method is for the run path (touches the spinner, the workflow bar, or per-step output), add it to `WorkflowDisplay` or `StepDisplay`, *not* `UserInterface`.

## Box Drawing

Use for bordered blocks.
Can be configured to optionally omit the title, header, and/or footer.

```
┌─ Title ──────────────────────────
│ Header content
├──────────────────────────────────
│ Content line 1
│ Content line 2
├──────────────────────────────────
│ Footer content
└──────────────────────────────────
```

All border characters rendered in dim. Title rendered in bold.
