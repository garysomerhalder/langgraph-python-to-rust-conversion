//! Conditional edge implementation for dynamic graph routing

use std::sync::Arc;
use serde::{Deserialize, Serialize};
use crate::state::{StateData, GraphState};
use crate::graph::{GraphError, Node};
use crate::Result;

/// Type alias for conditional functions
pub type ConditionFn = Arc<dyn Fn(&StateData) -> bool + Send + Sync>;

/// Type alias for routing functions that determine next node
pub type RoutingFn = Arc<dyn Fn(&StateData) -> String + Send + Sync>;

/// A conditional edge that routes based on state evaluation
#[derive(Clone)]
pub struct ConditionalEdge {
    /// Source node ID
    pub from: String,
    
    /// Possible target nodes with their conditions
    pub branches: Vec<ConditionalBranch>,
    
    /// Default target if no conditions match
    pub default: Option<String>,
}

/// A single branch in a conditional edge
#[derive(Clone)]
pub struct ConditionalBranch {
    /// Target node ID
    pub target: String,
    
    /// Condition that must be true to take this branch
    pub condition: ConditionFn,
    
    /// Optional priority (higher = evaluated first)
    pub priority: Option<i32>,
    
    /// Optional metadata
    pub metadata: Option<serde_json::Value>,
}

impl ConditionalEdge {
    /// Create a new conditional edge
    pub fn new(from: impl Into<String>) -> Self {
        Self {
            from: from.into(),
            branches: Vec::new(),
            default: None,
        }
    }
    
    /// Add a conditional branch
    pub fn add_branch(
        mut self,
        to: impl Into<String>,
        condition: ConditionFn,
        priority: i32,
    ) -> Self {
        self.branches.push(ConditionalBranch {
            target: to.into(),
            condition,
            priority: Some(priority),
            metadata: None,
        });
        // Sort branches by priority (highest first)
        self.branches.sort_by(|a, b| {
            let p_a = a.priority.unwrap_or(0);
            let p_b = b.priority.unwrap_or(0);
            p_b.cmp(&p_a)
        });
        self
    }
    
    /// Set the default target
    pub fn with_default(mut self, default: impl Into<String>) -> Self {
        self.default = Some(default.into());
        self
    }
    
    /// Evaluate the condition and return the target node
    pub fn evaluate(&self, state: &StateData) -> Result<String> {
        // Check each branch in priority order
        for branch in &self.branches {
            if (branch.condition)(state) {
                return Ok(branch.target.clone());
            }
        }
        
        // Use default if no conditions matched
        self.default
            .clone()
            .ok_or_else(|| GraphError::EdgeError(
                format!("No matching condition for edge from {}", self.from)
            ).into())
    }
}

/// Builder for conditional routing
pub struct ConditionalRouter {
    /// Source node for this router
    pub from: String,
    edges: Vec<ConditionalEdge>,
}

impl ConditionalRouter {
    /// Create a new conditional router
    pub fn new(from: impl Into<String>) -> Self {
        Self {
            from: from.into(),
            edges: Vec::new(),
        }
    }
    
    /// Add a conditional edge
    pub fn add_edge(mut self, edge: ConditionalEdge) -> Self {
        self.edges.push(edge);
        self
    }
    
    /// Route from a given node based on state
    pub fn route(&self, from: &str, state: &StateData) -> Result<String> {
        self.edges
            .iter()
            .find(|e| e.from == from)
            .ok_or_else(|| GraphError::EdgeError(
                format!("No conditional edge from {}", from)
            ).into())
            .and_then(|edge| edge.evaluate(state))
    }
    
    /// Route with specific branches (for testing)
    pub fn route_with_branches(&self, state: &StateData, branches: Vec<ConditionalBranch>) -> Option<String> {
        for branch in branches {
            if (branch.condition)(state) {
                return Some(branch.target);
            }
        }
        None
    }
}

/// Advanced routing with multiple conditions
pub struct MultiConditionalRouter {
    /// Routing function that returns next node based on state
    router: RoutingFn,
    
    /// Valid target nodes
    valid_targets: Vec<String>,
}

impl MultiConditionalRouter {
    /// Create a new multi-conditional router
    pub fn new(router: RoutingFn, valid_targets: Vec<String>) -> Self {
        Self {
            router,
            valid_targets,
        }
    }
    
    /// Route to next node
    pub fn route(&self, state: &StateData) -> Result<String> {
        let target = (self.router)(state);
        
        if self.valid_targets.contains(&target) {
            Ok(target)
        } else {
            Err(GraphError::EdgeError(
                format!("Invalid routing target: {}", target)
            ).into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    
    #[test]
    fn test_conditional_edge() {
        let mut state = StateData::new();
        state.insert("score".to_string(), json!(75));
        
        let edge = ConditionalEdge::new("start")
            .add_branch(
                "high_score",
                Arc::new(|s| {
                    s.get("score")
                        .and_then(|v| v.as_i64())
                        .map(|score| score >= 80)
                        .unwrap_or(false)
                }),
                10,
            )
            .add_branch(
                "medium_score",
                Arc::new(|s| {
                    s.get("score")
                        .and_then(|v| v.as_i64())
                        .map(|score| score >= 60 && score < 80)
                        .unwrap_or(false)
                }),
                5,
            )
            .with_default("low_score");
        
        let target = edge.evaluate(&state).unwrap();
        assert_eq!(target, "medium_score");
    }
    
    #[test]
    fn test_conditional_router() {
        let mut state = StateData::new();
        state.insert("type".to_string(), json!("question"));
        
        let router = ConditionalRouter::new("start")
            .add_edge(
                ConditionalEdge::new("classifier")
                    .add_branch(
                        "question_handler",
                        Arc::new(|s| {
                            s.get("type")
                                .and_then(|v| v.as_str())
                                .map(|t| t == "question")
                                .unwrap_or(false)
                        }),
                        10,
                    )
                    .add_branch(
                        "statement_handler",
                        Arc::new(|s| {
                            s.get("type")
                                .and_then(|v| v.as_str())
                                .map(|t| t == "statement")
                                .unwrap_or(false)
                        }),
                        5,
                    )
                    .with_default("unknown_handler")
            );
        
        let target = router.route("classifier", &state).unwrap();
        assert_eq!(target, "question_handler");
    }
}