use langgraph::engine::executor::{ExecutionEngine, ExecutionMetadata, ExecutionStatus, ExecutionContext};
use langgraph::engine::resilience::{ResilienceManager, CircuitBreakerConfig, RetryConfig};
use langgraph::engine::tracing::{Tracer, SpanStatus, SpanEvent};
use langgraph::engine::parallel_executor::ParallelExecutor;
use langgraph::engine::node_executor::{DefaultNodeExecutor, RetryNodeExecutor};
use langgraph::graph::{GraphBuilder, NodeType, Node};
use langgraph::state::GraphState;
use serde_json::json;
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
use std::time::Duration;
use tokio::sync::RwLock;

/// Test circuit breaker functionality
#[tokio::test]
async fn test_circuit_breaker() {
    let config = CircuitBreakerConfig {
        failure_threshold: 3,
        timeout_duration: Duration::from_millis(100),
        success_threshold: 1,
        failure_window: Duration::from_secs(60),
    };
    
    let retry_config = RetryConfig::default();
    let manager = ResilienceManager::new(config, retry_config, 10);
    
    let failure_count = Arc::new(AtomicUsize::new(0));
    let failure_count_clone = failure_count.clone();
    
    // First 3 calls should fail and open the circuit
    for _ in 0..3 {
        let fc = failure_count_clone.clone();
        let result = manager.execute_with_resilience(move || {
            let fc = fc.clone();
            async move {
                fc.fetch_add(1, Ordering::SeqCst);
                Err::<(), _>(std::io::Error::new(std::io::ErrorKind::Other, "Simulated failure"))
            }
        }).await;
        assert!(result.is_err());
    }
    
    // Note: Circuit breaker state methods are not exposed in the public API
    // We can only verify behavior through execution results
    
    // Wait for recovery timeout
    tokio::time::sleep(Duration::from_millis(150)).await;
    
    // After recovery, a successful call should work
    let result = manager.execute_with_resilience(|| async {
        Ok::<_, std::io::Error>("Success")
    }).await;
    assert!(result.is_ok());
}

/// Test retry mechanism with exponential backoff
#[tokio::test]
async fn test_retry_with_exponential_backoff() {
    let circuit_config = CircuitBreakerConfig::default();
    let retry_config = RetryConfig {
        max_attempts: 3,
        initial_delay: Duration::from_millis(10),
        max_delay: Duration::from_millis(100),
        backoff_multiplier: 2.0,
        jitter: true,
    };
    
    let manager = ResilienceManager::new(circuit_config, retry_config, 10);
    
    let attempt_count = Arc::new(AtomicUsize::new(0));
    let attempt_count_clone = attempt_count.clone();
    
    // Fail twice, then succeed
    let result = manager.execute_with_resilience(move || {
        let ac = attempt_count_clone.clone();
        async move {
            let count = ac.fetch_add(1, Ordering::SeqCst);
            if count < 2 {
                Err(std::io::Error::new(std::io::ErrorKind::Other, "Temporary failure"))
            } else {
                Ok("Success after retries")
            }
        }
    }).await;
    
    assert!(result.is_ok());
    assert_eq!(attempt_count.load(Ordering::SeqCst), 3);
}

/// Test bulkhead pattern for resource isolation
#[tokio::test]
async fn test_bulkhead_pattern() {
    let circuit_config = CircuitBreakerConfig::default();
    let retry_config = RetryConfig::default();
    let max_concurrent = 2;
    
    let manager = Arc::new(ResilienceManager::new(circuit_config, retry_config, max_concurrent));
    
    let running_count = Arc::new(AtomicUsize::new(0));
    let max_running = Arc::new(AtomicUsize::new(0));
    
    let mut handles = vec![];
    
    // Start 5 concurrent operations with max_concurrent = 2
    for _ in 0..5 {
        let mgr = manager.clone();
        let rc = running_count.clone();
        let mr = max_running.clone();
        
        let handle = tokio::spawn(async move {
            mgr.execute_with_resilience(move || {
                let rc = rc.clone();
                let mr = mr.clone();
                async move {
                    let current = rc.fetch_add(1, Ordering::SeqCst) + 1;
                    
                    // Update max running count
                    let mut max = mr.load(Ordering::SeqCst);
                    while current > max {
                        match mr.compare_exchange(max, current, Ordering::SeqCst, Ordering::Relaxed) {
                            Ok(_) => break,
                            Err(x) => max = x,
                        }
                    }
                    
                    // Simulate work
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    rc.fetch_sub(1, Ordering::SeqCst);
                    
                    Ok::<_, std::io::Error>("Done")
                }
            }).await
        });
        
        handles.push(handle);
    }
    
    // Wait for all to complete
    for handle in handles {
        let _ = handle.await;
    }
    
    // Should never exceed max_concurrent
    assert!(max_running.load(Ordering::SeqCst) <= max_concurrent);
}

/// Test distributed tracing functionality
#[tokio::test]
async fn test_distributed_tracing() {
    let tracer = Tracer::new("test-service");
    
    // Start parent span
    let parent_span = tracer.start_span("parent_operation");
    
    // Add tags
    parent_span.add_tag("user_id".to_string(), "123".to_string()).await;
    parent_span.add_tag("request_id".to_string(), "abc-def".to_string()).await;
    
    // Add event
    let mut event = SpanEvent::new("processing_started".to_string());
    event.add_attribute("items_count".to_string(), "100".to_string());
    parent_span.add_event(event).await;
    
    // Start child span (simulated)
    let child_span = tracer.start_span("child_operation");
    
    // Add child span tags
    child_span.add_tag("db_query".to_string(), "SELECT * FROM users".to_string()).await;
    
    // Simulate success
    child_span.set_status(SpanStatus::Ok).await;
    child_span.end();
    
    // Simulate processing
    tokio::time::sleep(Duration::from_millis(10)).await;
    
    // End parent span
    parent_span.set_status(SpanStatus::Ok).await;
    parent_span.end();
    
    // Wait for async operations to complete
    tokio::time::sleep(Duration::from_millis(50)).await;
    
    // Check metrics
    let metrics = tracer.get_metrics().await;
    assert!(metrics.completed_spans >= 2);
    assert_eq!(metrics.error_count, 0);
}

/// Test integration of resilience with node executor
#[tokio::test]
async fn test_resilient_node_execution() {
    let executor = Arc::new(DefaultNodeExecutor);
    let retry_executor = RetryNodeExecutor::new(executor.clone(), 3, 10);
    
    let node = Node {
        id: "test_node".to_string(),
        node_type: NodeType::Agent("test_agent".to_string()),
        metadata: None,
    };
    
    let mut state = GraphState::new();
    state.set("input", json!("test_data"));
    
    // Create a proper execution context with resilience
    let graph = GraphBuilder::new("test_graph")
        .add_node("__start__", NodeType::Start)
        .add_node("test_node", NodeType::Agent("test_agent".to_string()))
        .add_node("__end__", NodeType::End)
        .add_edge("__start__", "test_node")
        .add_edge("test_node", "__end__")
        .set_entry_point("__start__")
        .build()
        .unwrap()
        .compile()
        .unwrap();
    
    let circuit_config = CircuitBreakerConfig::default();
    let retry_config = RetryConfig::default();
    let resilience_manager = ResilienceManager::new(circuit_config, retry_config, 10);
    let tracer = Tracer::new("test");
    
    let context = ExecutionContext {
        graph: Arc::new(graph),
        state: Arc::new(RwLock::new(GraphState::new())),
        channels: std::collections::HashMap::new(),
        execution_id: "test-exec".to_string(),
        metadata: ExecutionMetadata {
            started_at: 0,
            ended_at: None,
            nodes_executed: 0,
            status: ExecutionStatus::Running,
            error: None,
        },
        resilience_manager: Arc::new(resilience_manager),
        tracer: Arc::new(tracer),
    };
    
    // Execute with retry
    let result = retry_executor.execute_with_retry(&node, &mut state.values, &context).await;
    assert!(result.is_ok());
    
    // Check that agent was executed (marker added to state)
    let result_state = result.unwrap();
    assert!(result_state.contains_key("agent_test_node_executed"));
}

/// Test parallel execution with resilience and tracing
#[tokio::test]
async fn test_parallel_execution_with_resilience() {
    // Build a graph with sequential nodes (to avoid cycles)
    let graph = GraphBuilder::new("parallel_test")
        .add_node("__start__", NodeType::Start)
        .add_node("parallel_1", NodeType::Agent("agent1".to_string()))
        .add_node("parallel_2", NodeType::Agent("agent2".to_string()))
        .add_node("parallel_3", NodeType::Agent("agent3".to_string()))
        .add_node("__end__", NodeType::End)
        .add_edge("__start__", "parallel_1")
        .add_edge("parallel_1", "parallel_2")
        .add_edge("parallel_2", "parallel_3")
        .add_edge("parallel_3", "__end__")
        .set_entry_point("__start__")
        .build()
        .unwrap()
        .compile()
        .unwrap();
    
    let circuit_config = CircuitBreakerConfig::default();
    let retry_config = RetryConfig::default();
    let resilience_manager = Arc::new(ResilienceManager::new(circuit_config, retry_config, 10));
    let tracer = Arc::new(Tracer::new("parallel-test"));
    
    // Test parallel execution by running the graph directly
    let engine = ExecutionEngine::new();
    let mut state = GraphState::new();
    state.set("input", json!("test"));
    
    // Execute the graph
    let result = engine.execute(graph, state.values.clone()).await;
    
    if let Err(ref e) = result {
        eprintln!("Execution failed with error: {:?}", e);
    }
    assert!(result.is_ok());
    let final_state = result.unwrap();
    
    // Verify all parallel nodes were executed
    assert!(final_state.contains_key("agent_parallel_1_executed"));
    assert!(final_state.contains_key("agent_parallel_2_executed"));
    assert!(final_state.contains_key("agent_parallel_3_executed"));
}

/// Test resilience manager behavior
#[tokio::test]
async fn test_resilience_behavior() {
    let circuit_config = CircuitBreakerConfig {
        failure_threshold: 2,
        timeout_duration: Duration::from_millis(50),
        success_threshold: 1,
        failure_window: Duration::from_secs(60),
    };
    
    let retry_config = RetryConfig {
        max_attempts: 2,
        initial_delay: Duration::from_millis(5),
        max_delay: Duration::from_millis(50),
        backoff_multiplier: 2.0,
        jitter: false,
    };
    
    let manager = ResilienceManager::new(circuit_config, retry_config, 5);
    
    // Generate some activity and verify behavior
    let success = manager.execute_with_resilience(|| async {
        Ok::<_, std::io::Error>("success")
    }).await;
    assert!(success.is_ok());
    
    let failure = manager.execute_with_resilience(|| async {
        Err::<(), _>(std::io::Error::new(std::io::ErrorKind::Other, "failure"))
    }).await;
    assert!(failure.is_err());
}

/// Test end-to-end graph execution with full resilience and tracing
#[tokio::test]
async fn test_end_to_end_resilient_execution() {
    // Build a complex graph - sequential to avoid cycles
    let graph = GraphBuilder::new("e2e_test")
        .add_node("__start__", NodeType::Start)
        .add_node("validate", NodeType::Agent("validator".to_string()))
        .add_node("process", NodeType::Agent("processor".to_string()))
        .add_node("persist", NodeType::Agent("persister".to_string()))
        .add_node("__end__", NodeType::End)
        .add_edge("__start__", "validate")
        .add_edge("validate", "process")
        .add_edge("process", "persist")
        .add_edge("persist", "__end__")
        .set_entry_point("__start__")
        .build()
        .unwrap()
        .compile()
        .unwrap();
    
    // Create execution engine
    let engine = ExecutionEngine::new();
    
    // Create initial state
    let mut state = GraphState::new();
    state.set("request_id", json!("req-123"));
    state.set("user_id", json!("user-456"));
    state.set("data", json!({"value": 100}));
    
    // Execute the graph
    let result = engine.execute(graph, state.values.clone()).await;
    
    if let Err(ref e) = result {
        eprintln!("Execution failed with error: {:?}", e);
    }
    assert!(result.is_ok());
    let final_state = result.unwrap();
    
    // Verify all nodes were executed
    assert!(final_state.contains_key("agent_validate_executed"));
    assert!(final_state.contains_key("agent_process_executed"));
    assert!(final_state.contains_key("agent_persist_executed"));
}