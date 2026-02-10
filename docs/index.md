# Why Bivvy?

> "Set up your bivvy before the mission."

Every project has a setup script. And every setup script eventually becomes a mess.

## The Problem

You've seen it. Maybe you wrote it:

```bash
#!/bin/bash
# bin/setup - "just run this!"

brew bundle
asdf install
bundle install
yarn install
cp .env.example .env
docker-compose up -d
rails db:create db:migrate db:seed
echo "Done! Maybe. Check the output for errors."
```

It works. Until it doesn't.

- **No state**: Run it twice, it does everything twice
- **No recovery**: Step 4 fails, you re-run the whole thing
- **No visibility**: What's stale? What actually needs to run?
- **No sharing**: Every project reinvents the same 20 lines
- **No help**: It failed. Good luck.

## What Bivvy Does

Bivvy replaces your setup script with a declarative config and a smart CLI:

```yaml
# .bivvy/config.yml
app_name: myapp

steps:
  brew:
    template: brew
  ruby:
    template: bundler
    watches: [Gemfile.lock]
  node:
    template: yarn
    watches: [yarn.lock]
  docker:
    template: docker/compose
  db:
    command: "rails db:prepare"
    depends_on: [docker]

workflows:
  default:
    steps: [brew, ruby, node, docker, db]
```

Then:

```bash
bivvy run
```

```bash
bivvy status
```

## What Makes Bivvy Different

### 1. State Tracking

Bivvy remembers what's done.

```
$ bivvy status
myapp - Status

  [✓] brew         2 hours ago
  [!] ruby         stale (Gemfile.lock changed)
  [✓] node         2 hours ago
  [✓] docker       running
  [pending] db     not yet run
```

Run `bivvy run` and it only runs what's needed.

### 2. Watch Files

```yaml
ruby:
  template: bundler
  watches: [Gemfile.lock]
```

Changed `Gemfile.lock`? Ruby step is stale. Didn't change it? Skip.

No more "just run the whole script to be safe."

### 3. Error Recovery

Step fails? Bivvy doesn't just dump you back to the terminal:

```
✗ ruby failed: bundle install exited 1

How do you want to proceed?
  [r] Retry     - run the step again
  [f] Fix       - bundle update nokogiri
  [s] Skip      - continue without this step
  [x] Shell     - drop to shell to debug
  [q] Abort     - stop the workflow
```

Context-aware suggestions. Real options. Not "figure it out yourself."

### 4. Template Registry

Stop copying the same setup logic between projects:

```yaml
# Your config just references templates
steps:
  ruby:
    template: bundler
```

Templates are reusable, shareable, and overridable. Bivvy ships with common ones. You can add your own. Teams can share them.

### 5. Remote Sources for Teams

Central team control, distributed execution:

```yaml
template_sources:
  - name: company
    type: git
    url: git@github.com:mycompany/bivvy-templates.git

steps:
  ruby:
    template: company:ruby/standard  # Team-managed template
```

Platform team updates the template. Every project gets the update. No more copy-paste drift.

### 6. Two UI Modes

Interactive terminal? Full-screen TUI with keyboard navigation.

CI pipeline? Clean line-based output with non-zero exit codes.

Bivvy detects which to use. Or you tell it: `--tui` / `--no-tui`.

## How Bivvy Compares

| Tool | What It Does | What's Missing |
|------|--------------|----------------|
| **bin/setup** | Runs commands | No state, no recovery, no reuse |
| **Make/Task** | Runs commands with dependencies | No state tracking, no watch files, no templates |
| **mise/asdf** | Manages runtime versions | Doesn't orchestrate setup steps |
| **Docker** | Containerized environments | Different paradigm - containers, not local setup |
| **Nix/devenv** | Hermetic reproducibility | Steep learning curve, replaces your whole toolchain |

**Bivvy's lane**: Orchestrate your existing tools (brew, bundle, yarn, docker) with state tracking, smart re-runs, and good UX.

## Bivvy vs. Nix

This comes up a lot. Here's the sharp distinction:

- **Nix** = "I want mathematically reproducible environments"
- **Bivvy** = "I want my setup script to be smarter"

Nix is the right choice if you need hermetic builds where every byte is identical across machines.

Bivvy is the right choice if you need:
- Setup that remembers what it did
- Re-runs that only do what's necessary
- Error recovery that helps instead of abandons
- Templates your team can share

Most teams don't need Nix-level reproducibility. They need "run bundle install if Gemfile.lock changed, and if it fails, let me retry or drop to a shell."

That's bivvy.

## Who It's For

**Individual devs** who:
- Work on multiple projects with similar stacks
- Are tired of re-running full setup scripts "just in case"
- Want to know what's stale without reading the script

**Teams** who:
- Onboard new devs regularly
- Want consistent setup across projects
- Need central control of setup standards
- Have flaky steps that need retry logic

**Platform engineers** who:
- Maintain setup templates for the org
- Want to push updates without touching every repo
- Need visibility into what projects are running

## Getting Started

Install (pick one):

```bash
curl -fsSL https://bivvy.dev/install.sh | sh
```

```bash
brew install bivvy-dev/bivvy/bivvy
```

```bash
cargo install bivvy
```

```bash
npm install -g bivvy
```

```bash
gem install bivvy
```

```bash
pip install bivvy
```

Initialize in your project:

```bash
bivvy init
```

Run setup:

```bash
bivvy run
```

Check status anytime:

```bash
bivvy status
```

See [Installation docs](installation/index.md) for details on each method.

## The Name

A bivvy (or bivouac) is a temporary camp set up quickly before a mission. That's what dev environment setup should be: fast, reliable, and out of your way so you can focus on the actual work.

Set up your bivvy. Get to work.
