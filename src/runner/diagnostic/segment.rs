//! Stage 2: Segment normalized output into tagged lines.
//!
//! Tags each line as `ErrorSignal`, `ResolutionCandidate`, or `Noise` based
//! on structural patterns that tools impose on their output. A line can carry
//! multiple tags (e.g., both error signal and resolution candidate).

use regex::Regex;
use std::sync::LazyLock;

/// Tag assigned to a line by the segmenter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LineTag {
    /// Line describes what went wrong.
    ErrorSignal,
    /// Line suggests what to do.
    ResolutionCandidate,
    /// Neither — stack trace, boilerplate, blank, etc.
    Noise,
}

/// A line with its assigned tags.
#[derive(Debug, Clone)]
pub struct TaggedLine {
    /// The line content.
    pub text: String,
    /// Tags assigned to this line (may have multiple).
    pub tags: Vec<LineTag>,
}

// === Error signal patterns ===

static RE_ERROR_KEYWORDS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(error|fatal|danger|failed)\b").unwrap());

static RE_ERROR_CODES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(ERESOLVE|E0\d{3}|ENOSPC|EACCES|EADDRINUSE|PEP 668)\b").unwrap()
});

static RE_PYTHON_EXCEPTION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[A-Z][a-z]+(?:[A-Z][a-z]+)+Error:").unwrap());

static RE_RUBY_EXCEPTION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[A-Z]\w+(?:::[A-Z]\w+)+Error").unwrap());

static RE_EXIT_CODE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(exit code|exit status|returned non-zero|exited with)").unwrap()
});

static RE_ERROR_COLON: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^[a-z_\-./]+:\s*(error|fatal):").unwrap());

// === Resolution candidate patterns ===

static RE_RESOLUTION_PREFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(Try |Please |You may |You might |Hint:|Tip:|Fix:|Note:|note:)").unwrap()
});

static RE_BACKTICK_COMMAND: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`[^`]{2,}`").unwrap());

static RE_ARROW_COMMAND: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(→|=>|-->)\s+\S").unwrap());

// === Noise patterns (explicit exclusions from resolution candidate) ===

static RE_NOISE_FRAMEWORK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)(^Tasks:\s|^bin/\S+ aborted|See full trace|^\(See full trace|^at\s+\S+\s+\(|^from /|^\s+File ".+",\s+line\s+\d+)"#,
    )
    .unwrap()
});

static RE_ACTION_VERBS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(install|run|start|stop|restart|enable|update|upgrade|downgrade|create|add|remove|delete|set|unset|export|use|try|activate|configure|build|rebuild|clean|reset|fix|repair|migrate|check|verify|ensure|make sure|set up|sign in|log in|opt in|switch to|add to|point to)\b").unwrap()
});

static RE_NOUN_SIGNALS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(path|permission|version|module|package|dependency|variable|environment|config|configuration|credentials|token|key|certificate)\b").unwrap()
});

/// Segment normalized text into tagged lines.
pub fn segment(text: &str) -> Vec<TaggedLine> {
    text.lines()
        .map(|line| {
            let mut tags = Vec::new();

            if is_error_signal(line) {
                tags.push(LineTag::ErrorSignal);
            }
            if is_resolution_candidate(line) {
                tags.push(LineTag::ResolutionCandidate);
            }
            if tags.is_empty() {
                tags.push(LineTag::Noise);
            }

            TaggedLine {
                text: line.to_string(),
                tags,
            }
        })
        .collect()
}

fn is_error_signal(line: &str) -> bool {
    RE_ERROR_KEYWORDS.is_match(line)
        || RE_ERROR_CODES.is_match(line)
        || RE_PYTHON_EXCEPTION.is_match(line)
        || RE_RUBY_EXCEPTION.is_match(line)
        || RE_EXIT_CODE.is_match(line)
        || RE_ERROR_COLON.is_match(line)
}

fn is_resolution_candidate(line: &str) -> bool {
    // Exclude known noise patterns (framework wrappers, stack traces)
    if RE_NOISE_FRAMEWORK.is_match(line) {
        return false;
    }
    RE_RESOLUTION_PREFIX.is_match(line)
        || (RE_BACKTICK_COMMAND.is_match(line)
            && (RE_ACTION_VERBS.is_match(line) || RE_NOUN_SIGNALS.is_match(line)))
        || RE_ARROW_COMMAND.is_match(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has_tag(line: &TaggedLine, tag: LineTag) -> bool {
        line.tags.contains(&tag)
    }

    #[test]
    fn error_keyword_tagged() {
        let lines = segment("error: something went wrong");
        assert!(has_tag(&lines[0], LineTag::ErrorSignal));
    }

    #[test]
    fn fatal_keyword_tagged() {
        let lines = segment("FATAL:  database \"myapp\" does not exist");
        assert!(has_tag(&lines[0], LineTag::ErrorSignal));
    }

    #[test]
    fn python_exception_tagged() {
        let lines = segment("ModuleNotFoundError: No module named 'flask'");
        assert!(has_tag(&lines[0], LineTag::ErrorSignal));
    }

    #[test]
    fn ruby_exception_tagged() {
        let lines = segment("ActiveRecord::NoDatabaseError");
        assert!(has_tag(&lines[0], LineTag::ErrorSignal));
    }

    #[test]
    fn error_code_tagged() {
        let lines = segment("npm ERR! code ERESOLVE");
        assert!(has_tag(&lines[0], LineTag::ErrorSignal));
    }

    #[test]
    fn try_prefix_is_resolution() {
        let lines = segment("Try running `bundle update` to fix this");
        assert!(has_tag(&lines[0], LineTag::ResolutionCandidate));
    }

    #[test]
    fn please_prefix_is_resolution() {
        let lines = segment("Please check the output above for any errors");
        assert!(has_tag(&lines[0], LineTag::ResolutionCandidate));
    }

    #[test]
    fn stack_trace_is_noise() {
        let lines = segment("  File \"app.py\", line 1, in <module>");
        assert!(has_tag(&lines[0], LineTag::Noise));
    }

    #[test]
    fn rails_boilerplate_is_noise() {
        let lines = segment("bin/rails aborted!");
        assert!(has_tag(&lines[0], LineTag::Noise));
    }

    #[test]
    fn separator_is_noise() {
        let lines = segment("---");
        assert!(has_tag(&lines[0], LineTag::Noise));
    }

    #[test]
    fn pg_dump_error_line() {
        let lines = segment("pg_dump: error: aborting because of server version mismatch");
        assert!(has_tag(&lines[0], LineTag::ErrorSignal));
    }

    #[test]
    fn eaddrinuse_tagged() {
        let lines = segment("Error: listen EADDRINUSE: address already in use :::3000");
        assert!(has_tag(&lines[0], LineTag::ErrorSignal));
    }

    #[test]
    fn backtick_with_action_is_resolution() {
        let lines = segment("Make sure that `gem install pg` succeeds before bundling");
        assert!(has_tag(&lines[0], LineTag::ResolutionCandidate));
    }

    #[test]
    fn hint_prefix_is_resolution() {
        let lines = segment("Hint: use a virtual environment");
        assert!(has_tag(&lines[0], LineTag::ResolutionCandidate));
    }

    #[test]
    fn note_prefix_is_resolution() {
        let lines =
            segment("note: If you wish to install a Python package, use a virtual environment.");
        assert!(has_tag(&lines[0], LineTag::ResolutionCandidate));
    }

    #[test]
    fn plain_text_is_noise() {
        let lines = segment("Loading application...");
        assert!(has_tag(&lines[0], LineTag::Noise));
    }

    #[test]
    fn multiline_segmentation() {
        let text = "ModuleNotFoundError: No module named 'flask'\n  File \"app.py\", line 1, in <module>\nTry `pip install flask`";
        let lines = segment(text);
        assert!(has_tag(&lines[0], LineTag::ErrorSignal));
        assert!(has_tag(&lines[1], LineTag::Noise));
        assert!(has_tag(&lines[2], LineTag::ResolutionCandidate));
    }

    #[test]
    fn rake_task_line_is_noise() {
        let lines = segment("Tasks: TOP => db:prepare");
        assert!(has_tag(&lines[0], LineTag::Noise));
    }

    #[test]
    fn see_full_trace_is_noise() {
        let lines = segment("(See full trace by running task with --trace)");
        assert!(has_tag(&lines[0], LineTag::Noise));
    }

    #[test]
    fn js_stack_frame_is_noise() {
        let lines = segment("at Object.<anonymous> (/app/index.js:1:1)");
        assert!(has_tag(&lines[0], LineTag::Noise));
    }

    #[test]
    fn ruby_stack_frame_is_noise() {
        let lines = segment("from /usr/lib/ruby/2.7.0/net/http.rb:933:in `connect'");
        assert!(has_tag(&lines[0], LineTag::Noise));
    }
}
