//! Builder pattern for constructing graphs

use crate::graph::{Edge, Node, NodeType, StateGraph};
use crate::Result;

/// Builder for constructing a StateGraph
pub struct GraphBuilder {
    graph: StateGraph,
    pending_edges: Vec<PendingEdge>,
}

/// Represents an edge to be added
struct PendingEdge {
    from: String,
    to: String,
    edge: Edge,
}

impl GraphBuilder {
    /// Create a new graph builder
    pub fn new(name: impl Into<String>) -> Self {
        let mut graph = StateGraph::new(name);
        
        // Add special nodes
        graph.add_node(Node {
            id: "__start__".to_string(),
            node_type: NodeType::Start,
            metadata: None,
        });
        
        graph.add_node(Node {
            id: "__end__".to_string(),
            node_type: NodeType::End,
            metadata: None,
        });
        
        Self {
            graph,
            pending_edges: Vec::new(),
        }
    }
    
    /// Add a node to the graph
    pub fn add_node(mut self, id: impl Into<String>, node_type: NodeType) -> Self {
        let node = Node {
            id: id.into(),
            node_type,
            metadata: None,
        };
        self.graph.add_node(node);
        self
    }
    
    /// Add a node with metadata
    pub fn add_node_with_metadata(
        mut self,
        id: impl Into<String>,
        node_type: NodeType,
        metadata: serde_json::Value,
    ) -> Self {
        let node = Node {
            id: id.into(),
            node_type,
            metadata: Some(metadata),
        };
        self.graph.add_node(node);
        self
    }
    
    /// Add an edge between two nodes
    pub fn add_edge(mut self, from: impl Into<String>, to: impl Into<String>) -> Self {
        self.pending_edges.push(PendingEdge {
            from: from.into(),
            to: to.into(),
            edge: Edge::direct(),
        });
        self
    }
    
    /// Add a conditional edge
    pub fn add_conditional_edge(
        mut self,
        from: impl Into<String>,
        condition: String,
        to: impl Into<String>,
    ) -> Self {
        let to_str = to.into();
        self.pending_edges.push(PendingEdge {
            from: from.into(),
            to: to_str.clone(),
            edge: Edge::conditional(condition, to_str),
        });
        self
    }
    
    /// Add a conditional edge with fallback
    pub fn add_conditional_edge_with_fallback(
        mut self,
        from: impl Into<String>,
        condition: String,
        to: impl Into<String>,
        fallback: impl Into<String>,
    ) -> Self {
        let to_str = to.into();
        self.pending_edges.push(PendingEdge {
            from: from.into(),
            to: to_str.clone(),
            edge: Edge::conditional_with_fallback(condition, to_str, fallback.into()),
        });
        self
    }
    
    /// Set the entry point of the graph
    pub fn set_entry_point(mut self, node: impl Into<String>) -> Self {
        let node_str = node.into();
        
        // Add edge from __start__ to entry point
        self.pending_edges.push(PendingEdge {
            from: "__start__".to_string(),
            to: node_str.clone(),
            edge: Edge::direct(),
        });
        
        self
    }
    
    /// Set graph metadata
    pub fn with_metadata(mut self, key: &str, value: serde_json::Value) -> Self {
        if self.graph.metadata.extra.is_none() {
            self.graph.metadata.extra = Some(serde_json::json!({}));
        }
        
        if let Some(extra) = self.graph.metadata.extra.as_mut() {
            if let Some(obj) = extra.as_object_mut() {
                obj.insert(key.to_string(), value);
            }
        }
        
        self
    }
    
    /// Set graph description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.graph.metadata.description = Some(description.into());
        self
    }
    
    /// Set graph version
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.graph.metadata.version = version.into();
        self
    }
    
    /// Build the graph
    pub fn build(mut self) -> Result<StateGraph> {
        // Add all pending edges
        for pending in self.pending_edges {
            self.graph.add_edge(&pending.from, &pending.to, pending.edge)?;
        }
        
        // Set entry point if we have edges from __start__
        if let Some(&start_idx) = self.graph.node_map.get("__start__") {
            use petgraph::Direction;
            let neighbors: Vec<_> = self.graph.graph.neighbors_directed(start_idx, Direction::Outgoing).collect();
            if let Some(&target_idx) = neighbors.first() {
                let target_node_id = self.graph.graph.node_weight(target_idx)
                    .map(|node| node.id.clone());
                if let Some(node_id) = target_node_id {
                    self.graph.set_entry_point(&node_id)?;
                }
            }
        }
        
        Ok(self.graph)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_builder_basic() {
        let graph = GraphBuilder::new("test_graph")
            .add_node("node1", NodeType::Agent("agent1".to_string()))
            .add_node("node2", NodeType::Agent("agent2".to_string()))
            .add_edge("node1", "node2")
            .set_entry_point("node1")
            .build()
            .unwrap();
        
        assert!(graph.get_node("node1").is_some());
        assert!(graph.get_node("node2").is_some());
        assert!(graph.get_node("__start__").is_some());
        assert!(graph.get_node("__end__").is_some());
    }
    
    #[test]
    fn test_builder_with_metadata() {
        let graph = GraphBuilder::new("test_graph")
            .with_description("Test graph description")
            .with_version("1.0.0")
            .with_metadata("author", serde_json::json!("test_author"))
            .add_node("node1", NodeType::Agent("agent1".to_string()))
            .set_entry_point("node1")
            .build()
            .unwrap();
        
        assert_eq!(graph.metadata.description, Some("Test graph description".to_string()));
        assert_eq!(graph.metadata.version, "1.0.0");
        assert!(graph.metadata.extra.is_some());
    }
    
    #[test]
    fn test_builder_conditional_edges() {
        let graph = GraphBuilder::new("test_graph")
            .add_node("node1", NodeType::Agent("agent1".to_string()))
            .add_node("node2", NodeType::Agent("agent2".to_string()))
            .add_node("node3", NodeType::Agent("agent3".to_string()))
            .add_conditional_edge_with_fallback(
                "node1",
                "check_condition".to_string(),
                "node2",
                "node3"
            )
            .set_entry_point("node1")
            .build()
            .unwrap();
        
        assert!(graph.get_node("node1").is_some());
        assert!(graph.get_node("node2").is_some());
        assert!(graph.get_node("node3").is_some());
    }
}