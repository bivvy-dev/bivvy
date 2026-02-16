//! Feedback command implementation.
//!
//! Captures and manages friction points during dogfooding, with automatic
//! session correlation and optional external delivery via GitHub issues
//! or email.

use anyhow::Result;
use clap::{Args, Subcommand};

use crate::feedback::delivery::{
    self, build_github_url, build_mailto_url, build_preview, build_title, DeliveryMethod,
};
use crate::feedback::{default_store_path, FeedbackEntry, FeedbackStatus, FeedbackStore};
use crate::session::{default_store_path as session_store_path, Session, SessionId, SessionStore};
use crate::ui::{Prompt, PromptOption, PromptResult, PromptType, UserInterface};

use super::dispatcher::{Command, CommandResult};

/// Known feedback categories that map to delivery routing.
const KNOWN_CATEGORIES: &[&str] = &["bug", "feature", "ux", "other"];

/// Arguments for the `feedback` command.
#[derive(Debug, Clone, Args)]
pub struct FeedbackArgs {
    #[command(subcommand)]
    pub command: Option<FeedbackSubcommand>,

    /// Feedback message (when not using a subcommand).
    #[arg(trailing_var_arg = true)]
    pub message: Vec<String>,

    /// Tags for categorization.
    #[arg(short, long, value_delimiter = ',')]
    pub tag: Vec<String>,

    /// Session ID to attach (defaults to most recent).
    #[arg(long)]
    pub session: Option<String>,

    /// Skip the delivery prompt (save locally only).
    #[arg(long)]
    pub no_deliver: bool,
}

/// Feedback subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum FeedbackSubcommand {
    /// List feedback entries.
    List {
        /// Filter by status (open, resolved, wontfix).
        #[arg(long)]
        status: Option<String>,
        /// Filter by tag.
        #[arg(long)]
        tag: Option<String>,
        /// Show all (including resolved).
        #[arg(long)]
        all: bool,
    },
    /// Resolve a feedback entry.
    Resolve {
        /// Feedback ID.
        id: String,
        /// Resolution note.
        #[arg(short, long)]
        note: Option<String>,
    },
    /// Show feedback for a session.
    Session {
        /// Session ID (defaults to most recent).
        id: Option<String>,
    },
}

/// The feedback command implementation.
pub struct FeedbackCommand {
    args: FeedbackArgs,
}

impl FeedbackCommand {
    /// Create a new feedback command.
    pub fn new(args: FeedbackArgs) -> Self {
        Self { args }
    }
}

impl Command for FeedbackCommand {
    fn execute(&self, ui: &mut dyn UserInterface) -> crate::error::Result<CommandResult> {
        let store = FeedbackStore::new(default_store_path());

        let exit_code = match &self.args.command {
            Some(FeedbackSubcommand::List { status, tag, all }) => {
                list_feedback(&store, status.clone(), tag.clone(), *all, ui)?
            }
            Some(FeedbackSubcommand::Resolve { id, note }) => {
                resolve_feedback(&store, id, note.clone(), ui)?
            }
            Some(FeedbackSubcommand::Session { id }) => {
                show_session_feedback(&store, id.clone(), ui)?
            }
            None => {
                if self.args.message.is_empty() {
                    capture_interactive(&store, self.args.tag.clone(), self.args.no_deliver, ui)?
                } else {
                    let message = self.args.message.join(" ");
                    let category = category_from_tags(&self.args.tag);
                    capture_feedback(
                        &store,
                        &message,
                        &category,
                        self.args.tag.clone(),
                        self.args.session.clone(),
                        self.args.no_deliver,
                        ui,
                    )?
                }
            }
        };

        Ok(if exit_code == 0 {
            CommandResult::success()
        } else {
            CommandResult::failure(exit_code)
        })
    }
}

/// Extract a delivery category from the tag list.
///
/// Uses the first tag that matches a known category. Falls back to "other".
fn category_from_tags(tags: &[String]) -> String {
    tags.iter()
        .find(|t| KNOWN_CATEGORIES.contains(&t.as_str()))
        .cloned()
        .unwrap_or_else(|| "other".to_string())
}

fn capture_feedback(
    store: &FeedbackStore,
    message: &str,
    category: &str,
    tags: Vec<String>,
    session_id: Option<String>,
    no_deliver: bool,
    ui: &mut dyn UserInterface,
) -> Result<i32> {
    let session_store = SessionStore::new(session_store_path());

    // Get session ID (provided, or most recent)
    let session = if let Some(id_str) = session_id {
        SessionId::parse(&id_str)
    } else {
        session_store.get_latest()?.map(|s| s.id)
    };

    let mut entry = FeedbackEntry::new(message);
    if !tags.is_empty() {
        entry = entry.with_tags(tags);
    }
    if let Some(sid) = session.clone() {
        entry = entry.with_session(sid.clone());
    }

    store.append(&entry)?;

    let session_note = session
        .as_ref()
        .map(|s| format!(" (session {})", s))
        .unwrap_or_default();

    ui.success(&format!("Feedback captured{}", session_note));

    // Offer external delivery if interactive and not suppressed
    if !no_deliver && ui.is_interactive() {
        let full_session = session.and_then(|sid| session_store.load(&sid).ok());
        offer_delivery(store, &entry, category, full_session.as_ref(), ui)?;
    }

    Ok(0)
}

fn capture_interactive(
    store: &FeedbackStore,
    tags: Vec<String>,
    no_deliver: bool,
    ui: &mut dyn UserInterface,
) -> Result<i32> {
    if !ui.is_interactive() {
        ui.error("Interactive mode not available. Provide feedback as argument.");
        return Ok(1);
    }

    // Ask for category first
    let category_prompt = Prompt {
        key: "feedback_category".to_string(),
        question: "What kind of feedback?".to_string(),
        prompt_type: PromptType::Select {
            options: vec![
                PromptOption {
                    label: "Bug - something broke".to_string(),
                    value: "bug".to_string(),
                },
                PromptOption {
                    label: "UX - confusing or awkward".to_string(),
                    value: "ux".to_string(),
                },
                PromptOption {
                    label: "Feature - missing or incomplete".to_string(),
                    value: "feature".to_string(),
                },
                PromptOption {
                    label: "Other".to_string(),
                    value: "other".to_string(),
                },
            ],
        },
        default: None,
    };

    let category = match ui.prompt(&category_prompt)? {
        PromptResult::String(s) => s,
        _ => "other".to_string(),
    };

    let prompt = Prompt {
        key: "feedback_message".to_string(),
        question: "Describe your feedback:".to_string(),
        prompt_type: PromptType::Input,
        default: None,
    };

    let result = ui.prompt(&prompt)?;
    let message = match result {
        PromptResult::String(s) => s,
        _ => String::new(),
    };

    if message.is_empty() {
        ui.warning("No feedback provided");
        return Ok(1);
    }

    // Merge category into tags
    let mut all_tags = tags;
    if !category.is_empty() && category != "other" {
        all_tags.insert(0, category.clone());
    }

    let all_tags = if all_tags.is_empty() {
        let tag_prompt = Prompt {
            key: "feedback_tags".to_string(),
            question: "Tags (optional, comma-separated):".to_string(),
            prompt_type: PromptType::Input,
            default: None,
        };

        match ui.prompt(&tag_prompt)? {
            PromptResult::String(s) if !s.is_empty() => {
                s.split(',').map(|s| s.trim().to_string()).collect()
            }
            _ => vec![],
        }
    } else {
        all_tags
    };

    capture_feedback(store, &message, &category, all_tags, None, no_deliver, ui)
}

/// Offer to deliver feedback externally via GitHub issue or email.
fn offer_delivery(
    store: &FeedbackStore,
    entry: &FeedbackEntry,
    category: &str,
    session: Option<&Session>,
    ui: &mut dyn UserInterface,
) -> Result<()> {
    let method = delivery::determine_method(category);

    // Show preview
    let preview = build_preview(entry, category, session, &method);
    ui.message(&preview);

    // Prompt for confirmation
    let confirm_prompt = Prompt {
        key: "feedback_deliver".to_string(),
        question: "Send feedback?".to_string(),
        prompt_type: PromptType::Select {
            options: vec![
                PromptOption {
                    label: "Open in browser".to_string(),
                    value: "yes".to_string(),
                },
                PromptOption {
                    label: "Skip".to_string(),
                    value: "no".to_string(),
                },
            ],
        },
        default: None,
    };

    let response = ui.prompt(&confirm_prompt)?;
    let choice = match response {
        PromptResult::String(s) => s,
        _ => "no".to_string(),
    };

    if choice == "yes" {
        let url = match &method {
            DeliveryMethod::GitHubIssue { labels } => {
                let title = build_title(entry, category);
                let body = delivery::scrub_payload(&delivery::build_github_body(entry, session));
                build_github_url(&title, &body, labels)
            }
            DeliveryMethod::Email { to } => {
                let subject = delivery::build_email_subject(entry, category);
                let body = delivery::scrub_payload(&delivery::build_email_body(entry, session));
                build_mailto_url(to, &subject, &body)
            }
        };

        match delivery::open_url(&url) {
            Ok(()) => {
                ui.success("Opened in browser. Complete submission there.");
                // Mark as delivered
                let _ = store.update(&entry.id, |e| {
                    e.delivered = Some(true);
                });
            }
            Err(e) => {
                ui.warning(&format!("Could not open browser: {}", e));
                ui.message(&format!("You can open this URL manually:\n  {}", url));
            }
        }
    } else {
        ui.message("Feedback saved locally.");
    }

    Ok(())
}

fn list_feedback(
    store: &FeedbackStore,
    status: Option<String>,
    tag: Option<String>,
    all: bool,
    ui: &mut dyn UserInterface,
) -> Result<i32> {
    let entries = if let Some(status_str) = status {
        let status = match status_str.as_str() {
            "open" => FeedbackStatus::Open,
            "resolved" => FeedbackStatus::Resolved,
            "wontfix" => FeedbackStatus::WontFix,
            "inprogress" | "in_progress" => FeedbackStatus::InProgress,
            _ => {
                ui.error(&format!("Unknown status: {}", status_str));
                return Ok(1);
            }
        };
        store.list_by_status(status)?
    } else if let Some(tag) = tag {
        store.list_by_tag(&tag)?
    } else if all {
        store.list_all()?
    } else {
        // Default: show open only
        store.list_by_status(FeedbackStatus::Open)?
    };

    if entries.is_empty() {
        ui.message("No feedback entries found");
        return Ok(0);
    }

    ui.message(&format!("{} feedback entries:\n", entries.len()));
    for entry in entries {
        let status_icon = match entry.status {
            FeedbackStatus::Open => "[ ]",
            FeedbackStatus::InProgress => "[~]",
            FeedbackStatus::Resolved => "[x]",
            FeedbackStatus::WontFix => "[-]",
        };
        let tags = if entry.tags.is_empty() {
            String::new()
        } else {
            format!(" [{}]", entry.tags.join(", "))
        };
        let session = entry
            .session_id
            .map(|s| format!(" ({})", s))
            .unwrap_or_default();

        ui.message(&format!(
            "{} {} {}{}{}",
            status_icon, entry.id, entry.message, tags, session
        ));
    }

    Ok(0)
}

fn resolve_feedback(
    store: &FeedbackStore,
    id: &str,
    note: Option<String>,
    ui: &mut dyn UserInterface,
) -> Result<i32> {
    let resolution = note.unwrap_or_else(|| "Resolved".to_string());

    let found = store.update(id, |entry| {
        entry.resolve(&resolution);
    })?;

    if found {
        ui.success(&format!("Resolved {}", id));
        Ok(0)
    } else {
        ui.error(&format!("Feedback {} not found", id));
        Ok(1)
    }
}

fn show_session_feedback(
    store: &FeedbackStore,
    session_id: Option<String>,
    ui: &mut dyn UserInterface,
) -> Result<i32> {
    let session_store = SessionStore::new(session_store_path());

    let session = if let Some(id_str) = session_id {
        SessionId::parse(&id_str)
    } else {
        session_store.get_latest()?.map(|s| s.id)
    };

    let Some(sid) = session else {
        ui.message("No sessions found");
        return Ok(0);
    };

    let entries = store.list_by_session(&sid)?;
    if entries.is_empty() {
        ui.message(&format!("No feedback for session {}", sid));
        return Ok(0);
    }

    ui.message(&format!("Feedback for session {}:\n", sid));
    for entry in entries {
        ui.message(&format!("  {} {}", entry.id, entry.message));
    }

    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::MockUI;
    use tempfile::TempDir;

    fn create_test_store(temp: &TempDir) -> FeedbackStore {
        FeedbackStore::new(temp.path().join("feedback.jsonl"))
    }

    #[test]
    fn list_empty_feedback() {
        let temp = TempDir::new().unwrap();
        let store = create_test_store(&temp);
        let mut ui = MockUI::new();

        let result = list_feedback(&store, None, None, false, &mut ui).unwrap();
        assert_eq!(result, 0);
    }

    #[test]
    fn list_feedback_with_entries() {
        let temp = TempDir::new().unwrap();
        let store = create_test_store(&temp);
        let mut ui = MockUI::new();

        store.append(&FeedbackEntry::new("test issue")).unwrap();

        let result = list_feedback(&store, None, None, false, &mut ui).unwrap();
        assert_eq!(result, 0);
    }

    #[test]
    fn capture_feedback_basic() {
        let temp = TempDir::new().unwrap();
        let store = create_test_store(&temp);
        let mut ui = MockUI::new();

        let result = capture_feedback(
            &store,
            "test feedback",
            "other",
            vec![],
            None,
            true,
            &mut ui,
        )
        .unwrap();

        assert_eq!(result, 0);

        let entries = store.list_all().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].message, "test feedback");
    }

    #[test]
    fn capture_feedback_with_tags() {
        let temp = TempDir::new().unwrap();
        let store = create_test_store(&temp);
        let mut ui = MockUI::new();

        let result = capture_feedback(
            &store,
            "ux problem",
            "ux",
            vec!["ux".to_string(), "critical".to_string()],
            None,
            true,
            &mut ui,
        )
        .unwrap();

        assert_eq!(result, 0);

        let entries = store.list_all().unwrap();
        assert_eq!(entries[0].tags, vec!["ux", "critical"]);
    }

    #[test]
    fn resolve_feedback_entry() {
        let temp = TempDir::new().unwrap();
        let store = create_test_store(&temp);
        let mut ui = MockUI::new();

        let entry = FeedbackEntry::new("issue to resolve");
        let entry_id = entry.id.clone();
        store.append(&entry).unwrap();

        let result =
            resolve_feedback(&store, &entry_id, Some("Fixed!".to_string()), &mut ui).unwrap();

        assert_eq!(result, 0);

        let entries = store.list_all().unwrap();
        assert_eq!(entries[0].status, FeedbackStatus::Resolved);
    }

    #[test]
    fn resolve_nonexistent_feedback() {
        let temp = TempDir::new().unwrap();
        let store = create_test_store(&temp);
        let mut ui = MockUI::new();

        let result = resolve_feedback(&store, "fb_nonexistent", None, &mut ui).unwrap();

        assert_eq!(result, 1); // Not found
    }

    #[test]
    fn list_by_status() {
        let temp = TempDir::new().unwrap();
        let store = create_test_store(&temp);
        let mut ui = MockUI::new();

        let mut resolved = FeedbackEntry::new("resolved issue");
        resolved.resolve("done");
        store.append(&resolved).unwrap();
        store.append(&FeedbackEntry::new("open issue")).unwrap();

        // List only resolved
        let result =
            list_feedback(&store, Some("resolved".to_string()), None, false, &mut ui).unwrap();

        assert_eq!(result, 0);
    }

    #[test]
    fn list_invalid_status() {
        let temp = TempDir::new().unwrap();
        let store = create_test_store(&temp);
        let mut ui = MockUI::new();

        let result =
            list_feedback(&store, Some("invalid".to_string()), None, false, &mut ui).unwrap();

        assert_eq!(result, 1); // Error
    }

    #[test]
    fn session_feedback_no_sessions() {
        let temp = TempDir::new().unwrap();
        let store = create_test_store(&temp);
        let mut ui = MockUI::new();

        let result = show_session_feedback(&store, None, &mut ui).unwrap();

        // Should return 0, just message about no sessions
        assert_eq!(result, 0);
    }

    #[test]
    fn feedback_args_parsing() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(flatten)]
            feedback: FeedbackArgs,
        }

        let cli = TestCli::parse_from(["test", "--tag", "ux,perf", "some", "feedback"]);
        assert_eq!(cli.feedback.tag, vec!["ux", "perf"]);
        assert_eq!(cli.feedback.message, vec!["some", "feedback"]);
    }

    #[test]
    fn list_all_feedback() {
        let temp = TempDir::new().unwrap();
        let store = create_test_store(&temp);
        let mut ui = MockUI::new();

        let mut resolved = FeedbackEntry::new("resolved");
        resolved.resolve("done");
        store.append(&resolved).unwrap();
        store.append(&FeedbackEntry::new("open")).unwrap();

        // List all (including resolved)
        let result = list_feedback(&store, None, None, true, &mut ui).unwrap();
        assert_eq!(result, 0);
    }

    #[test]
    fn list_feedback_by_tag() {
        let temp = TempDir::new().unwrap();
        let store = create_test_store(&temp);
        let mut ui = MockUI::new();

        store
            .append(&FeedbackEntry::new("bug").with_tags(vec!["bug"]))
            .unwrap();
        store
            .append(&FeedbackEntry::new("ux").with_tags(vec!["ux"]))
            .unwrap();

        // List by tag
        let result = list_feedback(&store, None, Some("bug".to_string()), false, &mut ui).unwrap();
        assert_eq!(result, 0);
    }

    #[test]
    fn list_by_different_statuses() {
        let temp = TempDir::new().unwrap();
        let store = create_test_store(&temp);
        let mut ui = MockUI::new();

        // Test various status strings
        let result = list_feedback(&store, Some("open".to_string()), None, false, &mut ui).unwrap();
        assert_eq!(result, 0);

        let result =
            list_feedback(&store, Some("wontfix".to_string()), None, false, &mut ui).unwrap();
        assert_eq!(result, 0);

        let result =
            list_feedback(&store, Some("inprogress".to_string()), None, false, &mut ui).unwrap();
        assert_eq!(result, 0);

        let result = list_feedback(
            &store,
            Some("in_progress".to_string()),
            None,
            false,
            &mut ui,
        )
        .unwrap();
        assert_eq!(result, 0);
    }

    #[test]
    fn resolve_with_default_note() {
        let temp = TempDir::new().unwrap();
        let store = create_test_store(&temp);
        let mut ui = MockUI::new();

        let entry = FeedbackEntry::new("issue");
        let entry_id = entry.id.clone();
        store.append(&entry).unwrap();

        // Resolve without note
        let result = resolve_feedback(&store, &entry_id, None, &mut ui).unwrap();
        assert_eq!(result, 0);

        let entries = store.list_all().unwrap();
        assert_eq!(entries[0].status, FeedbackStatus::Resolved);
        assert_eq!(entries[0].resolution, Some("Resolved".to_string()));
    }

    #[test]
    fn capture_interactive_non_interactive() {
        let temp = TempDir::new().unwrap();
        let store = create_test_store(&temp);
        let mut ui = MockUI::new();

        // MockUI is non-interactive, so this should fail
        let result = capture_interactive(&store, vec![], false, &mut ui).unwrap();
        assert_eq!(result, 1);
    }

    #[test]
    fn feedback_prompt_uses_neutral_framing() {
        // Verify the prompt text is neutral, not negative
        let prompt = Prompt {
            key: "feedback_message".to_string(),
            question: "Describe your feedback:".to_string(),
            prompt_type: PromptType::Input,
            default: None,
        };
        assert!(!prompt.question.contains("wrong"));
        assert!(prompt.question.contains("feedback"));
    }

    #[test]
    fn feedback_category_options_exist() {
        let category_prompt = Prompt {
            key: "feedback_category".to_string(),
            question: "What kind of feedback?".to_string(),
            prompt_type: PromptType::Select {
                options: vec![
                    PromptOption {
                        label: "Bug - something broke".to_string(),
                        value: "bug".to_string(),
                    },
                    PromptOption {
                        label: "UX - confusing or awkward".to_string(),
                        value: "ux".to_string(),
                    },
                    PromptOption {
                        label: "Feature - missing or incomplete".to_string(),
                        value: "feature".to_string(),
                    },
                    PromptOption {
                        label: "Other".to_string(),
                        value: "other".to_string(),
                    },
                ],
            },
            default: None,
        };

        if let PromptType::Select { options } = &category_prompt.prompt_type {
            assert_eq!(options.len(), 4);
            assert_eq!(options[0].value, "bug");
            assert_eq!(options[1].value, "ux");
            assert_eq!(options[2].value, "feature");
            assert_eq!(options[3].value, "other");
        } else {
            panic!("Expected Select prompt type");
        }
    }

    // --- New delivery-related tests ---

    #[test]
    fn no_deliver_flag_parsing() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(flatten)]
            feedback: FeedbackArgs,
        }

        let cli = TestCli::parse_from(["test", "--no-deliver", "some", "feedback"]);
        assert!(cli.feedback.no_deliver);
    }

    #[test]
    fn capture_feedback_skips_delivery_when_non_interactive() {
        let temp = TempDir::new().unwrap();
        let store = create_test_store(&temp);
        let mut ui = MockUI::new();
        // MockUI defaults to non-interactive

        let result =
            capture_feedback(&store, "test msg", "bug", vec![], None, false, &mut ui).unwrap();
        assert_eq!(result, 0);

        // No delivery prompt should have been shown
        assert!(!ui.prompts_shown().contains(&"feedback_deliver".to_string()));
    }

    #[test]
    fn capture_feedback_skips_delivery_with_no_deliver_flag() {
        let temp = TempDir::new().unwrap();
        let store = create_test_store(&temp);
        let mut ui = MockUI::new();
        ui.set_interactive(true);

        let result =
            capture_feedback(&store, "test msg", "bug", vec![], None, true, &mut ui).unwrap();
        assert_eq!(result, 0);

        // Delivery prompt should NOT be shown when no_deliver=true
        assert!(!ui.prompts_shown().contains(&"feedback_deliver".to_string()));
    }

    #[test]
    fn capture_feedback_offers_delivery_when_interactive() {
        let temp = TempDir::new().unwrap();
        let store = create_test_store(&temp);
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        ui.set_prompt_response("feedback_deliver", "no");

        let result =
            capture_feedback(&store, "test msg", "bug", vec![], None, false, &mut ui).unwrap();
        assert_eq!(result, 0);

        // Delivery prompt SHOULD be shown
        assert!(ui.prompts_shown().contains(&"feedback_deliver".to_string()));
        // User said "no", so saved locally message
        assert!(ui.has_message("Feedback saved locally"));
    }

    #[test]
    fn offer_delivery_shows_preview() {
        let temp = TempDir::new().unwrap();
        let store = create_test_store(&temp);
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        ui.set_prompt_response("feedback_deliver", "no");

        let entry = FeedbackEntry::new("test issue");
        store.append(&entry).unwrap();

        offer_delivery(&store, &entry, "bug", None, &mut ui).unwrap();

        // Preview should be shown before the prompt
        let messages = ui.messages();
        let has_preview = messages.iter().any(|m| m.contains("What we'll send"));
        assert!(has_preview, "Preview should be shown");
    }

    #[test]
    fn category_from_tags_finds_known_category() {
        assert_eq!(
            category_from_tags(&["bug".to_string(), "ui".to_string()]),
            "bug"
        );
        assert_eq!(
            category_from_tags(&["critical".to_string(), "feature".to_string()]),
            "feature"
        );
    }

    #[test]
    fn category_from_tags_defaults_to_other() {
        assert_eq!(
            category_from_tags(&["critical".to_string(), "ui".to_string()]),
            "other"
        );
        assert_eq!(category_from_tags(&[]), "other");
    }

    #[test]
    fn quick_capture_with_tag_uses_category() {
        let temp = TempDir::new().unwrap();
        let store = create_test_store(&temp);
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        ui.set_prompt_response("feedback_deliver", "no");

        // Use "bug" as first tag — should route as bug category
        let result = capture_feedback(
            &store,
            "broken",
            "bug",
            vec!["bug".to_string()],
            None,
            false,
            &mut ui,
        )
        .unwrap();
        assert_eq!(result, 0);

        // Delivery was offered (user declined)
        assert!(ui.prompts_shown().contains(&"feedback_deliver".to_string()));
        // Preview should mention GitHub (bug routes to GitHub)
        let has_github = ui.messages().iter().any(|m| m.contains("GitHub issue"));
        assert!(has_github, "Bug should route to GitHub");
    }

    #[test]
    fn delivery_declined_saves_locally_only() {
        let temp = TempDir::new().unwrap();
        let store = create_test_store(&temp);
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        ui.set_prompt_response("feedback_deliver", "no");

        let entry = FeedbackEntry::new("test");
        store.append(&entry).unwrap();

        offer_delivery(&store, &entry, "bug", None, &mut ui).unwrap();

        assert!(ui.has_message("Feedback saved locally"));
        // Entry should NOT be marked as delivered
        let updated = store.get(&entry.id).unwrap().unwrap();
        assert!(updated.delivered.is_none());
    }
}
