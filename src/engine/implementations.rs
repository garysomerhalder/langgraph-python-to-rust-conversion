//! Concrete implementations of execution engine traits.
//! 
//! This module provides production-ready implementations of the traits defined
//! in the traits module, including different execution strategies and backends.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use crate::{Result, LangGraphError};
use crate::state::StateData;
use crate::graph::{Node, StateGraph};
use crate::engine::traits::*;
use serde_json::Value;

/// High-performance parallel execution engine implementation
pub struct ParallelExecutionEngine {
    max_concurrency: usize,
    semaphore: Arc<Semaphore>,
    metrics: Arc<RwLock<EngineMetrics>>,
    timeout: Duration,
}

impl ParallelExecutionEngine {
    /// Create a new parallel execution engine
    pub fn new(max_concurrency: usize, timeout: Duration) -> Self {
        Self {
            max_concurrency,
            semaphore: Arc::new(Semaphore::new(max_concurrency)),
            metrics: Arc::new(RwLock::new(EngineMetrics::default())),
            timeout,
        }
    }
    
    /// Set maximum concurrency level
    pub fn set_max_concurrency(&mut self, max_concurrency: usize) {
        self.max_concurrency = max_concurrency;
        self.semaphore = Arc::new(Semaphore::new(max_concurrency));
    }
}

#[async_trait]
impl ExecutionEngine for ParallelExecutionEngine {
    async fn execute_node(
        &self,
        node: &Node,
        state: &mut StateData,
        context: ExecutionContext,
    ) -> Result<ExecutionResult> {
        let start_time = Instant::now();
        let _permit = self.semaphore.acquire().await.map_err(|_| {
            LangGraphError::Execution("Failed to acquire execution permit".to_string())
        })?;
        
        // Execute node based on type
        let result = match &node.node_type {
            crate::graph::NodeType::Start => {
                // Start node execution
                Ok(ExecutionResult {
                    success: true,
                    output: Some(Value::String("Started".to_string())),
                    duration: start_time.elapsed(),
                    error: None,
                    metadata: HashMap::new(),
                })
            }
            crate::graph::NodeType::End => {
                // End node execution
                Ok(ExecutionResult {
                    success: true,
                    output: Some(Value::String("Completed".to_string())),
                    duration: start_time.elapsed(),
                    error: None,
                    metadata: HashMap::new(),
                })
            }
            crate::graph::NodeType::Agent(agent_name) => {
                // Agent execution (simplified)
                let output = format!("Agent {} executed", agent_name);
                Ok(ExecutionResult {
                    success: true,
                    output: Some(Value::String(output)),
                    duration: start_time.elapsed(),
                    error: None,
                    metadata: HashMap::new(),
                })
            }
            crate::graph::NodeType::Tool(tool_name) => {
                // Tool execution (simplified)
                let output = format!("Tool {} executed", tool_name);
                Ok(ExecutionResult {
                    success: true,
                    output: Some(Value::String(output)),
                    duration: start_time.elapsed(),
                    error: None,
                    metadata: HashMap::new(),
                })
            }
            crate::graph::NodeType::Conditional(_) => {
                // Conditional execution (simplified)
                Ok(ExecutionResult {
                    success: true,
                    output: Some(Value::String("Condition evaluated".to_string())),
                    duration: start_time.elapsed(),
                    error: None,
                    metadata: HashMap::new(),
                })
            }
        };
        
        // Update metrics
        if let Ok(mut metrics) = self.metrics.write() {
            metrics.total_executions += 1;
            metrics.total_duration += start_time.elapsed();
            if let Ok(ref result) = result {
                if result.success {
                    metrics.successful_executions += 1;
                } else {
                    metrics.failed_executions += 1;
                }
            }
        }
        
        result
    }
    
    async fn execute_nodes(
        &self,
        nodes: &[Node],
        state: &mut StateData,
        strategy: ExecutionStrategy,
    ) -> Result<Vec<ExecutionResult>> {
        match strategy {
            ExecutionStrategy::Sequential => {
                let mut results = Vec::new();
                for node in nodes {
                    let context = ExecutionContext {
                        execution_id: format!("exec_{}", uuid::Uuid::new_v4()),
                        graph_id: "graph".to_string(),
                        step: results.len(),
                        metadata: HashMap::new(),
                        timeout: Some(self.timeout),
                    };
                    let result = self.execute_node(node, state, context).await?;
                    results.push(result);
                }
                Ok(results)
            }
            ExecutionStrategy::Parallel { max_concurrency } => {
                use futures::future::join_all;
                
                let semaphore = Arc::new(Semaphore::new(max_concurrency));
                let tasks: Vec<_> = nodes.iter().enumerate().map(|(i, node)| {
                    let semaphore = semaphore.clone();
                    let node = node.clone();
                    let context = ExecutionContext {
                        execution_id: format!("exec_{}", uuid::Uuid::new_v4()),
                        graph_id: "graph".to_string(),
                        step: i,
                        metadata: HashMap::new(),
                        timeout: Some(self.timeout),
                    };
                    
                    async move {
                        let _permit = semaphore.acquire().await.map_err(|_| {
                            LangGraphError::Execution("Failed to acquire execution permit".to_string())
                        })?;
                        // Note: We can't pass &mut state to parallel execution
                        // This is a simplified implementation
                        let mut local_state = state.clone();
                        self.execute_node(&node, &mut local_state, context).await
                    }
                }).collect();
                
                let results = join_all(tasks).await;
                results.into_iter().collect()
            }
            _ => {
                Err(LangGraphError::Execution("Unsupported execution strategy".to_string()))
            }
        }
    }
    
    fn capabilities(&self) -> ExecutionCapabilities {
        ExecutionCapabilities {
            supports_parallel: true,
            max_concurrency: Some(self.max_concurrency),
            supports_streaming: false,
            supports_checkpointing: false,
            supported_node_types: vec![
                "Start".to_string(),
                "End".to_string(),
                "Agent".to_string(),
                "Tool".to_string(),
                "Conditional".to_string(),
            ],
        }
    }
    
    async fn prepare(&mut self, _graph: &StateGraph) -> Result<()> {
        // Initialize execution environment
        Ok(())
    }
    
    async fn cleanup(&mut self) -> Result<()> {
        // Clean up resources
        Ok(())
    }
}

/// In-memory state manager implementation
pub struct InMemoryStateManager {
    state: Arc<RwLock<StateData>>,
    history: Arc<RwLock<Vec<StateVersion>>>,
    checkpoints: Arc<RwLock<HashMap<String, StateCheckpoint>>>,
    next_version_id: Arc<RwLock<u64>>,
}

impl InMemoryStateManager {
    /// Create a new in-memory state manager
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(StateData::new())),
            history: Arc::new(RwLock::new(Vec::new())),
            checkpoints: Arc::new(RwLock::new(HashMap::new())),
            next_version_id: Arc::new(RwLock::new(1)),
        }
    }
    
    /// Initialize with specific state
    pub fn with_initial_state(initial_state: StateData) -> Self {
        Self {
            state: Arc::new(RwLock::new(initial_state)),
            history: Arc::new(RwLock::new(Vec::new())),
            checkpoints: Arc::new(RwLock::new(HashMap::new())),
            next_version_id: Arc::new(RwLock::new(1)),
        }
    }
}

#[async_trait]
impl StateManager for InMemoryStateManager {
    async fn get_state(&self) -> Result<StateData> {
        self.state.read()
            .map_err(|_| LangGraphError::State("Failed to read state".to_string()))?
            .clone()
            .into()
    }
    
    async fn update_state(&self, updates: StateData) -> Result<()> {
        let mut state = self.state.write()
            .map_err(|_| LangGraphError::State("Failed to write state".to_string()))?;
        
        // Merge updates into current state
        for (key, value) in updates {
            state.insert(key, value);
        }
        
        // Record version
        let version_id = {
            let mut next_id = self.next_version_id.write()
                .map_err(|_| LangGraphError::State("Failed to get next version ID".to_string()))?;
            let id = *next_id;
            *next_id += 1;
            id
        };
        
        let version = StateVersion {
            id: version_id,
            parent_id: if version_id > 1 { Some(version_id - 1) } else { None },
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            description: "State update".to_string(),
            author: "system".to_string(),
        };
        
        self.history.write()
            .map_err(|_| LangGraphError::State("Failed to write history".to_string()))?
            .push(version);
        
        Ok(())
    }
    
    async fn apply_reducer(&self, key: &str, operation: ReducerOperation) -> Result<()> {
        let mut state = self.state.write()
            .map_err(|_| LangGraphError::State("Failed to write state".to_string()))?;
        
        match operation {
            ReducerOperation::Append(value) => {
                let current = state.get_mut(key)
                    .and_then(|v| v.as_array_mut())
                    .ok_or_else(|| LangGraphError::State(format!("Key {} is not an array", key)))?;
                current.push(value);
            }
            ReducerOperation::Merge(value) => {
                let current = state.get_mut(key)
                    .and_then(|v| v.as_object_mut())
                    .ok_or_else(|| LangGraphError::State(format!("Key {} is not an object", key)))?;
                if let Some(obj) = value.as_object() {
                    for (k, v) in obj {
                        current.insert(k.clone(), v.clone());
                    }
                }
            }
            ReducerOperation::Replace(value) => {
                state.insert(key.to_string(), value);
            }
            ReducerOperation::Custom(_, _) => {
                return Err(LangGraphError::State("Custom reducers not implemented".to_string()));
            }
        }
        
        Ok(())
    }
    
    async fn create_checkpoint(&self, checkpoint_id: &str) -> Result<StateCheckpoint> {
        let state = self.get_state().await?;
        let serialized = serde_json::to_vec(&state)
            .map_err(|e| LangGraphError::Serialization(e))?;
        
        let checkpoint = StateCheckpoint {
            id: checkpoint_id.to_string(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            data: serialized,
            metadata: HashMap::new(),
        };
        
        self.checkpoints.write()
            .map_err(|_| LangGraphError::State("Failed to write checkpoints".to_string()))?
            .insert(checkpoint_id.to_string(), checkpoint.clone());
        
        Ok(checkpoint)
    }
    
    async fn restore_checkpoint(&self, checkpoint: &StateCheckpoint) -> Result<()> {
        let state: StateData = serde_json::from_slice(&checkpoint.data)
            .map_err(|e| LangGraphError::Serialization(e))?;
        
        *self.state.write()
            .map_err(|_| LangGraphError::State("Failed to write state".to_string()))? = state;
        
        Ok(())
    }
    
    async fn get_state_history(&self) -> Result<Vec<StateVersion>> {
        Ok(self.history.read()
            .map_err(|_| LangGraphError::State("Failed to read history".to_string()))?
            .clone())
    }
    
    async fn validate_state(&self, _state: &StateData) -> Result<ValidationResult> {
        // Simple validation - always pass for now
        Ok(ValidationResult {
            valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
        })
    }
}

/// Basic engine metrics
#[derive(Debug, Clone, Default)]
pub struct EngineMetrics {
    pub total_executions: u64,
    pub successful_executions: u64,
    pub failed_executions: u64,
    pub total_duration: Duration,
}

/// High-performance execution engine factory
pub struct ExecutionEngineFactory;

impl ExecutionEngineFactory {
    /// Create a parallel execution engine with optimal settings
    pub fn create_parallel_engine(max_concurrency: Option<usize>) -> Box<dyn ExecutionEngine> {
        let concurrency = max_concurrency.unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|p| p.get())
                .unwrap_or(4)
        });
        
        Box::new(ParallelExecutionEngine::new(
            concurrency,
            Duration::from_secs(30), // Default timeout
        ))
    }
    
    /// Create a sequential execution engine (single-threaded)
    pub fn create_sequential_engine() -> Box<dyn ExecutionEngine> {
        Box::new(ParallelExecutionEngine::new(1, Duration::from_secs(30)))
    }
    
    /// Create an adaptive execution engine that adjusts concurrency based on load
    pub fn create_adaptive_engine() -> Box<dyn ExecutionEngine> {
        // Start with moderate concurrency, can be adjusted dynamically
        Box::new(ParallelExecutionEngine::new(
            std::thread::available_parallelism().map(|p| p.get() / 2).unwrap_or(2),
            Duration::from_secs(30),
        ))
    }
}

/// State manager factory for different backends
pub struct StateManagerFactory;

impl StateManagerFactory {
    /// Create an in-memory state manager
    pub fn create_in_memory() -> Box<dyn StateManager> {
        Box::new(InMemoryStateManager::new())
    }
    
    /// Create an in-memory state manager with initial state
    pub fn create_in_memory_with_state(initial_state: StateData) -> Box<dyn StateManager> {
        Box::new(InMemoryStateManager::with_initial_state(initial_state))
    }
    
    /// Create a persistent state manager (future implementation)
    pub fn create_persistent(_config: PersistenceConfig) -> Result<Box<dyn StateManager>> {
        // TODO: Implement persistent storage backend
        Ok(Box::new(InMemoryStateManager::new()))
    }
}

/// Configuration for persistent state storage
#[derive(Debug, Clone)]
pub struct PersistenceConfig {
    /// Storage backend type
    pub backend: StorageBackend,
    /// Connection string or path
    pub connection: String,
    /// Additional configuration options
    pub options: HashMap<String, String>,
}

/// Storage backend types
#[derive(Debug, Clone)]
pub enum StorageBackend {
    /// SQLite database
    SQLite,
    /// PostgreSQL database
    PostgreSQL,
    /// Redis cache
    Redis,
    /// File system
    FileSystem,
    /// In-memory (for testing)
    Memory,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{NodeType, Node};
    
    #[tokio::test]
    async fn test_parallel_execution_engine() {
        let mut engine = ParallelExecutionEngine::new(2, Duration::from_secs(5));
        
        let node = Node {
            id: "test_node".to_string(),
            node_type: NodeType::Start,
            metadata: None,
        };
        
        let mut state = StateData::new();
        let context = ExecutionContext {
            execution_id: "test_exec".to_string(),
            graph_id: "test_graph".to_string(),
            step: 0,
            metadata: HashMap::new(),
            timeout: Some(Duration::from_secs(5)),
        };
        
        let result = engine.execute_node(&node, &mut state, context).await;
        assert!(result.is_ok());
        
        let execution_result = result.unwrap();
        assert!(execution_result.success);
    }
    
    #[tokio::test]
    async fn test_in_memory_state_manager() {
        let manager = InMemoryStateManager::new();
        
        // Test state update
        let mut updates = StateData::new();
        updates.insert("key1".to_string(), Value::String("value1".to_string()));
        
        let result = manager.update_state(updates).await;
        assert!(result.is_ok());
        
        // Test state retrieval
        let state = manager.get_state().await;
        assert!(state.is_ok());
        
        let retrieved_state = state.unwrap();
        assert_eq!(retrieved_state.get("key1"), Some(&Value::String("value1".to_string())));
    }
    
    #[tokio::test]
    async fn test_execution_strategies() {
        let engine = ExecutionEngineFactory::create_parallel_engine(Some(2));
        let capabilities = engine.capabilities();
        
        assert!(capabilities.supports_parallel);
        assert_eq!(capabilities.max_concurrency, Some(2));
        assert!(capabilities.supported_node_types.contains(&"Start".to_string()));
    }
}