//! Environment variable layering.
//!
//! This module provides priority-based environment variable management
//! with source tracking for debugging.

use std::collections::HashMap;

/// Represents a layer of environment variables.
///
/// # Example
///
/// ```
/// use bivvy::config::EnvLayer;
///
/// let mut layer = EnvLayer::new("config.yml");
/// layer.set("DATABASE_URL", "postgres://localhost/db");
/// layer.set("DEBUG", "true");
///
/// assert_eq!(layer.vars.get("DATABASE_URL").map(String::as_str), Some("postgres://localhost/db"));
/// assert_eq!(layer.source, "config.yml");
/// ```
#[derive(Debug, Clone, Default)]
pub struct EnvLayer {
    /// Variables in this layer.
    pub vars: HashMap<String, String>,
    /// Source of this layer (for debugging).
    pub source: String,
}

impl EnvLayer {
    /// Create a new layer with the given source name.
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            vars: HashMap::new(),
            source: source.into(),
        }
    }

    /// Add a variable to this layer.
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.vars.insert(key.into(), value.into());
    }

    /// Check if this layer has a variable.
    pub fn contains(&self, key: &str) -> bool {
        self.vars.contains_key(key)
    }

    /// Get the number of variables in this layer.
    pub fn len(&self) -> usize {
        self.vars.len()
    }

    /// Check if this layer is empty.
    pub fn is_empty(&self) -> bool {
        self.vars.is_empty()
    }
}

/// Manages layered environment variables.
///
/// Variables from higher layers override variables from lower layers.
/// The first layer pushed has lowest priority, the last has highest.
///
/// # Example
///
/// ```
/// use bivvy::config::{EnvLayer, EnvLayerStack};
///
/// let mut stack = EnvLayerStack::new();
///
/// // Base layer (lowest priority)
/// let mut base = EnvLayer::new("base");
/// base.set("KEY", "base_value");
/// base.set("BASE_ONLY", "from_base");
/// stack.push(base);
///
/// // Override layer (higher priority)
/// let mut overlay = EnvLayer::new("overlay");
/// overlay.set("KEY", "override_value");
/// stack.push(overlay);
///
/// assert_eq!(stack.get("KEY"), Some("override_value"));
/// assert_eq!(stack.get("BASE_ONLY"), Some("from_base"));
/// assert_eq!(stack.source_of("KEY"), Some("overlay"));
/// ```
pub struct EnvLayerStack {
    /// Layers from lowest to highest priority.
    layers: Vec<EnvLayer>,
}

impl EnvLayerStack {
    /// Create a new empty stack.
    pub fn new() -> Self {
        Self { layers: vec![] }
    }

    /// Add a layer with the given priority.
    ///
    /// Later layers have higher priority.
    pub fn push(&mut self, layer: EnvLayer) {
        self.layers.push(layer);
    }

    /// Get the resolved value for a variable.
    ///
    /// Returns the value from the highest priority layer that contains the key.
    pub fn get(&self, key: &str) -> Option<&str> {
        // Higher index = higher priority
        for layer in self.layers.iter().rev() {
            if let Some(value) = layer.vars.get(key) {
                return Some(value);
            }
        }
        None
    }

    /// Get all resolved variables.
    ///
    /// Higher priority layers override lower priority layers.
    pub fn resolve(&self) -> HashMap<String, String> {
        let mut result = HashMap::new();
        for layer in &self.layers {
            result.extend(layer.vars.clone());
        }
        result
    }

    /// Get the source of a variable's value.
    ///
    /// Returns the source of the highest priority layer that contains the key.
    pub fn source_of(&self, key: &str) -> Option<&str> {
        for layer in self.layers.iter().rev() {
            if layer.vars.contains_key(key) {
                return Some(&layer.source);
            }
        }
        None
    }

    /// Get all layers for inspection.
    pub fn layers(&self) -> &[EnvLayer] {
        &self.layers
    }

    /// Get the number of layers.
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }
}

impl Default for EnvLayerStack {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn higher_layers_override_lower() {
        let mut stack = EnvLayerStack::new();

        let mut base = EnvLayer::new("base");
        base.set("KEY", "base");
        stack.push(base);

        let mut local = EnvLayer::new("local");
        local.set("KEY", "override");
        stack.push(local);

        assert_eq!(stack.get("KEY"), Some("override"));
    }

    #[test]
    fn tracks_variable_source() {
        let mut stack = EnvLayerStack::new();

        let mut layer = EnvLayer::new("config.yml");
        layer.set("KEY", "value");
        stack.push(layer);

        assert_eq!(stack.source_of("KEY"), Some("config.yml"));
    }

    #[test]
    fn resolve_merges_all_layers() {
        let mut stack = EnvLayerStack::new();

        let mut layer1 = EnvLayer::new("layer1");
        layer1.set("A", "1");
        stack.push(layer1);

        let mut layer2 = EnvLayer::new("layer2");
        layer2.set("B", "2");
        stack.push(layer2);

        let resolved = stack.resolve();
        assert_eq!(resolved.get("A"), Some(&"1".to_string()));
        assert_eq!(resolved.get("B"), Some(&"2".to_string()));
    }

    #[test]
    fn missing_key_returns_none() {
        let stack = EnvLayerStack::new();
        assert_eq!(stack.get("MISSING"), None);
        assert_eq!(stack.source_of("MISSING"), None);
    }

    #[test]
    fn layer_contains_check() {
        let mut layer = EnvLayer::new("test");
        layer.set("KEY", "value");

        assert!(layer.contains("KEY"));
        assert!(!layer.contains("OTHER"));
    }

    #[test]
    fn layer_len_and_is_empty() {
        let mut layer = EnvLayer::new("test");
        assert!(layer.is_empty());
        assert_eq!(layer.len(), 0);

        layer.set("KEY", "value");
        assert!(!layer.is_empty());
        assert_eq!(layer.len(), 1);
    }

    #[test]
    fn stack_layer_count() {
        let mut stack = EnvLayerStack::new();
        assert_eq!(stack.layer_count(), 0);

        stack.push(EnvLayer::new("layer1"));
        stack.push(EnvLayer::new("layer2"));
        assert_eq!(stack.layer_count(), 2);
    }

    #[test]
    fn default_stack_is_empty() {
        let stack = EnvLayerStack::default();
        assert_eq!(stack.layer_count(), 0);
    }

    #[test]
    fn multiple_overrides() {
        let mut stack = EnvLayerStack::new();

        let mut layer1 = EnvLayer::new("layer1");
        layer1.set("KEY", "value1");
        stack.push(layer1);

        let mut layer2 = EnvLayer::new("layer2");
        layer2.set("KEY", "value2");
        stack.push(layer2);

        let mut layer3 = EnvLayer::new("layer3");
        layer3.set("KEY", "value3");
        stack.push(layer3);

        assert_eq!(stack.get("KEY"), Some("value3"));
        assert_eq!(stack.source_of("KEY"), Some("layer3"));
    }
}
