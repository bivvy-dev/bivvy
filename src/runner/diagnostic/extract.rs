//! Stage 5: Extract resolution candidates from tool output.
//!
//! The "dictionary" stage — but the dictionary is a vocabulary of action
//! phrases, not a mapping of error messages to fixes. Extracted resolutions
//! from the tool's own output are higher quality than anything we can guess.

use regex::Regex;
use std::sync::LazyLock;

use super::classify::ErrorCategory;
use super::segment::{LineTag, TaggedLine};
use super::{CategoryMatch, ResolutionCandidate, ResolutionSource, StepContext};

// === Command extraction patterns ===

static RE_BACKTICK_CMD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`([^`]{2,})`").unwrap());

static RE_SHELL_CMD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^\s*\$?\s*((?:sudo\s+)?(?:brew|apt|dnf|yum|pacman|pip|npm|yarn|bundle|gem|cargo|go|mix|docker|systemctl|chmod|chown|mkdir|export|source)\s+\S+.*)").unwrap()
});

// === Boilerplate detection ===

static RE_BOILERPLATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(check your configuration|see full traceback|for more information|visit https?://|see the documentation|run .* for details)").unwrap()
});

/// Extract resolution candidates from tagged output lines.
pub fn extract_resolutions(
    lines: &[TaggedLine],
    categories: &[CategoryMatch],
    step_ctx: &StepContext<'_>,
) -> Vec<ResolutionCandidate> {
    let mut resolutions = Vec::new();

    let resolution_lines: Vec<&TaggedLine> = lines
        .iter()
        .filter(|l| l.tags.contains(&LineTag::ResolutionCandidate))
        .collect();

    for line in &resolution_lines {
        let is_boilerplate = RE_BOILERPLATE.is_match(&line.text)
            || restates_failed_command(&line.text, step_ctx.command);
        let boilerplate_penalty: f32 = if is_boilerplate { -0.15 } else { 0.0 };

        // Try backtick-quoted command extraction (highest quality)
        if let Some(caps) = RE_BACKTICK_CMD.captures(&line.text) {
            let cmd = caps[1].to_string();
            let alignment_boost = alignment_boost(&cmd, categories);
            let confidence = (0.5 + alignment_boost + boilerplate_penalty).clamp(0.0, 1.0);

            resolutions.push(ResolutionCandidate {
                label: cmd.clone(),
                command: Some(cmd),
                explanation: line.text.clone(),
                confidence,
                source: ResolutionSource::Extracted,
                platform: None,
            });
            continue;
        }

        // Try shell-formatted command extraction (medium quality)
        if let Some(caps) = RE_SHELL_CMD.captures(&line.text) {
            let cmd = caps[1].trim().to_string();
            let alignment_boost = alignment_boost(&cmd, categories);
            let confidence = (0.4 + alignment_boost + boilerplate_penalty).clamp(0.0, 1.0);

            resolutions.push(ResolutionCandidate {
                label: cmd.clone(),
                command: Some(cmd),
                explanation: line.text.clone(),
                confidence,
                source: ResolutionSource::Extracted,
                platform: None,
            });
            continue;
        }

        // Natural language resolution (lower quality)
        if !is_boilerplate {
            let confidence = (0.25 + alignment_boost_text(&line.text, categories)).clamp(0.0, 1.0);
            resolutions.push(ResolutionCandidate {
                label: truncate_label(&line.text, 60),
                command: None,
                explanation: line.text.clone(),
                confidence,
                source: ResolutionSource::Extracted,
                platform: None,
            });
        }
    }

    resolutions
}

/// Adjust confidence when the resolution command aligns with or contradicts
/// the diagnosis. Returns positive boost (+0.2) for alignment, negative
/// penalty (-0.1) for contradiction, or 0.0 for neutral.
fn alignment_boost(cmd: &str, categories: &[CategoryMatch]) -> f32 {
    if categories.is_empty() {
        return 0.0;
    }

    let mut best = 0.0_f32;
    let primary = categories.first().map(|c| c.category);

    for cat in categories {
        let aligned = match cat.category {
            ErrorCategory::VersionMismatch => {
                cmd.contains("update") || cmd.contains("upgrade") || cmd.contains("install")
            }
            ErrorCategory::NotFound => {
                cmd.contains("install") || cmd.contains("add") || cmd.contains("create")
            }
            ErrorCategory::ConnectionRefused => cmd.contains("start") || cmd.contains("restart"),
            ErrorCategory::SyncIssue => {
                cmd.contains("lock") || cmd.contains("update") || cmd.contains("sync")
            }
            ErrorCategory::PermissionDenied => {
                cmd.contains("chmod") || cmd.contains("chown") || cmd.contains("sudo")
            }
            ErrorCategory::PortConflict => {
                cmd.contains("kill") || cmd.contains("stop") || cmd.contains("down")
            }
            ErrorCategory::SystemConstraint => cmd.contains("venv") || cmd.contains("virtualenv"),
            _ => false,
        };
        if aligned {
            best = best.max(0.2);
        }
    }

    // If no alignment found, check for contradiction with the primary category.
    // A resolution that suggests an action for a clearly different category gets
    // demoted. We keep it as a low-ranked option since it may be partially relevant.
    if best == 0.0 {
        if let Some(primary_cat) = primary {
            let contradicts = match primary_cat {
                ErrorCategory::NotFound => cmd.contains("restart") || cmd.contains("stop"),
                ErrorCategory::ConnectionRefused => cmd.contains("chmod") || cmd.contains("chown"),
                ErrorCategory::PermissionDenied => cmd.contains("start") || cmd.contains("restart"),
                _ => false,
            };
            if contradicts {
                best = -0.1;
            }
        }
    }

    best
}

/// Similar to alignment_boost but for natural language text.
///
/// Also detects contradiction: e.g., "make sure pg_dump is installed" is a
/// NotFound framing that contradicts a VersionMismatch diagnosis.
fn alignment_boost_text(text: &str, categories: &[CategoryMatch]) -> f32 {
    let lower = text.to_lowercase();
    let mut boost = 0.0_f32;
    let primary = categories.first().map(|c| c.category);

    for cat in categories {
        let aligned = match cat.category {
            ErrorCategory::VersionMismatch => lower.contains("update") || lower.contains("upgrade"),
            ErrorCategory::NotFound => lower.contains("install") || lower.contains("create"),
            ErrorCategory::ConnectionRefused => {
                lower.contains("start") || lower.contains("running")
            }
            ErrorCategory::SystemConstraint => {
                lower.contains("virtual environment") || lower.contains("venv")
            }
            _ => false,
        };
        if aligned {
            boost = boost.max(0.15);
        }
    }

    // Contradiction: text uses NotFound framing ("is installed", "make sure...installed")
    // but primary diagnosis is something else (e.g., VersionMismatch).
    if boost == 0.0 {
        if let Some(primary_cat) = primary {
            let is_existence_check = lower.contains("is installed") || lower.contains("make sure");
            if is_existence_check && primary_cat != ErrorCategory::NotFound {
                boost = -0.1;
            }
        }
    }

    boost
}

/// Detect when a resolution merely restates the failed command.
///
/// E.g., "Make sure that `gem install pg` succeeds" when the step command
/// is "gem install". These get a boilerplate penalty since they don't add
/// information beyond what the user already tried.
fn restates_failed_command(resolution_text: &str, step_command: &str) -> bool {
    // Extract the core command (first 2-3 words, no args) for comparison
    let step_words: Vec<&str> = step_command.split_whitespace().take(3).collect();
    if step_words.is_empty() {
        return false;
    }

    // Check if any backtick-quoted command in the resolution contains the step command root
    if let Some(caps) = RE_BACKTICK_CMD.captures(resolution_text) {
        let quoted = &caps[1];
        let quoted_words: Vec<&str> = quoted.split_whitespace().collect();
        // If the quoted command starts with the same tool + action as the step command
        if quoted_words.len() >= 2
            && step_words.len() >= 2
            && quoted_words[0] == step_words[0]
            && quoted_words[1] == step_words[1]
        {
            return true;
        }
    }

    false
}

fn truncate_label(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else {
        // Find a char boundary at or before max_len - 3 to avoid panicking
        // on multi-byte UTF-8 characters.
        let end = max_len - 3;
        let boundary = text
            .char_indices()
            .take_while(|(i, _)| *i <= end)
            .last()
            .map(|(i, _)| i)
            .unwrap_or(0);
        format!("{}...", &text[..boundary])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::diagnostic::segment::segment;

    fn default_ctx() -> StepContext<'static> {
        StepContext {
            name: "test",
            command: "echo test",
            requires: &[],
            template: None,
        }
    }

    fn extract_from(text: &str, categories: &[CategoryMatch]) -> Vec<ResolutionCandidate> {
        let lines = segment(text);
        extract_resolutions(&lines, categories, &default_ctx())
    }

    #[test]
    fn extracts_backtick_command() {
        let text = "Try running `bundle update nokogiri` to fix this";
        let cats = vec![CategoryMatch {
            category: ErrorCategory::NotFound,
            confidence: 0.3,
        }];
        let res = extract_from(text, &cats);
        assert!(!res.is_empty());
        assert_eq!(res[0].command.as_deref(), Some("bundle update nokogiri"));
        assert_eq!(res[0].source, ResolutionSource::Extracted);
    }

    #[test]
    fn boilerplate_natural_language_is_dropped() {
        // Pure boilerplate without a backtick or shell command is dropped entirely
        // (not just penalized) because natural language extraction skips boilerplate.
        let text = "Please check your configuration and try again";
        let res = extract_from(text, &[]);
        assert!(
            res.is_empty(),
            "Expected boilerplate natural language to be dropped, got {} resolution(s)",
            res.len()
        );
    }

    #[test]
    fn boilerplate_with_command_gets_penalty() {
        // Boilerplate that contains a backtick command IS extracted but with a penalty.
        let text = "Please check your configuration and run `bundle install`";
        let cats = vec![CategoryMatch {
            category: ErrorCategory::NotFound,
            confidence: 0.3,
        }];
        let res = extract_from(text, &cats);
        assert!(!res.is_empty(), "Expected resolution to be extracted");
        // Base 0.5 (backtick) + 0.2 (alignment: install for NotFound) - 0.15 (boilerplate) = 0.55
        assert!(
            res[0].confidence < 0.6,
            "Expected penalized confidence < 0.6, got {}",
            res[0].confidence
        );
    }

    #[test]
    fn alignment_boosts_confidence() {
        let text = "Try `pip install flask`";
        let cats = vec![CategoryMatch {
            category: ErrorCategory::NotFound,
            confidence: 0.3,
        }];
        let res = extract_from(text, &cats);
        assert!(!res.is_empty());
        // Should have alignment boost
        assert!(res[0].confidence >= 0.5);
    }

    #[test]
    fn natural_language_resolution() {
        let text = "Hint: use a virtual environment to isolate packages";
        let cats = vec![CategoryMatch {
            category: ErrorCategory::SystemConstraint,
            confidence: 0.3,
        }];
        let res = extract_from(text, &cats);
        assert!(!res.is_empty());
        assert!(res[0].command.is_none());
    }

    #[test]
    fn empty_input_no_resolutions() {
        let res = extract_from("", &[]);
        assert!(res.is_empty());
    }

    #[test]
    fn pep668_extracts_resolutions() {
        let text = "note: If you wish to install a Python package, use a virtual environment.\nTry `python -m venv .venv` to create one.";
        let cats = vec![CategoryMatch {
            category: ErrorCategory::SystemConstraint,
            confidence: 0.5,
        }];
        let res = extract_from(text, &cats);
        assert!(!res.is_empty());
    }

    #[test]
    fn restatement_of_failed_command_gets_penalty() {
        let text = "Make sure that `gem install pg` succeeds before bundling";
        let cats = vec![CategoryMatch {
            category: ErrorCategory::NotFound,
            confidence: 0.3,
        }];
        let ctx = StepContext {
            name: "deps",
            command: "gem install pg",
            requires: &[],
            template: None,
        };
        let lines = segment(text);
        let res = extract_resolutions(&lines, &cats, &ctx);
        assert!(!res.is_empty());
        // Should have boilerplate penalty applied (0.5 + 0.2 alignment - 0.15 penalty = 0.55)
        assert!(res[0].confidence < 0.6);
    }

    #[test]
    fn existence_check_framing_demoted_for_version_mismatch() {
        // "make sure pg_dump is installed" is a NotFound framing. When primary
        // diagnosis is VersionMismatch, this resolution is lower-ranked.
        // The backtick extracts "pg_dump" as a command, but "pg_dump" alone
        // doesn't contain update/upgrade/install → no alignment boost → neutral.
        // The resolution is present but low-confidence since the extracted
        // "command" is just a tool name, not an actionable fix.
        let text =
            "Please check the output above for any errors and make sure that `pg_dump` is installed in your PATH and has proper permissions.";
        let cats = vec![CategoryMatch {
            category: ErrorCategory::VersionMismatch,
            confidence: 0.5,
        }];
        let res = extract_from(text, &cats);
        assert!(!res.is_empty());
        // "pg_dump" extracted via backtick, base 0.5, no alignment boost, no penalty
        // = 0.5, which is below the 0.6 "Fix" threshold → shows as "Suggestion"
        assert!(res[0].confidence < 0.6);
    }

    #[test]
    fn install_command_aligns_with_version_mismatch() {
        // "brew install postgresql@16" is a valid fix for version mismatch
        let text = "Try `brew install postgresql@16`";
        let cats = vec![CategoryMatch {
            category: ErrorCategory::VersionMismatch,
            confidence: 0.5,
        }];
        let res = extract_from(text, &cats);
        assert!(!res.is_empty());
        // install aligns with VersionMismatch → +0.2 boost
        // Base 0.5 + 0.2 = 0.7
        assert!(res[0].confidence >= 0.6);
    }
}
