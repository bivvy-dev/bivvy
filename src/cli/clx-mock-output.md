# Bivvy CLX — Expected Terminal Output

> What you should see when running each command in an interactive TTY.
>
> **Colors shown as annotations** — in a real terminal, ANSI escape codes
> produce the colors described in `[brackets]`.

---

## 1. `bivvy init` (Ruby + Node project)

```
                                                        ┌──────────────────┐
⛺ Project Setup                                        │ [magenta bold] ⛺ │
                                                        │ [bold] title     │
Scanning project...                                     └──────────────────┘

Detected technologies:
✓   Ruby - Gemfile, Rails                               ← [green] ✓
✓   Node.js - package.json, Yarn                        ← [green] ✓

Use [space] to toggle, [a] to toggle all, [enter] to confirm

? Select steps to include                               ← dialoguer multiselect
> [x] bundler — Gemfile detected
  [x] yarn — package.json with yarn.lock detected

Added .bivvy/config.local.yml to .gitignore
✓ Created .bivvy/config.yml                             ← [green] ✓

? Run setup now? [Yes / No]                             ← dialoguer select prompt
```

If user picks **No**:

```
  💡 Run `bivvy run` when you're ready to start setup.  ← [magenta dim] hint
```

If user picks **Yes** → flows into `bivvy run` below.

---

## 2. `bivvy run` (success — 3 steps)

```
                                                        ┌────────────────────┐
⛺ MyApp · default workflow · 3 steps                   │ [magenta bold] ⛺   │
                                                        │ [bold] MyApp       │
                                                        │ [dim] · default…   │
                                                        └────────────────────┘

[1/3] bundler — Install Ruby gems                       ← [dim] [1/3]  [bold] bundler
    ⠋ Running bundler...                                ← [magenta] spinner
    ⠙ Running bundler...                                   (animated)
      bundle install                                    ← live output lines
      Fetching gem metadata...                             scroll underneath
    ✓ bundler (2.3s)                                    ← spinner finishes [green] ✓
  [██████░░░░░░░░░░] 1/3 steps · 2.3s elapsed          ← [magenta] progress bar

[2/3] yarn — Install Node packages                      ← [dim] [2/3]  [bold] yarn
    ⠋ Running yarn...
    ✓ yarn (4.1s)
  [███████████░░░░░] 2/3 steps · 6.4s elapsed

[3/3] db_setup — Set up database                        ← [dim] [3/3]  [bold] db_setup
    ⠋ Running db_setup...
    ✓ db_setup (1.0s)
  [████████████████] 3/3 steps · 7.4s elapsed

  ┌─ Summary ──────────────────────────                 ← [dim] box borders
  │ ✓ bundler              2.3s                         ← [green] ✓  [dim] duration
  │ ✓ yarn                 4.1s                         ← [green] ✓
  │ ✓ db_setup             1.0s                         ← [green] ✓
  ├────────────────────────────────────
  │ Total: 7.4s · 3 run · 0 skipped                    ← [dim] · separators
  └────────────────────────────────────
  ✓ Setup complete!                                     ← [green] (from default impl)
  💡 Run `bivvy status` to verify setup health.         ← [magenta dim] hint
```

---

## 3. `bivvy run` (with skipped + failed step)

```
⛺ MyApp · default workflow · 4 steps

[1/4] bundler — Install Ruby gems
    ○ Skipped (bundle exec --version)                    ← [dim] ○

[2/4] yarn — Install Node packages
    ⠋ Running yarn...
    ✓ yarn (3.2s)
  [████████░░░░░░░░] 2/4 steps · 3.2s elapsed

[3/4] db_setup — Set up database
    ⠋ Running db_setup...
    ✗ Failed (0.8s)                                     ← [red bold] ✗

    ┌─ Command ──────────────────────────               ← [dim] box borders
    │ bin/rails db:setup                                ← [dim italic] command
    ├─ Output ───────────────────────────
    │ ActiveRecord::NoDatabaseError                     ← raw output lines
    │ FATAL: role "myapp" does not exist
    └────────────────────────────────────

    Hint: Check database credentials in .env            ← [magenta dim] hint text

  [████████████░░░░] 3/4 steps · 4.0s elapsed

[4/4] migrate — Run migrations
    ⊘ Blocked (dependency failed)                       ← [orange] ⊘
  [████████████████] 4/4 steps · 4.0s elapsed

  ┌─ Summary ──────────────────────────
  │ ○ bundler              bundle exec --version             ← [dim] ○  [dim] detail
  │ ✓ yarn                 3.2s                         ← [green] ✓
  │ ✗ db_setup             0.8s                         ← [red bold] ✗
  │ ⊘ migrate                                           ← [orange] ⊘
  ├────────────────────────────────────
  │ Total: 4.0s · 2 run · 1 skipped
  └────────────────────────────────────
  ✗ Setup failed: db_setup                              ← [red bold] ✗
  💡 Fix and re-run: `bivvy run --only=db_setup`        ← [magenta dim] hint
```

---

## 4. `bivvy run` (interactive — skippable step prompts)

```
⛺ MyApp · default workflow · 2 steps

? Already complete. Re-run [1/2] bundler — Install Ruby gems? [y/N]
                                                        ← dialoguer confirm
    (user presses N)
    ○ Skipped (bundle exec --version)

? Run [2/2] db_setup — Set up database? [Y/n]          ← dialoguer confirm
    (user presses Y)

    ⠋ Running db_setup...
    ✓ db_setup (1.2s)
  [████████████████] 2/2 steps · 1.2s elapsed

  ┌─ Summary ──────────────────────────
  │ ○ bundler              bundle exec --version
  │ ✓ db_setup             1.2s
  ├────────────────────────────────────
  │ Total: 1.2s · 1 run · 1 skipped
  └────────────────────────────────────
  ✓ Setup complete!
  💡 Run `bivvy status` to verify setup health.
```

---

## 5. `bivvy status` (mixed state)

```
  ⛺ MyApp — Status                                     ← [magenta bold] ⛺
                                                           [bold] MyApp
                                                           [dim] — Status

  Last run: 2 minutes ago · default workflow            ← [bold] key  [dim] values

  Steps:                                                ← [bold] label
    ✓ bundler              2.3s                         ← [green] ✓  [dim] 2.3s
    ✓ yarn                 4.1s                         ← [green] ✓
    ✗ db_setup             0.8s                         ← [red bold] ✗
    ◌ migrate                                           ← [dim] ◌ (never run)

  💡 Fix and re-run: `bivvy run --only=db_setup`        ← [magenta dim] hint
```

---

## 6. `bivvy status` (fresh project — nothing run yet)

```
  ⛺ MyApp — Status

  Steps:
    ◌ bundler                                           ← [dim] ◌
    ◌ yarn                                              ← [dim] ◌
    ◌ db_setup                                          ← [dim] ◌

  💡 Run `bivvy run` to start setup.                    ← [magenta dim] hint
```

---

## 7. `bivvy list`

```
  Steps:                                                ← [bold] label
    bundler (template: bundler)                         ← [bold] name  [dim] (template…)
    yarn (template: yarn)                               ← [bold] name
    db_setup — bin/rails db:setup                       ← [bold] name  [dim] —  [dim italic] cmd
      Set up the application database                   ← [dim] description
    migrate — bin/rails db:migrate                      ← [bold] name
      └── depends on: db_setup                          ← [dim] dependency tree

  Workflows:                                            ← [bold] label
    default: bundler → yarn → db_setup → migrate        ← [bold] name  [dim] arrow chain
      Full development setup                            ← [dim] description
    ci: bundler → yarn                                  ← [bold] name
```

---

## 8. `bivvy last`

```
  ⛺ Last Run                                           ← [magenta bold] ⛺  [bold] title

  Workflow:  default
  When:      2 minutes ago (2026-02-14 15:30:45)        ← [dim] relative + absolute
  Duration:  7.4s                                       ← [dim] duration
  Status:    ✓ Success                                  ← [green] ✓

  Steps:                                                ← [bold] label
    ✓ bundler              2.3s                         ← [green] ✓
    ✓ yarn                 4.1s
    ✓ db_setup             1.0s
```

If the last run had failures:

```
  ⛺ Last Run

  Workflow:  default
  When:      5 minutes ago (2026-02-14 15:25:12)
  Duration:  4.0s
  Status:    ✗ Failed                                   ← [red bold] ✗

  Steps:
    ✓ bundler              2.3s
    ✓ yarn                 3.2s
    ✗ db_setup             0.8s                         ← [red bold] ✗
    ○ migrate              skipped                      ← [dim] ○

  ✗ Error: Step 'db_setup' failed                       ← [red bold]
```

---

## 9. `bivvy history`

```
  ⛺ Run History                                        ← [magenta bold] ⛺  [bold] title

    ✓  2 minutes ago      default      3 steps  7.4s   ← [green] ✓  aligned columns
    ✗  1 hour ago         default      2 steps  4.0s   ← [red bold] ✗
    ✓  yesterday          ci           2 steps  5.1s   ← [green] ✓
    ✓  3 days ago         default      3 steps  8.2s
```

---

## Color Reference

| Theme Slot     | Color                  | Used For                         |
|----------------|------------------------|----------------------------------|
| `success`      | green                  | ✓ icons, success messages        |
| `error`        | red bold               | ✗ icons, error messages          |
| `warning`      | orange (256-color 208) | ⚠ icons, ⊘ blocked              |
| `info`         | magenta                | ◆ running, progress bars         |
| `dim`          | dim/gray               | secondary text, ○ ◌, durations   |
| `highlight`    | bold                   | app names, step names            |
| `header`       | magenta bold           | ⛺ icon, section headers         |
| `step_number`  | dim                    | [1/3] counters                   |
| `step_title`   | bold                   | step names in run output         |
| `duration`     | dim                    | 2.3s, time elapsed               |
| `command`      | dim italic             | command strings in error blocks  |
| `border`       | dim                    | ┌ │ ├ └ box-drawing characters   |
| `hint`         | magenta dim            | 💡 contextual hints              |
| `key`          | bold                   | "Workflow:", "Steps:" labels      |

---

## StatusKind Icons

| Kind      | TTY Icon | Non-TTY    | Color        |
|-----------|----------|------------|--------------|
| Success   | ✓        | [ok]       | green        |
| Failed    | ✗        | [FAIL]     | red bold     |
| Skipped   | ○        | [skip]     | dim          |
| Pending   | ◌        | [pending]  | dim          |
| Running   | ◆        | [run]      | magenta      |
| Blocked   | ⊘        | [blocked]  | orange       |
| Warning   | ⚠        | [warn]     | orange       |
