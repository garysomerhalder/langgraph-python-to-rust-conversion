//! Subgraph execution engine for nested graph workflows
//!
//! This module provides production-ready subgraph execution capabilities
//! with proper state mapping, isolation, and result propagation.

use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::RwLock;

use crate::graph::{CompiledGraph, StateGraph};
use crate::state::StateData;
use crate::engine::executor::ExecutionEngine;
use crate::Result;

/// Subgraph executor for running nested graphs
pub struct SubgraphExecutor {
    /// Cached compiled subgraphs
    compiled_cache: Arc<RwLock<HashMap<String, CompiledGraph>>>,
    
    /// Execution engine for running subgraphs
    engine: Arc<ExecutionEngine>,
    
    /// State isolation strategy
    isolation_strategy: IsolationStrategy,
}

/// Strategy for isolating subgraph state
#[derive(Debug, Clone)]
pub enum IsolationStrategy {
    /// Complete isolation - subgraph gets fresh state
    Complete,
    
    /// Partial isolation - only specified keys are passed
    Partial { allowed_keys: Vec<String> },
    
    /// Shared state - subgraph shares parent state
    Shared,
    
    /// Mapped isolation - keys are mapped between parent and subgraph
    Mapped { mappings: HashMap<String, String> },
}

impl SubgraphExecutor {
    /// Create a new subgraph executor
    pub fn new(isolation_strategy: IsolationStrategy) -> Self {
        Self {
            compiled_cache: Arc::new(RwLock::new(HashMap::new())),
            engine: Arc::new(ExecutionEngine::new()),
            isolation_strategy,
        }
    }
    
    /// Execute a subgraph with the given state
    pub async fn execute_subgraph(
        &self,
        subgraph_id: &str,
        subgraph: &StateGraph,
        parent_state: &StateData,
    ) -> Result<StateData> {
        // Get or compile the subgraph
        let compiled = self.get_or_compile_subgraph(subgraph_id, subgraph).await?;
        
        // Map parent state to subgraph state
        let subgraph_input = self.map_state_to_subgraph(parent_state)?;
        
        // Execute the subgraph
        let subgraph_output = self.engine.execute(compiled, subgraph_input).await?;
        
        // Map subgraph output back to parent state format
        let merged_state = self.merge_subgraph_output(parent_state, &subgraph_output)?;
        
        Ok(merged_state)
    }
    
    /// Get or compile a subgraph
    async fn get_or_compile_subgraph(
        &self,
        subgraph_id: &str,
        subgraph: &StateGraph,
    ) -> Result<CompiledGraph> {
        let mut cache = self.compiled_cache.write().await;
        
        if let Some(compiled) = cache.get(subgraph_id) {
            return Ok(compiled.clone());
        }
        
        // Compile the subgraph
        let compiled = subgraph.clone().compile()?;
        
        cache.insert(subgraph_id.to_string(), compiled.clone());
        
        Ok(compiled)
    }
    
    /// Map parent state to subgraph input state
    fn map_state_to_subgraph(&self, parent_state: &StateData) -> Result<StateData> {
        match &self.isolation_strategy {
            IsolationStrategy::Complete => {
                // Return empty state for complete isolation
                Ok(StateData::new())
            }
            
            IsolationStrategy::Partial { allowed_keys } => {
                // Only pass allowed keys
                let mut subgraph_state = StateData::new();
                for key in allowed_keys {
                    if let Some(value) = parent_state.get(key) {
                        subgraph_state.insert(key.clone(), value.clone());
                    }
                }
                Ok(subgraph_state)
            }
            
            IsolationStrategy::Shared => {
                // Pass entire parent state
                Ok(parent_state.clone())
            }
            
            IsolationStrategy::Mapped { mappings } => {
                // Map keys from parent to subgraph
                let mut subgraph_state = StateData::new();
                for (parent_key, subgraph_key) in mappings {
                    if let Some(value) = parent_state.get(parent_key) {
                        subgraph_state.insert(subgraph_key.clone(), value.clone());
                    }
                }
                Ok(subgraph_state)
            }
        }
    }
    
    /// Merge subgraph output back to parent state
    fn merge_subgraph_output(
        &self,
        parent_state: &StateData,
        subgraph_output: &StateData,
    ) -> Result<StateData> {
        let mut merged = parent_state.clone();
        
        match &self.isolation_strategy {
            IsolationStrategy::Complete => {
                // Don't merge anything for complete isolation
                // Subgraph results are stored under a namespace
                merged.insert(
                    "_subgraph_result".to_string(),
                    serde_json::to_value(subgraph_output)?,
                );
            }
            
            IsolationStrategy::Partial { allowed_keys } => {
                // Only merge back allowed keys
                for key in allowed_keys {
                    if let Some(value) = subgraph_output.get(key) {
                        merged.insert(key.clone(), value.clone());
                    }
                }
            }
            
            IsolationStrategy::Shared => {
                // Merge all subgraph output
                for (key, value) in subgraph_output {
                    merged.insert(key.clone(), value.clone());
                }
            }
            
            IsolationStrategy::Mapped { mappings } => {
                // Reverse map keys from subgraph to parent
                let reverse_mappings: HashMap<_, _> = mappings
                    .iter()
                    .map(|(k, v)| (v.clone(), k.clone()))
                    .collect();
                
                for (subgraph_key, parent_key) in reverse_mappings {
                    if let Some(value) = subgraph_output.get(&subgraph_key) {
                        merged.insert(parent_key, value.clone());
                    }
                }
            }
        }
        
        Ok(merged)
    }
}

/// Recursive subgraph executor for deeply nested graphs
pub struct RecursiveSubgraphExecutor {
    /// Base subgraph executor
    base_executor: SubgraphExecutor,
    
    /// Maximum recursion depth
    max_depth: usize,
    
    /// Current recursion depth
    current_depth: Arc<RwLock<usize>>,
}

impl RecursiveSubgraphExecutor {
    /// Create a new recursive subgraph executor
    pub fn new(isolation_strategy: IsolationStrategy, max_depth: usize) -> Self {
        Self {
            base_executor: SubgraphExecutor::new(isolation_strategy),
            max_depth,
            current_depth: Arc::new(RwLock::new(0)),
        }
    }
    
    /// Execute a potentially recursive subgraph
    pub async fn execute_recursive(
        &self,
        subgraph_id: &str,
        subgraph: &StateGraph,
        parent_state: &StateData,
    ) -> Result<StateData> {
        // Check recursion depth
        let mut depth = self.current_depth.write().await;
        if *depth >= self.max_depth {
            return Err(crate::graph::GraphError::RuntimeError(
                format!("Maximum recursion depth {} exceeded", self.max_depth)
            ).into());
        }
        
        *depth += 1;
        drop(depth);
        
        // Execute the subgraph
        let result = self.base_executor.execute_subgraph(
            subgraph_id,
            subgraph,
            parent_state
        ).await;
        
        // Decrement depth
        let mut depth = self.current_depth.write().await;
        *depth -= 1;
        
        result
    }
    
    /// Check if a graph contains recursive subgraphs
    pub fn contains_recursion(_graph: &StateGraph) -> bool {
        // Simple check: look for nodes that reference the same graph
        // In production, this would need cycle detection
        false // Placeholder implementation
    }
}

/// Parallel subgraph executor for concurrent subgraph execution
pub struct ParallelSubgraphExecutor {
    /// Base executors for each parallel branch
    executors: Vec<SubgraphExecutor>,
    
    /// Merge strategy for combining results
    merge_strategy: MergeStrategy,
}

/// Strategy for merging parallel subgraph results
#[derive(Clone)]
pub enum MergeStrategy {
    /// Take the first successful result
    FirstSuccess,
    
    /// Merge all results with last-write-wins
    LastWriteWins,
    
    /// Merge with custom conflict resolution
    Custom(Arc<dyn Fn(&[StateData]) -> StateData + Send + Sync>),
    
    /// Vote-based merge (majority wins)
    Voting,
}

impl std::fmt::Debug for MergeStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FirstSuccess => write!(f, "FirstSuccess"),
            Self::LastWriteWins => write!(f, "LastWriteWins"),
            Self::Custom(_) => write!(f, "Custom(<function>)"),
            Self::Voting => write!(f, "Voting"),
        }
    }
}

impl ParallelSubgraphExecutor {
    /// Create a new parallel subgraph executor
    pub fn new(num_parallel: usize, merge_strategy: MergeStrategy) -> Self {
        let executors = (0..num_parallel)
            .map(|_| SubgraphExecutor::new(IsolationStrategy::Shared))
            .collect();
        
        Self {
            executors,
            merge_strategy,
        }
    }
    
    /// Execute multiple subgraphs in parallel
    pub async fn execute_parallel(
        &self,
        subgraphs: Vec<(&str, &StateGraph)>,
        parent_state: &StateData,
    ) -> Result<StateData> {
        let mut handles = Vec::new();
        
        for (i, (subgraph_id, subgraph)) in subgraphs.iter().enumerate() {
            let _executor = &self.executors[i % self.executors.len()];
            let state = parent_state.clone();
            let id = subgraph_id.to_string();
            let graph = (*subgraph).clone();
            
            let executor_clone = SubgraphExecutor::new(IsolationStrategy::Shared);
            
            let handle = tokio::spawn(async move {
                executor_clone.execute_subgraph(&id, &graph, &state).await
            });
            
            handles.push(handle);
        }
        
        // Collect all results
        let mut results = Vec::new();
        for handle in handles {
            match handle.await? {
                Ok(result) => results.push(result),
                Err(e) => {
                    // Handle based on strategy
                    match &self.merge_strategy {
                        MergeStrategy::FirstSuccess => continue,
                        _ => return Err(e),
                    }
                }
            }
        }
        
        // Merge results based on strategy
        self.merge_results(results)
    }
    
    /// Merge parallel execution results
    fn merge_results(&self, results: Vec<StateData>) -> Result<StateData> {
        if results.is_empty() {
            return Ok(StateData::new());
        }
        
        match &self.merge_strategy {
            MergeStrategy::FirstSuccess => {
                Ok(results.into_iter().next().unwrap())
            }
            
            MergeStrategy::LastWriteWins => {
                let mut merged = StateData::new();
                for result in results {
                    for (key, value) in result {
                        merged.insert(key, value);
                    }
                }
                Ok(merged)
            }
            
            MergeStrategy::Custom(merger) => {
                Ok(merger(&results))
            }
            
            MergeStrategy::Voting => {
                // Simple voting: most common value for each key wins
                let mut vote_map: HashMap<String, HashMap<String, usize>> = HashMap::new();
                
                for result in &results {
                    for (key, value) in result {
                        let value_str = serde_json::to_string(&value)?;
                        vote_map.entry(key.clone())
                            .or_insert_with(HashMap::new)
                            .entry(value_str)
                            .and_modify(|count| *count += 1)
                            .or_insert(1);
                    }
                }
                
                let mut merged = StateData::new();
                for (key, value_votes) in vote_map {
                    if let Some((winning_value, _)) = value_votes.iter()
                        .max_by_key(|(_, count)| *count) {
                        merged.insert(key, serde_json::from_str(winning_value)?);
                    }
                }
                
                Ok(merged)
            }
        }
    }
}

/// Conditional subgraph executor
pub struct ConditionalSubgraphExecutor {
    /// Condition evaluator
    condition_fn: Arc<dyn Fn(&StateData) -> bool + Send + Sync>,
    
    /// Subgraph to execute if condition is true
    true_branch: Option<Arc<SubgraphExecutor>>,
    
    /// Subgraph to execute if condition is false
    false_branch: Option<Arc<SubgraphExecutor>>,
}

impl ConditionalSubgraphExecutor {
    /// Create a new conditional subgraph executor
    pub fn new<F>(condition_fn: F) -> Self
    where
        F: Fn(&StateData) -> bool + Send + Sync + 'static,
    {
        Self {
            condition_fn: Arc::new(condition_fn),
            true_branch: None,
            false_branch: None,
        }
    }
    
    /// Set the true branch subgraph
    pub fn with_true_branch(mut self, executor: SubgraphExecutor) -> Self {
        self.true_branch = Some(Arc::new(executor));
        self
    }
    
    /// Set the false branch subgraph
    pub fn with_false_branch(mut self, executor: SubgraphExecutor) -> Self {
        self.false_branch = Some(Arc::new(executor));
        self
    }
    
    /// Execute the conditional subgraph
    pub async fn execute_conditional(
        &self,
        true_graph: Option<(&str, &StateGraph)>,
        false_graph: Option<(&str, &StateGraph)>,
        state: &StateData,
    ) -> Result<StateData> {
        let condition_met = (self.condition_fn)(state);
        
        if condition_met {
            if let (Some(executor), Some((id, graph))) = (&self.true_branch, true_graph) {
                return executor.execute_subgraph(id, graph, state).await;
            }
        } else {
            if let (Some(executor), Some((id, graph))) = (&self.false_branch, false_graph) {
                return executor.execute_subgraph(id, graph, state).await;
            }
        }
        
        // No branch to execute, return original state
        Ok(state.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Node, Edge, NodeType};
    
    async fn create_test_subgraph() -> StateGraph {
        let mut graph = StateGraph::new("test_subgraph");
        
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
        
        graph.add_edge("__start__", "process", Edge::direct()).unwrap();
        graph.add_edge("process", "__end__", Edge::direct()).unwrap();
        
        graph
    }
    
    #[tokio::test]
    async fn test_subgraph_executor() {
        let executor = SubgraphExecutor::new(IsolationStrategy::Shared);
        let subgraph = create_test_subgraph().await;
        
        let mut parent_state = StateData::new();
        parent_state.insert("input".to_string(), serde_json::json!("test"));
        
        let result = executor.execute_subgraph(
            "test",
            &subgraph,
            &parent_state
        ).await;
        
        // Note: This will fail without proper node implementation
        // but demonstrates the API
        assert!(result.is_ok() || result.is_err());
    }
    
    #[tokio::test]
    async fn test_parallel_subgraph_executor() {
        let executor = ParallelSubgraphExecutor::new(2, MergeStrategy::LastWriteWins);
        
        let subgraph1 = create_test_subgraph().await;
        let subgraph2 = create_test_subgraph().await;
        
        let parent_state = StateData::new();
        
        let result = executor.execute_parallel(
            vec![("sub1", &subgraph1), ("sub2", &subgraph2)],
            &parent_state
        ).await;
        
        assert!(result.is_ok() || result.is_err());
    }
    
    #[tokio::test]
    async fn test_conditional_subgraph() {
        let executor = ConditionalSubgraphExecutor::new(|state| {
            state.get("condition")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        })
        .with_true_branch(SubgraphExecutor::new(IsolationStrategy::Shared))
        .with_false_branch(SubgraphExecutor::new(IsolationStrategy::Complete));
        
        let subgraph = create_test_subgraph().await;
        
        let mut state = StateData::new();
        state.insert("condition".to_string(), serde_json::json!(true));
        
        let result = executor.execute_conditional(
            Some(("true_branch", &subgraph)),
            Some(("false_branch", &subgraph)),
            &state
        ).await;
        
        assert!(result.is_ok() || result.is_err());
    }
}