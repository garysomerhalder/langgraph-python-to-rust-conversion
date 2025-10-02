//! State management for LangGraph execution
//!
//! This module provides state types, reducers, and management utilities
//! for maintaining state across graph execution.

use std::collections::HashMap;
use std::sync::Arc;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use smallvec::SmallVec;

pub mod reducer;
pub mod channel;
pub mod state_channels;
pub mod advanced;
pub mod versioning;
pub mod validation;
pub mod advanced_channels;
pub mod schema;

pub use reducer::{Reducer, ReducerFn, DefaultReducer, AppendReducer};
pub use channel::{Channel, ChannelType};
pub use state_channels::StateChannels;
pub use versioning::{
    StateVersioningSystem, VersionId, Version, StateSnapshot, StateDelta,
    VersionMetadata, VersioningConfig, VersionStorage, InMemoryStorage,
    BranchManager, VersioningMetrics
};
pub use advanced_channels::{
    LastValueChannel, TopicChannel, ContextChannel, CustomReducer,
    ChannelComposer, ChannelType as AdvancedChannelType
};
pub use schema::{
    Schema as ValidationSchema, FieldDefinition, FieldDefinitionBuilder,
    FieldType, StateValidator, SchemaError
};

/// Trait for types that can be used as state in the graph
pub trait State: Send + Sync + Clone + 'static {
    /// Get a value from the state
    fn get_value(&self, key: &str) -> Option<Value>;
    
    /// Set a value in the state
    fn set_value(&mut self, key: String, value: Value);
    
    /// Convert to JSON representation
    fn to_json(&self) -> Value;
}

/// Type alias for state data
pub type StateData = HashMap<String, Value>;

/// Thread-safe state container
pub type SharedState = Arc<DashMap<String, Value>>;

/// Represents the state of a graph execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphState {
    /// Current state values
    pub values: StateData,
    
    /// Execution history (using SmallVec for better performance with small histories)
    pub history: SmallVec<[StateTransition; 8]>,
    
    /// Current node being executed
    pub current_node: Option<String>,
    
    /// Thread ID for conversation tracking
    pub thread_id: Option<String>,
    
    /// Metadata about the execution
    pub metadata: StateMetadata,
}

/// Metadata about the state execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateMetadata {
    /// Timestamp when execution started
    pub started_at: u64,
    
    /// Last update timestamp
    pub updated_at: u64,
    
    /// Number of transitions
    pub transition_count: usize,
    
    /// Custom metadata
    pub custom: Option<Value>,
}

/// Represents a state transition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateTransition {
    /// Source node ID
    pub from: String,
    
    /// Target node ID
    pub to: String,
    
    /// Timestamp of transition
    pub timestamp: u64,
    
    /// State snapshot before transition
    pub state_before: Option<StateData>,
    
    /// State changes applied
    pub changes: Option<StateData>,
    
    /// Optional transition metadata
    pub metadata: Option<Value>,
}

impl GraphState {
    /// Create a new graph state
    pub fn new() -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        Self {
            values: HashMap::new(),
            history: SmallVec::new(),
            current_node: None,
            thread_id: None,
            metadata: StateMetadata {
                started_at: now,
                updated_at: now,
                transition_count: 0,
                custom: None,
            },
        }
    }
    
    /// Create a new graph state with thread ID
    pub fn with_thread_id(thread_id: impl Into<String>) -> Self {
        let mut state = Self::new();
        state.thread_id = Some(thread_id.into());
        state
    }
    
    /// Get a value from the state
    #[inline]
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.values.get(key)
    }
    
    /// Set a value in the state
    #[inline]
    pub fn set(&mut self, key: impl Into<String>, value: Value) {
        self.values.insert(key.into(), value);
        self.update_timestamp();
    }
    
    /// Update multiple values in the state
    pub fn update(&mut self, updates: StateData) {
        for (key, value) in updates {
            self.values.insert(key, value);
        }
        self.update_timestamp();
    }
    
    /// Apply a reducer to merge state updates
    pub fn apply_update(&mut self, key: &str, value: Value, reducer: &dyn Reducer) {
        let existing = self.values.get(key);
        let new_value = reducer.reduce(existing, value);
        self.values.insert(key.to_string(), new_value);
        self.update_timestamp();
    }
    
    /// Record a state transition
    pub fn add_transition(&mut self, from: String, to: String, changes: Option<StateData>) {
        let transition = StateTransition {
            from,
            to,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            state_before: Some(self.values.clone()),
            changes,
            metadata: None,
        };
        
        self.history.push(transition);
        self.metadata.transition_count += 1;
        self.update_timestamp();
    }
    
    /// Update the last modified timestamp
    #[inline]
    fn update_timestamp(&mut self) {
        self.metadata.updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
    }
    
    /// Create a snapshot of the current state
    pub fn snapshot(&self) -> StateData {
        self.values.clone()
    }
    
    /// Restore state from a snapshot
    pub fn restore(&mut self, snapshot: StateData) {
        self.values = snapshot;
        self.update_timestamp();
    }
}

impl Default for GraphState {
    fn default() -> Self {
        Self::new()
    }
}

impl State for GraphState {
    fn get_value(&self, key: &str) -> Option<Value> {
        self.get(key).cloned()
    }
    
    fn set_value(&mut self, key: String, value: Value) {
        self.set(key, value);
    }
    
    fn to_json(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

/// State schema definition
#[derive(Debug, Clone)]
pub struct StateSchema {
    /// Channel definitions
    pub channels: HashMap<String, Channel>,
    
    /// Required channels
    pub required: Vec<String>,
    
    /// Schema metadata
    pub metadata: Option<Value>,
}

impl StateSchema {
    /// Create a new state schema
    pub fn new() -> Self {
        Self {
            channels: HashMap::new(),
            required: Vec::new(),
            metadata: None,
        }
    }
    
    /// Add a channel to the schema
    pub fn add_channel(&mut self, name: impl Into<String>, channel: Channel) {
        let name_str = name.into();
        if channel.required {
            self.required.push(name_str.clone());
        }
        self.channels.insert(name_str, channel);
    }
    
    /// Validate state against schema
    pub fn validate(&self, state: &StateData) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        
        // Check required fields
        for required in &self.required {
            if !state.contains_key(required) {
                errors.push(format!("Missing required field: {}", required));
            }
        }
        
        // Validate types if specified
        for (key, value) in state {
            if let Some(channel) = self.channels.get(key) {
                if !channel.validate_type(value) {
                    errors.push(format!(
                        "Type mismatch for field '{}': expected {:?}",
                        key, channel.channel_type
                    ));
                }
            }
        }
        
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

impl Default for StateSchema {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_graph_state_creation() {
        let state = GraphState::new();
        assert!(state.values.is_empty());
        assert!(state.history.is_empty());
        assert!(state.current_node.is_none());
        assert!(state.thread_id.is_none());
    }
    
    #[test]
    fn test_graph_state_with_thread_id() {
        let state = GraphState::with_thread_id("thread-123");
        assert_eq!(state.thread_id, Some("thread-123".to_string()));
    }
    
    #[test]
    fn test_state_get_set() {
        let mut state = GraphState::new();
        state.set("key1", serde_json::json!("value1"));
        
        assert_eq!(state.get("key1"), Some(&serde_json::json!("value1")));
        assert_eq!(state.get("key2"), None);
    }
    
    #[test]
    fn test_state_update() {
        let mut state = GraphState::new();
        let mut updates = StateData::new();
        updates.insert("key1".to_string(), serde_json::json!("value1"));
        updates.insert("key2".to_string(), serde_json::json!(42));
        
        state.update(updates);
        
        assert_eq!(state.get("key1"), Some(&serde_json::json!("value1")));
        assert_eq!(state.get("key2"), Some(&serde_json::json!(42)));
    }
    
    #[test]
    fn test_state_transition() {
        let mut state = GraphState::new();
        state.set("key1", serde_json::json!("value1"));
        
        let mut changes = StateData::new();
        changes.insert("key2".to_string(), serde_json::json!("value2"));
        
        state.add_transition("node1".to_string(), "node2".to_string(), Some(changes));
        
        assert_eq!(state.history.len(), 1);
        assert_eq!(state.metadata.transition_count, 1);
        
        let transition = &state.history[0];
        assert_eq!(transition.from, "node1");
        assert_eq!(transition.to, "node2");
        assert!(transition.state_before.is_some());
        assert!(transition.changes.is_some());
    }
    
    #[test]
    fn test_state_snapshot_restore() {
        let mut state = GraphState::new();
        state.set("key1", serde_json::json!("value1"));
        state.set("key2", serde_json::json!(42));
        
        let snapshot = state.snapshot();
        
        state.set("key1", serde_json::json!("modified"));
        state.set("key3", serde_json::json!("new"));
        
        state.restore(snapshot);
        
        assert_eq!(state.get("key1"), Some(&serde_json::json!("value1")));
        assert_eq!(state.get("key2"), Some(&serde_json::json!(42)));
        assert_eq!(state.get("key3"), None);
    }
    
    #[test]
    fn test_state_schema_validation() {
        let mut schema = StateSchema::new();
        schema.add_channel("required_field", Channel {
            channel_type: ChannelType::String,
            required: true,
            reducer: None,
            default: None,
            metadata: None,
        });
        
        let mut state = StateData::new();
        
        // Should fail - missing required field
        let result = schema.validate(&state);
        assert!(result.is_err());
        
        // Should pass - required field present
        state.insert("required_field".to_string(), serde_json::json!("value"));
        let result = schema.validate(&state);
        assert!(result.is_ok());
    }
}