//! Stage 1: Normalize raw command output.
//!
//! Pure text cleanup so subsequent stages don't have to handle formatting
//! variants. No classification, no confidence change.

/// Strip ANSI escape codes, normalize line endings, collapse blank lines,
/// and trim whitespace.
pub fn normalize(input: &str) -> String {
    let stripped = strip_ansi(input);
    let normalized = stripped.replace("\r\n", "\n").replace('\r', "\n");

    let mut result = String::with_capacity(normalized.len());
    let mut prev_blank = false;

    for line in normalized.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !prev_blank {
                result.push('\n');
                prev_blank = true;
            }
        } else {
            result.push_str(trimmed);
            result.push('\n');
            prev_blank = false;
        }
    }

    // Remove trailing newline for consistency
    while result.ends_with('\n') {
        result.pop();
    }

    result
}

/// Strip ANSI escape sequences (CSI sequences, OSC sequences, simple escapes).
fn strip_ansi(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // ESC sequence
            match chars.peek() {
                Some('[') => {
                    // CSI sequence: ESC [ ... final_byte
                    chars.next();
                    while let Some(&next) = chars.peek() {
                        chars.next();
                        if next.is_ascii_alphabetic() || next == '~' || next == '@' {
                            break;
                        }
                    }
                }
                Some(']') => {
                    // OSC sequence: ESC ] ... ST (BEL or ESC \)
                    chars.next();
                    while let Some(&next) = chars.peek() {
                        if next == '\x07' {
                            chars.next();
                            break;
                        }
                        if next == '\x1b' {
                            chars.next();
                            if chars.peek() == Some(&'\\') {
                                chars.next();
                            }
                            break;
                        }
                        chars.next();
                    }
                }
                Some(&c2) if c2.is_ascii_alphabetic() || c2 == '(' || c2 == ')' => {
                    // Simple escape like ESC(B
                    chars.next();
                    if c2 == '(' || c2 == ')' {
                        chars.next(); // consume charset designator
                    }
                }
                _ => {
                    // Lone ESC, skip it
                }
            }
        } else {
            result.push(c);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_ansi_color_codes() {
        let input = "\x1b[31merror:\x1b[0m something failed";
        assert_eq!(normalize(input), "error: something failed");
    }

    #[test]
    fn normalizes_crlf() {
        let input = "line one\r\nline two\r\n";
        assert_eq!(normalize(input), "line one\nline two");
    }

    #[test]
    fn collapses_blank_lines() {
        let input = "line one\n\n\n\nline two";
        assert_eq!(normalize(input), "line one\n\nline two");
    }

    #[test]
    fn trims_whitespace() {
        let input = "  hello  \n  world  ";
        assert_eq!(normalize(input), "hello\nworld");
    }

    #[test]
    fn handles_empty_input() {
        assert_eq!(normalize(""), "");
    }

    #[test]
    fn handles_bold_ansi() {
        let input = "\x1b[1mBOLD\x1b[22m normal";
        assert_eq!(normalize(input), "BOLD normal");
    }

    #[test]
    fn preserves_content_between_ansi() {
        let input = "\x1b[32m✓\x1b[0m Step passed";
        assert_eq!(normalize(input), "✓ Step passed");
    }

    #[test]
    fn complex_ansi_sequences() {
        let input = "\x1b[38;5;196merror\x1b[0m: \x1b[1;4mfatal\x1b[0m problem";
        assert_eq!(normalize(input), "error: fatal problem");
    }
}
