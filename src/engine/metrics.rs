//! Metrics collection for graph execution
//!
//! Provides Prometheus-compatible metrics for monitoring graph execution
//! performance, resource usage, and error rates.

use lazy_static::lazy_static;
use prometheus::{
    register_counter_vec, register_gauge_vec, register_histogram_vec,
    register_int_counter_vec, register_int_gauge,
    CounterVec, GaugeVec, HistogramVec, IntCounterVec, IntGauge,
    Encoder, TextEncoder,
};
use std::time::Instant;

lazy_static! {
    /// Counter for total graph executions
    static ref GRAPH_EXECUTIONS: IntCounterVec = register_int_counter_vec!(
        "langgraph_executions_total",
        "Total number of graph executions",
        &["graph_name", "status"]
    ).unwrap();
    
    /// Histogram for execution duration
    static ref EXECUTION_DURATION: HistogramVec = register_histogram_vec!(
        "langgraph_execution_duration_seconds",
        "Graph execution duration in seconds",
        &["graph_name"],
        vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 10.0]
    ).unwrap();
    
    /// Counter for node executions
    static ref NODE_EXECUTIONS: IntCounterVec = register_int_counter_vec!(
        "langgraph_node_executions_total",
        "Total number of node executions",
        &["node_type", "status"]
    ).unwrap();
    
    /// Histogram for node execution duration
    static ref NODE_DURATION: HistogramVec = register_histogram_vec!(
        "langgraph_node_duration_seconds",
        "Node execution duration in seconds",
        &["node_type"],
        vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0]
    ).unwrap();
    
    /// Gauge for active executions
    static ref ACTIVE_EXECUTIONS: IntGauge = register_int_gauge!(
        "langgraph_active_executions",
        "Number of currently active graph executions"
    ).unwrap();
    
    /// Counter for state operations
    static ref STATE_OPERATIONS: CounterVec = register_counter_vec!(
        "langgraph_state_operations_total",
        "Total number of state operations",
        &["operation"]
    ).unwrap();
    
    /// Gauge for state size
    static ref STATE_SIZE: GaugeVec = register_gauge_vec!(
        "langgraph_state_size_bytes",
        "Current state size in bytes",
        &["graph_name"]
    ).unwrap();
    
    /// Counter for errors
    static ref ERRORS: IntCounterVec = register_int_counter_vec!(
        "langgraph_errors_total",
        "Total number of errors",
        &["error_type", "component"]
    ).unwrap();
    
    /// Histogram for checkpoint operations
    static ref CHECKPOINT_DURATION: HistogramVec = register_histogram_vec!(
        "langgraph_checkpoint_duration_seconds",
        "Checkpoint operation duration in seconds",
        &["operation"],
        vec![0.001, 0.01, 0.1, 1.0]
    ).unwrap();
    
    /// Counter for circuit breaker state changes
    static ref CIRCUIT_BREAKER_STATE: IntCounterVec = register_int_counter_vec!(
        "langgraph_circuit_breaker_state_changes",
        "Circuit breaker state changes",
        &["from_state", "to_state"]
    ).unwrap();
    
    /// Counter for rate limit hits
    static ref RATE_LIMIT_HITS: IntCounterVec = register_int_counter_vec!(
        "langgraph_rate_limit_hits_total",
        "Total number of rate limit hits",
        &["limiter_name"]
    ).unwrap();
}

/// Metrics collector for graph execution
pub struct MetricsCollector {
    graph_name: String,
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub fn new(graph_name: String) -> Self {
        Self { graph_name }
    }
    
    /// Record graph execution start
    pub fn record_execution_start(&self) {
        ACTIVE_EXECUTIONS.inc();
    }
    
    /// Record graph execution end
    pub fn record_execution_end(&self, status: &str, duration: f64) {
        ACTIVE_EXECUTIONS.dec();
        GRAPH_EXECUTIONS
            .with_label_values(&[&self.graph_name, status])
            .inc();
        EXECUTION_DURATION
            .with_label_values(&[&self.graph_name])
            .observe(duration);
    }
    
    /// Record node execution
    pub fn record_node_execution(&self, node_type: &str, status: &str, duration: f64) {
        NODE_EXECUTIONS
            .with_label_values(&[node_type, status])
            .inc();
        NODE_DURATION
            .with_label_values(&[node_type])
            .observe(duration);
    }
    
    /// Record state operation
    pub fn record_state_operation(&self, operation: &str) {
        STATE_OPERATIONS
            .with_label_values(&[operation])
            .inc();
    }
    
    /// Update state size
    pub fn update_state_size(&self, size_bytes: f64) {
        STATE_SIZE
            .with_label_values(&[&self.graph_name])
            .set(size_bytes);
    }
    
    /// Record error
    pub fn record_error(&self, error_type: &str, component: &str) {
        ERRORS
            .with_label_values(&[error_type, component])
            .inc();
    }
    
    /// Record checkpoint operation
    pub fn record_checkpoint(&self, operation: &str, duration: f64) {
        CHECKPOINT_DURATION
            .with_label_values(&[operation])
            .observe(duration);
    }
    
    /// Record circuit breaker state change
    pub fn record_circuit_breaker_state(&self, from: &str, to: &str) {
        CIRCUIT_BREAKER_STATE
            .with_label_values(&[from, to])
            .inc();
    }
    
    /// Record rate limit hit
    pub fn record_rate_limit_hit(&self, limiter_name: &str) {
        RATE_LIMIT_HITS
            .with_label_values(&[limiter_name])
            .inc();
    }
}

/// Timer for measuring operation duration
pub struct Timer {
    start: Instant,
}

impl Timer {
    /// Start a new timer
    #[inline]
    pub fn start() -> Self {
        Self {
            start: Instant::now(),
        }
    }
    
    /// Get elapsed time in seconds
    #[inline]
    pub fn elapsed_secs(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }
}

/// SIMD-optimized batch metrics processor for high-performance analytics
pub struct BatchMetricsProcessor;

impl BatchMetricsProcessor {
    /// Process multiple timing measurements using SIMD optimizations
    pub fn process_timings(timings: &[f64]) -> MetricsAnalysis {
        if timings.is_empty() {
            return MetricsAnalysis::default();
        }
        
        let count = timings.len() as f64;
        let sum = crate::utils::simd_ops::SimdMath::vector_sum(timings);
        let mean = sum / count;
        
        // Calculate variance using SIMD
        let mean_vec = vec![mean; timings.len()];
        let diff_vec = crate::utils::simd_ops::SimdMath::vector_elementwise_op(
            timings, 
            &mean_vec, 
            crate::utils::simd_ops::VectorOp::Subtract
        );
        let squared_diffs = crate::utils::simd_ops::SimdMath::vector_elementwise_op(
            &diff_vec, 
            &diff_vec, 
            crate::utils::simd_ops::VectorOp::Multiply
        );
        let variance = crate::utils::simd_ops::SimdMath::vector_sum(&squared_diffs) / count;
        let std_dev = variance.sqrt();
        
        let min = timings.iter().copied().fold(f64::INFINITY, f64::min);
        let max = timings.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        
        // Calculate percentiles
        let mut sorted_timings = timings.to_vec();
        sorted_timings.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let len = sorted_timings.len();
        let p50 = sorted_timings[len / 2];
        let p95 = sorted_timings[(len as f64 * 0.95) as usize];
        let p99 = sorted_timings[(len as f64 * 0.99) as usize];
        
        MetricsAnalysis {
            count,
            sum,
            mean,
            std_dev,
            min,
            max,
            p50,
            p95,
            p99,
        }
    }
    
    /// Process multiple throughput measurements
    pub fn process_throughput(values: &[f64], time_window_secs: f64) -> ThroughputMetrics {
        let total_ops = crate::utils::simd_ops::SimdMath::vector_sum(values);
        let avg_ops_per_sec = total_ops / time_window_secs;
        
        // Calculate moving average using SIMD
        let window_size = 10.min(values.len());
        let moving_avgs = if values.len() >= window_size {
            values.windows(window_size)
                .map(|window| crate::utils::simd_ops::SimdMath::vector_sum(window) / window_size as f64)
                .collect()
        } else {
            vec![avg_ops_per_sec]
        };
        
        ThroughputMetrics {
            total_operations: total_ops,
            avg_ops_per_sec,
            peak_ops_per_sec: moving_avgs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            moving_averages: moving_avgs,
        }
    }
    
    /// Batch normalize metrics for comparison
    pub fn normalize_metrics(values: &[f64]) -> Vec<f64> {
        crate::utils::simd_ops::SimdBatch::batch_process_values(
            values, 
            crate::utils::simd_ops::BatchOp::Normalize
        )
    }
}

/// Statistical analysis of metrics
#[derive(Debug, Clone)]
pub struct MetricsAnalysis {
    /// Number of samples
    pub count: f64,
    /// Sum of all values
    pub sum: f64,
    /// Mean value
    pub mean: f64,
    /// Standard deviation
    pub std_dev: f64,
    /// Minimum value
    pub min: f64,
    /// Maximum value
    pub max: f64,
    /// 50th percentile (median)
    pub p50: f64,
    /// 95th percentile
    pub p95: f64,
    /// 99th percentile
    pub p99: f64,
}

impl Default for MetricsAnalysis {
    fn default() -> Self {
        Self {
            count: 0.0,
            sum: 0.0,
            mean: 0.0,
            std_dev: 0.0,
            min: 0.0,
            max: 0.0,
            p50: 0.0,
            p95: 0.0,
            p99: 0.0,
        }
    }
}

/// Throughput metrics analysis
#[derive(Debug, Clone)]
pub struct ThroughputMetrics {
    /// Total number of operations
    pub total_operations: f64,
    /// Average operations per second
    pub avg_ops_per_sec: f64,
    /// Peak operations per second
    pub peak_ops_per_sec: f64,
    /// Moving averages over time windows
    pub moving_averages: Vec<f64>,
}

/// Export metrics in Prometheus format
pub fn export_metrics() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();
    
    encoder.encode(&metric_families, &mut buffer)
        .map_err(|e| format!("Failed to encode metrics: {}", e))?;
    
    String::from_utf8(buffer)
        .map_err(|e| format!("Failed to convert metrics to UTF-8: {}", e).into())
}

/// Global metrics instance
pub struct GlobalMetrics {
    collector: Option<MetricsCollector>,
}

impl GlobalMetrics {
    /// Create a new global metrics instance
    pub fn new() -> Self {
        Self { collector: None }
    }
    
    /// Initialize with a graph name
    pub fn init(&mut self, graph_name: String) {
        self.collector = Some(MetricsCollector::new(graph_name));
    }
    
    /// Get the metrics collector
    pub fn collector(&self) -> Option<&MetricsCollector> {
        self.collector.as_ref()
    }
}

impl Default for GlobalMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_metrics_collector() {
        let collector = MetricsCollector::new("test_graph".to_string());
        
        // Record some metrics
        collector.record_execution_start();
        collector.record_node_execution("agent", "success", 0.1);
        collector.record_state_operation("get");
        collector.update_state_size(1024.0);
        collector.record_execution_end("success", 1.5);
        
        // Export metrics
        let metrics = export_metrics().unwrap_or_else(|e| {
            eprintln!("Warning: Failed to export metrics: {}", e);
            String::new()
        });
        assert!(metrics.contains("langgraph_executions_total"));
        assert!(metrics.contains("test_graph"));
    }
    
    #[test]
    fn test_timer() {
        let timer = Timer::start();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let elapsed = timer.elapsed_secs();
        assert!(elapsed >= 0.01);
    }
}