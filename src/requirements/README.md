# Requirements Subsystem

Developer-facing design doc for the requirements/gap detection system.

## Purpose

The requirements subsystem detects whether system-level prerequisites
(Ruby, Node, PostgreSQL, Docker, etc.) are available before a step runs.
When a gap is found, it offers to install the missing tool or start the
stopped service — interactively or automatically.

## Module Responsibilities

| Module | Purpose |
|--------|---------|
| `registry.rs` | Built-in requirement definitions and the `RequirementRegistry` |
| `probe.rs` | `EnvironmentProbe` — discovers version managers at well-known paths |
| `checker.rs` | `GapChecker` — evaluates each requirement against the current system |
| `status.rs` | `RequirementStatus` enum — Satisfied, SystemOnly, Inactive, ServiceDown, Missing, Unknown |
| `installer.rs` | Installation orchestration — resolves install deps and runs templates |

## Architecture

```
Step.requires: ["ruby", "postgres-server"]
        │
        ▼
  GapChecker.check_step()
        │
        ├── registry.get("ruby")  →  RequirementCheck::ManagedCommand { ... }
        │       │
        │       ▼
        │   probe.augmented_path  →  look for ruby in managed locations
        │       │
        │       ▼
        │   RequirementStatus::Satisfied | Inactive | SystemOnly | Missing
        │
        ├── registry.get("postgres-server")  →  RequirementCheck::ServiceReachable { ... }
        │       │
        │       ▼
        │   RequirementStatus::Satisfied | ServiceDown | Missing
        │
        ▼
  Vec<GapResult> — per-requirement status for the step
```

## Probing Strategy

The `EnvironmentProbe` scans well-known paths for version managers
**before** any requirement is checked. This avoids repeated filesystem
lookups during the run.

Detected managers and their search locations:

| Manager | Env var | Fallback path |
|---------|---------|---------------|
| mise | `$MISE_DATA_DIR` | `~/.local/share/mise`, `~/.local/bin` |
| nvm | `$NVM_DIR` | `~/.nvm` |
| rbenv | `$RBENV_ROOT` | `~/.rbenv` |
| pyenv | `$PYENV_ROOT` | `~/.pyenv` |
| volta | `$VOLTA_HOME` | `~/.volta` |
| homebrew | `$HOMEBREW_PREFIX` | `/opt/homebrew` (arm64), `/usr/local` (x64), `/home/linuxbrew/.linuxbrew` (Linux) |

The probe builds an `augmented_path` that includes shim/bin directories
from detected managers. This allows `ManagedCommand` checks to find
binaries that aren't on the user's current `PATH`.

## Requirement Check Types

| Variant | Use case |
|---------|----------|
| `CommandSucceeds(cmd)` | Simple tool presence (e.g., `brew --version`) |
| `FileExists(path)` | Config file or binary exists |
| `ServiceReachable(cmd)` | Service health check (e.g., `pg_isready -q`) |
| `ManagedCommand { .. }` | Language runtime with version manager awareness |
| `Any(checks)` | First successful check wins (e.g., `python3` or `python`) |

## Caching

`GapChecker` caches results per requirement name for the duration of a
single run. After an install template runs, the cache entry for that
requirement is invalidated so the next check picks up the fresh state.

## Install-During-Run Flow

1. `GapChecker` finds a gap (Missing, ServiceDown, etc.)
2. If interactive, prompt user to install/start
3. Resolve install dependencies (e.g., `mise-ruby` depends on `mise`)
4. Run install templates in dependency order
5. Invalidate cache for installed requirements
6. Re-check the original requirement

## Testing

- Unit tests mock the `EnvironmentProbe` with controlled paths
- `RequirementRegistry` tests verify all 10 built-in definitions
- `GapChecker` tests use temp directories with fake binaries
- Integration tests use `assert_cmd` with config fixtures that declare
  requirements
