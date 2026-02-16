//! Feedback delivery via GitHub issues or email.
//!
//! Routes feedback to the appropriate external channel:
//! - Bug and feature reports → GitHub issue (browser-based, no token required)
//! - UX and other feedback → Email via mailto link

use crate::secrets::{OutputMasker, SecretMatcher};
use crate::session::Session;

use super::FeedbackEntry;

/// Target repository for GitHub issues.
const GITHUB_REPO: &str = "bivvy-dev/bivvy";

/// Email address for non-GitHub feedback.
const FEEDBACK_EMAIL: &str = "hello@bivvy.dev";

/// Maximum URL length before truncation (browsers have ~8000 char limits).
const MAX_URL_LEN: usize = 8000;

/// How feedback should be delivered externally.
#[derive(Debug, Clone, PartialEq)]
pub enum DeliveryMethod {
    /// Open a pre-filled GitHub issue in the browser.
    GitHubIssue { labels: Vec<String> },
    /// Open a mailto link in the default mail client.
    Email { to: String },
}

/// Determine the delivery method based on feedback category.
pub fn determine_method(category: &str) -> DeliveryMethod {
    match category {
        "bug" => DeliveryMethod::GitHubIssue {
            labels: vec!["bug".to_string(), "user-feedback".to_string()],
        },
        "feature" => DeliveryMethod::GitHubIssue {
            labels: vec!["enhancement".to_string(), "user-feedback".to_string()],
        },
        "ux" => DeliveryMethod::Email {
            to: FEEDBACK_EMAIL.to_string(),
        },
        _ => DeliveryMethod::Email {
            to: FEEDBACK_EMAIL.to_string(),
        },
    }
}

/// Build the issue/email title from a feedback entry.
pub fn build_title(entry: &FeedbackEntry, category: &str) -> String {
    let msg = if entry.message.len() > 60 {
        format!("{}...", &entry.message[..60])
    } else {
        entry.message.clone()
    };
    format!("[{}] {}", category, msg)
}

/// Build the GitHub issue body from a feedback entry and optional session.
pub fn build_github_body(entry: &FeedbackEntry, session: Option<&Session>) -> String {
    let mut sections = Vec::new();

    // Feedback section (always present)
    sections.push(format!("## Feedback\n\n{}", entry.message));

    // Context section
    let version = env!("CARGO_PKG_VERSION");
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    let mut context_lines = vec![format!("- **Bivvy:** {} · {} {}", version, os, arch)];

    if let Some(session) = session {
        let meta = &session.metadata;
        let args_str = if meta.args.is_empty() {
            String::new()
        } else {
            format!(" {}", meta.args.join(" "))
        };
        let exit_str = meta
            .exit_code
            .map(|c| format!(" → exit {}", c))
            .unwrap_or_default();
        context_lines.push(format!(
            "- **Command:** `bivvy {}{}`{}",
            meta.command, args_str, exit_str
        ));

        if let Some(ref hash) = meta.config_hash {
            let step_count = meta.context.step_results.len();
            context_lines.push(format!(
                "- **Config:** {} ({} steps)",
                &hash[..hash.len().min(8)],
                step_count
            ));
        }
    }

    sections.push(format!("## Context\n\n{}", context_lines.join("\n")));

    // Step results table (only if session has steps)
    if let Some(session) = session {
        let steps = &session.metadata.context.step_results;
        if !steps.is_empty() {
            let mut table = String::from("## Step Results\n\n");
            table.push_str("| Step | Status | Duration |\n");
            table.push_str("|------|--------|----------|\n");
            for step in steps {
                let duration = step
                    .duration_ms
                    .map(|ms| format!("{:.1}s", ms as f64 / 1000.0))
                    .unwrap_or_else(|| "-".to_string());
                table.push_str(&format!(
                    "| {} | {} | {} |\n",
                    step.name, step.status, duration
                ));
            }
            sections.push(table);
        }
    }

    // Errors section (only if present)
    if let Some(session) = session {
        let errors = &session.metadata.context.errors;
        if !errors.is_empty() {
            let error_text = errors.join("\n");
            // Truncate long error text
            let truncated = if error_text.len() > 2000 {
                format!("{}...\n(truncated)", &error_text[..2000])
            } else {
                error_text
            };
            sections.push(format!("## Errors\n\n```\n{}\n```", truncated));
        }
    }

    // Footer
    sections.push(
        "---\n*Submitted via `bivvy feedback` · [bivvy docs](https://bivvy.dev)*".to_string(),
    );

    sections.join("\n\n")
}

/// Build the email subject line.
pub fn build_email_subject(entry: &FeedbackEntry, category: &str) -> String {
    let msg = if entry.message.len() > 50 {
        format!("{}...", &entry.message[..50])
    } else {
        entry.message.clone()
    };
    format!("[bivvy {}] {}", category, msg)
}

/// Build the email body as plain text.
pub fn build_email_body(entry: &FeedbackEntry, session: Option<&Session>) -> String {
    let mut lines = Vec::new();

    lines.push(format!("Feedback: {}", entry.message));
    lines.push(String::new());

    let version = env!("CARGO_PKG_VERSION");
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    lines.push(format!("Bivvy: {} · {} {}", version, os, arch));

    if let Some(session) = session {
        let meta = &session.metadata;
        let args_str = if meta.args.is_empty() {
            String::new()
        } else {
            format!(" {}", meta.args.join(" "))
        };
        let exit_str = meta
            .exit_code
            .map(|c| format!(" → exit {}", c))
            .unwrap_or_default();
        lines.push(format!(
            "Command: bivvy {}{}{}",
            meta.command, args_str, exit_str
        ));

        let steps = &meta.context.step_results;
        if !steps.is_empty() {
            lines.push(String::new());
            lines.push("Steps:".to_string());
            for step in steps {
                let duration = step
                    .duration_ms
                    .map(|ms| format!(" ({:.1}s)", ms as f64 / 1000.0))
                    .unwrap_or_default();
                lines.push(format!("  {} - {}{}", step.name, step.status, duration));
            }
        }

        let errors = &meta.context.errors;
        if !errors.is_empty() {
            lines.push(String::new());
            lines.push("Errors:".to_string());
            for err in errors {
                lines.push(format!("  {}", err));
            }
        }
    }

    lines.push(String::new());
    lines.push("Sent via bivvy feedback".to_string());

    lines.join("\n")
}

/// Build a pre-filled GitHub issue URL.
pub fn build_github_url(title: &str, body: &str, labels: &[String]) -> String {
    let encoded_title = urlencoding::encode(title);
    let encoded_labels = labels.join(",");
    let encoded_labels = urlencoding::encode(&encoded_labels);

    // Start with everything except body to measure remaining space
    let prefix = format!(
        "https://github.com/{}/issues/new?title={}&labels={}",
        GITHUB_REPO, encoded_title, encoded_labels
    );

    let body_budget = MAX_URL_LEN.saturating_sub(prefix.len() + "&body=".len());
    let encoded_body = urlencoding::encode(body);

    let final_body = if encoded_body.len() > body_budget {
        // Truncate the raw body, then re-encode
        let mut truncated = body.to_string();
        // Binary search for the right truncation point
        while urlencoding::encode(&truncated).len() > body_budget {
            // Remove roughly 10% or 100 chars, whichever is larger
            let remove = (truncated.len() / 10).max(100).min(truncated.len());
            truncated.truncate(truncated.len() - remove);
        }
        if truncated.len() < body.len() {
            truncated.push_str("\n\n(truncated)");
        }
        urlencoding::encode(&truncated).into_owned()
    } else {
        encoded_body.into_owned()
    };

    format!("{}&body={}", prefix, final_body)
}

/// Build a mailto URL.
pub fn build_mailto_url(to: &str, subject: &str, body: &str) -> String {
    let encoded_subject = urlencoding::encode(subject);
    let encoded_body = urlencoding::encode(body);
    format!(
        "mailto:{}?subject={}&body={}",
        to, encoded_subject, encoded_body
    )
}

/// Scrub sensitive content from text before sending externally.
///
/// Masks:
/// - Environment variable values matching secret patterns
/// - Home directory paths
/// - Usernames in paths
pub fn scrub_payload(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }

    let mut masker = OutputMasker::new();
    let matcher = SecretMatcher::with_builtins();

    // Collect secret env var values
    for (key, value) in std::env::vars() {
        if matcher.is_secret(&key) && !value.is_empty() {
            masker.add_secret(value);
        }
    }

    let mut result = masker.mask(text);

    // Replace home directory
    if let Some(home) = dirs::home_dir() {
        let home_str = home.display().to_string();
        if !home_str.is_empty() {
            result = result.replace(&home_str, "~");
        }
    }

    // Replace username in paths (e.g., /home/username/ or /Users/username/)
    if let Some(home) = dirs::home_dir() {
        if let Some(username) = home.file_name() {
            let username_str = username.to_string_lossy();
            // Only replace in path-like contexts
            let patterns = [
                format!("/home/{}/", username_str),
                format!("/Users/{}/", username_str),
                format!("\\Users\\{}\\", username_str),
                format!("/home/{}", username_str),
                format!("/Users/{}", username_str),
            ];
            for pattern in &patterns {
                let replacement = pattern.replace(&*username_str, "<user>");
                result = result.replace(pattern, &replacement);
            }
        }
    }

    result
}

/// Open a URL in the default browser or mail client.
pub fn open_url(url: &str) -> anyhow::Result<()> {
    open::that(url).map_err(|e| {
        anyhow::anyhow!(
            "Failed to open URL. You can open it manually:\n  {}\n\nError: {}",
            url,
            e
        )
    })
}

/// Build a delivery preview string for showing to the user before they confirm.
pub fn build_preview(
    entry: &FeedbackEntry,
    category: &str,
    session: Option<&Session>,
    method: &DeliveryMethod,
) -> String {
    let title = build_title(entry, category);

    let body = match method {
        DeliveryMethod::GitHubIssue { .. } => build_github_body(entry, session),
        DeliveryMethod::Email { .. } => build_email_body(entry, session),
    };

    let scrubbed_body = scrub_payload(&body);

    let target_description = match method {
        DeliveryMethod::GitHubIssue { .. } => {
            format!("This will open a GitHub issue on {}.", GITHUB_REPO)
        }
        DeliveryMethod::Email { to } => {
            format!("This will open an email to {}.", to)
        }
    };

    format!(
        "  ┌─ What we'll send ─────────────────────────────\n  \
         │\n  \
         │  Title: {}\n  \
         │\n  \
         │  {}\n  \
         │\n  \
         │  NO env vars, secrets, or file paths — just the above.\n  \
         │\n  \
         └────────────────────────────────────────────────\n\n  \
         {}",
        title,
        scrubbed_body.replace('\n', "\n  │  "),
        target_description,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{SessionId, SessionMetadata, StepResultSummary};

    fn test_entry(message: &str) -> FeedbackEntry {
        FeedbackEntry {
            id: "fb_test123456".to_string(),
            timestamp: None,
            message: message.to_string(),
            tags: vec![],
            session_id: None,
            status: crate::feedback::FeedbackStatus::Open,
            resolution: None,
            resolved_at: None,
            delivered: None,
        }
    }

    fn test_session() -> Session {
        let mut meta = SessionMetadata::new("run", vec!["--verbose".to_string()]);
        meta.finalize(1, String::new(), String::new());
        meta.set_config(".bivvy/config.yml", "abc12345def67890");
        meta.add_step_result(StepResultSummary {
            name: "install_deps".to_string(),
            status: "failed".to_string(),
            duration_ms: Some(800),
            error: Some("Bundler error".to_string()),
        });
        meta.add_error("Bundler could not find compatible versions for gem \"nokogiri\"");

        Session {
            id: SessionId::new(),
            metadata: meta,
        }
    }

    // --- Routing logic tests ---

    #[test]
    fn determine_method_bug_routes_to_github() {
        let method = determine_method("bug");
        assert_eq!(
            method,
            DeliveryMethod::GitHubIssue {
                labels: vec!["bug".to_string(), "user-feedback".to_string()]
            }
        );
    }

    #[test]
    fn determine_method_feature_routes_to_github() {
        let method = determine_method("feature");
        assert_eq!(
            method,
            DeliveryMethod::GitHubIssue {
                labels: vec!["enhancement".to_string(), "user-feedback".to_string()]
            }
        );
    }

    #[test]
    fn determine_method_ux_routes_to_email() {
        let method = determine_method("ux");
        assert_eq!(
            method,
            DeliveryMethod::Email {
                to: "hello@bivvy.dev".to_string()
            }
        );
    }

    #[test]
    fn determine_method_other_routes_to_email() {
        let method = determine_method("other");
        assert_eq!(
            method,
            DeliveryMethod::Email {
                to: "hello@bivvy.dev".to_string()
            }
        );
    }

    #[test]
    fn determine_method_unknown_defaults_to_email() {
        let method = determine_method("random-category");
        assert!(matches!(method, DeliveryMethod::Email { .. }));
    }

    // --- Payload building tests ---

    #[test]
    fn build_title_includes_category_and_message() {
        let entry = test_entry("bundle install fails on M3 Macs");
        let title = build_title(&entry, "bug");
        assert_eq!(title, "[bug] bundle install fails on M3 Macs");
    }

    #[test]
    fn build_title_truncates_long_messages() {
        let long_msg = "a".repeat(100);
        let entry = test_entry(&long_msg);
        let title = build_title(&entry, "bug");
        assert!(title.ends_with("..."));
        // [bug] + space + 60 chars + ...
        assert!(title.len() <= 70);
    }

    #[test]
    fn build_github_body_includes_all_sections() {
        let entry = test_entry("bundle install fails");
        let session = test_session();
        let body = build_github_body(&entry, Some(&session));

        assert!(body.contains("## Feedback"));
        assert!(body.contains("bundle install fails"));
        assert!(body.contains("## Context"));
        assert!(body.contains(&format!("**Bivvy:** {}", env!("CARGO_PKG_VERSION"))));
        assert!(body.contains("bivvy feedback"));
    }

    #[test]
    fn build_github_body_includes_step_results_table() {
        let entry = test_entry("test");
        let session = test_session();
        let body = build_github_body(&entry, Some(&session));

        assert!(body.contains("## Step Results"));
        assert!(body.contains("| install_deps | failed | 0.8s |"));
    }

    #[test]
    fn build_github_body_includes_scrubbed_errors() {
        let entry = test_entry("test");
        let session = test_session();
        let body = build_github_body(&entry, Some(&session));

        assert!(body.contains("## Errors"));
        assert!(body.contains("nokogiri"));
    }

    #[test]
    fn build_github_body_omits_empty_sections() {
        let entry = test_entry("just a note");
        // No session → no step results, no errors
        let body = build_github_body(&entry, None);

        assert!(body.contains("## Feedback"));
        assert!(body.contains("## Context"));
        assert!(!body.contains("## Step Results"));
        assert!(!body.contains("## Errors"));
    }

    #[test]
    fn build_github_body_omits_errors_when_none() {
        let mut meta = SessionMetadata::new("run", vec![]);
        meta.finalize(0, String::new(), String::new());
        // No errors added
        let session = Session {
            id: SessionId::new(),
            metadata: meta,
        };

        let entry = test_entry("test");
        let body = build_github_body(&entry, Some(&session));

        assert!(!body.contains("## Errors"));
    }

    #[test]
    fn build_email_subject_includes_category() {
        let entry = test_entry("confusing error message");
        let subject = build_email_subject(&entry, "bug");
        assert_eq!(subject, "[bivvy bug] confusing error message");
    }

    #[test]
    fn build_email_body_plain_text() {
        let entry = test_entry("confusing flow");
        let session = test_session();
        let body = build_email_body(&entry, Some(&session));

        // No markdown tables
        assert!(!body.contains("| Step |"));
        // Has plain text step listing
        assert!(body.contains("install_deps - failed"));
        assert!(body.contains("Sent via bivvy feedback"));
    }

    // --- URL construction tests ---

    #[test]
    fn github_url_percent_encodes_title() {
        let url = build_github_url("test title with spaces", "body", &[]);
        assert!(url.contains("title=test%20title%20with%20spaces"));
    }

    #[test]
    fn github_url_percent_encodes_body() {
        let url = build_github_url("title", "## Feedback\n\ntest", &[]);
        assert!(url.contains("body="));
        assert!(!url.contains("## Feedback\n"));
    }

    #[test]
    fn github_url_includes_labels() {
        let url = build_github_url(
            "title",
            "body",
            &["bug".to_string(), "user-feedback".to_string()],
        );
        assert!(url.contains("labels=bug%2Cuser-feedback"));
    }

    #[test]
    fn github_url_truncates_long_body() {
        let long_body = "x".repeat(20000);
        let url = build_github_url("title", &long_body, &[]);
        assert!(url.len() <= MAX_URL_LEN + 500); // Some tolerance for encoding
    }

    #[test]
    fn github_url_truncation_preserves_message_and_context() {
        // Build a body where the message is at the top
        let message = "This is the important message";
        let padding = "error line\n".repeat(2000);
        let body = format!("## Feedback\n\n{}\n\n{}", message, padding);

        let url = build_github_url("title", &body, &[]);
        let decoded =
            urlencoding::decode(url.split("body=").nth(1).unwrap_or_default()).unwrap_or_default();

        // Message should survive truncation (it's at the start)
        assert!(decoded.contains(message));
    }

    #[test]
    fn mailto_url_encodes_subject_and_body() {
        let url = build_mailto_url("test@example.com", "My Subject", "Hello World");
        assert!(url.starts_with("mailto:test@example.com?"));
        assert!(url.contains("subject=My%20Subject"));
        assert!(url.contains("body=Hello%20World"));
    }

    #[test]
    fn mailto_url_handles_empty_body() {
        let url = build_mailto_url("test@example.com", "subject", "");
        assert!(url.contains("body="));
    }

    // --- Scrubbing tests ---

    #[test]
    fn scrub_replaces_home_dir() {
        if let Some(home) = dirs::home_dir() {
            let path = format!("{}/projects/myapp", home.display());
            let scrubbed = scrub_payload(&path);
            assert!(scrubbed.contains("~/projects/myapp"));
            assert!(!scrubbed.contains(&home.display().to_string()));
        }
    }

    #[test]
    fn scrub_preserves_non_sensitive_content() {
        let text = "install_deps failed with exit code 1";
        let scrubbed = scrub_payload(text);
        assert_eq!(scrubbed, text);
    }

    #[test]
    fn scrub_handles_empty_input() {
        assert_eq!(scrub_payload(""), "");
    }

    // --- Edge case tests ---

    #[test]
    fn github_url_with_no_session_still_works() {
        let entry = test_entry("standalone feedback");
        let body = build_github_body(&entry, None);
        let url = build_github_url(&build_title(&entry, "bug"), &body, &["bug".to_string()]);

        assert!(url.starts_with("https://github.com/bivvy-dev/bivvy/issues/new"));
        assert!(url.contains("standalone%20feedback"));
    }

    #[test]
    fn github_url_with_unicode_message() {
        let url = build_github_url("[bug] emoji 🐛 test", "body with 日本語", &[]);
        assert!(url.contains("issues/new"));
        // Should not panic or error
    }

    #[test]
    fn email_with_multiline_message() {
        let entry = test_entry("line one\nline two\nline three");
        let body = build_email_body(&entry, None);
        assert!(body.contains("line one\nline two\nline three"));
    }

    #[test]
    fn build_preview_contains_title_and_body() {
        let entry = test_entry("test message");
        let method = determine_method("bug");
        let preview = build_preview(&entry, "bug", None, &method);

        assert!(preview.contains("[bug] test message"));
        assert!(preview.contains("GitHub issue"));
        assert!(preview.contains("bivvy-dev/bivvy"));
    }

    #[test]
    fn build_preview_email_shows_email_target() {
        let entry = test_entry("ux issue");
        let method = determine_method("ux");
        let preview = build_preview(&entry, "ux", None, &method);

        assert!(preview.contains("email to hello@bivvy.dev"));
    }
}
