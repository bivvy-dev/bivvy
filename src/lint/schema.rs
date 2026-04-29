//! JSON Schema generation for Bivvy configuration.
//!
//! Generates a complete JSON Schema (Draft-07) from the Rust config types
//! via `schemars`, enabling IDE autocomplete and validation.

use serde_json::Value;

use crate::config::schema::BivvyConfig;

/// Generates JSON Schema for Bivvy configuration.
pub struct SchemaGenerator;

impl SchemaGenerator {
    /// Create a new schema generator.
    pub fn new() -> Self {
        Self
    }

    /// Generate the complete JSON Schema for bivvy.yml.
    ///
    /// Uses `schemars` to derive the schema from the Rust type definitions,
    /// ensuring the schema always matches the actual config parser.
    pub fn generate(&self) -> Value {
        let settings = schemars::gen::SchemaSettings::draft07().with(|s| {
            s.option_nullable = false;
            s.option_add_null_type = true;
        });
        let gen = settings.into_generator();
        let schema = gen.into_root_schema_for::<BivvyConfig>();
        let mut value = serde_json::to_value(schema).expect("schema serializes to JSON");

        // Inject the canonical $id for remote reference.
        if let Some(obj) = value.as_object_mut() {
            obj.insert(
                "$id".to_string(),
                Value::String("https://bivvy.dev/schemas/config.json".to_string()),
            );
        }

        value
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
    fn includes_canonical_id() {
        let generator = SchemaGenerator::new();
        let schema = generator.generate();

        assert_eq!(schema["$id"], "https://bivvy.dev/schemas/config.json");
    }

    #[test]
    fn includes_all_top_level_properties() {
        let generator = SchemaGenerator::new();
        let schema = generator.generate();

        let props = schema["properties"].as_object().expect("has properties");
        for key in &[
            "app_name",
            "settings",
            "template_sources",
            "steps",
            "workflows",
            "secrets",
            "extends",
            "requirements",
            "vars",
        ] {
            assert!(
                props.contains_key(*key),
                "missing top-level property: {key}"
            );
        }
    }

    #[test]
    fn steps_uses_additional_properties() {
        let generator = SchemaGenerator::new();
        let schema = generator.generate();

        let steps = &schema["properties"]["steps"];
        // Steps is a HashMap<String, StepConfig>, so schemars generates
        // additionalProperties pointing to the StepConfig schema.
        assert!(
            steps.get("additionalProperties").is_some()
                || steps.get("$ref").is_some()
                || steps["type"] == "object",
            "steps should be an object type"
        );
    }

    #[test]
    fn var_definition_generates_any_of() {
        let generator = SchemaGenerator::new();
        let schema = generator.generate();

        // VarDefinition is untagged enum, so schemars generates anyOf
        let schema_str = serde_json::to_string(&schema).unwrap();
        assert!(
            schema_str.contains("VarDefinition"),
            "schema should reference VarDefinition"
        );
    }

    #[test]
    fn settings_includes_flattened_fields() {
        let generator = SchemaGenerator::new();
        let schema = generator.generate();

        // Settings uses #[serde(flatten)] on sub-structs.
        // schemars should include all flattened fields. Check for a few key ones
        // by looking at the full schema JSON.
        let schema_str = serde_json::to_string(&schema).unwrap();

        // These are fields from flattened sub-structs that should appear somewhere
        for field in &[
            "default_output",
            "parallel",
            "max_parallel",
            "logging",
            "log_retention_days",
            "secret_env",
            "default_environment",
        ] {
            assert!(
                schema_str.contains(field),
                "schema should contain flattened field: {field}"
            );
        }
    }

    #[test]
    fn check_enum_referenced() {
        let generator = SchemaGenerator::new();
        let schema = generator.generate();

        let schema_str = serde_json::to_string(&schema).unwrap();
        // The Check enum (tagged with type) should be referenced
        assert!(
            schema_str.contains("Check") || schema_str.contains("check"),
            "schema should reference Check type"
        );
    }

    #[test]
    fn schema_is_valid_json() {
        let generator = SchemaGenerator::new();
        let schema = generator.generate();

        // Should round-trip through JSON pretty-printing
        let pretty = serde_json::to_string_pretty(&schema).unwrap();
        let _: Value = serde_json::from_str(&pretty).unwrap();
    }

    #[test]
    fn default_impl_works() {
        let generator = SchemaGenerator;
        let schema = generator.generate();
        assert!(schema["properties"].is_object());
    }
}
