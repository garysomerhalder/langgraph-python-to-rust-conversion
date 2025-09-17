//! Utility functions for LangGraph operations

use crate::state::StateData;
use crate::Result;
use serde_json::Value;
use std::collections::HashMap;

pub mod object_pool;
pub mod zero_copy;

/// State manipulation utilities
pub mod state {
    use super::*;
    
    /// Deep merge two state objects
    pub fn deep_merge(base: &StateData, overlay: &StateData) -> StateData {
        let mut result = base.clone();
        
        for (key, value) in overlay {
            match (base.get(key), value) {
                (Some(Value::Object(base_obj)), Value::Object(overlay_obj)) => {
                    let mut merged = base_obj.clone();
                    for (k, v) in overlay_obj {
                        merged.insert(k.clone(), v.clone());
                    }
                    result.insert(key.clone(), Value::Object(merged));
                }
                (Some(Value::Array(base_arr)), Value::Array(overlay_arr)) => {
                    let mut merged = base_arr.clone();
                    merged.extend(overlay_arr.clone());
                    result.insert(key.clone(), Value::Array(merged));
                }
                _ => {
                    result.insert(key.clone(), value.clone());
                }
            }
        }
        
        result
    }
    
    /// Extract a subset of state based on keys
    pub fn extract_subset(state: &StateData, keys: &[&str]) -> StateData {
        let mut subset = StateData::new();
        
        for key in keys {
            if let Some(value) = state.get(*key) {
                subset.insert(key.to_string(), value.clone());
            }
        }
        
        subset
    }
    
    /// Transform state values using a mapping function
    pub fn transform_values<F>(state: &StateData, transform: F) -> StateData
    where
        F: Fn(&str, &Value) -> Value,
    {
        let mut transformed = StateData::new();
        
        for (key, value) in state {
            transformed.insert(key.clone(), transform(key, value));
        }
        
        transformed
    }
    
    /// Validate state against a schema
    pub fn validate_schema(state: &StateData, schema: &HashMap<String, String>) -> Result<()> {
        for (key, expected_type) in schema {
            match state.get(key) {
                None => {
                    return Err(crate::LangGraphError::StateError(
                        format!("Missing required field: {}", key)
                    ));
                }
                Some(value) => {
                    let actual_type = match value {
                        Value::Null => "null",
                        Value::Bool(_) => "boolean",
                        Value::Number(_) => "number",
                        Value::String(_) => "string",
                        Value::Array(_) => "array",
                        Value::Object(_) => "object",
                    };
                    
                    if actual_type != expected_type {
                        return Err(crate::LangGraphError::StateError(
                            format!(
                                "Type mismatch for field '{}': expected {}, got {}",
                                key, expected_type, actual_type
                            )
                        ));
                    }
                }
            }
        }
        
        Ok(())
    }
}

/// Testing utilities
pub mod testing {
    use super::*;
    use crate::graph::{StateGraph, NodeType};
    
    /// Create a mock state with test data
    pub fn create_test_state(data: Vec<(&str, Value)>) -> StateData {
        let mut state = StateData::new();
        for (key, value) in data {
            state.insert(key.to_string(), value);
        }
        state
    }
    
    /// Assert state contains expected values
    pub fn assert_state_contains(state: &StateData, expected: &[(&str, Value)]) {
        for (key, expected_value) in expected {
            let actual = state.get(*key)
                .expect(&format!("State missing key: {}", key));
            assert_eq!(actual, expected_value, "Value mismatch for key: {}", key);
        }
    }
    
    /// Create a simple test graph
    pub fn create_simple_test_graph() -> StateGraph {
        use crate::graph::Node;
        
        let mut graph = StateGraph::new("test_graph");
        
        let process_node = Node {
            id: "process".to_string(),
            node_type: NodeType::Agent("processor".to_string()),
            metadata: None,
        };
        
        graph.add_node(process_node);
        
        // Note: add_edge also needs proper implementation
        // For now, return the graph with just the node
        
        graph
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    
    #[test]
    fn test_deep_merge() {
        let mut base = StateData::new();
        base.insert("a".to_string(), json!(1));
        base.insert("b".to_string(), json!({"x": 1}));
        
        let mut overlay = StateData::new();
        overlay.insert("b".to_string(), json!({"y": 2}));
        overlay.insert("c".to_string(), json!(3));
        
        let merged = state::deep_merge(&base, &overlay);
        assert_eq!(merged.get("a"), Some(&json!(1)));
        assert_eq!(merged.get("c"), Some(&json!(3)));
        
        let b_value = merged.get("b").unwrap();
        assert!(b_value.is_object());
    }
    
    #[test]
    fn test_extract_subset() {
        let mut state = StateData::new();
        state.insert("a".to_string(), json!(1));
        state.insert("b".to_string(), json!(2));
        state.insert("c".to_string(), json!(3));
        
        let subset = state::extract_subset(&state, &["a", "c"]);
        assert_eq!(subset.len(), 2);
        assert_eq!(subset.get("a"), Some(&json!(1)));
        assert_eq!(subset.get("c"), Some(&json!(3)));
        assert_eq!(subset.get("b"), None);
    }
    
    #[test]
    fn test_validate_schema() {
        let mut state = StateData::new();
        state.insert("name".to_string(), json!("Alice"));
        state.insert("age".to_string(), json!(30));
        state.insert("active".to_string(), json!(true));
        
        let mut schema = HashMap::new();
        schema.insert("name".to_string(), "string".to_string());
        schema.insert("age".to_string(), "number".to_string());
        schema.insert("active".to_string(), "boolean".to_string());
        
        assert!(state::validate_schema(&state, &schema).is_ok());
        
        // Test missing field
        schema.insert("missing".to_string(), "string".to_string());
        assert!(state::validate_schema(&state, &schema).is_err());
    }
}