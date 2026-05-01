---
title: Remote Template Sources
description: Configuring remote template repositories
---

# Remote Template Sources

Bivvy can fetch templates from remote HTTP sources, allowing teams to share template repositories and keep templates up-to-date automatically.

> Bivvy currently fetches remote templates over HTTP only. Git-based fetching is not yet supported, so the source URL must point at a YAML payload served over HTTP/HTTPS.

## HTTP Sources

A `template_sources` entry pulls one or more templates from a URL. The URL must return either a single template YAML document or a YAML list of templates.

```yaml
template_sources:
  - url: https://example.com/templates/index.yml
    cache:
      ttl: "24h"        # how long to keep the cached payload before refetching
      strategy: etag    # use ETag headers for conditional requests
```

### Options

| Option | Description | Default |
|--------|-------------|---------|
| `url` | URL to a template YAML file or list | required |
| `priority` | Source priority when the same template name appears in multiple remotes (lower = higher priority) | `100` |
| `timeout` | Network timeout in seconds for the HTTP fetch | `30` |
| `cache` | Cache configuration (see below) | optional |
| `auth` | Authentication configuration (see below) | optional |

If `cache` is omitted, Bivvy still caches the fetched payload using a default TTL of seven days.

## Cache Configuration

The `cache` block controls how long a remote source's payload is reused before Bivvy refetches it.

```yaml
template_sources:
  - url: https://example.com/templates/index.yml
    cache:
      ttl: "7d"
      strategy: etag
```

| Field | Description |
|-------|-------------|
| `ttl` | Required. Duration string: `30s`, `15m`, `24h`, `7d`. Bare integers are treated as seconds. |
| `strategy` | Optional. Cache validation strategy: `etag` (default) or `git`. |

### Cache strategies

Two strategies are recognised:

- **`etag`** (default) — for HTTP sources, makes a conditional request using the cached `If-None-Match` header so the server can answer `304 Not Modified` and avoid retransmitting the body.
- **`git`** — for remote sources backed by a Git host that surfaces commit SHAs. Bivvy validates against the cached SHA. Because Bivvy does not yet clone Git repositories directly, this strategy currently only changes how the cache decides to refresh; the fetch itself is still HTTP.

> A `ttl` strategy is not configurable. The TTL is applied to every cache strategy and bounds how long the cached payload is reused before validation runs again.

## Authentication

Use `auth` for sources that require credentials. The token is read from the named environment variable at runtime so secrets stay out of the config file.

### Bearer tokens

```yaml
template_sources:
  - url: https://templates.internal.example.com/index.yml
    auth:
      type: bearer
      token_env: BIVVY_TEMPLATES_TOKEN
```

Bivvy adds an `Authorization: Bearer <token>` header using the value of the `BIVVY_TEMPLATES_TOKEN` environment variable.

### Custom headers

```yaml
template_sources:
  - url: https://templates.internal.example.com/index.yml
    auth:
      type: header
      token_env: BIVVY_TEMPLATES_TOKEN
      header: X-Templates-Auth
```

The named header is sent with the value of the environment variable.

## Source Priority

When the same template name is provided by multiple remote sources, the lower `priority` wins. Local templates always take precedence over remotes regardless of priority:

1. Project-local templates (`.bivvy/templates/`)
2. User-local templates (`~/.bivvy/templates/`)
3. Remote templates (in ascending order of `priority`)
4. Built-in templates

## Example Configuration

```yaml
# .bivvy/config.yml
app_name: "MyApp"

template_sources:
  # Internal company templates — checked first
  - url: https://templates.internal.example.com/bivvy/index.yml
    priority: 10
    timeout: 15
    cache:
      ttl: "1h"
      strategy: etag
    auth:
      type: bearer
      token_env: COMPANY_TEMPLATES_TOKEN

  # Public template registry — fallback
  - url: https://registry.example.com/bivvy/templates.yml
    priority: 100
    cache:
      ttl: "24h"
      strategy: etag

steps:
  deps:
    template: company-deps   # resolved from one of the remote sources above
```

## See Also

- [Templates overview](./index.md)
- [Cache management](../commands/cache.md)
