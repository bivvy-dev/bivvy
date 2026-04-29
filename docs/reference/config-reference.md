---
title: Configuration Reference
description: Complete field reference for all Bivvy configuration types
---

# Configuration Reference

Complete reference for every configurable field in Bivvy. See the annotated YAML files for copy-paste examples:

- [config-reference.yml](config-reference.yml) — full `.bivvy/config.yml` reference
- [template-reference.yml](template-reference.yml) — full custom template reference

## Config File (`.bivvy/config.yml`)

### Top-Level Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `app_name` | string | — | Display name for the project |
| `settings` | [Settings](#settings) | `{}` | Global defaults |
| `steps` | map of [Step](#step) | `{}` | Named setup tasks |
| `workflows` | map of [Workflow](#workflow) | `{}` | Step sequences |
| `template_sources` | list of [TemplateSource](#template-source) | `[]` | Remote template registries |
| `secrets` | map of [Secret](#secret) | `{}` | External secret providers |
| `requirements` | map of [CustomRequirement](#custom-requirement) | `{}` | Custom requirement definitions |
| `vars` | map of [VarDefinition](#var-definition) | `{}` | User-defined variables for interpolation |
| `extends` | list of `{url}` | — | Base configs to inherit |

### Settings

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `default_output` | `verbose` \| `quiet` \| `silent` | `verbose` | Output verbosity |
| `logging` | bool | `false` | Write logs to file |
| `log_path` | path | — | Log file path (relative to project root) |
| `env` | map | `{}` | Global environment variables |
| `env_file` | path | — | Global env file to load |
| `secret_env` | list | `[]` | Additional patterns to mask in output |
| `parallel` | bool | `false` | Enable parallel execution |
| `max_parallel` | int | `4` | Max concurrent steps |
| `history_retention` | int | `50` | Execution history entries to keep |
| `default_environment` | string | — | Default environment when `--env` is not set |
| `environments` | map of [EnvironmentConfig](#environment-config) | `{}` | Custom environment definitions |
| `defaults` | [Defaults](#defaults) | `{}` | Default values for step behavior flags |

### Defaults

Project-wide (or system-wide) defaults for step behavior. Step-level settings override these, and workflow `step_overrides` override step-level settings.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `auto_run` | bool | `true` | Whether steps auto-run when the pipeline says they need to run. When `false`, the user is prompted before each step executes. |
| `prompt_on_rerun` | bool | `true` | Whether to prompt before re-running a recently completed step. When `false`, recently-completed steps are silently skipped. |
| `rerun_window` | string | `"4h"` | Default rerun window for all steps. Accepts duration strings: `"4h"`, `"30m"`, `"7d"`, `"0"`/`"never"`, `"forever"`. |

### Step

At minimum, a step needs either `command` or `template`.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `command` | string | — | Shell command to execute |
| `template` | string | — | Template name from registry |
| `inputs` | map | `{}` | Inputs to pass to template |
| `title` | string | step key | Display title |
| `description` | string | — | Human-readable description |
| `depends_on` | list | `[]` | Steps that must run first |
| `check` | [Check](#check) | — | Single check (presence, execution, change) |
| `checks` | list of [Check](#check) | `[]` | Multiple checks (implicit all) |
| `satisfied_when` | list | `[]` | Conditions declaring step fulfilled |
| `precondition` | [Check](#check) | — | Gate that must pass before step runs (not bypassed by `--force`) |
| `skippable` | bool | `true` | User can skip interactively |
| `required` | bool | `false` | Cannot be skipped |
| `auto_run` | bool | — | Auto-run when pipeline says step needs to run. `None` = use global default. |
| `prompt_on_rerun` | bool | `true` | Ask before re-running |
| `allow_failure` | bool | `false` | Continue workflow on failure |
| `retry` | int | `0` | Retry attempts on failure |
| `env` | map | `{}` | Step-specific env vars |
| `env_file` | path | — | Env file for this step |
| `env_file_optional` | bool | `false` | Don't fail if env file missing |
| `required_env` | list | `[]` | Env vars that must be set |
| `prompts` | list of [Prompt](#prompt) | `[]` | Interactive prompts |
| `output` | [StepOutput](#step-output) | — | Output settings override |
| `sensitive` | bool | `false` | Hide command and suppress output |
| `requires_sudo` | bool | `false` | Needs elevated permissions |
| `before` | list | `[]` | Commands to run before step |
| `after` | list | `[]` | Commands to run after step |
| `requires` | list | `[]` | System-level prerequisites |
| `only_environments` | list | `[]` | Limit step to these environments (empty = all) |
| `environments` | map of [StepEnvironmentOverride](#step-environment-override) | `{}` | Per-environment field overrides |

### Check

Tagged union — the `type` field determines which other fields apply.

| Type | Fields | Description |
|------|--------|-------------|
| `presence` | `target`, `kind` (optional) | Check if file/directory exists |
| `execution` | `command`, `validation` | Check if command exits 0 |
| `change` | `target`, `on_change`, `kind`, `baseline` | Detect changes to a file or directory |
| `all` | `checks` (list) | Every sub-check must pass |
| `any` | `checks` (list) | At least one sub-check must pass |

### Prompt

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `key` | string | **required** | Unique key (used in `${key}` interpolation) |
| `question` | string | **required** | Question to display |
| `type` | `select` \| `multiselect` \| `confirm` \| `input` | **required** | Prompt type |
| `options` | list of `{label, value}` | `[]` | Choices (for select/multiselect) |
| `default` | varies | — | Default value |

### Step Output

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `default` | `verbose` \| `quiet` \| `silent` | — | Output mode for this step |
| `logging` | bool | — | Override logging for this step |

### Workflow

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `description` | string | — | Human-readable description |
| `steps` | list | `[]` | Ordered step names to execute |
| `overrides` | map of [StepOverride](#step-override) | `{}` | Per-step behavior overrides |
| `settings` | [WorkflowSettings](#workflow-settings) | — | Workflow-level settings |
| `auto_run_steps` | bool | — | Override `auto_run` for all steps in this workflow. Individual step overrides take precedence. |
| `env` | map | `{}` | Workflow-level env vars |
| `env_file` | path | — | Workflow-level env file |

### Step Override

Used inside `workflows.<name>.overrides.<step>` to tweak step behavior for a specific workflow.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `skip_prompt` | bool | `false` | Skip prompts, just run |
| `required` | bool | — | Override step's `required` flag |
| `auto_run` | bool | — | Override step's `auto_run` flag |
| `prompt_on_rerun` | bool | — | Override step's `prompt_on_rerun` flag |
| `check` | [Check](#check) | — | Override step's check |

### Workflow Settings

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `non_interactive` | bool | `false` | Force non-interactive mode |

### Template Source

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `url` | string | **required** | URL to template repository |
| `priority` | int | `100` | Lower = higher priority |
| `timeout` | int | `30` | Network timeout (seconds) |
| `cache` | [Cache](#cache) | — | Cache configuration |
| `auth` | [Auth](#auth) | — | Authentication |

### Cache

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `ttl` | string | **required** | Time-to-live (`"7d"`, `"24h"`) |
| `strategy` | `etag` \| `git` | `etag` | Cache invalidation strategy |

### Auth

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `type` | `bearer` \| `header` | **required** | Auth type |
| `token_env` | string | **required** | Env var containing the token |
| `header` | string | — | Custom header name (for `type: header`) |

### Secret

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `command` | string | **required** | Command whose stdout is the secret |

### Environment Config

Used inside `settings.environments.<name>`.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `detect` | list of [EnvironmentDetectRule](#environment-detect-rule) | `[]` | Auto-detection rules |
| `default_workflow` | string | — | Workflow to use when this environment is active |
| `provided_requirements` | list | `[]` | Requirements assumed satisfied |

### Environment Detect Rule

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `env` | string | **required** | Environment variable name to check |
| `value` | string | — | Expected value (omit to match on presence) |

### Step Environment Override

Used inside `steps.<name>.environments.<env>`. All fields are optional — only specified fields override the base step.

| Field | Type | Description |
|-------|------|-------------|
| `title` | string | Override display title |
| `description` | string | Override description |
| `command` | string | Override shell command |
| `env` | map of string → string\|null | Override env vars (`null` removes a key) |
| `check` | [Check](#check) | Override completion check |
| `precondition` | [Check](#check) | Override precondition |
| `skippable` | bool | Override skip permission |
| `allow_failure` | bool | Override failure behavior |
| `requires_sudo` | bool | Override sudo requirement |
| `sensitive` | bool | Override sensitive flag |
| `before` | list | Override pre-step hooks |
| `after` | list | Override post-step hooks |
| `depends_on` | list | Override dependencies |
| `requires` | list | Override system requirements |
| `retry` | int | Override retry attempts |

### Var Definition

Each entry in the top-level `vars` map is either a static string or a computed value.

| Form | YAML Syntax | Description |
|------|-------------|-------------|
| Static | `name: "value"` | Plain string |
| Computed | `name: { command: "..." }` | Shell command whose trimmed stdout becomes the value |

Computed variables run once at workflow start. If the command exits non-zero, the workflow fails.

Variables are resolved in priority order: prompts > preferences > vars > env > builtins.

### Custom Requirement

Used inside the top-level `requirements` map.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `check` | [CustomRequirementCheck](#custom-requirement-check) | **required** | How to verify the requirement |
| `install_template` | string | — | Template for installation |
| `install_hint` | string | — | Human-readable install instructions |

### Custom Requirement Check

Tagged union — the `type` field determines which other fields apply.

| Type | Fields | Description |
|------|--------|-------------|
| `command_succeeds` | `command` | Run command, pass on exit 0 |
| `file_exists` | `path` | Check if file/directory exists |
| `service_reachable` | `command` | Run command that probes a service |

---

## Template File (`.bivvy/templates/steps/<name>.yml`)

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `name` | string | **required** | Unique template name |
| `description` | string | **required** | Human-readable description |
| `category` | string | **required** | Category (`ruby`, `node`, `custom`, etc.) |
| `version` | string | `"1.0.0"` | Semantic version |
| `min_bivvy_version` | string | — | Minimum Bivvy version required |
| `platforms` | list | `[macos, linux, windows]` | Supported platforms |
| `detects` | list of [Detection](#detection) | `[]` | Auto-detection rules for `bivvy init` |
| `inputs` | map of [TemplateInput](#template-input) | `{}` | Parameterized inputs |
| `step` | [TemplateStep](#template-step) | **required** | Step definition |
| `environment_impact` | [EnvironmentImpact](#environment-impact) | — | Shell environment side effects |

### Detection

| Field | Type | Description |
|-------|------|-------------|
| `file` | string | Suggest template if this file exists |
| `command` | string | Suggest template if this command exits 0 |

### Template Input

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `description` | string | **required** | Human-readable description |
| `type` | `string` \| `number` \| `boolean` \| `enum` | **required** | Input type |
| `required` | bool | `false` | Must be provided |
| `default` | varies | — | Default value |
| `values` | list | `[]` | Valid values (for `enum` type) |

### Template Step

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `title` | string | — | Display title (supports `${input}`) |
| `description` | string | — | Description |
| `command` | string | — | Command (supports `${input}`) |
| `check` | [Check](#check) | — | Single completion check |
| `checks` | list of [Check](#check) | `[]` | Multiple completion checks |
| `env` | map | `{}` | Environment variables |

### Environment Impact

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `modifies_path` | bool | `false` | Step modifies PATH |
| `shell_files` | list | `[]` | Shell config files affected |
| `path_additions` | list | `[]` | Paths added to PATH |
| `note` | string | — | Note displayed after completion |
