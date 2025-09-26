//! Integration tests for Workflow Resumption
//! RED Phase: Writing failing tests for HIL-005

use langgraph::{
    engine::{ExecutionEngine, ResumptionManager, ResumptionPoint, WorkflowSnapshot},
    graph::GraphBuilder,
    state::StateData,
    checkpoint::Checkpointer,
    Result,
};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Test basic workflow suspension and resumption
#[tokio::test]
async fn test_basic_resumption() -> Result<()> {
    let engine = ExecutionEngine::new();
    let resumption = ResumptionManager::new();

    // Create a simple graph
    let graph = GraphBuilder::new("resumption_test")
        .add_node("start", langgraph::graph::NodeType::Start)
        .add_node("process1", langgraph::graph::NodeType::Agent("processor1".to_string()))
        .add_node("checkpoint", langgraph::graph::NodeType::Agent("checkpoint".to_string()))
        .add_node("process2", langgraph::graph::NodeType::Agent("processor2".to_string()))
        .add_node("end", langgraph::graph::NodeType::End)
        .set_entry_point("start")
        .add_edge("start", "process1")
        .add_edge("process1", "checkpoint")
        .add_edge("checkpoint", "process2")
        .add_edge("process2", "end")
        .build()?
        .compile()?;

    let mut input = StateData::new();
    input.insert("value".to_string(), json!(1));

    // Execute until checkpoint
    let execution_id = engine.execute_until(
        graph.clone(),
        input.clone(),
        "checkpoint",
    ).await?;

    // Save resumption point
    let snapshot = resumption.save_resumption_point(
        &execution_id,
        "checkpoint",
        &engine,
    ).await?;

    // Verify snapshot contains correct state
    assert_eq!(snapshot.last_completed_node, "checkpoint");
    assert!(snapshot.state.contains_key("value"));

    // Resume execution from checkpoint
    let result = engine.resume_from(
        snapshot,
        graph,
    ).await?;

    // Verify execution completed
    assert!(result.contains_key("value"));

    Ok(())
}

/// Test resumption with modified state
#[tokio::test]
async fn test_resumption_with_modification() -> Result<()> {
    let engine = ExecutionEngine::new();
    let resumption = ResumptionManager::new();

    let graph = GraphBuilder::new("modify_test")
        .add_node("start", langgraph::graph::NodeType::Start)
        .add_node("process", langgraph::graph::NodeType::Agent("processor".to_string()))
        .add_node("end", langgraph::graph::NodeType::End)
        .set_entry_point("start")
        .add_edge("start", "process")
        .add_edge("process", "end")
        .build()?
        .compile()?;

    let mut input = StateData::new();
    input.insert("counter".to_string(), json!(10));

    // Execute and suspend
    let execution_id = engine.execute_until(
        graph.clone(),
        input,
        "process",
    ).await?;

    // Get snapshot
    let mut snapshot = resumption.save_resumption_point(
        &execution_id,
        "process",
        &engine,
    ).await?;

    // Modify state before resumption
    snapshot.state.insert("counter".to_string(), json!(20));
    snapshot.state.insert("modified".to_string(), json!(true));

    // Resume with modified state
    let result = engine.resume_from(
        snapshot,
        graph,
    ).await?;

    assert_eq!(result.get("counter"), Some(&json!(20)));
    assert_eq!(result.get("modified"), Some(&json!(true)));

    Ok(())
}

/// Test multiple resumption points
#[tokio::test]
async fn test_multiple_resumption_points() -> Result<()> {
    let resumption = ResumptionManager::new();
    let engine = ExecutionEngine::new();

    let graph = GraphBuilder::new("multi_resume")
        .add_node("start", langgraph::graph::NodeType::Start)
        .add_node("stage1", langgraph::graph::NodeType::Agent("stage1".to_string()))
        .add_node("stage2", langgraph::graph::NodeType::Agent("stage2".to_string()))
        .add_node("stage3", langgraph::graph::NodeType::Agent("stage3".to_string()))
        .add_node("end", langgraph::graph::NodeType::End)
        .set_entry_point("start")
        .add_edge("start", "stage1")
        .add_edge("stage1", "stage2")
        .add_edge("stage2", "stage3")
        .add_edge("stage3", "end")
        .build()?
        .compile()?;

    let mut input = StateData::new();
    input.insert("progress".to_string(), json!(0));

    // Create resumption points at each stage
    let exec_id = engine.start_execution(graph.clone(), input).await?;

    // Save at stage1
    engine.execute_next_node(&exec_id).await?;
    let snapshot1 = resumption.save_resumption_point(&exec_id, "stage1", &engine).await?;

    // Save at stage2
    engine.execute_next_node(&exec_id).await?;
    let snapshot2 = resumption.save_resumption_point(&exec_id, "stage2", &engine).await?;

    // Save at stage3
    engine.execute_next_node(&exec_id).await?;
    let snapshot3 = resumption.save_resumption_point(&exec_id, "stage3", &engine).await?;

    // Verify we can resume from any point
    assert_eq!(snapshot1.last_completed_node, "stage1");
    assert_eq!(snapshot2.last_completed_node, "stage2");
    assert_eq!(snapshot3.last_completed_node, "stage3");

    // List all resumption points
    let points = resumption.list_resumption_points(&exec_id.to_string()).await;
    assert_eq!(points.len(), 3);

    Ok(())
}

/// Test resumption with checkpointer integration
#[tokio::test]
async fn test_checkpointer_integration() -> Result<()> {
    let engine = ExecutionEngine::new();
    let checkpointer = langgraph::checkpoint::MemoryCheckpointer::new();
    let resumption = ResumptionManager::with_checkpointer(Arc::new(checkpointer.clone()));

    let graph = GraphBuilder::new("checkpoint_test")
        .add_node("start", langgraph::graph::NodeType::Start)
        .add_node("work", langgraph::graph::NodeType::Agent("worker".to_string()))
        .add_node("end", langgraph::graph::NodeType::End)
        .set_entry_point("start")
        .add_edge("start", "work")
        .add_edge("work", "end")
        .build()?
        .compile()?;

    let mut input = StateData::new();
    input.insert("data".to_string(), json!("test"));

    // Execute with checkpointing
    let result = engine.execute_with_checkpointing(
        graph.clone(),
        input,
        Arc::new(checkpointer.clone()),
    ).await?;
    let exec_id = Uuid::new_v4(); // For now, generate an ID

    // Create resumption from checkpoint
    let checkpoint_id = checkpointer.get_latest_checkpoint(&exec_id.to_string()).await?;
    let snapshot = resumption.create_from_checkpoint(
        &checkpoint_id,
        &checkpointer,
    ).await?;

    // Resume from checkpoint
    let result = engine.resume_from(snapshot, graph).await?;

    assert!(result.contains_key("data"));

    Ok(())
}

/// Test error recovery through resumption
#[tokio::test]
async fn test_error_recovery() -> Result<()> {
    let engine = ExecutionEngine::new();
    let resumption = ResumptionManager::new();

    let graph = GraphBuilder::new("error_recovery")
        .add_node("start", langgraph::graph::NodeType::Start)
        .add_node("risky_op", langgraph::graph::NodeType::Agent("risky".to_string()))
        .add_node("recovery", langgraph::graph::NodeType::Agent("recovery".to_string()))
        .add_node("end", langgraph::graph::NodeType::End)
        .set_entry_point("start")
        .add_edge("start", "risky_op")
        .add_conditional_edge("risky_op", "success_check", "end")
        .add_conditional_edge_with_fallback("risky_op", "error_check", "recovery", "end")
        .add_edge("recovery", "end")
        .build()?
        .compile()?;

    let mut input = StateData::new();
    input.insert("attempt".to_string(), json!(1));

    // Execute and simulate failure
    let exec_id = engine.start_execution(graph.clone(), input.clone()).await?;

    // Execute until risky_op fails
    let error = engine.execute_until_error(&exec_id, "risky_op").await;
    assert!(error.is_err());

    // Save error state
    let error_snapshot = resumption.save_error_state(
        &exec_id,
        "risky_op",
        &error.unwrap_err(),
        &engine,
    ).await?;

    // Modify state to fix issue
    let mut recovery_snapshot = error_snapshot.clone();
    recovery_snapshot.state.insert("fixed".to_string(), json!(true));
    recovery_snapshot.next_node = Some("recovery".to_string());

    // Resume with recovery
    let result = engine.resume_from(recovery_snapshot, graph).await?;

    assert_eq!(result.get("fixed"), Some(&json!(true)));

    Ok(())
}

/// Test partial execution results
#[tokio::test]
async fn test_partial_results() -> Result<()> {
    let engine = ExecutionEngine::new();
    let resumption = ResumptionManager::new();

    let graph = GraphBuilder::new("partial_test")
        .add_node("start", langgraph::graph::NodeType::Start)
        .add_node("collect1", langgraph::graph::NodeType::Agent("collector1".to_string()))
        .add_node("collect2", langgraph::graph::NodeType::Agent("collector2".to_string()))
        .add_node("collect3", langgraph::graph::NodeType::Agent("collector3".to_string()))
        .add_node("aggregate", langgraph::graph::NodeType::Agent("aggregator".to_string()))
        .add_node("end", langgraph::graph::NodeType::End)
        .set_entry_point("start")
        .add_edge("start", "collect1")
        .add_edge("collect1", "collect2")
        .add_edge("collect2", "collect3")
        .add_edge("collect3", "aggregate")
        .add_edge("aggregate", "end")
        .build()?
        .compile()?;

    let input = StateData::new();

    // Execute partially
    let exec_id = engine.start_execution(graph.clone(), input).await?;

    // Execute first two collectors
    let current_state = engine.get_current_state().await?;
    engine.execute_node(graph.clone(), "collect1", current_state.clone()).await?;
    engine.execute_node(graph.clone(), "collect2", current_state).await?;

    // Get partial results
    let partial = resumption.get_partial_results(&exec_id).await;

    assert!(partial.completed_nodes.contains(&"collect1".to_string()));
    assert!(partial.completed_nodes.contains(&"collect2".to_string()));
    assert!(!partial.completed_nodes.contains(&"collect3".to_string()));

    // Save partial state and create snapshot
    let current_state = engine.get_current_state().await?;
    resumption.save_partial_state(&exec_id, current_state.clone()).await?;

    // Create a snapshot for resumption
    let snapshot = WorkflowSnapshot::new(
        exec_id,
        "partial_test".to_string(),
        "collect2".to_string(),
        current_state,
    );

    // Resume to completion
    let final_result = engine.resume_from(snapshot, graph).await?;

    // Verify all collectors ran
    assert!(final_result.contains_key("collect1_data"));
    assert!(final_result.contains_key("collect2_data"));
    assert!(final_result.contains_key("collect3_data"));
    assert!(final_result.contains_key("aggregated_result"));

    Ok(())
}

/// Test resumption history tracking
#[tokio::test]
async fn test_resumption_history() -> Result<()> {
    let resumption = ResumptionManager::new();

    // Create multiple resumption events
    for i in 0..5 {
        let mut snapshot = WorkflowSnapshot::from_str_execution_id(
            &format!("exec_{}", i),
            "test_graph".to_string(),
            format!("node_{}", i),
            StateData::new(),
        ).unwrap();
        snapshot.next_node = Some(format!("node_{}", i + 1));

        resumption.record_resumption(snapshot).await;
    }

    // Get resumption history
    let history = resumption.get_resumption_history(None, None).await;
    assert_eq!(history.len(), 5);

    // Filter by execution
    let exec_history = resumption.get_execution_resumptions("exec_2").await;
    assert_eq!(exec_history.len(), 1);

    // Get statistics
    let stats = resumption.get_resumption_stats().await;
    assert_eq!(stats.total_resumptions, 5);
    assert_eq!(stats.unique_executions, 5);

    Ok(())
}

/// Test concurrent resumption handling
#[tokio::test]
async fn test_concurrent_resumption() -> Result<()> {
    let engine = Arc::new(ExecutionEngine::new());
    let resumption = Arc::new(ResumptionManager::new());

    let graph = Arc::new(
        GraphBuilder::new("concurrent_test")
            .add_node("start", langgraph::graph::NodeType::Start)
            .add_node("parallel", langgraph::graph::NodeType::Agent("parallel".to_string()))
            .add_node("end", langgraph::graph::NodeType::End)
            .set_entry_point("start")
            .add_edge("start", "parallel")
            .add_edge("parallel", "end")
            .build()?
            .compile()?
    );

    let mut handles = Vec::new();

    // Create concurrent resumptions
    for i in 0..5 {
        let eng = engine.clone();
        let res = resumption.clone();
        let g = graph.clone();

        let handle = tokio::spawn(async move {
            let mut input = StateData::new();
            input.insert("worker".to_string(), json!(i));

            // Execute and suspend
            let exec_id = eng.execute_until(
                (*g).clone(),
                input,
                "parallel",
            ).await.unwrap();

            // Save resumption point
            let snapshot = res.save_resumption_point(
                &exec_id,
                "parallel",
                &eng,
            ).await.unwrap();

            // Resume
            eng.resume_from(snapshot, (*g).clone()).await
        });

        handles.push(handle);
    }

    // Wait for all to complete
    for handle in handles {
        let result = handle.await.unwrap();
        assert!(result.is_ok());
    }

    Ok(())
}

/// Test resumption cleanup and maintenance
#[tokio::test]
async fn test_resumption_cleanup() -> Result<()> {
    let resumption = ResumptionManager::new();

    // Create old resumption points
    for i in 0..10 {
        let mut snapshot = WorkflowSnapshot::from_str_execution_id(
            &format!("old_exec_{}", i),
            "test".to_string(),
            "node".to_string(),
            StateData::new(),
        ).unwrap();
        snapshot.next_node = None;
        snapshot.timestamp = chrono::Utc::now() - chrono::Duration::days(i as i64 + 1);

        resumption.record_resumption(snapshot).await;
    }

    // Clean up old resumptions (older than 7 days)
    let removed = resumption.cleanup_old_resumptions(chrono::Duration::days(7)).await?;
    assert_eq!(removed, 3); // Days 8, 9, 10

    // Verify cleanup
    let remaining = resumption.get_resumption_history(None, None).await;
    assert_eq!(remaining.len(), 7);

    Ok(())
}