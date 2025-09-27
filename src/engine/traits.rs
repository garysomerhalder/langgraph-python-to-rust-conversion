//! Trait abstractions for execution engines, state management, and system components.
//!
//! This module provides a comprehensive set of traits that enable pluggable
//! implementations of core LangGraph functionality, promoting modularity and testability.

use crate::graph::{Node, StateGraph};
use crate::state::StateData;
use crate::{LangGraphError, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;

/// Core execution engine trait for processing graph nodes
#[async_trait]
pub trait ExecutionEngine: Send + Sync {
    /// Execute a single node with the given state
    async fn execute_node(
        &self,
        node: &Node,
        state: &mut StateData,
        context: ExecutionContext,
    ) -> Result<ExecutionResult>;

    /// Execute multiple nodes with different strategies
    async fn execute_nodes(
        &self,
        nodes: &[Node],
        state: &mut StateData,
        strategy: ExecutionStrategy,
    ) -> Result<Vec<ExecutionResult>>;

    /// Get execution capabilities and constraints
    fn capabilities(&self) -> ExecutionCapabilities;

    /// Prepare execution environment for a graph
    async fn prepare(&mut self, graph: &StateGraph) -> Result<()>;

    /// Clean up resources after execution
    async fn cleanup(&mut self) -> Result<()>;
}

/// State management abstraction for different backends
#[async_trait]
pub trait StateManager: Send + Sync {
    /// Get the current state
    async fn get_state(&self) -> Result<StateData>;

    /// Update state with new values
    async fn update_state(&self, updates: StateData) -> Result<()>;

    /// Apply a reducer function to state
    async fn apply_reducer(&self, key: &str, operation: ReducerOperation) -> Result<()>;

    /// Create a state checkpoint
    async fn create_checkpoint(&self, checkpoint_id: &str) -> Result<StateCheckpoint>;

    /// Restore state from a checkpoint
    async fn restore_checkpoint(&self, checkpoint: &StateCheckpoint) -> Result<()>;

    /// Get state history and versioning information
    async fn get_state_history(&self) -> Result<Vec<StateVersion>>;

    /// Validate state against schema or constraints
    async fn validate_state(&self, state: &StateData) -> Result<ValidationResult>;
}

/// Metrics collection and analysis abstraction
#[async_trait]
pub trait MetricsCollector: Send + Sync {
    /// Record execution timing
    async fn record_execution_time(&self, node_id: &str, duration: Duration) -> Result<()>;

    /// Record state operation metrics
    async fn record_state_operation(&self, operation: &str, duration: Duration) -> Result<()>;

    /// Record error occurrences
    async fn record_error(&self, error_type: &str, context: ErrorContext) -> Result<()>;

    /// Record custom metrics
    async fn record_custom_metric(
        &self,
        name: &str,
        value: f64,
        labels: HashMap<String, String>,
    ) -> Result<()>;

    /// Export collected metrics
    async fn export_metrics(&self, format: MetricsFormat) -> Result<String>;

    /// Get real-time metrics analysis
    async fn get_metrics_analysis(&self, time_window: Duration) -> Result<TraitsMetricsAnalysis>;

    /// Reset or clear collected metrics
    async fn reset_metrics(&self) -> Result<()>;
}

/// Resilience and fault tolerance abstraction
#[async_trait]
pub trait ResilienceProvider: Send + Sync {
    /// Execute operation with retry logic
    async fn execute_with_retry<F, R>(&self, operation: F, config: RetryConfig) -> Result<R>
    where
        F: Fn() -> Box<dyn std::future::Future<Output = Result<R>> + Send> + Send + Sync,
        R: Send + 'static;

    /// Execute operation with circuit breaker protection
    async fn execute_with_circuit_breaker<F, R>(&self, operation: F, circuit_id: &str) -> Result<R>
    where
        F: Fn() -> Box<dyn std::future::Future<Output = Result<R>> + Send> + Send + Sync,
        R: Send + 'static;

    /// Execute operation with timeout
    async fn execute_with_timeout<F, R>(&self, operation: F, timeout: Duration) -> Result<R>
    where
        F: std::future::Future<Output = Result<R>> + Send,
        R: Send + 'static;

    /// Get current resilience status
    async fn get_resilience_status(&self) -> Result<ResilienceStatus>;

    /// Configure resilience parameters
    async fn configure(&self, config: ResilienceConfig) -> Result<()>;
}

/// Stream processing abstraction for different streaming strategies
#[async_trait]
pub trait StreamProcessor: Send + Sync {
    /// Process items in a stream with backpressure control
    async fn process_stream<T>(
        &self,
        stream: Box<dyn Stream<T>>,
        processor: Box<dyn StreamItemProcessor<T>>,
    ) -> Result<()>
    where
        T: Send + 'static;

    /// Create a buffered stream with specified buffer size
    async fn create_buffered_stream<T>(
        &self,
        buffer_size: usize,
    ) -> Result<Box<dyn BufferedStream<T>>>
    where
        T: Send + 'static;

    /// Apply flow control to a stream
    async fn apply_flow_control<T>(
        &self,
        stream: Box<dyn Stream<T>>,
        config: FlowControlConfig,
    ) -> Result<Box<dyn Stream<T>>>
    where
        T: Send + 'static;

    /// Get stream processing metrics
    async fn get_stream_metrics(&self) -> Result<StreamMetrics>;
}

/// Graph traversal strategy abstraction
#[async_trait]
pub trait GraphTraverser: Send + Sync {
    /// Traverse graph using specified strategy
    async fn traverse(
        &self,
        graph: &StateGraph,
        start_node: &str,
        strategy: TraversalStrategy,
    ) -> Result<TraversalResult>;

    /// Find optimal path between nodes
    async fn find_path(
        &self,
        graph: &StateGraph,
        from: &str,
        to: &str,
        constraints: PathConstraints,
    ) -> Result<Vec<String>>;

    /// Detect cycles in graph
    async fn detect_cycles(&self, graph: &StateGraph) -> Result<Vec<Vec<String>>>;

    /// Analyze graph structure and properties
    async fn analyze_graph(&self, graph: &StateGraph) -> Result<GraphAnalysis>;
}

/// Tool execution abstraction for different tool types
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// Execute a tool with given parameters
    async fn execute_tool(
        &self,
        tool_name: &str,
        parameters: Value,
        context: ToolExecutionContext,
    ) -> Result<ToolExecutionResult>;

    /// Validate tool parameters before execution
    async fn validate_parameters(
        &self,
        tool_name: &str,
        parameters: &Value,
    ) -> Result<ValidationResult>;

    /// Get available tools and their specifications
    async fn get_available_tools(&self) -> Result<Vec<ToolSpecification>>;

    /// Register a new tool
    async fn register_tool(
        &mut self,
        spec: ToolSpecification,
        executor: Box<dyn ToolFunction>,
    ) -> Result<()>;

    /// Remove a tool
    async fn unregister_tool(&mut self, tool_name: &str) -> Result<()>;
}

// Supporting types and enums

/// Execution strategy for different processing modes
#[derive(Debug, Clone)]
pub enum ExecutionStrategy {
    /// Execute nodes sequentially
    Sequential,
    /// Execute nodes in parallel with specified concurrency
    Parallel { max_concurrency: usize },
    /// Execute with custom pipeline stages
    Pipeline { stages: Vec<String> },
    /// Execute with adaptive strategy based on load
    Adaptive {
        base_strategy: Box<ExecutionStrategy>,
    },
}

/// Execution context providing runtime information
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// Unique execution ID
    pub execution_id: String,
    /// Graph being executed
    pub graph_id: String,
    /// Current step in execution
    pub step: usize,
    /// Execution metadata
    pub metadata: HashMap<String, Value>,
    /// Timeout for this execution
    pub timeout: Option<Duration>,
}

/// Result of node execution
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// Whether execution was successful
    pub success: bool,
    /// Output data from execution
    pub output: Option<Value>,
    /// Execution duration
    pub duration: Duration,
    /// Error information if failed
    pub error: Option<String>,
    /// Execution metadata
    pub metadata: HashMap<String, Value>,
}

/// Execution engine capabilities
#[derive(Debug, Clone)]
pub struct ExecutionCapabilities {
    /// Supports parallel execution
    pub supports_parallel: bool,
    /// Maximum concurrency level
    pub max_concurrency: Option<usize>,
    /// Supports streaming execution
    pub supports_streaming: bool,
    /// Supports checkpoint/resume
    pub supports_checkpointing: bool,
    /// Supported node types
    pub supported_node_types: Vec<String>,
}

/// State reducer operation
#[derive(Debug, Clone)]
pub enum ReducerOperation {
    /// Append value to array
    Append(Value),
    /// Merge objects
    Merge(Value),
    /// Replace entire value
    Replace(Value),
    /// Custom operation with function
    Custom(String, Value),
}

/// State checkpoint for persistence
#[derive(Debug, Clone)]
pub struct StateCheckpoint {
    /// Checkpoint ID
    pub id: String,
    /// Checkpoint timestamp
    pub timestamp: u64,
    /// Serialized state data
    pub data: Vec<u8>,
    /// Checkpoint metadata
    pub metadata: HashMap<String, String>,
}

/// State version information
#[derive(Debug, Clone)]
pub struct StateVersion {
    /// Version ID
    pub id: u64,
    /// Parent version ID
    pub parent_id: Option<u64>,
    /// Version timestamp
    pub timestamp: u64,
    /// Change description
    pub description: String,
    /// Change author
    pub author: String,
}

/// Validation result for state or parameters
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Whether validation passed
    pub valid: bool,
    /// Validation errors
    pub errors: Vec<String>,
    /// Validation warnings
    pub warnings: Vec<String>,
}

/// Retry configuration
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum retry attempts
    pub max_attempts: usize,
    /// Initial delay between retries
    pub initial_delay: Duration,
    /// Backoff multiplier
    pub backoff_multiplier: f64,
    /// Maximum delay between retries
    pub max_delay: Duration,
    /// Whether to add jitter
    pub jitter: bool,
}

/// Resilience status information
#[derive(Debug, Clone)]
pub struct ResilienceStatus {
    /// Circuit breaker states
    pub circuit_breakers: HashMap<String, CircuitBreakerState>,
    /// Active retry operations
    pub active_retries: usize,
    /// Total operations processed
    pub total_operations: u64,
    /// Failed operations count
    pub failed_operations: u64,
}

/// Circuit breaker state
#[derive(Debug, Clone)]
pub enum CircuitBreakerState {
    /// Circuit is closed (normal operation)
    Closed,
    /// Circuit is open (failing fast)
    Open { open_since: u64 },
    /// Circuit is half-open (testing)
    HalfOpen,
}

/// Resilience configuration
#[derive(Debug, Clone)]
pub struct ResilienceConfig {
    /// Default retry configuration
    pub default_retry: RetryConfig,
    /// Circuit breaker thresholds
    pub circuit_breaker_threshold: usize,
    /// Circuit breaker timeout
    pub circuit_breaker_timeout: Duration,
    /// Global timeout for operations
    pub global_timeout: Duration,
}

/// Stream processing traits
pub trait Stream<Item>: Send + Sync {
    /// Get next item from stream
    fn next(&mut self) -> Option<Item>;
    /// Check if stream has more items
    fn has_more(&self) -> bool;
}

pub trait StreamItemProcessor<T>: Send + Sync {
    /// Process a single stream item
    fn process(&mut self, item: T) -> Result<()>;
}

pub trait BufferedStream<T>: Stream<T> {
    /// Add item to buffer
    fn push(&mut self, item: T) -> Result<()>;
    /// Get buffer size
    fn buffer_size(&self) -> usize;
    /// Check if buffer is full
    fn is_buffer_full(&self) -> bool;
}

/// Flow control configuration
#[derive(Debug, Clone)]
pub struct FlowControlConfig {
    /// Maximum items per second
    pub max_rate: Option<f64>,
    /// Buffer size
    pub buffer_size: usize,
    /// Backpressure strategy
    pub backpressure_strategy: BackpressureStrategy,
}

/// Backpressure handling strategies
#[derive(Debug, Clone)]
pub enum BackpressureStrategy {
    /// Block until buffer has space
    Block,
    /// Drop oldest items
    DropOldest,
    /// Drop newest items
    DropNewest,
    /// Error on buffer full
    Error,
}

/// Stream processing metrics
#[derive(Debug, Clone)]
pub struct StreamMetrics {
    /// Items processed
    pub items_processed: u64,
    /// Items dropped
    pub items_dropped: u64,
    /// Average processing time
    pub avg_processing_time: Duration,
    /// Current buffer utilization
    pub buffer_utilization: f64,
}

/// Graph traversal strategies
#[derive(Debug, Clone)]
pub enum TraversalStrategy {
    /// Breadth-first search
    BreadthFirst,
    /// Depth-first search
    DepthFirst,
    /// Topological ordering
    Topological,
    /// Custom strategy with priorities
    Custom { priorities: HashMap<String, i32> },
}

/// Path finding constraints
#[derive(Debug, Clone)]
pub struct PathConstraints {
    /// Maximum path length
    pub max_length: Option<usize>,
    /// Excluded nodes
    pub excluded_nodes: Vec<String>,
    /// Required intermediate nodes
    pub required_nodes: Vec<String>,
    /// Path weight limit
    pub max_weight: Option<f64>,
}

/// Graph traversal result
#[derive(Debug, Clone)]
pub struct TraversalResult {
    /// Visited nodes in order
    pub visited_nodes: Vec<String>,
    /// Total traversal time
    pub duration: Duration,
    /// Path weights if applicable
    pub path_weights: Option<Vec<f64>>,
    /// Traversal metadata
    pub metadata: HashMap<String, Value>,
}

/// Graph analysis results
#[derive(Debug, Clone)]
pub struct GraphAnalysis {
    /// Number of nodes
    pub node_count: usize,
    /// Number of edges
    pub edge_count: usize,
    /// Graph density
    pub density: f64,
    /// Strongly connected components
    pub strongly_connected_components: Vec<Vec<String>>,
    /// Critical path
    pub critical_path: Option<Vec<String>>,
    /// Graph metrics
    pub metrics: HashMap<String, f64>,
}

/// Tool execution context
#[derive(Debug, Clone)]
pub struct ToolExecutionContext {
    /// Execution ID
    pub execution_id: String,
    /// Current state
    pub state: StateData,
    /// Tool metadata
    pub metadata: HashMap<String, Value>,
    /// Execution timeout
    pub timeout: Option<Duration>,
}

/// Tool execution result
#[derive(Debug, Clone)]
pub struct ToolExecutionResult {
    /// Whether execution was successful
    pub success: bool,
    /// Tool output
    pub output: Option<Value>,
    /// Execution duration
    pub duration: Duration,
    /// Error message if failed
    pub error: Option<String>,
    /// Result metadata
    pub metadata: HashMap<String, Value>,
}

/// Tool specification
#[derive(Debug, Clone)]
pub struct ToolSpecification {
    /// Tool name
    pub name: String,
    /// Tool description
    pub description: String,
    /// Input parameters schema
    pub parameters_schema: Value,
    /// Output schema
    pub output_schema: Option<Value>,
    /// Tool metadata
    pub metadata: HashMap<String, Value>,
}

/// Tool function trait
#[async_trait]
pub trait ToolFunction: Send + Sync {
    /// Execute the tool function
    async fn execute(
        &self,
        parameters: Value,
        context: ToolExecutionContext,
    ) -> Result<ToolExecutionResult>;
    /// Validate parameters
    async fn validate(&self, parameters: &Value) -> Result<ValidationResult>;
}

/// Metrics formats for export
#[derive(Debug, Clone)]
pub enum MetricsFormat {
    /// Prometheus format
    Prometheus,
    /// JSON format
    Json,
    /// CSV format
    Csv,
    /// Custom format
    Custom(String),
}

/// Error context for metrics
#[derive(Debug, Clone)]
pub struct ErrorContext {
    /// Node ID where error occurred
    pub node_id: Option<String>,
    /// Error severity
    pub severity: ErrorSeverity,
    /// Additional context
    pub context: HashMap<String, Value>,
}

/// Error severity levels
#[derive(Debug, Clone)]
pub enum ErrorSeverity {
    /// Low severity
    Low,
    /// Medium severity
    Medium,
    /// High severity
    High,
    /// Critical severity
    Critical,
}

/// Metrics analysis results for traits
#[derive(Debug, Clone)]
pub struct TraitsMetricsAnalysis {
    /// Analysis time window
    pub time_window: Duration,
    /// Total operations in window
    pub total_operations: u64,
    /// Success rate percentage
    pub success_rate: f64,
    /// Average response time
    pub avg_response_time: Duration,
    /// Peak throughput
    pub peak_throughput: f64,
    /// Error breakdown by type
    pub error_breakdown: HashMap<String, u64>,
}

/// Default implementations of traits for common use cases
impl Default for ExecutionStrategy {
    fn default() -> Self {
        Self::Sequential
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(100),
            backoff_multiplier: 2.0,
            max_delay: Duration::from_secs(30),
            jitter: true,
        }
    }
}

impl Default for FlowControlConfig {
    fn default() -> Self {
        Self {
            max_rate: None,
            buffer_size: 1000,
            backpressure_strategy: BackpressureStrategy::Block,
        }
    }
}

impl Default for PathConstraints {
    fn default() -> Self {
        Self {
            max_length: None,
            excluded_nodes: Vec::new(),
            required_nodes: Vec::new(),
            max_weight: None,
        }
    }
}
