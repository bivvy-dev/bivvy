---
title: bivvy cache
description: Manage cached templates
---

# bivvy cache

Manage the template cache used for remote template sources.

## Subcommands

### `bivvy cache list`

List all cached template entries.

```bash
bivvy cache list
```

```bash
bivvy cache list --verbose
```

```bash
bivvy cache list --json
```

### `bivvy cache clear`

Clear the template cache.

```bash
bivvy cache clear
```

```bash
bivvy cache clear --force
```

```bash
bivvy cache clear --expired
```

### `bivvy cache stats`

Show cache statistics.

```bash
bivvy cache stats
```

Output:
```
Cache Statistics:

  Total entries: 5
  Fresh: 3
  Expired: 2
  Total size: 12847 bytes
  Location: /Users/you/.cache/bivvy/templates
```

## Cache Location

The template cache is stored in:

- **macOS**: `~/Library/Caches/bivvy/templates/`
- **Linux**: `~/.cache/bivvy/templates/`
- **Windows**: `%LOCALAPPDATA%\bivvy\cache\templates\`

## See Also

- [Remote Template Sources](../templates/remote-sources.md)
- [Configuration](../configuration/schema.md)
