# Task 020: Pregel Core Architecture

## 📋 Task Details
- **ID**: 020
- **Phase**: 🟡 YELLOW
- **Priority**: P0 (Critical Path)
- **Estimated Hours**: 16
- **Status**: Not Started
- **Owner**: Unassigned
- **Created**: 2024-12-15
- **Updated**: 2024-12-15

## 📝 Description
Implement the core Pregel execution engine for LangGraph in Rust. This is the heart of the graph execution system, implementing Google's Bulk Synchronous Parallel (BSP) model adapted for stateful LLM workflows. Must maintain compatibility with Python LangGraph's execution semantics.

## 🎯 Acceptance Criteria
- [ ] Core `Pregel` struct implemented with generics
- [ ] BSP execution model with supersteps
- [ ] Channel-based communication between nodes
- [ ] Task scheduling and execution framework
- [ ] State management during execution
- [ ] Interrupt and resume capability
- [ ] Streaming output support
- [ ] Thread-safe concurrent execution
- [ ] Integration tests against Python behavior

## 🔧 Technical Details

### Core Pregel Structure
```rust
// In crates/langgraph-pregel/src/lib.rs

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::channel::Channel;
use crate::node::PregelNode;
use crate::task::PregelTask;

/// Core Pregel execution engine
pub struct Pregel<State, Input, Output> {
    /// Graph nodes indexed by name
    nodes: HashMap<String, Arc<PregelNode<State>>>,
    
    /// Channels for inter-node communication
    channels: Arc<RwLock<HashMap<String, Box<dyn Channel>>>>,
    
    /// Compiled execution plan
    execution_plan: ExecutionPlan,
    
    /// Current checkpoint
    checkpoint: Option<Checkpoint>,
    
    /// Runtime configuration
    config: PregelConfig,
    
    /// Phantom data for types
    _phantom: std::marker::PhantomData<(State, Input, Output)>,
}

impl<State, Input, Output> Pregel<State, Input, Output> 
where
    State: Clone + Send + Sync + 'static,
    Input: Send + 'static,
    Output: Send + 'static,
{
    /// Create a new Pregel instance
    pub fn new(config: PregelConfig) -> Self {
        Self {
            nodes: HashMap::new(),
            channels: Arc::new(RwLock::new(HashMap::new())),
            execution_plan: ExecutionPlan::default(),
            checkpoint: None,
            config,
            _phantom: std::marker::PhantomData,
        }
    }
    
    /// Add a node to the graph
    pub fn add_node(&mut self, name: String, node: PregelNode<State>) {
        self.nodes.insert(name, Arc::new(node));
    }
    
    /// Execute the graph with given input
    pub async fn invoke(&mut self, input: Input) -> Result<Output, PregelError> {
        // Initialize channels with input
        self.initialize_channels(input).await?;
        
        // Execute supersteps until completion
        while !self.is_complete().await? {
            self.execute_superstep().await?;
        }
        
        // Extract and return output
        self.extract_output().await
    }
    
    /// Execute a single superstep (BSP model)
    async fn execute_superstep(&mut self) -> Result<(), PregelError> {
        // Phase 1: Read from channels
        let tasks = self.prepare_tasks().await?;
        
        // Phase 2: Execute tasks in parallel
        let results = self.execute_tasks(tasks).await?;
        
        // Phase 3: Write to channels
        self.apply_writes(results).await?;
        
        // Phase 4: Checkpoint if configured
        if self.config.checkpoint_enabled {
            self.save_checkpoint().await?;
        }
        
        Ok(())
    }
    
    /// Stream execution with progressive output
    pub async fn stream(
        &mut self, 
        input: Input
    ) -> impl Stream<Item = StreamOutput> {
        // Implementation for streaming execution
        todo!()
    }
}

/// Execution plan compiled from graph structure
#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    /// Topologically sorted node order
    node_order: Vec<String>,
    
    /// Dependencies between nodes
    dependencies: HashMap<String, Vec<String>>,
    
    /// Channels each node reads from
    node_inputs: HashMap<String, Vec<String>>,
    
    /// Channels each node writes to
    node_outputs: HashMap<String, Vec<String>>,
}

/// Configuration for Pregel execution
#[derive(Debug, Clone)]
pub struct PregelConfig {
    /// Enable checkpointing
    pub checkpoint_enabled: bool,
    
    /// Maximum number of supersteps
    pub max_steps: usize,
    
    /// Parallelism level
    pub parallelism: usize,
    
    /// Interrupt on error
    pub interrupt_on_error: bool,
    
    /// Stream mode configuration
    pub stream_mode: StreamMode,
}
```

### Task Execution Framework
```rust
// In crates/langgraph-pregel/src/task.rs

/// Represents a task to be executed in a superstep
pub struct PregelTask {
    /// Unique task ID
    pub id: TaskId,
    
    /// Node that will execute this task
    pub node: String,
    
    /// Input data for the task
    pub input: TaskInput,
    
    /// Channels to read from
    pub reads: Vec<String>,
    
    /// Channels to write to
    pub writes: Vec<String>,
}

/// Task execution result
pub struct TaskResult {
    /// Task that was executed
    pub task_id: TaskId,
    
    /// Writes to apply to channels
    pub writes: Vec<ChannelWrite>,
    
    /// Execution metadata
    pub metadata: TaskMetadata,
}

/// Async task executor
pub struct TaskExecutor {
    /// Thread pool for CPU-bound tasks
    thread_pool: Arc<ThreadPool>,
    
    /// Tokio runtime for async tasks
    runtime: Arc<Runtime>,
}

impl TaskExecutor {
    /// Execute tasks in parallel with proper isolation
    pub async fn execute_batch(
        &self,
        tasks: Vec<PregelTask>,
    ) -> Result<Vec<TaskResult>, PregelError> {
        let futures = tasks.into_iter().map(|task| {
            self.execute_single(task)
        });
        
        // Execute all tasks concurrently
        let results = futures::future::join_all(futures).await;
        
        // Collect results and handle errors
        results.into_iter().collect::<Result<Vec<_>, _>>()
    }
}
```

### Python Compatibility Requirements
| Python Feature | Rust Implementation | Notes |
|----------------|-------------------|-------|
| `Pregel.invoke()` | `Pregel::invoke()` | Async in Rust |
| `Pregel.stream()` | `Pregel::stream()` | Returns async stream |
| Checkpointing | Built-in with traits | Pluggable backends |
| Interrupts | Cancellation tokens | Graceful shutdown |
| Parallelism | Tokio + thread pool | Configurable |

## 🔗 Dependencies
- **Blocked By**: 
  - Task 003 (Define Channel Traits)
  - Task 004 (Create Test Framework)
- **Blocks**: 
  - Task 021 (Async Executor Design)
  - Task 023 (StateGraph Builder API)
  - Task 042 (PyO3 Bindings Setup)
- **Related**: 
  - Task 022 (Task Scheduler Implementation)
  - Task 026 (Checkpoint Base Implementation)

## 🚦 Traffic-Light Status
- **RED Phase Goal**: Define structure without implementation
- **YELLOW Phase Goal**: Basic execution working
- **GREEN Phase Goal**: Full BSP model with all features

## ⚠️ Risks & Mitigations
| Risk | Impact | Mitigation |
|------|--------|------------|
| BSP model complexity | High | Study Python implementation closely |
| Async/sync mismatch | Medium | Clear async boundaries |
| Performance regression | High | Benchmark against Python |
| Memory management | Medium | Use Arc/Rc carefully |

## 🧪 Testing Requirements
- [ ] Unit tests for each component
- [ ] Integration tests against Python examples
- [ ] Concurrent execution tests
- [ ] Checkpoint/resume tests
- [ ] Error handling tests
- [ ] Performance benchmarks

### Integration Test Example
```rust
#[tokio::test]
async fn test_pregel_execution() {
    // Create a simple graph
    let mut pregel = Pregel::new(PregelConfig::default());
    
    // Add nodes
    pregel.add_node("input".to_string(), input_node());
    pregel.add_node("process".to_string(), process_node());
    pregel.add_node("output".to_string(), output_node());
    
    // Add edges
    pregel.add_edge("input", "process");
    pregel.add_edge("process", "output");
    
    // Execute
    let result = pregel.invoke(test_input()).await.unwrap();
    
    // Verify against Python output
    assert_eq!(result, expected_output());
}

#[tokio::test]
async fn test_streaming_execution() {
    let mut pregel = create_test_graph();
    
    let mut stream = pregel.stream(test_input()).await;
    let mut outputs = vec![];
    
    while let Some(output) = stream.next().await {
        outputs.push(output);
    }
    
    // Verify streaming behavior matches Python
    assert_eq!(outputs.len(), 3);
    assert_eq!(outputs[0].node, "input");
    assert_eq!(outputs[1].node, "process");
    assert_eq!(outputs[2].node, "output");
}
```

## 📊 Success Metrics
- BSP execution model correctly implemented
- Performance: 10x faster than Python on benchmarks
- Memory usage: <50% of Python implementation
- Test coverage: >90%
- Zero data races (verified by tests)

## 🔄 Progress Log
| Date | Status | Notes |
|------|--------|-------|
| 2024-12-15 | Created | Task defined |

## 📝 Implementation Notes
- Study Python's `pregel/__init__.py` and `pregel/_algo.py`
- Use Tokio for async runtime
- Consider using `petgraph` for graph operations
- Implement progressive checkpointing
- Design for extensibility (custom executors)

## ✅ Completion Checklist
- [ ] Core Pregel struct implemented
- [ ] BSP superstep execution working
- [ ] Task scheduling implemented
- [ ] Channel communication working
- [ ] Checkpointing functional
- [ ] Streaming support added
- [ ] Integration tests passing
- [ ] Performance benchmarks meeting targets
- [ ] Documentation complete
- [ ] Code review passed
- [ ] Task marked complete in tracker

---

*Task Template Version: 1.0*