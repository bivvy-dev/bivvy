---
title: Shell Completions
description: Setting up tab completions for Bivvy
---

# Shell Completions

Bivvy supports tab completions for bash, zsh, fish, and PowerShell.

## Generate Completions

Bash:

```bash
bivvy completions bash > ~/.local/share/bash-completion/completions/bivvy
```

Zsh:

```bash
bivvy completions zsh > ~/.zfunc/_bivvy
```

Fish:

```bash
bivvy completions fish > ~/.config/fish/completions/bivvy.fish
```

PowerShell:

```powershell
bivvy completions powershell >> $PROFILE
```

## Bash Setup

Add to your `~/.bashrc`:

```bash
if [ -f ~/.local/share/bash-completion/completions/bivvy ]; then
    source ~/.local/share/bash-completion/completions/bivvy
fi
```

## Zsh Setup

Add to your `~/.zshrc`:

```zsh
fpath=(~/.zfunc $fpath)
autoload -Uz compinit && compinit
```

## Fish Setup

Completions are automatically loaded from `~/.config/fish/completions/`.

## Homebrew

If you installed via Homebrew, completions are automatically set up.
