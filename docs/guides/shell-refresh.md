---
title: Shell Refresh Handling
description: How Bivvy handles steps that require shell refresh
---

# Shell Refresh Handling

Some setup steps modify PATH or environment in ways that require a
shell reload to take effect. Bivvy detects and handles these situations
automatically.

## When Shell Refresh is Needed

Common scenarios that require shell refresh:

- Installing version managers (nvm, rbenv, pyenv)
- Installing package managers (Homebrew on new systems)
- Modifying shell configuration files (.bashrc, .zshrc)

## How Bivvy Handles It

When a step requires shell refresh:

1. **Detection**: Bivvy tracks expected PATH changes and detects
   when the current shell doesn't have them yet.

2. **Save state**: Current progress is saved to disk including:
   - Which workflow was running
   - Which steps have completed
   - Which step triggered the reload

3. **Prompt**: User is asked how to proceed:
   - Reload shell and continue (recommended)
   - Exit and reload manually
   - Skip the step

4. **Resume**: After the shell reload, the next `bivvy run --resume` picks
   up where the previous run left off.

## Example Flow

```
$ bivvy run

✓ Installing Homebrew
→ Installing rbenv (requires shell refresh)

Shell refresh required to add rbenv to PATH.
How would you like to proceed?

  > Reload shell and continue (recommended)
    Exit and reload manually
    Skip this step

Saving progress... done
Please run: exec bash

$ exec bash
$ bivvy run --resume

Resuming from previous run...
✓ Installing Homebrew (already complete)
→ Installing rbenv
```

## Manual Resume

If you close the terminal or the process is interrupted, you can
resume manually:

```
$ bivvy run --resume
```

Without `--resume`, Bivvy starts a fresh run and ignores any saved
state -- the saved file stays on disk until the next resumed run
overwrites it (or you delete it manually).

## Where the Resume State Lives

Bivvy saves a single `resume-state.json` file under the platform's
user-local data directory. There is one file per machine, not per
project; whichever project last hit a shell-refresh step "owns" the
saved state until the next reload.

| Platform | Path |
|----------|------|
| Linux | `$XDG_DATA_HOME/bivvy/resume-state.json` (defaults to `~/.local/share/bivvy/resume-state.json`) |
| macOS | `~/Library/Application Support/bivvy/resume-state.json` |
| Windows | `%LOCALAPPDATA%\bivvy\resume-state.json` |

### Cross-project behavior

Because the file is global, running `bivvy run --resume` always resumes
the most recently saved state -- even if you are now `cd`'d into a
different project. If you don't want that, run `bivvy run` without
`--resume` to start a fresh run in the current project, or delete the
resume-state file before re-running:

```bash
# Linux
rm -f ~/.local/share/bivvy/resume-state.json

# macOS
rm -f "$HOME/Library/Application Support/bivvy/resume-state.json"
```

There is no `bivvy run --no-resume` flag. To start fresh, simply omit
`--resume`; Bivvy only consults the saved state when you opt in.
