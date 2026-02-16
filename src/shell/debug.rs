//! Debug shell for step failure investigation.
//!
//! Spawns an interactive shell with the step's environment for debugging
//! when a step fails during `bivvy run`.

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use crate::error::Result;
use crate::shell::platform::detect_shell;

/// Spawn an interactive debug shell for investigating a step failure.
///
/// The shell inherits stdin/stdout/stderr (takes over the terminal) and
/// blocks until the user exits. Sets `BIVVY_DEBUG=1` and
/// `BIVVY_DEBUG_STEP=<step_name>` in the environment.
pub fn spawn_debug_shell(
    step_name: &str,
    project_root: &Path,
    step_env: &HashMap<String, String>,
    global_env: &HashMap<String, String>,
) -> Result<()> {
    let shell_info = detect_shell();
    let shell_exe = shell_info.executable;

    let mut cmd = Command::new(&shell_exe);
    cmd.arg("-i"); // Interactive — loads user's dotfiles

    // Set working directory
    cmd.current_dir(project_root);

    // Merge global env + step env (step overrides global)
    let mut env: HashMap<String, String> = global_env.clone();
    env.extend(step_env.iter().map(|(k, v)| (k.clone(), v.clone())));

    // Add bivvy debug markers
    env.insert("BIVVY_DEBUG".to_string(), "1".to_string());
    env.insert("BIVVY_DEBUG_STEP".to_string(), step_name.to_string());

    cmd.envs(&env);

    // Inherit stdio (takes over terminal)
    cmd.stdin(std::process::Stdio::inherit());
    cmd.stdout(std::process::Stdio::inherit());
    cmd.stderr(std::process::Stdio::inherit());

    // Spawn and wait
    let mut child = cmd
        .spawn()
        .map_err(|e| crate::error::BivvyError::ShellError {
            message: format!(
                "Failed to spawn debug shell '{}': {}",
                shell_exe.display(),
                e
            ),
        })?;

    child
        .wait()
        .map_err(|e| crate::error::BivvyError::ShellError {
            message: format!("Debug shell error: {}", e),
        })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_shell_env_vars() {
        // We can't actually spawn an interactive shell in tests,
        // but we can verify the env setup logic by building a non-interactive
        // command that echoes the env vars.
        let temp = tempfile::TempDir::new().unwrap();
        let step_env = HashMap::new();
        let mut global_env: HashMap<String, String> = HashMap::new();
        global_env.insert(
            "PATH".to_string(),
            std::env::var("PATH").unwrap_or_default(),
        );

        // Use a non-interactive command to verify env vars are set
        let shell_info = detect_shell();
        let mut cmd = Command::new(&shell_info.executable);
        cmd.args(["-c", "echo $BIVVY_DEBUG:$BIVVY_DEBUG_STEP"]);
        cmd.current_dir(temp.path());

        let mut env = global_env;
        env.extend(step_env);
        env.insert("BIVVY_DEBUG".to_string(), "1".to_string());
        env.insert("BIVVY_DEBUG_STEP".to_string(), "test_step".to_string());
        cmd.envs(&env);

        let output = cmd.output().unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("1:test_step"),
            "Expected '1:test_step' in output, got: {}",
            stdout
        );
    }

    #[test]
    fn debug_shell_cwd() {
        let temp = tempfile::TempDir::new().unwrap();
        let shell_info = detect_shell();

        let mut cmd = Command::new(&shell_info.executable);
        cmd.args(["-c", "pwd"]);
        cmd.current_dir(temp.path());

        let mut env: HashMap<String, String> = HashMap::new();
        env.insert(
            "PATH".to_string(),
            std::env::var("PATH").unwrap_or_default(),
        );
        cmd.envs(&env);

        let output = cmd.output().unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let resolved_temp = temp.path().canonicalize().unwrap();
        let stdout_trimmed = stdout.trim();
        let resolved_stdout = std::path::Path::new(stdout_trimmed)
            .canonicalize()
            .unwrap_or_else(|_| std::path::PathBuf::from(stdout_trimmed));
        assert_eq!(resolved_stdout, resolved_temp);
    }
}
