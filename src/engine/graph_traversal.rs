//! Graph traversal algorithms for execution

use std::collections::{HashMap, HashSet, VecDeque};
use crate::graph::{CompiledGraph, Node, NodeType, EdgeType};
use crate::state::StateData;
use crate::Result;
use crate::engine::ExecutionError;

/// Graph traversal strategy
#[derive(Debug, Clone)]
pub enum TraversalStrategy {
    /// Breadth-first traversal
    BreadthFirst,
    /// Depth-first traversal
    DepthFirst,
    /// Topological order (for DAGs)
    Topological,
    /// Priority-based traversal
    Priority,
}

/// Graph traverser for executing nodes in order
pub struct GraphTraverser {
    strategy: TraversalStrategy,
}

impl GraphTraverser {
    /// Create a new graph traverser
    pub fn new(strategy: TraversalStrategy) -> Self {
        Self { strategy }
    }
    
    /// Get the execution order for a graph
    pub fn get_execution_order(&self, graph: &CompiledGraph) -> Result<Vec<String>> {
        match self.strategy {
            TraversalStrategy::BreadthFirst => self.breadth_first_order(graph),
            TraversalStrategy::DepthFirst => self.depth_first_order(graph),
            TraversalStrategy::Topological => self.topological_order(graph),
            TraversalStrategy::Priority => self.priority_order(graph),
        }
    }
    
    /// Breadth-first traversal order
    fn breadth_first_order(&self, graph: &CompiledGraph) -> Result<Vec<String>> {
        let mut order = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        
        // Start from __start__ node
        queue.push_back("__start__".to_string());
        
        while let Some(node_id) = queue.pop_front() {
            if visited.contains(&node_id) {
                continue;
            }
            
            visited.insert(node_id.clone());
            order.push(node_id.clone());
            
            // Add all neighbors to queue
            let edges = graph.graph().get_edges_from(&node_id);
            for (next_node, _) in edges {
                if !visited.contains(&next_node.id) {
                    queue.push_back(next_node.id.clone());
                }
            }
        }
        
        Ok(order)
    }
    
    /// Depth-first traversal order
    fn depth_first_order(&self, graph: &CompiledGraph) -> Result<Vec<String>> {
        let mut order = Vec::new();
        let mut visited = HashSet::new();
        
        self.dfs_visit(graph, "__start__", &mut visited, &mut order)?;
        
        Ok(order)
    }
    
    /// DFS visit helper
    fn dfs_visit(
        &self,
        graph: &CompiledGraph,
        node_id: &str,
        visited: &mut HashSet<String>,
        order: &mut Vec<String>,
    ) -> Result<()> {
        if visited.contains(node_id) {
            return Ok(());
        }
        
        visited.insert(node_id.to_string());
        order.push(node_id.to_string());
        
        // Visit all neighbors
        let edges = graph.graph().get_edges_from(node_id);
        for (next_node, _) in edges {
            self.dfs_visit(graph, &next_node.id, visited, order)?;
        }
        
        Ok(())
    }
    
    /// Topological order (for DAGs)
    fn topological_order(&self, graph: &CompiledGraph) -> Result<Vec<String>> {
        // Check for cycles first
        if graph.graph().has_cycles() {
            return Err(ExecutionError::InvalidState(
                "Cannot perform topological sort on graph with cycles".to_string()
            ).into());
        }
        
        let mut order = Vec::new();
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut queue = VecDeque::new();
        
        // Initialize in-degree for all nodes
        let mut all_nodes = HashSet::new();
        
        // Collect all nodes by checking both get_node calls and edges
        all_nodes.insert("__start__".to_string());
        all_nodes.insert("__end__".to_string());
        
        // Add nodes from edges
        let mut current = "__start__".to_string();
        let mut visited = HashSet::new();
        let mut to_visit = vec![current.clone()];
        
        while let Some(node) = to_visit.pop() {
            if visited.contains(&node) {
                continue;
            }
            visited.insert(node.clone());
            all_nodes.insert(node.clone());
            
            let edges = graph.graph().get_edges_from(&node);
            for (next_node, _) in edges {
                all_nodes.insert(next_node.id.clone());
                to_visit.push(next_node.id.clone());
            }
        }
        
        // Calculate in-degrees
        for node in &all_nodes {
            in_degree.insert(node.clone(), 0);
        }
        
        for node in &all_nodes {
            let edges = graph.graph().get_edges_from(node);
            for (next_node, _) in edges {
                *in_degree.get_mut(&next_node.id).unwrap() += 1;
            }
        }
        
        // Find nodes with no incoming edges
        for (node, &degree) in &in_degree {
            if degree == 0 {
                queue.push_back(node.clone());
            }
        }
        
        // Process nodes in topological order
        while let Some(node) = queue.pop_front() {
            order.push(node.clone());
            
            // Reduce in-degree of neighbors
            let edges = graph.graph().get_edges_from(&node);
            for (next_node, _) in edges {
                let degree = in_degree.get_mut(&next_node.id).unwrap();
                *degree -= 1;
                if *degree == 0 {
                    queue.push_back(next_node.id.clone());
                }
            }
        }
        
        Ok(order)
    }
    
    /// Priority-based order (uses edge weights/priorities)
    fn priority_order(&self, graph: &CompiledGraph) -> Result<Vec<String>> {
        // For now, fall back to breadth-first
        // In a real implementation, this would use edge priorities
        self.breadth_first_order(graph)
    }
    
    /// Find the next node(s) to execute based on current state
    pub fn get_next_nodes(
        &self,
        graph: &CompiledGraph,
        current_node: &str,
        state: &StateData,
    ) -> Result<Vec<String>> {
        let mut next_nodes = Vec::new();
        
        // Get all edges from current node
        let edges = graph.graph().get_edges_from(current_node);
        
        if edges.is_empty() {
            // No outgoing edges, we're done
            return Ok(vec![]);
        }
        
        // Check for conditional edges
        let has_conditions = edges.iter().any(|(_, edge)| {
            matches!(edge.edge_type, EdgeType::Conditional { .. })
        });
        
        if has_conditions {
            // Evaluate conditions
            for (node, edge) in edges {
                if let EdgeType::Conditional(cond_edge) = &edge.edge_type {
                    // TODO: Implement condition evaluation
                    // For now, just take the first matching condition
                    next_nodes.push(cond_edge.target.clone());
                    break;
                }
            }
        } else {
            // No conditions, return all targets (for parallel execution)
            for (node, _) in edges {
                next_nodes.push(node.id.clone());
            }
        }
        
        Ok(next_nodes)
    }
}

/// Parallel execution coordinator
pub struct ParallelExecutor {
    max_concurrency: usize,
}

impl ParallelExecutor {
    /// Create a new parallel executor
    pub fn new(max_concurrency: usize) -> Self {
        Self { max_concurrency }
    }
    
    /// Identify nodes that can be executed in parallel
    pub fn find_parallel_nodes(
        &self,
        graph: &CompiledGraph,
        executed: &HashSet<String>,
    ) -> Vec<Vec<String>> {
        let mut parallel_groups = Vec::new();
        let mut current_group = Vec::new();
        let mut visited = executed.clone();
        
        // Find all nodes whose dependencies have been satisfied
        let mut candidates = VecDeque::new();
        candidates.push_back("__start__".to_string());
        
        while let Some(node) = candidates.pop_front() {
            if visited.contains(&node) {
                continue;
            }
            
            // Check if all dependencies are satisfied
            if self.dependencies_satisfied(graph, &node, executed) {
                current_group.push(node.clone());
                visited.insert(node.clone());
                
                // Add to parallel group if within concurrency limit
                if current_group.len() >= self.max_concurrency {
                    parallel_groups.push(current_group.clone());
                    current_group.clear();
                }
            }
            
            // Add neighbors to candidates
            let edges = graph.graph().get_edges_from(&node);
            for (next_node, _) in edges {
                candidates.push_back(next_node.id.clone());
            }
        }
        
        if !current_group.is_empty() {
            parallel_groups.push(current_group);
        }
        
        parallel_groups
    }
    
    /// Check if all dependencies of a node have been executed
    fn dependencies_satisfied(
        &self,
        graph: &CompiledGraph,
        node: &str,
        executed: &HashSet<String>,
    ) -> bool {
        // For now, simple check - would need to track incoming edges
        // in a real implementation
        node == "__start__" || executed.contains("__start__")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{StateGraph, Node, Edge};
    
    fn create_test_graph() -> CompiledGraph {
        use crate::graph::GraphBuilder;
        use std::sync::Arc;
        use crate::state::StateManager;
        
        let mut graph = StateGraph::new("test");
        
        // Add nodes
        graph.add_node(Node {
            id: "__start__".to_string(),
            node_type: NodeType::Start,
            metadata: None,
        });
        
        graph.add_node(Node {
            id: "process".to_string(),
            node_type: NodeType::Agent("processor".to_string()),
            metadata: None,
        });
        
        graph.add_node(Node {
            id: "__end__".to_string(),
            node_type: NodeType::End,
            metadata: None,
        });
        
        // Add edges
        graph.add_edge("__start__", "process", Edge::direct()).unwrap();
        graph.add_edge("process", "__end__", Edge::direct()).unwrap();
        
        // Compile
        GraphBuilder::new(graph)
            .with_state_manager(Arc::new(StateManager::new()))
            .compile()
            .unwrap()
    }
    
    #[test]
    fn test_breadth_first_traversal() {
        let graph = create_test_graph();
        let traverser = GraphTraverser::new(TraversalStrategy::BreadthFirst);
        
        let order = traverser.get_execution_order(&graph).unwrap();
        assert_eq!(order[0], "__start__");
        assert!(order.contains(&"process".to_string()));
        assert!(order.contains(&"__end__".to_string()));
    }
    
    #[test]
    fn test_depth_first_traversal() {
        let graph = create_test_graph();
        let traverser = GraphTraverser::new(TraversalStrategy::DepthFirst);
        
        let order = traverser.get_execution_order(&graph).unwrap();
        assert_eq!(order[0], "__start__");
        assert!(order.contains(&"process".to_string()));
    }
    
    #[test]
    fn test_topological_order() {
        let graph = create_test_graph();
        let traverser = GraphTraverser::new(TraversalStrategy::Topological);
        
        let order = traverser.get_execution_order(&graph).unwrap();
        
        // Verify topological properties
        let start_idx = order.iter().position(|x| x == "__start__").unwrap();
        let process_idx = order.iter().position(|x| x == "process").unwrap();
        let end_idx = order.iter().position(|x| x == "__end__").unwrap();
        
        assert!(start_idx < process_idx);
        assert!(process_idx < end_idx);
    }
}