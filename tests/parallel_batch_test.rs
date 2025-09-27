use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use langgraph::{
    batch::{
        BatchConfig, BatchExecutor, BatchJob, BatchJobStatus, ParallelScheduler, WorkerMetrics,
    },
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

/// Test work-stealing scheduler with multiple workers
#[tokio::test]
async fn test_work_stealing_scheduler() {
    // This test will fail because we haven't implemented work stealing yet
    let engine = Arc::new(ExecutionEngine::new());
    let config = BatchConfig {
        concurrency_limit: 4, // 4 workers
        ..Default::default()
    };

    // Create ParallelScheduler for advanced scheduling
    let scheduler = ParallelScheduler::new(config, engine);

    let graph = create_test_graph();
    let mut jobs = Vec::new();

    // Create 10 jobs that should be distributed across 4 workers
    for i in 0..10 {
        let input: StateData = HashMap::from([
            ("job_id".to_string(), json!(i)),
            ("work_duration".to_string(), json!(100)), // 100ms of work
        ]);

        jobs.push(BatchJob {
            id: format!("job{}", i),
            graph: graph.clone(),
            input,
            priority: if i < 5 { 1 } else { 2 }, // First 5 jobs have higher priority
        });
    }

    let start_time = Instant::now();
    let results = scheduler
        .execute_parallel_batch(jobs)
        .await
        .expect("Parallel execution failed");
    let total_time = start_time.elapsed();

    // With 4 workers and 10 jobs, should complete in ~3x100ms = 300ms (parallel execution)
    // Sequential would take 10x100ms = 1000ms
    assert!(
        total_time < Duration::from_millis(500),
        "Should complete faster with parallel execution"
    );
    assert_eq!(results.len(), 10);

    // Verify all jobs completed successfully
    for result in &results {
        assert_eq!(result.status, BatchJobStatus::Completed);
        assert!(result.output.is_some());
        assert!(result.error.is_none());
    }
}

/// Test priority-based job scheduling
#[tokio::test]
async fn test_priority_job_scheduling() {
    // This test will fail because priority scheduling is not implemented
    let engine = Arc::new(ExecutionEngine::new());
    let scheduler = ParallelScheduler::new(BatchConfig::default(), engine);

    let graph = create_test_graph();
    let mut jobs = Vec::new();

    // Create jobs with different priorities
    for i in 0..6 {
        let priority = match i {
            0..=1 => 1, // High priority (urgent)
            2..=3 => 2, // Medium priority (normal)
            _ => 3,     // Low priority (background)
        };

        let input: StateData = HashMap::from([
            ("job_id".to_string(), json!(i)),
            ("priority".to_string(), json!(priority)),
        ]);

        jobs.push(BatchJob {
            id: format!("priority_job_{}", i),
            graph: graph.clone(),
            input,
            priority,
        });
    }

    let results = scheduler
        .execute_with_priority(jobs)
        .await
        .expect("Priority execution failed");

    // Results should be ordered by priority (high to low)
    let mut prev_priority = 0u8;
    for result in &results {
        // Priority should be non-decreasing (higher priority = lower number)
        let job_priority = result.job_id.chars().last().unwrap().to_digit(10).unwrap() as u8;
        assert!(
            job_priority >= prev_priority,
            "Jobs should be completed in priority order"
        );
        prev_priority = job_priority;
    }
}

/// Test job dependency resolution
#[tokio::test]
async fn test_job_dependency_resolution() {
    // This test will fail because dependency resolution is not implemented
    let engine = Arc::new(ExecutionEngine::new());
    let scheduler = ParallelScheduler::new(BatchConfig::default(), engine);

    let graph = create_test_graph();

    // Create jobs with dependencies: A → B → C, D → E
    let mut jobs = Vec::new();
    let dependencies = HashMap::from([
        ("job_b".to_string(), vec!["job_a".to_string()]),
        ("job_c".to_string(), vec!["job_b".to_string()]),
        ("job_e".to_string(), vec!["job_d".to_string()]),
    ]);

    for job_id in ["job_a", "job_b", "job_c", "job_d", "job_e"] {
        let input: StateData = HashMap::from([("job_id".to_string(), json!(job_id))]);

        jobs.push(BatchJob {
            id: job_id.to_string(),
            graph: graph.clone(),
            input,
            priority: 1,
        });
    }

    let results = scheduler
        .execute_with_dependencies(jobs, dependencies)
        .await
        .expect("Dependency resolution failed");

    // Verify execution order respects dependencies
    let mut completed_jobs = HashMap::new();
    for (idx, result) in results.iter().enumerate() {
        completed_jobs.insert(result.job_id.clone(), idx);
    }

    // job_b should complete after job_a
    assert!(completed_jobs["job_b"] > completed_jobs["job_a"]);
    // job_c should complete after job_b
    assert!(completed_jobs["job_c"] > completed_jobs["job_b"]);
    // job_e should complete after job_d
    assert!(completed_jobs["job_e"] > completed_jobs["job_d"]);
}

/// Test load balancing across workers
#[tokio::test]
async fn test_load_balancing() {
    // This test will fail because load balancing is not implemented
    let engine = Arc::new(ExecutionEngine::new());
    let scheduler = ParallelScheduler::new(BatchConfig::default(), engine);

    let graph = create_test_graph();
    let mut jobs = Vec::new();

    // Create jobs with varying computational loads
    for i in 0..8 {
        let work_load = if i < 4 { 50 } else { 200 }; // Some jobs are 4x more work

        let input: StateData = HashMap::from([
            ("job_id".to_string(), json!(i)),
            ("work_load".to_string(), json!(work_load)),
        ]);

        jobs.push(BatchJob {
            id: format!("load_job_{}", i),
            graph: graph.clone(),
            input,
            priority: 1,
        });
    }

    let results = scheduler
        .execute_with_load_balancing(jobs)
        .await
        .expect("Load balanced execution failed");

    // All jobs should complete successfully
    assert_eq!(results.len(), 8);
    for result in &results {
        assert_eq!(result.status, BatchJobStatus::Completed);
    }

    // Load balancing should distribute work efficiently
    // (Specific timing assertions would depend on implementation)
}

/// Test worker pool auto-scaling
#[tokio::test]
async fn test_worker_pool_scaling() {
    // This test will fail because worker pool scaling is not implemented
    let engine = Arc::new(ExecutionEngine::new());
    let config = BatchConfig {
        concurrency_limit: 2, // Start with 2 workers
        ..Default::default()
    };

    let scheduler = ParallelScheduler::new(config, engine);

    let graph = create_test_graph();
    let mut jobs = Vec::new();

    // Create a large batch that would benefit from scaling up
    for i in 0..20 {
        let input: StateData = HashMap::from([("job_id".to_string(), json!(i))]);

        jobs.push(BatchJob {
            id: format!("scaling_job_{}", i),
            graph: graph.clone(),
            input,
            priority: 1,
        });
    }

    let results = scheduler
        .execute_with_scaling(jobs)
        .await
        .expect("Auto-scaling execution failed");

    assert_eq!(results.len(), 20);

    // Verify that worker pool metrics show scaling occurred
    let metrics = scheduler.get_worker_metrics();
    assert!(metrics.max_workers > 2, "Worker pool should have scaled up");
    assert!(
        metrics.peak_utilization > 0.8,
        "Should have high utilization during peak"
    );
}

/// Test circular dependency detection
#[tokio::test]
async fn test_circular_dependency_detection() {
    // This test will fail because circular dependency detection is not implemented
    let engine = Arc::new(ExecutionEngine::new());
    let scheduler = ParallelScheduler::new(BatchConfig::default(), engine);

    let graph = create_test_graph();
    let mut jobs = Vec::new();

    // Create jobs with circular dependencies: A → B → C → A
    let dependencies = HashMap::from([
        ("job_a".to_string(), vec!["job_c".to_string()]),
        ("job_b".to_string(), vec!["job_a".to_string()]),
        ("job_c".to_string(), vec!["job_b".to_string()]),
    ]);

    for job_id in ["job_a", "job_b", "job_c"] {
        let input: StateData = HashMap::from([("job_id".to_string(), json!(job_id))]);

        jobs.push(BatchJob {
            id: job_id.to_string(),
            graph: graph.clone(),
            input,
            priority: 1,
        });
    }

    // Should return an error indicating circular dependency
    let result = scheduler
        .execute_with_dependencies(jobs, dependencies)
        .await;
    assert!(result.is_err(), "Should detect circular dependency");

    let error = result.unwrap_err();
    let error_msg = error.to_string().to_lowercase();
    assert!(
        error_msg.contains("circular") || error_msg.contains("cycle"),
        "Error should mention circular dependency or cycle, got: {}",
        error
    );
}
