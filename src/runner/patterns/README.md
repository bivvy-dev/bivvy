# Error Pattern Registry

Declarative error pattern matching for step failure recovery.

## Purpose

When a step fails, Bivvy scans its stderr/stdout against a registry of known error patterns. Matches produce either **actionable fix suggestions** (shown in the recovery menu) or **hints** (shown below the error block). This module contains the pattern registry and all ecosystem-specific pattern definitions.

## How It Works

1. A step fails and its combined output is captured.
2. `find_fix()` iterates patterns in specificity order, looking for the first high-confidence match.
3. `find_hint()` does the same for low-confidence matches.
4. Matched patterns produce a `FixSuggestion` via a declarative `FixTemplate` — no closures, no custom logic per pattern.

Patterns are tried in the order returned by `built_in_patterns()`: ecosystem-specific patterns first, general catch-alls (command not found, permission denied) last. First match wins.

## Key Types

| Type | Role |
|------|------|
| `ErrorPattern` | A registered pattern: name, regex, context filter, confidence, and fix template |
| `FixTemplate` | Declarative description of the fix to suggest (see variants below) |
| `FixSuggestion` | Concrete output: label, command, explanation, confidence |
| `PatternContext` | When a pattern should fire: `Always`, `CommandContains(...)`, or `RequiresAny(...)` |
| `Confidence` | `High` (actionable menu item) or `Low` (hint text) |

## FixTemplate Variants

Every pattern defines its fix as one of these declarative variants. No pattern uses closures or custom logic.

| Variant | Use case |
|---------|----------|
| `Static` | Fixed label/command/explanation, no dynamic parts |
| `Template` | Uses `{1}`, `{2}` placeholders replaced with regex capture groups |
| `Hint` | Advisory only — no runnable command |
| `PlatformAware` | Different commands for macOS vs Linux |
| `ContextSwitch` | Picks from alternatives based on step command content |

## Ecosystem Modules

Each file exports a `patterns() -> Vec<ErrorPattern>` function. Patterns within a module are ordered from most specific to most general.

| Module | Ecosystem | Example patterns |
|--------|-----------|-----------------|
| `ruby.rs` | Ruby / Bundler | Native extension failures, version conflicts, missing gems |
| `node.rs` | Node.js / npm / Yarn | Missing modules, ERESOLVE, OpenSSL, Corepack |
| `python.rs` | Python / pip / Poetry | ModuleNotFoundError, externally-managed environments, venv |
| `rust_cargo.rs` | Rust / Cargo | Linker not found, pkg-config, lock file, toolchain |
| `go.rs` | Go | Missing go.sum entries, checksum mismatches |
| `java.rs` | Java / Gradle / Maven | JAVA_HOME, version detection, wrapper permissions |
| `dotnet.rs` | .NET | SDK not found, NuGet restore failures |
| `elixir.rs` | Elixir / Mix | Mix deps, compilation errors |
| `docker.rs` | Docker | Daemon not running, port conflicts, missing networks |
| `postgres.rs` | PostgreSQL | Connection refused, role missing, database not found |
| `redis.rs` | Redis | Connection refused |
| `rails.rs` | Rails | Pending migrations, database not created, credentials |
| `general.rs` | Cross-ecosystem | Command not found, permission denied, SSL errors, Git SSH |

## Adding a New Ecosystem

1. Create `src/runner/patterns/<ecosystem>.rs`.

2. Define regex patterns using the `lazy_regex!` macro:
   ```rust
   lazy_regex!(RE_MY_ERROR, r"some error pattern (\S+)");
   ```

3. Export a `patterns()` function returning `Vec<ErrorPattern>`:
   ```rust
   use super::{Confidence, ErrorPattern, FixTemplate, PatternContext};

   pub fn patterns() -> Vec<ErrorPattern> {
       vec![
           ErrorPattern {
               name: "my_ecosystem_error",       // Unique name across all patterns
               regex: RE_MY_ERROR.as_str(),
               context: PatternContext::CommandContains("mytool"),
               confidence: Confidence::High,
               fix: FixTemplate::Template {
                   label: "mytool fix {1}",
                   command: "mytool fix {1}",
                   explanation: "Something went wrong with {1}",
               },
           },
       ]
   }
   ```

4. Register the module in `mod.rs`:
   - Add `mod <ecosystem>;` to the module declarations.
   - Add `all.extend(<ecosystem>::patterns());` in `built_in_patterns()`, before `general::patterns()` (general must remain last).

5. Write tests in the same file. Every ecosystem module tests its patterns against realistic error output. Use the test helpers from `mod.rs`:
   ```rust
   #[cfg(test)]
   mod tests {
       use super::super::*;

       #[test]
       fn my_error_matches() {
           let ctx = StepContext {
               name: "build",
               command: "mytool build",
               requires: &[],
               template: None,
           };
           let error = "some error pattern widget";
           let fix = find_fix(error, &ctx).unwrap();
           assert_eq!(fix.command, "mytool fix widget");
       }
   }
   ```

## Design Decisions

**Declarative over procedural.** The previous implementation used closures to build `FixSuggestion` values. The current design uses `FixTemplate` enum variants with placeholder substitution. This makes patterns data-driven: easier to read, audit, serialize, and test.

**Context filtering.** Not every regex should fire for every step. `PatternContext` prevents false positives — a Bundler pattern should not match output from a Python step that happens to mention "installing". Most ecosystem patterns use `CommandContains` to scope to relevant commands.

**Specificity ordering.** Ecosystem patterns come before general patterns in `built_in_patterns()`. Within an ecosystem, more specific patterns come first. This ensures that a Ruby native extension error is caught by the Ruby module's targeted pattern rather than the general "command not found" fallback.

**Lazy regex compilation.** The `lazy_regex!` macro wraps each regex in a `LazyLock<Regex>` so it compiles once on first use. The `ErrorPattern` struct stores the raw `&'static str` pattern (via `.as_str()`) for the registry, and `find_fix`/`find_hint` recompile on each call. This is acceptable because pattern matching only runs on step failure, not in hot paths.

**Platform-aware fixes.** Some errors have different remedies on macOS vs Linux (e.g., installing a C linker). The `PlatformAware` variant handles this at the template level so pattern authors do not need conditional logic.

## Testing

Every ecosystem module includes tests that:
- Verify patterns match realistic error output
- Check that capture groups produce correct fix commands
- Confirm context filters exclude unrelated steps
- Test alternate error message variants

The `mod.rs` tests verify cross-cutting concerns:
- All patterns compile (`all_patterns_compile`)
- No duplicate pattern names (`no_duplicate_pattern_names`)
- Confidence routing works correctly (`find_fix` ignores low, `find_hint` ignores high)
- First-match-wins semantics
- No-match returns `None`
