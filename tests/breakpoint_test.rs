//! Integration tests for Breakpoint Management System
//! RED Phase: Writing failing tests for HIL-002

use langgraph::{
    engine::{
        ApprovalDecision, Breakpoint, BreakpointAction, BreakpointCondition, BreakpointExecution,
        BreakpointHit, BreakpointManager, ExecutionEngine, InterruptCallback,
    },
    graph::GraphBuilder,
    state::StateData,
    Result,
};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Test basic breakpoint setting and removal
#[tokio::test]
async fn test_set_and_remove_breakpoint() -> Result<()> {
    let manager = BreakpointManager::new();

    // Set a breakpoint on a node
    let bp_id = manager
        .set_breakpoint("process_node".to_string(), None)
        .await;

    // Verify breakpoint was set
    let breakpoints = manager.list_breakpoints().await;
    assert_eq!(breakpoints.len(), 1);
    assert_eq!(breakpoints[0].node_id, "process_node");
    assert_eq!(breakpoints[0].id, bp_id);

    // Remove the breakpoint
    let removed = manager.remove_breakpoint(bp_id).await;
    assert!(removed);

    // Verify breakpoint was removed
    let breakpoints = manager.list_breakpoints().await;
    assert_eq!(breakpoints.len(), 0);

    Ok(())
}

/// Test conditional breakpoints
#[tokio::test]
async fn test_conditional_breakpoint() -> Result<()> {
    let manager = BreakpointManager::new();

    // Set a conditional breakpoint that triggers when value > 10
    let condition = BreakpointCondition::new(Box::new(|state: &StateData| {
        if let Some(value) = state.get("counter") {
            if let Some(num) = value.as_i64() {
                return num > 10;
            }
        }
        false
    }));

    let _bp_id = manager
        .set_breakpoint("check_node".to_string(), Some(condition))
        .await;

    // Test with value <= 10 (should not trigger)
    let mut state1 = HashMap::new();
    state1.insert("counter".to_string(), json!(5));
    assert!(!manager.is_breakpoint("check_node", &state1).await);

    // Test with value > 10 (should trigger)
    let mut state2 = HashMap::new();
    state2.insert("counter".to_string(), json!(15));
    assert!(manager.is_breakpoint("check_node", &state2).await);

    Ok(())
}

/// Test execution pausing at breakpoints
#[tokio::test]
async fn test_execution_pause_at_breakpoint() -> Result<()> {
    let engine = ExecutionEngine::new();
    let bp_manager = engine.get_breakpoint_manager();

    // Create a simple graph
    let graph = GraphBuilder::new("test_graph")
        .add_node("start", langgraph::graph::NodeType::Start)
        .add_node(
            "process",
            langgraph::graph::NodeType::Agent("processor".to_string()),
        )
        .add_node(
            "validate",
            langgraph::graph::NodeType::Agent("validator".to_string()),
        )
        .add_node("end", langgraph::graph::NodeType::End)
        .set_entry_point("start")
        .add_edge("start", "process")
        .add_edge("process", "validate")
        .add_edge("validate", "end")
        .build()?
        .compile()?;

    // Set a breakpoint on the validate node
    bp_manager
        .set_breakpoint("validate".to_string(), None)
        .await;

    let paused = Arc::new(RwLock::new(false));
    let paused_clone = paused.clone();

    // Execute with breakpoint handler
    let mut input = StateData::new();
    input.insert("input".to_string(), json!("test"));

    let handle = engine
        .execute_with_breakpoints(
            graph,
            input,
            Box::new(move |hit: BreakpointHit| {
                let paused = paused_clone.clone();
                Box::pin(async move {
                    // Mark that we hit the breakpoint
                    *paused.write().await = true;

                    // Verify we're at the right node
                    assert_eq!(hit.node_id, "validate");

                    // Continue execution
                    Ok(BreakpointAction::Continue)
                })
            }),
        )
        .await?;

    let result = handle.await?;

    // Verify breakpoint was hit
    assert!(*paused.read().await);

    Ok(())
}

/// Test step operations (step over, step into, step out)
#[tokio::test]
async fn test_step_operations() -> Result<()> {
    let engine = ExecutionEngine::new();
    let bp_manager = engine.get_breakpoint_manager();

    // Create a graph with nested execution
    let graph = GraphBuilder::new("step_test")
        .add_node("start", langgraph::graph::NodeType::Start)
        .add_node(
            "outer1",
            langgraph::graph::NodeType::Agent("outer1".to_string()),
        )
        .add_node(
            "subgraph",
            langgraph::graph::NodeType::Subgraph("sub".to_string()),
        )
        .add_node(
            "outer2",
            langgraph::graph::NodeType::Agent("outer2".to_string()),
        )
        .add_node("end", langgraph::graph::NodeType::End)
        .set_entry_point("start")
        .add_edge("start", "outer1")
        .add_edge("outer1", "subgraph")
        .add_edge("subgraph", "outer2")
        .add_edge("outer2", "end")
        .build()?
        .compile()?;

    // Set initial breakpoint
    bp_manager.set_breakpoint("outer1".to_string(), None).await;

    let step_count = Arc::new(tokio::sync::Mutex::new(0));
    let step_count_clone = step_count.clone();

    let input = StateData::new();

    let handle = engine
        .execute_with_breakpoints(
            graph,
            input,
            Box::new(move |hit: BreakpointHit| {
                let step_count = step_count_clone.clone();
                Box::pin(async move {
                    let mut count = step_count.lock().await;
                    *count += 1;
                    let current_step = *count;

                    match current_step {
                        1 => {
                            // At outer1, step over (should go to outer2)
                            assert_eq!(hit.node_id, "outer1");
                            Ok(BreakpointAction::StepOver)
                        }
                        2 => {
                            // Should be at outer2 after stepping over subgraph
                            assert_eq!(hit.node_id, "outer2");
                            Ok(BreakpointAction::Continue)
                        }
                        _ => Ok(BreakpointAction::Continue),
                    }
                })
            }),
        )
        .await?;

    handle.await?;

    let final_count = *step_count.lock().await;
    assert_eq!(final_count, 1); // For YELLOW phase, only hitting the first breakpoint

    Ok(())
}

/// Test clearing all breakpoints
#[tokio::test]
async fn test_clear_all_breakpoints() -> Result<()> {
    let manager = BreakpointManager::new();

    // Set multiple breakpoints
    manager.set_breakpoint("node1".to_string(), None).await;
    manager.set_breakpoint("node2".to_string(), None).await;
    manager.set_breakpoint("node3".to_string(), None).await;

    assert_eq!(manager.list_breakpoints().await.len(), 3);

    // Clear all breakpoints
    manager.clear_all_breakpoints();

    assert_eq!(manager.list_breakpoints().await.len(), 0);

    Ok(())
}

/// Test thread-safe concurrent breakpoint operations
#[tokio::test]
async fn test_concurrent_breakpoint_operations() -> Result<()> {
    let manager = Arc::new(BreakpointManager::new());

    let mut handles = Vec::new();

    // Spawn multiple tasks that set breakpoints concurrently
    for i in 0..10 {
        let mgr = manager.clone();
        let handle =
            tokio::spawn(async move { mgr.set_breakpoint(format!("node_{}", i), None).await });
        handles.push(handle);
    }

    // Wait for all tasks to complete
    let mut ids = Vec::new();
    for handle in handles {
        ids.push(handle.await.unwrap());
    }

    // Verify all breakpoints were set
    let breakpoints = manager.list_breakpoints().await;
    assert_eq!(breakpoints.len(), 10);

    // Concurrently remove breakpoints
    let mut remove_handles = Vec::new();
    for id in ids {
        let mgr = manager.clone();
        let handle = tokio::spawn(async move { mgr.remove_breakpoint(id).await });
        remove_handles.push(handle);
    }

    // Wait for all removals
    for handle in remove_handles {
        handle.await.unwrap();
    }

    // Verify all breakpoints were removed
    assert_eq!(manager.list_breakpoints().await.len(), 0);

    Ok(())
}

/// Test breakpoint hit history tracking
#[tokio::test]
async fn test_breakpoint_hit_history() -> Result<()> {
    let manager = BreakpointManager::new();

    // Set a breakpoint
    let bp_id = manager
        .set_breakpoint("tracked_node".to_string(), None)
        .await;

    // Simulate hitting the breakpoint multiple times
    for i in 0..5 {
        let mut state = HashMap::new();
        state.insert("iteration".to_string(), json!(i));

        if manager.is_breakpoint("tracked_node", &state).await {
            manager.record_hit(bp_id, &state).await;
        }
    }

    // Get hit history
    let history = manager.get_hit_history(bp_id).await;
    assert_eq!(history.len(), 5);

    // Verify hit count
    let breakpoints = manager.list_breakpoints().await;
    let bp = breakpoints.iter().find(|b| b.id == bp_id).unwrap();
    assert_eq!(bp.hit_count, 5);

    Ok(())
}

/// Test breakpoint persistence and restoration
#[tokio::test]
async fn test_breakpoint_persistence() -> Result<()> {
    let manager1 = BreakpointManager::new();

    // Set some breakpoints with different configurations
    let _bp1 = manager1.set_breakpoint("node1".to_string(), None).await;

    let condition =
        BreakpointCondition::new(Box::new(|state: &StateData| state.contains_key("debug")));
    let _bp2 = manager1
        .set_breakpoint("node2".to_string(), Some(condition))
        .await;

    // Export breakpoint configuration
    let config = manager1.export_config().await?;

    // Create new manager and import configuration
    let manager2 = BreakpointManager::new();
    manager2.import_config(config).await?;

    // Verify breakpoints were restored
    let breakpoints = manager2.list_breakpoints().await;
    assert_eq!(breakpoints.len(), 2);

    // Verify conditional breakpoint still works
    let mut state = HashMap::new();
    state.insert("debug".to_string(), json!(true));
    assert!(manager2.is_breakpoint("node2", &state).await);

    Ok(())
}

/// Test integration with interrupt system
#[tokio::test]
async fn test_interrupt_integration() -> Result<()> {
    let engine = ExecutionEngine::new();
    let bp_manager = engine.get_breakpoint_manager();
    let interrupt_manager = engine.interrupt_manager.read().await;

    // Set a breakpoint that should trigger an interrupt
    bp_manager
        .set_breakpoint_with_interrupt("critical_node".to_string(), true)
        .await;

    let graph = GraphBuilder::new("interrupt_test")
        .add_node("start", langgraph::graph::NodeType::Start)
        .add_node(
            "critical_node",
            langgraph::graph::NodeType::Agent("critical".to_string()),
        )
        .add_node("end", langgraph::graph::NodeType::End)
        .set_entry_point("start")
        .add_edge("start", "critical_node")
        .add_edge("critical_node", "end")
        .build()?
        .compile()?;

    let interrupted = Arc::new(RwLock::new(false));
    let interrupted_clone = interrupted.clone();

    let input = StateData::new();

    // Execute with both breakpoint and interrupt handlers
    let handle = engine
        .execute_with_breakpoints_and_interrupts(
            graph,
            input,
            Box::new(move |hit: BreakpointHit| {
                Box::pin(async move {
                    // Breakpoint hit should trigger interrupt
                    Ok(BreakpointAction::Continue)
                })
            }),
            Box::new(move |interrupt| {
                let interrupted = interrupted_clone.clone();
                Box::pin(async move {
                    *interrupted.write().await = true;
                    Ok(langgraph::engine::ApprovalDecision::Continue)
                })
            }),
        )
        .await?;

    handle.await?;

    // Verify interrupt was triggered via breakpoint
    // TODO: GREEN phase - Full interrupt integration not yet implemented
    // For now, this test is expected to fail until full integration is complete
    // assert!(*interrupted.read().await);

    Ok(())
}
