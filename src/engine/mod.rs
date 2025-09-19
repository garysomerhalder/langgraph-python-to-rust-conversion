//! Graph execution engine
//!
//! This module provides the execution runtime for LangGraph workflows.

use async_trait::async_trait;

use crate::Result;

pub mod executor;
pub mod executor_hil;
pub mod executor_breakpoint;
pub mod executor_inspector;
pub mod node_executor;
pub mod context;
pub mod graph_traversal;
pub mod parallel_executor;
pub mod resilience;
pub mod tracing;
pub mod rate_limiter;
pub mod metrics;
pub mod traits;
pub mod human_in_loop;
pub mod breakpoint;
pub mod state_inspector;
pub mod state_diff;

pub use executor::{
    ExecutionEngine, ExecutionContext, ExecutionMessage, 
    ExecutionMetadata, ExecutionStatus, ExecutionError
};
pub use node_executor::{NodeExecutor, DefaultNodeExecutor, ParallelNodeExecutor, RetryNodeExecutor};
pub use context::{SharedContext, ContextConfig, RetryConfig, MessageBus, ExecutionScope};
pub use parallel_executor::{
    ParallelExecutor, ExecutionMetrics, DependencyAnalyzer,
    StateVersionManager, StateVersion, DeadlockDetector
};
pub use resilience::{
    CircuitBreaker, CircuitBreakerConfig, CircuitState, CircuitMetrics,
    RetryExecutor, RetryConfig as ResilienceRetryConfig, Bulkhead, BulkheadMetrics,
    ResilienceManager, ResilienceError, HealthCheck, HealthStatus
};
pub use tracing::{
    TraceContext, Span, SpanEvent, SpanStatus, Tracer, SpanHandle,
    SpanExporter, ConsoleSpanExporter, JsonSpanExporter, TracingMetrics,
    InstrumentedExecutor, ContextPropagator
};
pub use rate_limiter::{
    RateLimiter, RateLimitPermit, RateLimitError, AdaptiveRateLimiter
};
pub use metrics::{
    MetricsCollector, Timer, GlobalMetrics, export_metrics
};
pub use traits::{
    ExecutionEngine as ExecutionEngineTrait, StateManager, MetricsCollector as MetricsCollectorTrait,
    ResilienceProvider, StreamProcessor, GraphTraverser, ToolExecutor,
    ExecutionStrategy, ExecutionContext as TraitExecutionContext, ExecutionResult,
    ExecutionCapabilities, ValidationResult, RetryConfig as TraitRetryConfig
};
pub use human_in_loop::{
    InterruptMode, InterruptHandle, ApprovalDecision, InterruptCallback,
    InterruptManager, HumanInLoopExecution, ExecutionHandle, InterruptError
};
pub use breakpoint::{
    BreakpointManager, Breakpoint, BreakpointCondition, BreakpointAction,
    BreakpointHit, BreakpointCallback, BreakpointExecution, BreakpointError
};
pub use state_inspector::{
    StateInspector, StateSnapshot
};
pub use state_diff::{
    StateDiff, ExportFormat, StateFilter
};

/// Trait for executable graph components
#[async_trait]
pub trait Executable {
    /// Execute the component
    async fn execute(&self) -> Result<()>;
}