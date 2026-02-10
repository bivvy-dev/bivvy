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
   when the current shell doesn't have them yet

2. **Save State**: Current progress is saved to disk including:
   - Which workflow was running
   - Which steps have completed
   - Which step triggered the reload

3. **Prompt**: User is asked how to proceed:
   - Reload shell and continue (recommended)
   - Exit and reload manually
   - Skip the step

4. **Resume**: After shell reload, Bivvy can resume from where it
   left off

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
$ bivvy run

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

Bivvy will detect the saved state and offer to continue from the
last step.

## Clearing Resume State

To clear any saved resume state and start fresh:

```
$ bivvy run --no-resume
```
