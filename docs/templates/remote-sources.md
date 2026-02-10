---
title: Remote Template Sources
description: Configuring remote template repositories
---

# Remote Template Sources

Bivvy can fetch templates from remote sources, allowing teams to share
template repositories and keep templates up-to-date automatically.

## HTTP Sources

HTTP sources fetch templates directly from a URL:

```yaml
template_sources:
  - type: http
    url: https://example.com/templates
    cache:
      ttl: 3600  # 1 hour in seconds
      strategy: etag  # Use ETag headers for cache validation
```

### Options

| Option | Description | Default |
|--------|-------------|---------|
| `url` | Base URL for templates | (required) |
| `cache.ttl` | Time-to-live in seconds | 604800 (7 days) |
| `cache.strategy` | Cache invalidation strategy | `ttl` |

## Git Sources

Git sources clone templates from a git repository:

```yaml
template_sources:
  - type: git
    url: https://github.com/org/templates.git
    ref: main
    path: templates/
    cache:
      strategy: git  # Uses commit SHA for invalidation
```

### Options

| Option | Description | Default |
|--------|-------------|---------|
| `url` | Git repository URL | (required) |
| `ref` | Branch, tag, or commit | `HEAD` |
| `path` | Path within repository | (root) |
| `cache.ttl` | Time-to-live in seconds | 604800 (7 days) |
| `cache.strategy` | Cache invalidation strategy | `ttl` |

## Cache Strategies

Bivvy supports three cache invalidation strategies:

### TTL (Time-To-Live)

The default strategy. Templates are considered stale after the TTL expires.

```yaml
cache:
  ttl: 86400  # 24 hours
  strategy: ttl
```

### ETag

For HTTP sources, uses ETag headers to validate cache freshness. Makes
conditional requests with `If-None-Match` to check if content has changed.

```yaml
cache:
  strategy: etag
```

### Git

For Git sources, compares the cached commit SHA against the remote. Only
re-fetches if the remote has new commits.

```yaml
cache:
  strategy: git
```

## Source Priority

When the same template exists in multiple sources, Bivvy uses this priority:

1. Project-local templates (`.bivvy/templates/`)
2. User-local templates (`~/.bivvy/templates/`)
3. Remote templates (by configured priority)
4. Built-in templates

## Example Configuration

```yaml
# .bivvy/config.yml
app_name: "MyApp"

template_sources:
  # Company-wide templates with fast refresh
  - type: git
    url: https://github.com/mycompany/bivvy-templates.git
    ref: main
    cache:
      strategy: git
      ttl: 3600  # Check for updates hourly

  # Public template registry
  - type: http
    url: https://registry.bivvy.dev/templates
    cache:
      strategy: etag
      ttl: 86400  # Daily refresh

steps:
  deps:
    # Uses template from remote source
    template: company-deps
```

## See Also

- [Template Registry](./registry.md) - Local template management
- [Cache Management](../commands/cache.md) - Managing cached templates
