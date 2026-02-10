//! JSON Schema generation for Bivvy configuration.
//!
//! This module generates a JSON Schema (Draft-07) for the Bivvy configuration
//! file format, enabling IDE autocomplete and validation.

use serde_json::{json, Value};

/// Generates JSON Schema for Bivvy configuration.
pub struct SchemaGenerator;

impl SchemaGenerator {
    /// Create a new schema generator.
    pub fn new() -> Self {
        Self
    }

    /// Generate the complete JSON Schema for bivvy.yml.
    pub fn generate(&self) -> Value {
        json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "$id": "https://bivvy.dev/schemas/config.json",
            "title": "Bivvy Configuration",
            "description": "Configuration schema for Bivvy development environment setup",
            "type": "object",
            "properties": {
                "app_name": {
                    "type": "string",
                    "description": "Name of the application"
                },
                "settings": self.settings_schema(),
                "steps": self.steps_schema(),
                "workflows": self.workflows_schema()
            },
            "additionalProperties": false
        })
    }

    /// Generate schema for the settings object.
    fn settings_schema(&self) -> Value {
        json!({
            "type": "object",
            "description": "Global settings",
            "properties": {
                "default_output": {
                    "type": "string",
                    "enum": ["verbose", "quiet", "silent"],
                    "default": "verbose",
                    "description": "Default output verbosity"
                },
                "logging": {
                    "type": "boolean",
                    "default": false,
                    "description": "Enable logging"
                }
            },
            "additionalProperties": false
        })
    }

    /// Generate schema for steps.
    fn steps_schema(&self) -> Value {
        json!({
            "type": "object",
            "description": "Step definitions",
            "additionalProperties": {
                "type": "object",
                "properties": {
                    "template": {
                        "type": "string",
                        "description": "Template to use for this step"
                    },
                    "command": {
                        "type": "string",
                        "description": "Command to execute (if not using template)"
                    },
                    "description": {
                        "type": "string",
                        "description": "Human-readable description"
                    },
                    "depends_on": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Steps that must run before this one"
                    },
                    "completed_check": self.completed_check_schema(),
                    "watches": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Files to watch for changes"
                    },
                    "inputs": {
                        "type": "object",
                        "additionalProperties": true,
                        "description": "Input values for the template"
                    }
                },
                "additionalProperties": false
            }
        })
    }

    /// Generate schema for completed_check.
    fn completed_check_schema(&self) -> Value {
        json!({
            "type": "object",
            "description": "How to check if step is complete",
            "properties": {
                "type": {
                    "type": "string",
                    "enum": ["file_exists", "command_succeeds", "marker"],
                    "description": "Type of completion check"
                },
                "path": {
                    "type": "string",
                    "description": "Path for file_exists check"
                },
                "command": {
                    "type": "string",
                    "description": "Command for command_succeeds check"
                }
            },
            "required": ["type"]
        })
    }

    /// Generate schema for workflows.
    fn workflows_schema(&self) -> Value {
        json!({
            "type": "object",
            "description": "Workflow definitions",
            "additionalProperties": {
                "type": "object",
                "properties": {
                    "description": {
                        "type": "string",
                        "description": "Human-readable description"
                    },
                    "steps": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Steps to run in this workflow"
                    }
                },
                "required": ["steps"],
                "additionalProperties": false
            }
        })
    }
}

impl Default for SchemaGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_valid_json_schema() {
        let generator = SchemaGenerator::new();
        let schema = generator.generate();

        assert_eq!(schema["$schema"], "http://json-schema.org/draft-07/schema#");
        assert_eq!(schema["type"], "object");
    }

    #[test]
    fn includes_app_name_property() {
        let generator = SchemaGenerator::new();
        let schema = generator.generate();

        let app_name = &schema["properties"]["app_name"];
        assert_eq!(app_name["type"], "string");
    }

    #[test]
    fn includes_steps_schema() {
        let generator = SchemaGenerator::new();
        let schema = generator.generate();

        let steps = &schema["properties"]["steps"];
        assert_eq!(steps["type"], "object");
        assert!(steps["additionalProperties"].is_object());
    }

    #[test]
    fn includes_workflows_schema() {
        let generator = SchemaGenerator::new();
        let schema = generator.generate();

        let workflows = &schema["properties"]["workflows"];
        assert_eq!(workflows["type"], "object");
    }

    #[test]
    fn step_schema_has_required_fields() {
        let generator = SchemaGenerator::new();
        let schema = generator.generate();

        let step_props = &schema["properties"]["steps"]["additionalProperties"]["properties"];
        assert!(step_props["template"].is_object());
        assert!(step_props["depends_on"].is_object());
        assert!(step_props["watches"].is_object());
    }

    #[test]
    fn settings_schema_has_verbosity_enum() {
        let generator = SchemaGenerator::new();
        let schema = generator.generate();

        let settings = &schema["properties"]["settings"];
        let default_output = &settings["properties"]["default_output"];
        assert!(default_output["enum"].is_array());
    }

    #[test]
    fn completed_check_has_type_required() {
        let generator = SchemaGenerator::new();
        let schema = generator.generate();

        let completed_check =
            &schema["properties"]["steps"]["additionalProperties"]["properties"]["completed_check"];
        let required = completed_check["required"].as_array().unwrap();
        assert!(required.contains(&json!("type")));
    }

    #[test]
    fn default_impl_works() {
        let generator = SchemaGenerator;
        let schema = generator.generate();
        assert!(schema["properties"].is_object());
    }
}
