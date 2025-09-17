use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use langgraph::graph::{StateGraph, Node, NodeType, Edge, GraphBuilder};
use langgraph::state::{GraphState, StateData};
use langgraph::engine::executor::ExecutionEngine;
use langgraph::engine::parallel_executor::{ParallelExecutor, ExecutionMetrics};
use langgraph::engine::resilience::{ResilienceManager, RetryConfig, CircuitBreakerConfig};
use langgraph::engine::tracing::{TracingManager, TraceContext};
use langgraph::engine::rate_limiter::RateLimiter;
use langgraph::engine::metrics::MetricsCollector;
use langgraph::state::validation::{StateValidator, ValidationRuleBuilder, ValueType};
use langgraph::utils::object_pool::{ObjectPool, pools};
use serde_json::json;
use std::collections::HashMap;
use std::time::Duration;
use tokio::runtime::Runtime;

// State operations benchmarks
fn benchmark_state_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("state_operations");
    
    // Benchmark state creation
    group.bench_function("state_creation", |b| {
        b.iter(|| {
            let state = GraphState::new();
            black_box(state)
        })
    });
    
    // Benchmark state updates
    group.bench_function("state_single_update", |b| {
        let mut state = GraphState::new();
        b.iter(|| {
            state.set("key", json!("value"));
        })
    });
    
    // Benchmark bulk state updates
    group.bench_function("state_bulk_update", |b| {
        let mut state = GraphState::new();
        let updates: StateData = (0..100)
            .map(|i| (format!("key_{}", i), json!(i)))
            .collect();
        
        b.iter(|| {
            state.update(black_box(updates.clone()));
        })
    });
    
    // Benchmark state retrieval
    group.bench_function("state_retrieval", |b| {
        let mut state = GraphState::new();
        for i in 0..100 {
            state.set(format!("key_{}", i), json!(i));
        }
        
        b.iter(|| {
            for i in 0..100 {
                let _ = state.get(&format!("key_{}", i));
            }
        })
    });
    
    group.finish();
}

// Validation benchmarks
fn benchmark_validation(c: &mut Criterion) {
    let mut group = c.benchmark_group("validation");
    
    // Setup validator
    let mut validator = StateValidator::new();
    validator.add_rule(
        ValidationRuleBuilder::new("email")
            .with_type(ValueType::String)
            .with_pattern(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$")
            .required()
            .build()
    );
    validator.add_rule(
        ValidationRuleBuilder::new("age")
            .with_type(ValueType::Number)
            .with_range(Some(0.0), Some(150.0))
            .build()
    );
    
    let valid_state: StateData = HashMap::from([
        ("email".to_string(), json!("test@example.com")),
        ("age".to_string(), json!(25)),
    ]);
    
    group.bench_function("validation_valid", |b| {
        b.iter(|| {
            let result = validator.validate(&HashMap::new(), &valid_state);
            black_box(result)
        })
    });
    
    let invalid_state: StateData = HashMap::from([
        ("email".to_string(), json!("invalid-email")),
        ("age".to_string(), json!(200)),
    ]);
    
    group.bench_function("validation_invalid", |b| {
        b.iter(|| {
            let result = validator.validate(&HashMap::new(), &invalid_state);
            black_box(result)
        })
    });
    
    group.finish();
}

// Object pool benchmarks
fn benchmark_object_pools(c: &mut Criterion) {
    let mut group = c.benchmark_group("object_pools");
    
    // Benchmark HashMap pool
    group.bench_function("hashmap_pool_get_return", |b| {
        b.iter(|| {
            let map = pools::STATE_MAP_POOL.get();
            black_box(&map);
            drop(map);
        })
    });
    
    // Benchmark without pool
    group.bench_function("hashmap_no_pool", |b| {
        b.iter(|| {
            let map: HashMap<String, serde_json::Value> = HashMap::with_capacity(32);
            black_box(map);
        })
    });
    
    // Benchmark string pool
    group.bench_function("string_pool_get_return", |b| {
        b.iter(|| {
            let mut s = pools::STRING_POOL.get();
            s.push_str("test data");
            black_box(&s);
            drop(s);
        })
    });
    
    // Benchmark buffer pool
    group.bench_function("buffer_pool_get_return", |b| {
        b.iter(|| {
            let mut buf = pools::BUFFER_POOL.get();
            buf.extend_from_slice(b"test data");
            black_box(&buf);
            drop(buf);
        })
    });
    
    group.finish();
}

// Rate limiter benchmarks
fn benchmark_rate_limiter(c: &mut Criterion) {
    let mut group = c.benchmark_group("rate_limiter");
    let rt = Runtime::new().unwrap();
    
    // Benchmark basic rate limiter
    group.bench_function("rate_limiter_acquire", |b| {
        let limiter = RateLimiter::new(10000, Duration::from_secs(1));
        
        b.to_async(&rt).iter(|| async {
            let permit = limiter.acquire().await;
            black_box(permit)
        })
    });
    
    // Benchmark try_acquire
    group.bench_function("rate_limiter_try_acquire", |b| {
        let limiter = RateLimiter::new(10000, Duration::from_secs(1));
        
        b.iter(|| {
            let result = limiter.try_acquire();
            black_box(result)
        })
    });
    
    group.finish();
}

// Metrics collection benchmarks
fn benchmark_metrics(c: &mut Criterion) {
    let mut group = c.benchmark_group("metrics");
    let collector = MetricsCollector::new();
    
    group.bench_function("metrics_record_execution", |b| {
        b.iter(|| {
            collector.record_execution_time("test_node", Duration::from_millis(10));
        })
    });
    
    group.bench_function("metrics_increment_counter", |b| {
        b.iter(|| {
            collector.increment_node_executions("test_node");
        })
    });
    
    group.bench_function("metrics_record_error", |b| {
        b.iter(|| {
            collector.record_error("test_error");
        })
    });
    
    group.bench_function("metrics_export", |b| {
        // Add some data
        for i in 0..100 {
            collector.record_execution_time(&format!("node_{}", i), Duration::from_millis(i as u64));
        }
        
        b.iter(|| {
            let metrics = collector.export_metrics();
            black_box(metrics)
        })
    });
    
    group.finish();
}

// Parallel execution benchmarks
fn benchmark_parallel_execution(c: &mut Criterion) {
    let mut group = c.benchmark_group("parallel_execution");
    let rt = Runtime::new().unwrap();
    
    // Create different sized graphs
    for size in [5, 10, 20, 50].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            size,
            |b, &size| {
                let graph = create_parallel_graph(size);
                let compiled = graph.compile().unwrap();
                let executor = ParallelExecutor::new(4);
                
                b.to_async(&rt).iter(|| async {
                    let state = GraphState::new();
                    let result = executor.execute(&compiled, state).await;
                    black_box(result)
                })
            },
        );
    }
    
    group.finish();
}

// Resilience patterns benchmarks
fn benchmark_resilience(c: &mut Criterion) {
    let mut group = c.benchmark_group("resilience");
    let rt = Runtime::new().unwrap();
    
    // Circuit breaker benchmarks
    group.bench_function("circuit_breaker_closed", |b| {
        let manager = ResilienceManager::new()
            .with_circuit_breaker(5, Duration::from_secs(60));
        
        b.to_async(&rt).iter(|| async {
            let result = manager.execute("test", || async {
                Ok::<_, Box<dyn std::error::Error>>(42)
            }).await;
            black_box(result)
        })
    });
    
    // Retry benchmarks
    group.bench_function("retry_success", |b| {
        let manager = ResilienceManager::new()
            .with_retry(RetryConfig::exponential(3, Duration::from_millis(1)));
        
        b.to_async(&rt).iter(|| async {
            let result = manager.execute("test", || async {
                Ok::<_, Box<dyn std::error::Error>>(42)
            }).await;
            black_box(result)
        })
    });
    
    group.finish();
}

// Graph traversal benchmarks
fn benchmark_graph_traversal(c: &mut Criterion) {
    let mut group = c.benchmark_group("graph_traversal");
    
    for size in [10, 50, 100, 500].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            size,
            |b, &size| {
                let graph = create_linear_graph(size);
                let compiled = graph.compile().unwrap();
                
                b.iter(|| {
                    let order = compiled.get_topological_order();
                    black_box(order)
                })
            },
        );
    }
    
    group.finish();
}

// Helper functions
fn create_parallel_graph(size: usize) -> StateGraph {
    let mut builder = GraphBuilder::new("parallel_graph");
    
    // Add nodes that can run in parallel
    for i in 0..size {
        builder = builder.add_node(format!("node_{}", i), NodeType::Agent(format!("agent_{}", i)));
        builder = builder.add_edge("__start__", format!("node_{}", i));
        builder = builder.add_edge(format!("node_{}", i), "__end__");
    }
    
    builder.build().unwrap()
}

fn create_linear_graph(size: usize) -> StateGraph {
    let mut builder = GraphBuilder::new("linear_graph");
    
    if size > 0 {
        builder = builder.add_node("node_0", NodeType::Agent("agent_0".to_string()));
        builder = builder.add_edge("__start__", "node_0");
        
        for i in 1..size {
            builder = builder.add_node(
                format!("node_{}", i),
                NodeType::Agent(format!("agent_{}", i))
            );
            builder = builder.add_edge(
                format!("node_{}", i - 1),
                format!("node_{}", i)
            );
        }
        
        builder = builder.add_edge(format!("node_{}", size - 1), "__end__");
    }
    
    builder.build().unwrap()
}

// Combined benchmark groups
criterion_group!(
    benches,
    benchmark_state_operations,
    benchmark_validation,
    benchmark_object_pools,
    benchmark_rate_limiter,
    benchmark_metrics,
    benchmark_parallel_execution,
    benchmark_resilience,
    benchmark_graph_traversal
);

criterion_main!(benches);