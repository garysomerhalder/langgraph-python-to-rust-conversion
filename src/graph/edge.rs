//! Edge types and implementations for LangGraph

use serde::{Deserialize, Serialize};

/// Represents an edge in the graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    /// Type of the edge
    pub edge_type: EdgeType,

    /// Optional metadata
    pub metadata: Option<serde_json::Value>,
}

/// Types of edges supported in the graph
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EdgeType {
    /// Direct edge - always traversed
    Direct,

    /// Conditional edge - traversed based on condition
    Conditional(ConditionalEdge),

    /// Parallel edge - for parallel execution
    Parallel,
}

/// Conditional edge with routing logic
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConditionalEdge {
    /// Condition name or expression
    pub condition: String,

    /// Target node if condition is true
    pub target: String,

    /// Optional fallback target
    pub fallback: Option<String>,
}

impl Edge {
    /// Create a direct edge
    pub fn direct() -> Self {
        Self {
            edge_type: EdgeType::Direct,
            metadata: None,
        }
    }

    /// Create a conditional edge
    pub fn conditional(condition: String, target: String) -> Self {
        Self {
            edge_type: EdgeType::Conditional(ConditionalEdge {
                condition,
                target,
                fallback: None,
            }),
            metadata: None,
        }
    }

    /// Create a conditional edge with fallback
    pub fn conditional_with_fallback(condition: String, target: String, fallback: String) -> Self {
        Self {
            edge_type: EdgeType::Conditional(ConditionalEdge {
                condition,
                target,
                fallback: Some(fallback),
            }),
            metadata: None,
        }
    }

    /// Create a parallel edge
    pub fn parallel() -> Self {
        Self {
            edge_type: EdgeType::Parallel,
            metadata: None,
        }
    }

    /// Add metadata to the edge
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_direct_edge() {
        let edge = Edge::direct();
        assert_eq!(edge.edge_type, EdgeType::Direct);
        assert!(edge.metadata.is_none());
    }

    #[test]
    fn test_conditional_edge() {
        let edge = Edge::conditional("check_status".to_string(), "next_node".to_string());

        match edge.edge_type {
            EdgeType::Conditional(ref cond) => {
                assert_eq!(cond.condition, "check_status");
                assert_eq!(cond.target, "next_node");
                assert!(cond.fallback.is_none());
            }
            _ => panic!("Expected conditional edge"),
        }
    }

    #[test]
    fn test_conditional_edge_with_fallback() {
        let edge = Edge::conditional_with_fallback(
            "check_status".to_string(),
            "success_node".to_string(),
            "error_node".to_string(),
        );

        match edge.edge_type {
            EdgeType::Conditional(ref cond) => {
                assert_eq!(cond.condition, "check_status");
                assert_eq!(cond.target, "success_node");
                assert_eq!(cond.fallback, Some("error_node".to_string()));
            }
            _ => panic!("Expected conditional edge"),
        }
    }

    #[test]
    fn test_edge_with_metadata() {
        let edge = Edge::direct()
            .with_metadata(serde_json::json!({"priority": 1, "description": "Main flow"}));

        assert_eq!(edge.edge_type, EdgeType::Direct);
        assert!(edge.metadata.is_some());

        let metadata = edge.metadata.unwrap();
        assert_eq!(metadata["priority"], 1);
        assert_eq!(metadata["description"], "Main flow");
    }
}
