//! JSON Schema generation for Bivvy configuration.
//!
//! The schema is generated at runtime from Rust config types via `schemars`.
//! A [`LazyLock`] caches the result so generation only runs once per process.
//!
//! [`SchemaGenerator`] derives the schema from [`BivvyConfig`], ensuring the
//! schema always matches the actual config parser — no stale embedded files.

use std::sync::LazyLock;

use serde_json::Value;

use crate::config::schema::BivvyConfig;

/// Lazily-generated JSON Schema string, cached for the lifetime of the process.
///
/// This replaces the former `include_str!("../../generated/schema.json")` approach
/// which suffered from a circular dependency: the binary embedded the file it was
/// supposed to regenerate, so the output was always one build behind.
static SCHEMA_JSON: LazyLock<String> = LazyLock::new(|| {
    let schema = SchemaGenerator::new().generate();
    serde_json::to_string_pretty(&schema).expect("schema serializes to JSON")
});

/// Returns the JSON Schema as a string reference.
pub fn schema_json() -> &'static str {
    &SCHEMA_JSON
}

/// Returns the JSON Schema as a parsed [`Value`].
pub fn schema_value() -> Value {
    SchemaGenerator::new().generate()
}

/// Generates JSON Schema for Bivvy configuration at runtime.
///
/// Used by `scripts/generate-schema.sh` to regenerate `generated/schema.json`
/// and by tests to verify the embedded schema stays in sync with the config types.
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
        let gen = schemars::generate::SchemaSettings::draft2020_12().into_generator();
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

        assert_eq!(
            schema["$schema"],
            "https://json-schema.org/draft/2020-12/schema"
        );
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
            "defaults",
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

    #[test]
    fn schema_json_is_valid() {
        let schema: Value = serde_json::from_str(schema_json()).unwrap();
        assert_eq!(
            schema["$schema"],
            "https://json-schema.org/draft/2020-12/schema"
        );
        assert_eq!(schema["type"], "object");
    }

    #[test]
    fn schema_json_matches_generated() {
        let generated = SchemaGenerator::new().generate();
        let from_cache: Value = serde_json::from_str(schema_json()).unwrap();
        assert_eq!(generated, from_cache);
    }

    #[test]
    fn root_rejects_unknown_fields() {
        let schema = SchemaGenerator::new().generate();
        assert_eq!(
            schema["additionalProperties"],
            Value::Bool(false),
            "root config schema should set additionalProperties: false"
        );
    }

    #[test]
    fn settings_rejects_unknown_fields() {
        let schema = SchemaGenerator::new().generate();
        assert_eq!(
            schema["$defs"]["Settings"]["additionalProperties"],
            Value::Bool(false),
            "Settings schema should set additionalProperties: false (e.g., \
             reject `dark_mode` under `settings`)"
        );
    }

    #[test]
    fn step_config_rejects_unknown_fields() {
        let schema = SchemaGenerator::new().generate();
        assert_eq!(
            schema["$defs"]["StepConfig"]["additionalProperties"],
            Value::Bool(false),
            "StepConfig schema should set additionalProperties: false"
        );
    }

    #[test]
    fn defaults_settings_rejects_unknown_fields() {
        let schema = SchemaGenerator::new().generate();
        assert_eq!(
            schema["$defs"]["DefaultsSettings"]["additionalProperties"],
            Value::Bool(false),
            "DefaultsSettings schema should set additionalProperties: false"
        );
    }

    #[test]
    fn workflow_config_rejects_unknown_fields() {
        let schema = SchemaGenerator::new().generate();
        assert_eq!(
            schema["$defs"]["WorkflowConfig"]["additionalProperties"],
            Value::Bool(false),
            "WorkflowConfig schema should set additionalProperties: false"
        );
    }

    #[test]
    fn deserialize_rejects_unknown_root_field() {
        // `dark_mode` is not a known top-level field; deserialization should fail.
        let yaml = r#"
app_name: "demo"
dark_mode: true
"#;
        let result: Result<crate::config::schema::BivvyConfig, _> = serde_yaml::from_str(yaml);
        assert!(
            result.is_err(),
            "expected deserialization to reject unknown root field `dark_mode`, \
             but it succeeded"
        );
    }
}
