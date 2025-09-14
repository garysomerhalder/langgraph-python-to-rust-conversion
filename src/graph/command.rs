//! Command pattern for combined state updates and control flow

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

use crate::state::StateData;

/// Command object that combines state updates with routing decisions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    /// State updates to apply
    pub update: Option<StateData>,
    
    /// Next node to execute
    pub goto: Option<String>,
    
    /// Multiple nodes for parallel execution
    pub goto_multiple: Option<Vec<String>>,
    
    /// Navigate to parent graph
    pub graph: Option<GraphTarget>,
    
    /// Additional metadata
    pub metadata: Option<serde_json::Value>,
}

/// Target graph for navigation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum GraphTarget {
    /// Current graph
    Current,
    
    /// Parent graph
    Parent,
    
    /// Named subgraph
    Subgraph(String),
}

impl Command {
    /// Create a new empty command
    pub fn new() -> Self {
        Self {
            update: None,
            goto: None,
            goto_multiple: None,
            graph: None,
            metadata: None,
        }
    }
    
    /// Create a command with state update
    pub fn with_update(update: StateData) -> Self {
        Self {
            update: Some(update),
            goto: None,
            goto_multiple: None,
            graph: None,
            metadata: None,
        }
    }
    
    /// Create a command with goto
    pub fn with_goto(goto: impl Into<String>) -> Self {
        Self {
            update: None,
            goto: Some(goto.into()),
            goto_multiple: None,
            graph: None,
            metadata: None,
        }
    }
    
    /// Create a command with both update and goto
    pub fn with_update_and_goto(update: StateData, goto: impl Into<String>) -> Self {
        Self {
            update: Some(update),
            goto: Some(goto.into()),
            goto_multiple: None,
            graph: None,
            metadata: None,
        }
    }
    
    /// Create a command for parallel execution
    pub fn parallel(nodes: Vec<String>) -> Self {
        Self {
            update: None,
            goto: None,
            goto_multiple: Some(nodes),
            graph: None,
            metadata: None,
        }
    }
    
    /// Navigate to parent graph
    pub fn to_parent() -> Self {
        Self {
            update: None,
            goto: None,
            goto_multiple: None,
            graph: Some(GraphTarget::Parent),
            metadata: None,
        }
    }
    
    /// Add state update to command
    pub fn update(mut self, update: StateData) -> Self {
        self.update = Some(update);
        self
    }
    
    /// Add goto to command
    pub fn goto(mut self, node: impl Into<String>) -> Self {
        self.goto = Some(node.into());
        self
    }
    
    /// Add parallel nodes to command
    pub fn goto_multiple(mut self, nodes: Vec<String>) -> Self {
        self.goto_multiple = Some(nodes);
        self
    }
    
    /// Set graph target
    pub fn set_graph(mut self, target: GraphTarget) -> Self {
        self.graph = Some(target);
        self
    }
    
    /// Add metadata to command
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

impl Default for Command {
    fn default() -> Self {
        Self::new()
    }
}

/// Result type for node functions that return commands
pub type CommandResult = Result<Command, crate::LangGraphError>;

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_command_creation() {
        let cmd = Command::new();
        assert!(cmd.update.is_none());
        assert!(cmd.goto.is_none());
        assert!(cmd.goto_multiple.is_none());
    }
    
    #[test]
    fn test_command_with_update() {
        let mut state = StateData::new();
        state.insert("key".to_string(), serde_json::json!("value"));
        
        let cmd = Command::with_update(state.clone());
        assert_eq!(cmd.update, Some(state));
        assert!(cmd.goto.is_none());
    }
    
    #[test]
    fn test_command_with_goto() {
        let cmd = Command::with_goto("next_node");
        assert!(cmd.update.is_none());
        assert_eq!(cmd.goto, Some("next_node".to_string()));
    }
    
    #[test]
    fn test_command_with_update_and_goto() {
        let mut state = StateData::new();
        state.insert("key".to_string(), serde_json::json!("value"));
        
        let cmd = Command::with_update_and_goto(state.clone(), "next_node");
        assert_eq!(cmd.update, Some(state));
        assert_eq!(cmd.goto, Some("next_node".to_string()));
    }
    
    #[test]
    fn test_command_parallel() {
        let nodes = vec!["node1".to_string(), "node2".to_string()];
        let cmd = Command::parallel(nodes.clone());
        assert_eq!(cmd.goto_multiple, Some(nodes));
    }
    
    #[test]
    fn test_command_builder() {
        let mut state = StateData::new();
        state.insert("key".to_string(), serde_json::json!("value"));
        
        let cmd = Command::new()
            .update(state.clone())
            .goto("next_node")
            .with_metadata(serde_json::json!({"priority": 1}));
        
        assert_eq!(cmd.update, Some(state));
        assert_eq!(cmd.goto, Some("next_node".to_string()));
        assert!(cmd.metadata.is_some());
    }
}