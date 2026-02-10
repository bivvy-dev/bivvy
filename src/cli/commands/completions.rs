//! Shell completions generation.
//!
//! The `bivvy completions` command generates shell completion scripts.

use crate::cli::args::{Cli, CompletionsArgs};
use crate::ui::UserInterface;
use clap::CommandFactory;

use super::dispatcher::{Command, CommandResult};

/// The completions command implementation.
pub struct CompletionsCommand {
    args: CompletionsArgs,
}

impl CompletionsCommand {
    /// Create a new completions command.
    pub fn new(args: CompletionsArgs) -> Self {
        Self { args }
    }
}

impl Command for CompletionsCommand {
    fn execute(&self, _ui: &mut dyn UserInterface) -> crate::error::Result<CommandResult> {
        let mut cmd = Cli::command();
        clap_complete::generate(self.args.shell, &mut cmd, "bivvy", &mut std::io::stdout());
        Ok(CommandResult::success())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap_complete::Shell;

    #[test]
    fn generates_bash_completions() {
        let args = CompletionsArgs { shell: Shell::Bash };
        let mut cmd = Cli::command();
        let mut buf = Vec::new();
        clap_complete::generate(args.shell, &mut cmd, "bivvy", &mut buf);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("bivvy"));
        assert!(output.contains("complete"));
    }

    #[test]
    fn generates_zsh_completions() {
        let args = CompletionsArgs { shell: Shell::Zsh };
        let mut cmd = Cli::command();
        let mut buf = Vec::new();
        clap_complete::generate(args.shell, &mut cmd, "bivvy", &mut buf);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("bivvy"));
    }

    #[test]
    fn generates_fish_completions() {
        let args = CompletionsArgs { shell: Shell::Fish };
        let mut cmd = Cli::command();
        let mut buf = Vec::new();
        clap_complete::generate(args.shell, &mut cmd, "bivvy", &mut buf);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("bivvy"));
    }
}
