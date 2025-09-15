//! Graph data structures and algorithms for LangGraph
//!
//! This module provides the core graph structures including nodes, edges,
//! and the graph itself, along with builder patterns and traversal algorithms.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::state::StateData;
use crate::Result;

pub mod builder;
pub mod node;
pub mod edge;
pub mod command;
pub mod state_graph;
pub mod conditional;
pub mod subgraph;

pub use builder::GraphBuilder;
pub use node::{Node, NodeType, NodeFunction};
pub use edge::{Edge, EdgeType, ConditionalEdge};
pub use command::Command;
pub use state_graph::{StateGraphManager, StateConditionalEdge};
pub use conditional::{ConditionalRouter, ConditionalBranch};
pub use subgraph::{Subgraph, StateMapper, PassthroughMapper, SelectiveMapper, SubgraphBuilder, RecursiveSubgraph};

// Type alias for compatibility
pub type Graph = CompiledGraph;

/// Errors specific to graph operations
#[derive(Error, Debug)]
pub enum GraphError {
    #[error("Node not found: {0}")]
    NodeNotFound(String),
    
    #[error("Edge not found: from {from} to {to}")]
    EdgeNotFound { from: String, to: String },
    
    #[error("Cycle detected in graph")]
    CycleDetected,
    
    #[error("Invalid graph structure: {0}")]
    InvalidStructure(String),
    
    #[error("Orphaned node: {0}")]
    OrphanedNode(String),
    
    #[error("Node error: {0}")]
    NodeError(String),
    
    #[error("Edge error: {0}")]
    EdgeError(String),
    
    #[error("Validation error: {0}")]
    ValidationError(String),
    
    #[error("Runtime error: {0}")]
    RuntimeError(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
}

/// Main graph structure representing a LangGraph workflow
#[derive(Debug, Clone)]
pub struct StateGraph {
    /// The underlying directed graph
    graph: DiGraph<Node, Edge>,
    
    /// Node name to index mapping
    node_map: HashMap<String, NodeIndex>,
    
    /// Entry point of the graph
    entry_point: Option<NodeIndex>,
    
    /// Graph metadata
    metadata: GraphMetadata,
}

/// Metadata associated with a graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphMetadata {
    /// Graph name
    pub name: String,
    
    /// Graph version
    pub version: String,
    
    /// Graph description
    pub description: Option<String>,
    
    /// Additional metadata as JSON
    pub extra: Option<serde_json::Value>,
}

impl StateGraph {
    /// Get node by name
    pub fn get_node(&self, name: &str) -> Option<&Node> {
        self.node_map.get(name).and_then(|idx| self.graph.node_weight(*idx))
    }
    
    /// Get all edges for a node
    pub fn get_edges_from(&self, node_name: &str) -> Vec<(&Node, &Edge)> {
        if let Some(&idx) = self.node_map.get(node_name) {
            self.graph
                .edges(idx)
                .filter_map(|edge| {
                    self.graph.node_weight(edge.target())
                        .map(|target| (target, edge.weight()))
                })
                .collect()
        } else {
            Vec::new()
        }
    }
    
    /// Create a new empty graph
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            graph: DiGraph::new(),
            node_map: HashMap::new(),
            entry_point: None,
            metadata: GraphMetadata {
                name: name.into(),
                version: "0.1.0".to_string(),
                description: None,
                extra: None,
            },
        }
    }
    
    /// Add a node to the graph
    pub fn add_node(&mut self, node: Node) -> NodeIndex {
        let name = node.id.clone();
        let idx = self.graph.add_node(node);
        self.node_map.insert(name, idx);
        idx
    }
    
    /// Add an edge between two nodes
    pub fn add_edge(&mut self, from: &str, to: &str, edge: Edge) -> Result<()> {
        let from_idx = self.node_map.get(from)
            .ok_or_else(|| GraphError::NodeNotFound(from.to_string()))?;
        let to_idx = self.node_map.get(to)
            .ok_or_else(|| GraphError::NodeNotFound(to.to_string()))?;
        
        self.graph.add_edge(*from_idx, *to_idx, edge);
        Ok(())
    }
    
    /// Set the entry point of the graph
    pub fn set_entry_point(&mut self, node_name: &str) -> Result<()> {
        let idx = self.node_map.get(node_name)
            .ok_or_else(|| GraphError::NodeNotFound(node_name.to_string()))?;
        self.entry_point = Some(*idx);
        Ok(())
    }
    
    /// Get a mutable reference to a node by name
    pub fn get_node_mut(&mut self, name: &str) -> Option<&mut Node> {
        self.node_map.get(name)
            .and_then(|idx| self.graph.node_weight_mut(*idx))
    }
    
    /// Get the number of nodes in the graph
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }
    
    /// Check if the graph has cycles
    pub fn has_cycles(&self) -> bool {
        petgraph::algo::is_cyclic_directed(&self.graph)
    }
    
    /// Find all orphaned nodes (nodes with no incoming edges except entry point)
    pub fn find_orphaned_nodes(&self) -> Vec<String> {
        let mut orphaned = Vec::new();
        
        for (name, &idx) in &self.node_map {
            // Skip the entry point
            if Some(idx) == self.entry_point {
                continue;
            }
            
            // Check if node has incoming edges
            let has_incoming = self.graph
                .edges_directed(idx, petgraph::Direction::Incoming)
                .next()
                .is_some();
            
            if !has_incoming {
                orphaned.push(name.clone());
            }
        }
        
        orphaned
    }
    
    /// Validate the graph structure
    pub fn validate(&self) -> Result<()> {
        // Check for entry point
        if self.entry_point.is_none() {
            return Err(GraphError::InvalidStructure("No entry point defined".to_string()).into());
        }
        
        // Check for orphaned nodes
        let orphaned = self.find_orphaned_nodes();
        if !orphaned.is_empty() {
            return Err(GraphError::OrphanedNode(orphaned.join(", ")).into());
        }
        
        // Check for special nodes
        let has_start = self.node_map.contains_key("__start__");
        let has_end = self.node_map.contains_key("__end__");
        
        if !has_start {
            return Err(GraphError::InvalidStructure("Missing __start__ node".to_string()).into());
        }
        
        Ok(())
    }
    
    /// Compile the graph for execution
    pub fn compile(self) -> Result<CompiledGraph> {
        self.validate()?;
        
        Ok(CompiledGraph {
            graph: Arc::new(self),
            checkpointer: None,
        })
    }
}

/// A compiled graph ready for execution
#[derive(Clone)]
pub struct CompiledGraph {
    /// The underlying graph
    graph: Arc<StateGraph>,
    
    /// Optional checkpointer for state persistence
    checkpointer: Option<Arc<dyn crate::checkpoint::Checkpointer>>,
}

impl CompiledGraph {
    /// Get the underlying graph
    pub fn graph(&self) -> &StateGraph {
        &self.graph
    }
    
    /// Execute the graph with given input state
    pub async fn invoke(&self, input: StateData) -> Result<StateData> {
        // TODO: Implement execution logic
        unimplemented!("Graph execution not yet implemented")
    }
    
    /// Stream execution of the graph
    pub async fn stream(&self, _input: StateData) -> Result<futures::stream::Empty<Result<StateData>>> {
        // TODO: Implement streaming execution
        Ok(futures::stream::empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_graph_creation() {
        let graph = StateGraph::new("test_graph");
        assert_eq!(graph.metadata.name, "test_graph");
        assert_eq!(graph.metadata.version, "0.1.0");
    }
    
    #[test]
    fn test_add_node() {
        let mut graph = StateGraph::new("test_graph");
        let node = Node {
            id: "test_node".to_string(),
            node_type: NodeType::Start,
            metadata: None,
        };
        
        let idx = graph.add_node(node);
        assert!(graph.get_node("test_node").is_some());
    }
    
    #[test]
    fn test_add_edge() {
        let mut graph = StateGraph::new("test_graph");
        
        let node1 = Node {
            id: "node1".to_string(),
            node_type: NodeType::Start,
            metadata: None,
        };
        
        let node2 = Node {
            id: "node2".to_string(),
            node_type: NodeType::End,
            metadata: None,
        };
        
        graph.add_node(node1);
        graph.add_node(node2);
        
        let edge = Edge {
            edge_type: EdgeType::Direct,
            metadata: None,
        };
        
        assert!(graph.add_edge("node1", "node2", edge).is_ok());
    }
    
    #[test]
    fn test_validation_fails_without_start() {
        let graph = StateGraph::new("test_graph");
        assert!(graph.validate().is_err());
    }
}