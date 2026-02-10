//! Deep merge algorithm for YAML configuration values.
//!
//! Bivvy supports configuration layering where later configs override
//! earlier ones. This module implements the merge semantics.
//!
//! # Merge Rules
//!
//! - Objects are merged recursively
//! - Arrays are replaced entirely (not merged)
//! - Null values in overlay delete the corresponding key from base
//! - Scalars in overlay replace scalars in base

use serde_yaml::Value;

/// Deep merge two YAML values.
///
/// Later values override earlier values at the point of conflict.
/// Objects are merged recursively. Arrays are replaced entirely.
/// Null values in overlay delete the corresponding key from base.
///
/// # Arguments
///
/// * `base` - The base configuration
/// * `overlay` - The overlay configuration (takes precedence)
///
/// # Returns
///
/// A new Value with merged contents
pub fn deep_merge(base: &Value, overlay: &Value) -> Value {
    match (base, overlay) {
        // Both are mappings: merge recursively
        (Value::Mapping(base_map), Value::Mapping(overlay_map)) => {
            let mut result = base_map.clone();

            for (key, overlay_value) in overlay_map {
                if overlay_value.is_null() {
                    // Null in overlay = delete from result
                    result.remove(key);
                } else if let Some(base_value) = base_map.get(key) {
                    // Key exists in both: recurse
                    result.insert(key.clone(), deep_merge(base_value, overlay_value));
                } else {
                    // Key only in overlay: insert
                    result.insert(key.clone(), overlay_value.clone());
                }
            }

            Value::Mapping(result)
        }

        // Overlay is not a mapping, or base is not a mapping: overlay wins
        (_, overlay) => overlay.clone(),
    }
}

/// Merge multiple configs in order (later overrides earlier).
///
/// # Arguments
///
/// * `configs` - Slice of configs in merge order (first is base, last has highest priority)
///
/// # Returns
///
/// A single merged Value
pub fn merge_configs(configs: &[Value]) -> Value {
    configs
        .iter()
        .fold(Value::Mapping(Default::default()), |acc, config| {
            deep_merge(&acc, config)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn yaml(s: &str) -> Value {
        serde_yaml::from_str(s).unwrap()
    }

    #[test]
    fn deep_merge_replaces_at_conflict_point() {
        let base = yaml(
            r#"
steps:
  database:
    command: "rails db:setup"
    watches:
      - schema.rb
"#,
        );
        let overlay = yaml(
            r#"
steps:
  database:
    command: "rails db:prepare"
"#,
        );

        let result = deep_merge(&base, &overlay);

        assert_eq!(result["steps"]["database"]["command"], "rails db:prepare");
        // watches should be preserved
        assert_eq!(result["steps"]["database"]["watches"][0], "schema.rb");
    }

    #[test]
    fn arrays_are_replaced_not_merged() {
        let base = yaml(
            r#"
watches:
  - a.rb
  - b.rb
"#,
        );
        let overlay = yaml(
            r#"
watches:
  - c.rb
"#,
        );

        let result = deep_merge(&base, &overlay);
        let watches = result["watches"].as_sequence().unwrap();

        assert_eq!(watches.len(), 1);
        assert_eq!(watches[0], "c.rb");
    }

    #[test]
    fn null_removes_inherited_value() {
        let base = yaml(
            r#"
env:
  DEBUG: "true"
  LOG: verbose
"#,
        );
        let overlay = yaml(
            r#"
env:
  DEBUG: null
"#,
        );

        let result = deep_merge(&base, &overlay);

        assert!(result["env"].get("DEBUG").is_none());
        assert_eq!(result["env"]["LOG"], "verbose");
    }

    #[test]
    fn nested_objects_merge_recursively() {
        let base = yaml(
            r#"
settings:
  output: verbose
  logging: true
"#,
        );
        let overlay = yaml(
            r#"
settings:
  output: quiet
"#,
        );

        let result = deep_merge(&base, &overlay);

        assert_eq!(result["settings"]["output"], "quiet");
        assert_eq!(result["settings"]["logging"], true);
    }

    #[test]
    fn empty_overlay_returns_base_unchanged() {
        let base = yaml(
            r#"
app_name: Test
settings:
  output: verbose
"#,
        );
        // Empty YAML file parses to Null, which we want to treat as "no changes"
        // So use an empty mapping instead
        let overlay = yaml("{}");

        let result = deep_merge(&base, &overlay);

        assert_eq!(result["app_name"], "Test");
        assert_eq!(result["settings"]["output"], "verbose");
    }

    #[test]
    fn merge_configs_merges_multiple_in_order() {
        let configs = vec![yaml("a: 1\nb: 2"), yaml("b: 3\nc: 4"), yaml("c: 5")];

        let result = merge_configs(&configs);

        assert_eq!(result["a"], 1);
        assert_eq!(result["b"], 3);
        assert_eq!(result["c"], 5);
    }

    #[test]
    fn scalar_overlay_replaces_mapping_base() {
        let base = yaml(
            r#"
settings:
  output: verbose
"#,
        );
        let overlay = yaml(
            r#"
settings: disabled
"#,
        );

        let result = deep_merge(&base, &overlay);
        assert_eq!(result["settings"], "disabled");
    }

    #[test]
    fn deeply_nested_merge() {
        let base = yaml(
            r#"
a:
  b:
    c:
      d: 1
      e: 2
"#,
        );
        let overlay = yaml(
            r#"
a:
  b:
    c:
      d: 10
"#,
        );

        let result = deep_merge(&base, &overlay);
        assert_eq!(result["a"]["b"]["c"]["d"], 10);
        assert_eq!(result["a"]["b"]["c"]["e"], 2);
    }

    #[test]
    fn merge_empty_configs_returns_empty() {
        let result = merge_configs(&[]);
        assert!(result.as_mapping().unwrap().is_empty());
    }
}
