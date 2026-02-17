//! Shell command execution.

use crate::error::{BivvyError, Result};
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// Result of executing a shell command.
#[derive(Debug, Clone)]
pub struct CommandResult {
    /// Exit code (None if killed by signal).
    pub exit_code: Option<i32>,

    /// Standard output.
    pub stdout: String,

    /// Standard error.
    pub stderr: String,

    /// Execution duration.
    pub duration: Duration,

    /// Whether command succeeded (exit code 0).
    pub success: bool,
}

impl CommandResult {
    /// Create a success result.
    pub fn success(stdout: String, stderr: String, duration: Duration) -> Self {
        Self {
            exit_code: Some(0),
            stdout,
            stderr,
            duration,
            success: true,
        }
    }

    /// Create a failure result.
    pub fn failure(
        exit_code: Option<i32>,
        stdout: String,
        stderr: String,
        duration: Duration,
    ) -> Self {
        Self {
            exit_code,
            stdout,
            stderr,
            duration,
            success: false,
        }
    }
}

/// Options for command execution.
#[derive(Debug, Clone, Default)]
pub struct CommandOptions {
    /// Working directory.
    pub cwd: Option<std::path::PathBuf>,

    /// Environment variables (merged with system env).
    pub env: HashMap<String, String>,

    /// Capture stdout (if false, inherits from parent).
    pub capture_stdout: bool,

    /// Capture stderr (if false, inherits from parent).
    pub capture_stderr: bool,

    /// Timeout in seconds (None = no timeout).
    pub timeout: Option<u64>,
}

/// Output line from command execution.
#[derive(Debug, Clone)]
pub enum OutputLine {
    Stdout(String),
    Stderr(String),
}

/// Callback for streaming output.
pub type OutputCallback = Box<dyn Fn(OutputLine) + Send>;

/// Execute a shell command.
pub fn execute(command: &str, options: &CommandOptions) -> Result<CommandResult> {
    let start = Instant::now();

    let shell = detect_shell();
    let shell_flag = shell_flag(&shell);

    let mut cmd = Command::new(&shell);
    cmd.arg(shell_flag);
    cmd.arg(command);

    // Set working directory
    if let Some(cwd) = &options.cwd {
        cmd.current_dir(cwd);
    }

    // Set environment
    for (key, value) in &options.env {
        cmd.env(key, value);
    }

    // Configure stdio
    if options.capture_stdout {
        cmd.stdout(Stdio::piped());
    } else {
        cmd.stdout(Stdio::inherit());
    }

    if options.capture_stderr {
        cmd.stderr(Stdio::piped());
    } else {
        cmd.stderr(Stdio::inherit());
    }

    // Execute
    let output = cmd.output().map_err(|_| BivvyError::CommandFailed {
        command: command.to_string(),
        code: None,
    })?;

    let duration = start.elapsed();

    let stdout = if options.capture_stdout {
        String::from_utf8_lossy(&output.stdout).to_string()
    } else {
        String::new()
    };

    let stderr = if options.capture_stderr {
        String::from_utf8_lossy(&output.stderr).to_string()
    } else {
        String::new()
    };

    if output.status.success() {
        Ok(CommandResult::success(stdout, stderr, duration))
    } else {
        Ok(CommandResult::failure(
            output.status.code(),
            stdout,
            stderr,
            duration,
        ))
    }
}

/// Execute a command and return success/failure.
pub fn execute_check(command: &str, cwd: Option<&Path>) -> bool {
    let options = CommandOptions {
        cwd: cwd.map(|p| p.to_path_buf()),
        capture_stdout: true,
        capture_stderr: true,
        ..Default::default()
    };

    execute(command, &options)
        .map(|r| r.success)
        .unwrap_or(false)
}

/// Execute a command with streaming output.
pub fn execute_streaming(
    command: &str,
    options: &CommandOptions,
    callback: OutputCallback,
) -> Result<CommandResult> {
    let start = Instant::now();

    let shell = detect_shell();
    let shell_flag = shell_flag(&shell);

    let mut cmd = Command::new(&shell);
    cmd.arg(shell_flag);
    cmd.arg(command);

    if let Some(cwd) = &options.cwd {
        cmd.current_dir(cwd);
    }

    for (key, value) in &options.env {
        cmd.env(key, value);
    }

    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|_| BivvyError::CommandFailed {
        command: command.to_string(),
        code: None,
    })?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let (tx, rx) = mpsc::channel();
    let tx_stdout = tx.clone();
    let tx_stderr = tx;

    // Spawn threads to read stdout and stderr
    let stdout_handle = thread::spawn(move || {
        let reader = BufReader::new(stdout);
        let mut output = String::new();
        for line in reader.lines().map_while(std::result::Result::ok) {
            output.push_str(&line);
            output.push('\n');
            let _ = tx_stdout.send(OutputLine::Stdout(line));
        }
        output
    });

    let stderr_handle = thread::spawn(move || {
        let reader = BufReader::new(stderr);
        let mut output = String::new();
        for line in reader.lines().map_while(std::result::Result::ok) {
            output.push_str(&line);
            output.push('\n');
            let _ = tx_stderr.send(OutputLine::Stderr(line));
        }
        output
    });

    // Process output through callback
    for line in rx {
        callback(line);
    }

    let stdout_output = stdout_handle.join().unwrap_or_default();
    let stderr_output = stderr_handle.join().unwrap_or_default();

    let status = child.wait().map_err(|_| BivvyError::CommandFailed {
        command: command.to_string(),
        code: None,
    })?;

    let duration = start.elapsed();

    if status.success() {
        Ok(CommandResult::success(
            stdout_output,
            stderr_output,
            duration,
        ))
    } else {
        Ok(CommandResult::failure(
            status.code(),
            stdout_output,
            stderr_output,
            duration,
        ))
    }
}

/// Execute a command and collect output without streaming.
pub fn execute_quiet(command: &str, cwd: Option<&Path>) -> Result<CommandResult> {
    let options = CommandOptions {
        cwd: cwd.map(|p| p.to_path_buf()),
        capture_stdout: true,
        capture_stderr: true,
        ..Default::default()
    };
    execute(command, &options)
}

/// Detect the current shell.
fn detect_shell() -> String {
    if cfg!(target_os = "windows") {
        std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
    } else {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
    }
}

/// Get the flag to pass commands to the shell.
///
/// Uses `-lic` (interactive login shell) on Unix so that the user's
/// full shell environment is available. Tools like mise, asdf, nvm,
/// and rbenv are typically activated in `.zshrc`/`.bashrc` (interactive)
/// or `.zprofile`/`.bash_profile` (login). Without `-lic`, these tools
/// aren't in PATH and step commands fail with "command not found".
///
/// In CI environments, uses `-lc` (login, non-interactive) to avoid
/// `bash: cannot set terminal process group` errors caused by `-i`
/// trying to set up job control without a TTY.
fn shell_flag(_shell: &str) -> &'static str {
    if cfg!(target_os = "windows") {
        "/C"
    } else if super::is_ci() {
        "-lc"
    } else {
        "-lic"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execute_successful_command() {
        let options = CommandOptions {
            capture_stdout: true,
            capture_stderr: true,
            ..Default::default()
        };

        let result = execute("echo hello", &options).unwrap();

        assert!(result.success);
        assert_eq!(result.exit_code, Some(0));
        assert!(result.stdout.contains("hello"));
    }

    #[test]
    fn execute_failing_command() {
        let options = CommandOptions {
            capture_stdout: true,
            capture_stderr: true,
            ..Default::default()
        };

        let result = execute("exit 1", &options).unwrap();

        assert!(!result.success);
        assert_eq!(result.exit_code, Some(1));
    }

    #[test]
    fn execute_with_env() {
        let mut options = CommandOptions {
            capture_stdout: true,
            capture_stderr: true,
            ..Default::default()
        };
        options
            .env
            .insert("MY_VAR".to_string(), "my_value".to_string());

        let cmd = if cfg!(target_os = "windows") {
            "echo %MY_VAR%"
        } else {
            "echo $MY_VAR"
        };

        let result = execute(cmd, &options).unwrap();

        assert!(result.success);
        assert!(result.stdout.contains("my_value"));
    }

    #[test]
    fn execute_with_cwd() {
        let temp = tempfile::TempDir::new().unwrap();
        let options = CommandOptions {
            cwd: Some(temp.path().to_path_buf()),
            capture_stdout: true,
            ..Default::default()
        };

        let cmd = if cfg!(target_os = "windows") {
            "cd"
        } else {
            "pwd"
        };

        let result = execute(cmd, &options).unwrap();

        assert!(result.success);
    }

    #[test]
    fn execute_check_returns_bool() {
        assert!(execute_check("exit 0", None));
        assert!(!execute_check("exit 1", None));
    }

    #[test]
    fn command_result_tracks_duration() {
        let options = CommandOptions {
            capture_stdout: true,
            ..Default::default()
        };

        let result = execute("echo fast", &options).unwrap();

        assert!(result.duration.as_millis() < 5000);
    }

    #[test]
    fn execute_streaming_captures_output() {
        use std::sync::{Arc, Mutex};

        let lines = Arc::new(Mutex::new(Vec::new()));
        let lines_clone = Arc::clone(&lines);

        let callback: OutputCallback = Box::new(move |line| {
            lines_clone.lock().unwrap().push(line);
        });

        let options = CommandOptions::default();
        let result = execute_streaming("echo line1 && echo line2", &options, callback).unwrap();

        assert!(result.success);

        let captured = lines.lock().unwrap();
        assert!(captured.len() >= 2);
    }

    #[test]
    fn execute_streaming_captures_stderr() {
        use std::sync::{Arc, Mutex};

        let lines = Arc::new(Mutex::new(Vec::new()));
        let lines_clone = Arc::clone(&lines);

        let callback: OutputCallback = Box::new(move |line| {
            lines_clone.lock().unwrap().push(line);
        });

        let options = CommandOptions::default();
        let cmd = if cfg!(target_os = "windows") {
            "echo error 1>&2"
        } else {
            "echo error >&2"
        };

        let _ = execute_streaming(cmd, &options, callback);

        let captured = lines.lock().unwrap();
        assert!(captured.iter().any(|l| matches!(l, OutputLine::Stderr(_))));
    }

    #[test]
    fn execute_quiet_captures_silently() {
        let result = execute_quiet("echo hello", None).unwrap();
        assert!(result.success);
        assert!(result.stdout.contains("hello"));
    }

    #[test]
    fn shell_flag_uses_non_interactive_in_ci() {
        // When CI env var is set, shell_flag should use -lc (non-interactive)
        // to avoid "cannot set terminal process group" errors
        std::env::set_var("CI", "true");
        let flag = shell_flag("/bin/bash");
        std::env::remove_var("CI");
        assert_eq!(flag, "-lc");
    }

    #[test]
    fn shell_flag_uses_interactive_outside_ci() {
        // Ensure no CI env vars are set for this test
        let ci_vars = ["CI", "GITHUB_ACTIONS", "GITLAB_CI", "CIRCLECI", "TRAVIS"];
        let saved: Vec<_> = ci_vars
            .iter()
            .map(|k| (*k, std::env::var(k).ok()))
            .collect();
        for k in &ci_vars {
            std::env::remove_var(k);
        }

        let flag = shell_flag("/bin/bash");

        // Restore env vars
        for (k, v) in &saved {
            if let Some(val) = v {
                std::env::set_var(k, val);
            }
        }
        assert_eq!(flag, "-lic");
    }
}
