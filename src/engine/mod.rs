//! Graph execution engine
//!
//! This module provides the execution runtime for LangGraph workflows.

use async_trait::async_trait;

use crate::Result;

pub mod breakpoint;
pub mod context;
pub mod executor;
pub mod executor_breakpoint;
pub mod executor_hil;
pub mod executor_inspector;
pub mod graph_traversal;
pub mod human_in_loop;
pub mod metrics;
pub mod node_executor;
pub mod parallel_executor;
pub mod rate_limiter;
pub mod resilience;
pub mod resumption;
pub mod state_diff;
pub mod state_inspector;
pub mod tracing;
pub mod traits;
pub mod user_feedback;

pub use breakpoint::{
    Breakpoint, BreakpointAction, BreakpointCallback, BreakpointCondition, BreakpointError,
    BreakpointExecution, BreakpointHit, BreakpointManager,
};
pub use context::{ContextConfig, ExecutionScope, MessageBus, RetryConfig, SharedContext};
pub use executor::{
    ExecutionContext, ExecutionEngine, ExecutionError, ExecutionMessage, ExecutionMetadata,
    ExecutionStatus,
};
pub use human_in_loop::{
    ApprovalDecision, ExecutionHandle, HumanInLoopExecution, InterruptCallback, InterruptError,
    InterruptHandle, InterruptManager, InterruptMode,
};
pub use metrics::{export_metrics, GlobalMetrics, MetricsCollector, Timer};
pub use node_executor::{
    DefaultNodeExecutor, NodeExecutor, ParallelNodeExecutor, RetryNodeExecutor,
};
pub use parallel_executor::{
    DeadlockDetector, DependencyAnalyzer, ExecutionMetrics, ParallelExecutor, StateVersion,
    StateVersionManager,
};
pub use rate_limiter::{AdaptiveRateLimiter, RateLimitError, RateLimitPermit, RateLimiter};
pub use resilience::{
    Bulkhead, BulkheadMetrics, CircuitBreaker, CircuitBreakerConfig, CircuitMetrics, CircuitState,
    HealthCheck, HealthStatus, ResilienceError, ResilienceManager,
    RetryConfig as ResilienceRetryConfig, RetryExecutor,
};
pub use resumption::{ResumptionManager, ResumptionPoint, WorkflowSnapshot};
pub use state_diff::{ExportFormat, StateDiff, StateFilter};
pub use state_inspector::{StateInspector, StateSnapshot};
pub use tracing::{
    ConsoleSpanExporter, ContextPropagator, InstrumentedExecutor, JsonSpanExporter, Span,
    SpanEvent, SpanExporter, SpanHandle, SpanStatus, TraceContext, Tracer, TracingMetrics,
};
pub use traits::{
    ExecutionCapabilities, ExecutionContext as TraitExecutionContext,
    ExecutionEngine as ExecutionEngineTrait, ExecutionResult, ExecutionStrategy, GraphTraverser,
    MetricsCollector as MetricsCollectorTrait, ResilienceProvider, RetryConfig as TraitRetryConfig,
    StateManager, StreamProcessor, ToolExecutor, ValidationResult,
};
pub use user_feedback::{
    FeedbackHistory, FeedbackManager, FeedbackPerformanceMetrics, FeedbackRequest,
    FeedbackRequestStatus, FeedbackStats, FeedbackType, UserFeedback,
};

/// Trait for executable graph components
#[async_trait]
pub trait Executable {
    /// Execute the component
    async fn execute(&self) -> Result<()>;
}
