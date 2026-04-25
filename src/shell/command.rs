//! Shell command execution.

use crate::error::{BivvyError, Result};
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// Re-claim the terminal foreground process group.
///
/// After a child process exits, the parent shell (especially zsh) may
/// reclaim the foreground group, leaving bivvy as a background process.
/// Any subsequent terminal I/O would then trigger SIGTTOU/SIGTTIN.
/// This function re-asserts bivvy's ownership of the foreground group.
///
/// Safe to call multiple times — no-op if already foreground or if
/// `/dev/tty` is unavailable (non-TTY, CI, piped environments).
#[cfg(unix)]
pub(crate) fn claim_foreground() {
    // SAFETY: libc calls to open/close /dev/tty and set the foreground
    // process group. SIGTTOU is already ignored (main.rs) so tcsetpgrp
    // won't suspend us even if we're currently backgrounded.
    unsafe {
        let tty_fd = libc::open(c"/dev/tty".as_ptr(), libc::O_RDWR);
        if tty_fd >= 0 {
            libc::tcsetpgrp(tty_fd, libc::getpgrp());
            libc::close(tty_fd);
        }
    }
}

/// Drain any buffered terminal input to prevent queued keypresses
/// from triggering unintended actions after a child process exits.
#[cfg(unix)]
pub(crate) fn drain_input() {
    use std::io::Read;
    // Set stdin to non-blocking, read and discard, restore blocking
    unsafe {
        let fd = libc::STDIN_FILENO;
        let flags = libc::fcntl(fd, libc::F_GETFL);
        if flags >= 0 {
            libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
            let mut buf = [0u8; 1024];
            while std::io::stdin().read(&mut buf).unwrap_or(0) > 0 {}
            libc::fcntl(fd, libc::F_SETFL, flags);
        }
    }
}

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

    /// Redirect stdin from /dev/null instead of inheriting the terminal.
    ///
    /// Use this for commands that don't need interactive input (e.g.,
    /// variable evaluation, completed checks). Prevents child processes
    /// from accessing the terminal and interfering with process groups.
    pub stdin_null: bool,

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

    // Isolate child in its own process group so zsh's job control
    // doesn't reclaim the foreground when the child exits.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    // Set working directory
    if let Some(cwd) = &options.cwd {
        cmd.current_dir(cwd);
    }

    // Set environment
    for (key, value) in &options.env {
        cmd.env(key, value);
    }

    // Configure stdio
    if options.stdin_null {
        cmd.stdin(Stdio::null());
    }

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

    // Re-claim foreground after the child's process group exits.
    #[cfg(unix)]
    claim_foreground();

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
///
/// Stdin is redirected from `/dev/null` — check commands should not
/// interact with the terminal.
pub fn execute_check(command: &str, cwd: Option<&Path>) -> bool {
    let options = CommandOptions {
        cwd: cwd.map(|p| p.to_path_buf()),
        capture_stdout: true,
        capture_stderr: true,
        stdin_null: true,
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

    // Isolate child in its own process group so zsh's job control
    // doesn't reclaim the foreground when the child exits.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    if let Some(cwd) = &options.cwd {
        cmd.current_dir(cwd);
    }

    for (key, value) in &options.env {
        cmd.env(key, value);
    }

    if options.stdin_null {
        cmd.stdin(Stdio::null());
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

    // Re-claim foreground after the child's process group exits.
    #[cfg(unix)]
    claim_foreground();

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
///
/// Stdin is redirected from `/dev/null` — these commands run silently
/// and should not interact with the terminal.
pub fn execute_quiet(command: &str, cwd: Option<&Path>) -> Result<CommandResult> {
    let options = CommandOptions {
        cwd: cwd.map(|p| p.to_path_buf()),
        capture_stdout: true,
        capture_stderr: true,
        stdin_null: true,
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
/// Uses `-c` (non-login, non-interactive) on Unix. Bivvy inherits
/// the invoking shell's environment, so re-sourcing login configs
/// via `-l` is unnecessary and can override the correct PATH/tool
/// versions that are already set up in the parent environment.
///
/// **Why not `-l` (login)?**  The parent shell has already sourced
/// `.zprofile`/`.bash_profile`/`.zshenv`. Re-sourcing them in the
/// child can reset PATH modifications made by version managers
/// (mise, asdf, rbenv) that activate per-directory, causing the
/// wrong tool version to be used.
///
/// **Why not `-i` (interactive)?**  The `-i` flag makes the child
/// shell set up job control: it calls `setpgid`/`tcsetpgrp` to create
/// its own process group and steal the terminal foreground. This causes:
///   - **SIGTTOU** — bivvy becomes a background process; terminal
///     writes trigger "zsh: suspended (tty output)"
///   - **Ctrl+C/Ctrl+Z broken** — signals go to the child's process
///     group, not bivvy's, so the user can't interrupt or suspend
///   - **Prompt hangs** — after the child exits, bivvy is background;
///     `read_key()` returns EIO or blocks
fn shell_flag(_shell: &str) -> &'static str {
    if cfg!(target_os = "windows") {
        "/C"
    } else {
        "-c"
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
    fn shell_flag_uses_non_login_non_interactive() {
        // shell_flag uses -c (non-login, non-interactive) because bivvy
        // inherits the invoking shell's environment — re-sourcing login
        // configs is unnecessary and can override correct PATH/tool versions.
        let flag = shell_flag("/bin/bash");
        assert_eq!(flag, "-c");
    }
}
