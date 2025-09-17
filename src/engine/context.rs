//! Execution context management

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};
use serde::{Deserialize, Serialize};

use crate::state::StateData;
use crate::engine::executor::ExecutionMessage;

/// Shared execution context for all nodes
#[derive(Clone)]
pub struct SharedContext {
    /// Global state accessible to all nodes
    pub global_state: Arc<RwLock<StateData>>,
    
    /// Node-specific state
    pub node_states: Arc<RwLock<HashMap<String, StateData>>>,
    
    /// Shared configuration
    pub config: Arc<ContextConfig>,
    
    /// Message bus for inter-node communication
    pub message_bus: Arc<MessageBus>,
}

/// Configuration for execution context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextConfig {
    /// Maximum execution time in seconds
    pub timeout_secs: u64,
    
    /// Enable debug logging
    pub debug: bool,
    
    /// Maximum parallel nodes
    pub max_parallel: usize,
    
    /// Retry configuration
    pub retry_config: RetryConfig,
    
    /// Custom configuration values
    pub custom: HashMap<String, serde_json::Value>,
}

/// Retry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retries
    pub max_retries: u32,
    
    /// Initial retry delay in milliseconds
    pub initial_delay_ms: u64,
    
    /// Backoff multiplier
    pub backoff_multiplier: f64,
    
    /// Maximum delay between retries
    pub max_delay_ms: u64,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 300, // 5 minutes
            debug: false,
            max_parallel: 10,
            retry_config: RetryConfig::default(),
            custom: HashMap::new(),
        }
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay_ms: 100,
            backoff_multiplier: 2.0,
            max_delay_ms: 10000,
        }
    }
}

/// Message bus for inter-node communication
pub struct MessageBus {
    /// Channels for each node
    channels: RwLock<HashMap<String, mpsc::Sender<ExecutionMessage>>>,
    
    /// Broadcast channel for all nodes
    broadcast: mpsc::Sender<ExecutionMessage>,
}

impl MessageBus {
    /// Create a new message bus
    pub fn new() -> Self {
        let (broadcast_tx, _) = mpsc::channel(1000);
        
        Self {
            channels: RwLock::new(HashMap::new()),
            broadcast: broadcast_tx,
        }
    }
    
    /// Register a node with the message bus
    pub async fn register_node(&self, node_id: String) -> mpsc::Receiver<ExecutionMessage> {
        let (tx, rx) = mpsc::channel(100);
        
        let mut channels = self.channels.write().await;
        channels.insert(node_id.clone(), tx);
        
        rx
    }
    
    /// Send a message to a specific node
    pub async fn send(&self, message: ExecutionMessage) -> Result<(), String> {
        let channels = self.channels.read().await;
        
        if let Some(tx) = channels.get(&message.to) {
            tx.send(message).await.map_err(|e| e.to_string())?;
            Ok(())
        } else {
            Err(format!("Node {} not registered", message.to))
        }
    }
    
    /// Broadcast a message to all nodes
    pub async fn broadcast(&self, message: ExecutionMessage) -> Result<(), String> {
        self.broadcast.send(message).await.map_err(|e| e.to_string())
    }
}

impl SharedContext {
    /// Create a new shared context
    pub fn new(config: ContextConfig) -> Self {
        Self {
            global_state: Arc::new(RwLock::new(HashMap::new())),
            node_states: Arc::new(RwLock::new(HashMap::new())),
            config: Arc::new(config),
            message_bus: Arc::new(MessageBus::new()),
        }
    }
    
    /// Get global state value
    pub async fn get_global(&self, key: &str) -> Option<serde_json::Value> {
        let state = self.global_state.read().await;
        state.get(key).cloned()
    }
    
    /// Set global state value
    pub async fn set_global(&self, key: String, value: serde_json::Value) {
        let mut state = self.global_state.write().await;
        state.insert(key, value);
    }
    
    /// Get node-specific state
    pub async fn get_node_state(&self, node_id: &str) -> Option<StateData> {
        let states = self.node_states.read().await;
        states.get(node_id).cloned()
    }
    
    /// Set node-specific state
    pub async fn set_node_state(&self, node_id: String, state: StateData) {
        let mut states = self.node_states.write().await;
        states.insert(node_id, state);
    }
    
    /// Merge node state with global state
    pub async fn merge_to_global(&self, node_id: &str) {
        if let Some(node_state) = self.get_node_state(node_id).await {
            let mut global = self.global_state.write().await;
            for (key, value) in node_state {
                global.insert(key, value);
            }
        }
    }
}

/// Execution scope for managing variable visibility
pub struct ExecutionScope {
    /// Stack of scopes
    scopes: Vec<HashMap<String, serde_json::Value>>,
}

impl ExecutionScope {
    /// Create a new execution scope
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
        }
    }
    
    /// Push a new scope
    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }
    
    /// Pop the current scope
    pub fn pop_scope(&mut self) -> Option<HashMap<String, serde_json::Value>> {
        if self.scopes.len() > 1 {
            self.scopes.pop()
        } else {
            None
        }
    }
    
    /// Get a value from the scope chain
    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        for scope in self.scopes.iter().rev() {
            if let Some(value) = scope.get(key) {
                return Some(value);
            }
        }
        None
    }
    
    /// Set a value in the current scope
    pub fn set(&mut self, key: String, value: serde_json::Value) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(key, value);
        }
    }
    
    /// Get all values in the current scope chain
    pub fn flatten(&self) -> HashMap<String, serde_json::Value> {
        let mut result = HashMap::new();
        for scope in &self.scopes {
            for (key, value) in scope {
                result.insert(key.clone(), value.clone());
            }
        }
        result
    }
}

impl Default for ExecutionScope {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_shared_context() {
        let context = SharedContext::new(ContextConfig::default());
        
        // Test global state
        context.set_global("test_key".to_string(), serde_json::json!("test_value")).await;
        let value = context.get_global("test_key").await;
        assert_eq!(value, Some(serde_json::json!("test_value")));
        
        // Test node state
        let mut node_state = HashMap::new();
        node_state.insert("node_key".to_string(), serde_json::json!(42));
        context.set_node_state("node1".to_string(), node_state).await;
        
        let retrieved = context.get_node_state("node1").await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().get("node_key"), Some(&serde_json::json!(42)));
    }
    
    #[test]
    fn test_execution_scope() {
        let mut scope = ExecutionScope::new();
        
        // Set in root scope
        scope.set("var1".to_string(), serde_json::json!("value1"));
        assert_eq!(scope.get("var1"), Some(&serde_json::json!("value1")));
        
        // Push new scope
        scope.push_scope();
        scope.set("var2".to_string(), serde_json::json!("value2"));
        
        // Both should be accessible
        assert_eq!(scope.get("var1"), Some(&serde_json::json!("value1")));
        assert_eq!(scope.get("var2"), Some(&serde_json::json!("value2")));
        
        // Pop scope
        scope.pop_scope();
        
        // Only var1 should remain
        assert_eq!(scope.get("var1"), Some(&serde_json::json!("value1")));
        assert_eq!(scope.get("var2"), None);
    }
}