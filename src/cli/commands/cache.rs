//! Cache command implementation.
//!
//! Provides `bivvy cache list`, `bivvy cache clear`, etc.

use anyhow::Result;
use clap::{Args, Subcommand};

use crate::cache::{default_cache_dir, format_duration, CacheStore, CacheValidator};
use crate::ui::{Prompt, PromptResult, PromptType, UserInterface};

use super::dispatcher::{Command, CommandResult};

/// Arguments for the cache command.
#[derive(Debug, Clone, Args)]
pub struct CacheArgs {
    #[command(subcommand)]
    pub command: CacheSubcommand,
}

/// Cache subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum CacheSubcommand {
    /// List cached entries.
    List {
        /// Show detailed information.
        #[arg(long)]
        verbose: bool,
        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Clear the cache.
    Clear {
        /// Only clear expired entries.
        #[arg(long)]
        expired: bool,
        /// Don't prompt for confirmation.
        #[arg(short, long)]
        force: bool,
    },
    /// Show cache statistics.
    Stats,
}

/// The cache command implementation.
pub struct CacheCommand {
    args: CacheArgs,
}

impl CacheCommand {
    /// Create a new cache command.
    pub fn new(args: CacheArgs) -> Self {
        Self { args }
    }
}

impl Command for CacheCommand {
    fn execute(&self, ui: &mut dyn UserInterface) -> crate::error::Result<CommandResult> {
        let cache_dir = default_cache_dir();
        let store = CacheStore::new(&cache_dir);

        let exit_code = match &self.args.command {
            CacheSubcommand::List { verbose, json } => list_cache(&store, *verbose, *json, ui)?,
            CacheSubcommand::Clear { expired, force } => clear_cache(&store, *expired, *force, ui)?,
            CacheSubcommand::Stats => show_stats(&store, ui)?,
        };

        Ok(if exit_code == 0 {
            CommandResult::success()
        } else {
            CommandResult::failure(exit_code)
        })
    }
}

fn list_cache(
    store: &CacheStore,
    verbose: bool,
    json: bool,
    ui: &mut dyn UserInterface,
) -> Result<i32> {
    let entries = store.list()?;

    if entries.is_empty() {
        ui.message("Cache is empty");
        return Ok(0);
    }

    if json {
        let output = serde_json::to_string_pretty(&entries)?;
        ui.message(&output);
        return Ok(0);
    }

    ui.message(&format!("{} cached entries:\n", entries.len()));

    for entry in entries {
        let status = if entry.is_expired() {
            "expired"
        } else {
            "fresh"
        };
        let remaining = entry.metadata.remaining_ttl();
        let ttl_str = if remaining > 0 {
            format_duration(chrono::Duration::seconds(remaining))
        } else {
            "expired".to_string()
        };

        if verbose {
            ui.message(&format!("  {} ({})", entry.template_name, entry.source_id));
            ui.message(&format!("    Status: {}", status));
            ui.message(&format!("    TTL: {}", ttl_str));
            ui.message(&format!("    Size: {} bytes", entry.metadata.size_bytes));
            if let Some(etag) = &entry.metadata.etag {
                ui.message(&format!("    ETag: {}", etag));
            }
            if let Some(sha) = &entry.metadata.commit_sha {
                ui.message(&format!("    Commit: {}", sha));
            }
            ui.message("");
        } else {
            ui.message(&format!(
                "  {} [{}] {}",
                entry.template_name, status, ttl_str
            ));
        }
    }

    Ok(0)
}

fn clear_cache(
    store: &CacheStore,
    expired_only: bool,
    force: bool,
    ui: &mut dyn UserInterface,
) -> Result<i32> {
    if expired_only {
        let validator = CacheValidator::new(store);
        let removed = validator.cleanup_expired()?;
        ui.success(&format!("Cleared {} expired entries", removed));
        return Ok(0);
    }

    let entries = store.list()?;
    if entries.is_empty() {
        ui.message("Cache is already empty");
        return Ok(0);
    }

    let count = entries.len();
    if !force && ui.is_interactive() {
        let prompt = Prompt {
            key: "clear_cache".to_string(),
            question: format!("Clear {} cached entries?", count),
            prompt_type: PromptType::Confirm,
            default: Some("false".to_string()),
        };

        match ui.prompt(&prompt)? {
            PromptResult::Bool(true) => {}
            _ => {
                ui.message("Cancelled");
                return Ok(0);
            }
        }
    }

    let cleared = store.clear()?;
    ui.success(&format!("Cleared {} entries", cleared));

    Ok(0)
}

fn show_stats(store: &CacheStore, ui: &mut dyn UserInterface) -> Result<i32> {
    let entries = store.list()?;
    let total_size = store.total_size()?;
    let expired_count = entries.iter().filter(|e| e.is_expired()).count();
    let fresh_count = entries.len() - expired_count;

    ui.message("Cache Statistics:\n");
    ui.message(&format!("  Total entries: {}", entries.len()));
    ui.message(&format!("  Fresh: {}", fresh_count));
    ui.message(&format!("  Expired: {}", expired_count));
    ui.message(&format!("  Total size: {} bytes", total_size));
    ui.message(&format!("  Location: {}", store.root().display()));

    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_test_store() -> (TempDir, CacheStore) {
        let temp = TempDir::new().unwrap();
        let store = CacheStore::new(temp.path());
        (temp, store)
    }

    #[test]
    fn list_empty_cache() {
        let (_temp, store) = setup_test_store();

        let entries = store.list().unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn list_with_entries() {
        let (_temp, store) = setup_test_store();

        store
            .store("http:test", "template1", "content", 3600)
            .unwrap();
        store
            .store("http:test", "template2", "content", 3600)
            .unwrap();

        let entries = store.list().unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn show_stats_empty() {
        let (_temp, store) = setup_test_store();

        let entries = store.list().unwrap();
        let total = store.total_size().unwrap();

        assert_eq!(entries.len(), 0);
        assert_eq!(total, 0);
    }

    #[test]
    fn show_stats_with_entries() {
        let (_temp, store) = setup_test_store();

        store.store("http:test", "t1", "12345", 3600).unwrap();
        store.store("http:test", "t2", "123", 0).unwrap();

        let entries = store.list().unwrap();
        let total = store.total_size().unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(total, 8); // 5 + 3
    }
}
