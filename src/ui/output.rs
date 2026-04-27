//! Output mode and writer.

use std::io::Write;
use std::str::FromStr;

/// Output verbosity mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputMode {
    /// Show all output including command output.
    Verbose,
    /// Show progress and status only.
    #[default]
    Normal,
    /// Show minimal output (spinners + final status).
    Quiet,
    /// Show nothing except errors.
    Silent,
}

impl FromStr for OutputMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "verbose" => Ok(Self::Verbose),
            "normal" => Ok(Self::Normal),
            "quiet" => Ok(Self::Quiet),
            "silent" => Ok(Self::Silent),
            _ => Err(format!("unknown output mode: {}", s)),
        }
    }
}

impl From<crate::config::schema::OutputMode> for OutputMode {
    fn from(config_mode: crate::config::schema::OutputMode) -> Self {
        match config_mode {
            crate::config::schema::OutputMode::Verbose => Self::Verbose,
            crate::config::schema::OutputMode::Quiet => Self::Quiet,
            crate::config::schema::OutputMode::Silent => Self::Silent,
        }
    }
}

impl OutputMode {
    /// Check if this mode shows command output.
    pub fn shows_command_output(&self) -> bool {
        matches!(self, Self::Verbose)
    }

    /// Check if this mode shows progress spinners.
    pub fn shows_spinners(&self) -> bool {
        matches!(self, Self::Verbose | Self::Normal | Self::Quiet)
    }

    /// Check if this mode shows status messages.
    pub fn shows_status(&self) -> bool {
        !matches!(self, Self::Silent)
    }
}

/// Output writer that respects output mode.
#[derive(Debug)]
pub struct Output {
    mode: OutputMode,
}

impl Output {
    /// Create a new output writer.
    pub fn new(mode: OutputMode) -> Self {
        Self { mode }
    }

    /// Get the output mode.
    pub fn mode(&self) -> OutputMode {
        self.mode
    }

    /// Write a line if the mode allows status messages.
    pub fn println(&self, msg: &str) {
        if self.mode.shows_status() {
            println!("{}", msg);
        }
    }

    /// Write command output if verbose mode.
    pub fn command_output(&self, output: &str) {
        if self.mode.shows_command_output() {
            print!("{}", output);
            let _ = std::io::stdout().flush();
        }
    }
}

// ---------------------------------------------------------------------------
// Named OutputSink implementations
// ---------------------------------------------------------------------------

use crate::shell::command::{OutputLine, OutputSink};

/// Streams command output lines to stdout with a fixed indent.
///
/// Used in **non-interactive verbose** mode where no spinner is available.
/// Each non-empty line is printed as `"{indent}{text}"`.
pub struct VerboseStreamSink {
    indent: String,
}

impl VerboseStreamSink {
    /// Create a sink that indents each line with `n` spaces.
    pub fn new(indent_spaces: usize) -> Self {
        Self {
            indent: " ".repeat(indent_spaces),
        }
    }
}

impl OutputSink for VerboseStreamSink {
    fn write_line(&self, line: OutputLine) {
        let text = match &line {
            OutputLine::Stdout(s) | OutputLine::Stderr(s) => s.trim_end(),
        };
        if !text.is_empty() {
            let _ = writeln!(std::io::stdout(), "{}{text}", self.indent);
        }
    }
}

/// Streams fix-command output to stderr with a `"    fix: "` prefix.
///
/// Used by the recovery module when running a suggested fix command.
pub struct FixOutputSink;

impl OutputSink for FixOutputSink {
    fn write_line(&self, line: OutputLine) {
        let text = match &line {
            OutputLine::Stdout(s) | OutputLine::Stderr(s) => s,
        };
        let _ = writeln!(std::io::stderr(), "    fix: {}", text);
    }
}

// ---------------------------------------------------------------------------
// Presenter label helpers
// ---------------------------------------------------------------------------

/// Format a label for a step whose `satisfied_when` conditions all passed.
pub fn satisfaction_label(description: &str) -> String {
    format!("Satisfied ({})", description)
}

/// Format a label for a step whose check passed (work already done).
pub fn check_passed_label(description: &str) -> String {
    format!("Check passed ({})", description)
}

/// Format a prompt asking the user whether to re-run a check-passed step.
pub fn rerun_prompt_label(description: &str) -> String {
    format!("Check passed ({}). Run anyway?", description)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_mode_from_str() {
        assert_eq!("verbose".parse::<OutputMode>(), Ok(OutputMode::Verbose));
        assert_eq!("QUIET".parse::<OutputMode>(), Ok(OutputMode::Quiet));
        assert!("invalid".parse::<OutputMode>().is_err());
    }

    #[test]
    fn output_mode_shows_command_output() {
        assert!(OutputMode::Verbose.shows_command_output());
        assert!(!OutputMode::Normal.shows_command_output());
        assert!(!OutputMode::Quiet.shows_command_output());
        assert!(!OutputMode::Silent.shows_command_output());
    }

    #[test]
    fn output_mode_shows_spinners() {
        assert!(OutputMode::Verbose.shows_spinners());
        assert!(OutputMode::Normal.shows_spinners());
        assert!(OutputMode::Quiet.shows_spinners());
        assert!(!OutputMode::Silent.shows_spinners());
    }

    #[test]
    fn output_mode_shows_status() {
        assert!(OutputMode::Verbose.shows_status());
        assert!(OutputMode::Normal.shows_status());
        assert!(OutputMode::Quiet.shows_status());
        assert!(!OutputMode::Silent.shows_status());
    }

    #[test]
    fn output_mode_default() {
        assert_eq!(OutputMode::default(), OutputMode::Normal);
    }

    #[test]
    fn output_new_and_mode() {
        let output = Output::new(OutputMode::Quiet);
        assert_eq!(output.mode(), OutputMode::Quiet);
    }

    #[test]
    fn from_config_verbose() {
        let config_mode = crate::config::schema::OutputMode::Verbose;
        let ui_mode: OutputMode = config_mode.into();
        assert_eq!(ui_mode, OutputMode::Verbose);
    }

    #[test]
    fn from_config_quiet() {
        let config_mode = crate::config::schema::OutputMode::Quiet;
        let ui_mode: OutputMode = config_mode.into();
        assert_eq!(ui_mode, OutputMode::Quiet);
    }

    #[test]
    fn from_config_silent() {
        let config_mode = crate::config::schema::OutputMode::Silent;
        let ui_mode: OutputMode = config_mode.into();
        assert_eq!(ui_mode, OutputMode::Silent);
    }

    // --- OutputSink tests ---

    #[test]
    fn verbose_stream_sink_skips_empty_lines() {
        let sink = VerboseStreamSink::new(6);
        // Empty and whitespace-only lines should be skipped (no panic, no output).
        // Can't easily capture stdout in a unit test, so verify the call doesn't panic.
        sink.write_line(OutputLine::Stdout("".to_string()));
        sink.write_line(OutputLine::Stdout("   ".to_string()));
        // Verify the struct constructs with the right indent
        assert_eq!(sink.indent, "      ");
    }

    #[test]
    fn verbose_stream_sink_indent() {
        let sink = VerboseStreamSink::new(4);
        assert_eq!(sink.indent, "    ");

        let sink = VerboseStreamSink::new(0);
        assert_eq!(sink.indent, "");
    }

    // --- Label helper tests ---

    #[test]
    fn satisfaction_label_formats() {
        assert_eq!(
            satisfaction_label("node_modules exists"),
            "Satisfied (node_modules exists)"
        );
    }

    #[test]
    fn check_passed_label_formats() {
        assert_eq!(
            check_passed_label("yarn.lock exists"),
            "Check passed (yarn.lock exists)"
        );
    }

    #[test]
    fn rerun_prompt_label_formats() {
        assert_eq!(
            rerun_prompt_label("deps installed"),
            "Check passed (deps installed). Run anyway?"
        );
    }
}
