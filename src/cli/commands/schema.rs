//! Schema command implementation.
//!
//! The `bivvy schema` command prints the JSON Schema for config.yml.

use std::fs;

use crate::cli::args::SchemaArgs;
use crate::error::Result;
use crate::lint::schema_json;
use crate::ui::UserInterface;

use super::dispatcher::{Command, CommandResult};

/// The schema command implementation.
pub struct SchemaCommand {
    args: SchemaArgs,
}

impl SchemaCommand {
    /// Create a new schema command.
    pub fn new(args: SchemaArgs) -> Self {
        Self { args }
    }
}

impl Command for SchemaCommand {
    fn execute(&self, ui: &mut dyn UserInterface) -> Result<CommandResult> {
        if let Some(ref path) = self.args.output {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent)?;
                }
            }
            fs::write(path, format!("{}\n", schema_json()))?;
            ui.success(&format!("Wrote schema to {}", path.display()));
        } else {
            ui.message(schema_json());
        }

        Ok(CommandResult::success())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::MockUI;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn schema_prints_to_stdout() {
        let args = SchemaArgs::default();
        let cmd = SchemaCommand::new(args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
        assert_eq!(result.exit_code, 0);
        // Verify JSON was printed
        let messages = ui.messages();
        let output = messages.join("\n");
        assert!(output.contains("\"$schema\""));
        assert!(output.contains("https://json-schema.org/draft/2020-12/schema"));
    }

    #[test]
    fn schema_writes_to_file() {
        let temp = TempDir::new().unwrap();
        let output_path = temp.path().join("schema.json");

        let args = SchemaArgs {
            output: Some(output_path.clone()),
        };
        let cmd = SchemaCommand::new(args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
        assert!(output_path.exists());

        let content = std::fs::read_to_string(&output_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(
            parsed["$schema"],
            "https://json-schema.org/draft/2020-12/schema"
        );
    }

    #[test]
    fn schema_creates_parent_dirs() {
        let temp = TempDir::new().unwrap();
        let output_path = temp.path().join("nested/dir/schema.json");

        let args = SchemaArgs {
            output: Some(output_path.clone()),
        };
        let cmd = SchemaCommand::new(args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
        assert!(output_path.exists());
    }

    #[test]
    fn schema_output_is_valid_json_schema() {
        let args = SchemaArgs::default();
        let cmd = SchemaCommand::new(args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        let messages = ui.messages();
        let output = messages.join("\n");
        let schema: serde_json::Value = serde_json::from_str(&output).unwrap();

        assert_eq!(schema["type"], "object");
        assert!(schema["properties"].is_object());
        assert!(schema["properties"]["app_name"].is_object());
        assert!(schema["properties"]["steps"].is_object());
    }

    #[test]
    fn schema_to_file_path() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("out.json");
        let args = SchemaArgs {
            output: Some(PathBuf::from(path.to_str().unwrap())),
        };
        let cmd = SchemaCommand::new(args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();
        assert!(result.success);
        assert!(path.exists());
    }
}
