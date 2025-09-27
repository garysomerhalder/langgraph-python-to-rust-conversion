use std::collections::HashMap;
use std::sync::Arc;

use langgraph::{
    batch::{BatchConfig, BatchExecutor, BatchJob, BatchJobStatus},
    engine::ExecutionEngine,
    graph::{CompiledGraph, Edge, EdgeType, Node, NodeType, StateGraph},
    state::StateData,
};
use serde_json::json;

/// Helper function to create a minimal CompiledGraph for testing
fn create_test_graph() -> CompiledGraph {
    let mut graph = StateGraph::new("test_graph");

    // Add required __start__ node
    let start_node = Node {
        id: "__start__".to_string(),
        node_type: NodeType::Start,
        metadata: None,
    };
    graph.add_node(start_node);

    // Add an end node
    let end_node = Node {
        id: "__end__".to_string(),
        node_type: NodeType::End,
        metadata: None,
    };
    graph.add_node(end_node);

    // Connect start to end to avoid orphaned nodes
    let edge = Edge {
        edge_type: EdgeType::Direct,
        metadata: None,
    };
    graph
        .add_edge("__start__", "__end__", edge)
        .expect("Failed to add edge");

    // Set entry point
    graph
        .set_entry_point("__start__")
        .expect("Failed to set entry point");

    // Compile the graph
    graph.compile().expect("Failed to compile test graph")
}

#[tokio::test]
async fn test_batch_executor_single_job() {
    // This test will fail because we haven't implemented the batch executor yet
    let engine = Arc::new(ExecutionEngine::new());
    let config = BatchConfig::default();
    let executor = BatchExecutor::new(config, engine);

    // Use the minimal test graph for batch execution
    let graph = create_test_graph();
    let input: StateData = HashMap::from([("initial".to_string(), json!("test"))]);

    let jobs = vec![BatchJob {
        id: "job1".to_string(),
        graph,
        input,
        priority: 1,
    }];

    let results = executor
        .execute_batch(jobs)
        .await
        .expect("Batch execution failed");

    assert_eq!(results.len(), 1);
    let result = &results[0];
    assert_eq!(result.job_id, "job1");
    assert_eq!(result.status, BatchJobStatus::Completed);
    assert!(result.output.is_some());
    assert!(result.error.is_none());
    assert_eq!(result.attempts, 1);
}

#[tokio::test]
async fn test_batch_executor_multiple_jobs() {
    // Test batch execution with multiple jobs
    let engine = Arc::new(ExecutionEngine::new());
    let config = BatchConfig {
        concurrency_limit: 3,
        ..Default::default()
    };
    let executor = BatchExecutor::new(config, engine);

    let graph = create_test_graph();
    let mut jobs = Vec::new();

    // Create 5 jobs to test concurrency
    for i in 0..5 {
        let input: StateData = HashMap::from([
            ("job_id".to_string(), json!(i)),
            ("counter".to_string(), json!(i * 10)),
        ]);

        jobs.push(BatchJob {
            id: format!("job{}", i),
            graph: graph.clone(),
            input,
            priority: 1,
        });
    }

    let results = executor
        .execute_batch(jobs)
        .await
        .expect("Batch execution failed");
    assert_eq!(results.len(), 5);
}

#[tokio::test]
async fn test_batch_executor_concurrency_limit() {
    // Test that concurrency limit is respected
    let engine = Arc::new(ExecutionEngine::new());
    let config = BatchConfig {
        concurrency_limit: 2, // Only 2 concurrent jobs
        ..Default::default()
    };
    let executor = BatchExecutor::new(config, engine);

    let graph = create_test_graph();
    let mut jobs = Vec::new();

    for i in 0..5 {
        let input: StateData = HashMap::from([("job_id".to_string(), json!(i))]);

        jobs.push(BatchJob {
            id: format!("job{}", i),
            graph: graph.clone(),
            input,
            priority: 1,
        });
    }

    let results = executor
        .execute_batch(jobs)
        .await
        .expect("Batch execution failed");
    assert_eq!(results.len(), 5);
}

#[tokio::test]
async fn test_batch_executor_empty_batch() {
    // Test handling of empty batch
    let engine = Arc::new(ExecutionEngine::new());
    let config = BatchConfig::default();
    let executor = BatchExecutor::new(config, engine);

    let jobs = vec![];
    let results = executor
        .execute_batch(jobs)
        .await
        .expect("Empty batch execution failed");

    assert_eq!(results.len(), 0);
}

#[tokio::test]
async fn test_batch_executor_statistics() {
    // Test batch execution statistics
    let engine = Arc::new(ExecutionEngine::new());
    let config = BatchConfig::default();
    let executor = BatchExecutor::new(config, engine);

    // For RED phase, just test with empty results
    let results = vec![];
    let stats = executor.calculate_stats(&results);

    assert_eq!(stats.total_jobs, 0);
    assert_eq!(stats.completed_jobs, 0);
    assert_eq!(stats.failed_jobs, 0);
}
