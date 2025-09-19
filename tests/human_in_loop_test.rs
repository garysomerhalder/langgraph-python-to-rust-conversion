//! Integration tests for Human-in-the-Loop functionality
//! RED Phase: Writing failing tests first for interrupt/approve mechanism

use langgraph::{
    engine::{ExecutionEngine, ApprovalDecision, InterruptHandle, InterruptMode},
    graph::GraphBuilder,
    state::StateData,
    Result,
};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::time::Duration;

/// Test that execution can be interrupted before a specific node
#[tokio::test]
async fn test_interrupt_before_node() -> Result<()> {
    // Create a simple graph with interrupt points
    let mut builder = GraphBuilder::new("test_interrupt");

    // Add nodes with interrupt configuration
    builder.add_node("start", |state: StateData| async move {
        let mut state = state.clone();
        state.insert("started".to_string(), json!(true));
        Ok(state)
    });

    // This node should trigger an interrupt BEFORE execution
    builder.add_node_with_interrupt("review", InterruptMode::Before, |state: StateData| async move {
        let mut state = state.clone();
        state.insert("reviewed".to_string(), json!(true));
        Ok(state)
    });

    builder.add_node("end", |state: StateData| async move {
        let mut state = state.clone();
        state.insert("completed".to_string(), json!(true));
        Ok(state)
    });

    builder.add_edge("start", "review");
    builder.add_edge("review", "end");

    let graph = builder.compile()?;
    let engine = ExecutionEngine::new(graph);

    // Start execution with interrupt handler
    let interrupt_received = Arc::new(RwLock::new(false));
    let interrupt_received_clone = interrupt_received.clone();

    let handle = engine.execute_with_interrupts(
        json!({"input": "test"}),
        Box::new(move |handle: InterruptHandle| {
            let interrupt_received = interrupt_received_clone.clone();
            Box::pin(async move {
                // Mark that we received the interrupt
                *interrupt_received.write().await = true;

                // Verify the interrupt is at the right node
                assert_eq!(handle.node_id, "review");

                // Verify state is preserved
                assert!(handle.state_snapshot.contains_key("started"));
                assert!(handle.state_snapshot.get("started").unwrap().as_bool().unwrap());

                // Approve continuation
                Ok(ApprovalDecision::Continue)
            })
        })
    ).await?;

    // Wait for execution to complete
    let result = handle.await?;

    // Verify interrupt was triggered
    assert!(*interrupt_received.read().await);

    // Verify execution completed successfully
    assert!(result.get("completed").unwrap().as_bool().unwrap());
    assert!(result.get("reviewed").unwrap().as_bool().unwrap());

    Ok(())
}

/// Test that execution can be interrupted after a node completes
#[tokio::test]
async fn test_interrupt_after_node() -> Result<()> {
    let mut builder = GraphBuilder::new("test_interrupt_after");

    builder.add_node("process", |state: StateData| async move {
        let mut state = state.clone();
        state.insert("processed".to_string(), json!(true));
        state.insert("result".to_string(), json!(42));
        Ok(state)
    });

    // Interrupt AFTER this node completes
    builder.add_node_with_interrupt("validate", InterruptMode::After, |state: StateData| async move {
        let mut state = state.clone();
        state.insert("validated".to_string(), json!(true));
        Ok(state)
    });

    builder.add_edge("process", "validate");

    let graph = builder.compile()?;
    let engine = ExecutionEngine::new(graph);

    let handle = engine.execute_with_interrupts(
        json!({"input": "test"}),
        Box::new(|handle: InterruptHandle| {
            Box::pin(async move {
                // Verify the node already executed
                assert!(handle.state_snapshot.contains_key("validated"));

                // Verify we can see the results
                assert_eq!(handle.state_snapshot.get("result").unwrap().as_i64().unwrap(), 42);

                // Continue execution
                Ok(ApprovalDecision::Continue)
            })
        })
    ).await?;

    let result = handle.await?;
    assert!(result.get("validated").unwrap().as_bool().unwrap());

    Ok(())
}

/// Test that human can reject and redirect execution
#[tokio::test]
async fn test_interrupt_reject_and_redirect() -> Result<()> {
    let mut builder = GraphBuilder::new("test_redirect");

    builder.add_node("input", |state: StateData| async move {
        let mut state = state.clone();
        state.insert("value".to_string(), json!(100));
        Ok(state)
    });

    builder.add_node_with_interrupt("validate", InterruptMode::Before, |state: StateData| async move {
        // This should not execute if redirected
        panic!("Should not reach validate node when redirected");
    });

    builder.add_node("alternative", |state: StateData| async move {
        let mut state = state.clone();
        state.insert("redirected".to_string(), json!(true));
        Ok(state)
    });

    builder.add_edge("input", "validate");
    // Add alternative path
    builder.add_edge("input", "alternative");

    let graph = builder.compile()?;
    let engine = ExecutionEngine::new(graph);

    let handle = engine.execute_with_interrupts(
        json!({}),
        Box::new(|handle: InterruptHandle| {
            Box::pin(async move {
                // Redirect to alternative path
                Ok(ApprovalDecision::Redirect("alternative".to_string()))
            })
        })
    ).await?;

    let result = handle.await?;
    assert!(result.get("redirected").unwrap().as_bool().unwrap());
    assert!(!result.contains_key("validated"));

    Ok(())
}

/// Test that state can be modified during interrupt
#[tokio::test]
async fn test_interrupt_modify_state() -> Result<()> {
    let mut builder = GraphBuilder::new("test_modify");

    builder.add_node("calculate", |state: StateData| async move {
        let mut state = state.clone();
        state.insert("calculation".to_string(), json!(10));
        Ok(state)
    });

    builder.add_node_with_interrupt("review", InterruptMode::Before, |state: StateData| async move {
        let mut state = state.clone();
        let value = state.get("calculation").unwrap().as_i64().unwrap();
        state.insert("result".to_string(), json!(value * 2));
        Ok(state)
    });

    builder.add_edge("calculate", "review");

    let graph = builder.compile()?;
    let engine = ExecutionEngine::new(graph);

    let handle = engine.execute_with_interrupts(
        json!({}),
        Box::new(|mut handle: InterruptHandle| {
            Box::pin(async move {
                // Modify the calculation value during interrupt
                handle.modify_state(json!({
                    "calculation": 20,
                    "modified_by_human": true
                })).await?;

                Ok(ApprovalDecision::Continue)
            })
        })
    ).await?;

    let result = handle.await?;

    // Result should be based on modified value (20 * 2 = 40)
    assert_eq!(result.get("result").unwrap().as_i64().unwrap(), 40);
    assert!(result.get("modified_by_human").unwrap().as_bool().unwrap());

    Ok(())
}

/// Test timeout handling for interrupts
#[tokio::test]
async fn test_interrupt_timeout() -> Result<()> {
    let mut builder = GraphBuilder::new("test_timeout");

    builder.add_node_with_interrupt("wait", InterruptMode::Before, |state: StateData| async move {
        Ok(state)
    });

    let graph = builder.compile()?;
    let engine = ExecutionEngine::new(graph);

    // Configure a short timeout
    let handle = engine.execute_with_interrupts_and_timeout(
        json!({}),
        Box::new(|_handle: InterruptHandle| {
            Box::pin(async move {
                // Simulate human taking too long to respond
                tokio::time::sleep(Duration::from_secs(5)).await;
                Ok(ApprovalDecision::Continue)
            })
        }),
        Duration::from_secs(1) // 1 second timeout
    ).await;

    // Should timeout
    assert!(handle.is_err());
    let error = handle.unwrap_err();
    assert!(error.to_string().contains("timeout"));

    Ok(())
}

/// Test multiple interrupt points in a single execution
#[tokio::test]
async fn test_multiple_interrupt_points() -> Result<()> {
    let mut builder = GraphBuilder::new("test_multiple");

    builder.add_node_with_interrupt("step1", InterruptMode::After, |state: StateData| async move {
        let mut state = state.clone();
        state.insert("step1".to_string(), json!(true));
        Ok(state)
    });

    builder.add_node_with_interrupt("step2", InterruptMode::Before, |state: StateData| async move {
        let mut state = state.clone();
        state.insert("step2".to_string(), json!(true));
        Ok(state)
    });

    builder.add_node_with_interrupt("step3", InterruptMode::After, |state: StateData| async move {
        let mut state = state.clone();
        state.insert("step3".to_string(), json!(true));
        Ok(state)
    });

    builder.add_edge("step1", "step2");
    builder.add_edge("step2", "step3");

    let graph = builder.compile()?;
    let engine = ExecutionEngine::new(graph);

    let interrupt_count = Arc::new(RwLock::new(0));
    let interrupt_count_clone = interrupt_count.clone();

    let handle = engine.execute_with_interrupts(
        json!({}),
        Box::new(move |handle: InterruptHandle| {
            let interrupt_count = interrupt_count_clone.clone();
            Box::pin(async move {
                let mut count = interrupt_count.write().await;
                *count += 1;

                // Verify we're at the right interrupt point
                match *count {
                    1 => assert_eq!(handle.node_id, "step1"),
                    2 => assert_eq!(handle.node_id, "step2"),
                    3 => assert_eq!(handle.node_id, "step3"),
                    _ => panic!("Unexpected interrupt count"),
                }

                Ok(ApprovalDecision::Continue)
            })
        })
    ).await?;

    let result = handle.await?;

    // Verify all interrupts were triggered
    assert_eq!(*interrupt_count.read().await, 3);

    // Verify all steps executed
    assert!(result.get("step1").unwrap().as_bool().unwrap());
    assert!(result.get("step2").unwrap().as_bool().unwrap());
    assert!(result.get("step3").unwrap().as_bool().unwrap());

    Ok(())
}

/// Test abort decision during interrupt
#[tokio::test]
async fn test_interrupt_abort() -> Result<()> {
    let mut builder = GraphBuilder::new("test_abort");

    builder.add_node("start", |state: StateData| async move {
        let mut state = state.clone();
        state.insert("started".to_string(), json!(true));
        Ok(state)
    });

    builder.add_node_with_interrupt("danger", InterruptMode::Before, |state: StateData| async move {
        // This should not execute if aborted
        panic!("Should not execute after abort");
    });

    builder.add_edge("start", "danger");

    let graph = builder.compile()?;
    let engine = ExecutionEngine::new(graph);

    let handle = engine.execute_with_interrupts(
        json!({}),
        Box::new(|_handle: InterruptHandle| {
            Box::pin(async move {
                // Abort the execution
                Ok(ApprovalDecision::Abort("User cancelled operation".to_string()))
            })
        })
    ).await;

    // Execution should be aborted
    assert!(handle.is_err());
    let error = handle.unwrap_err();
    assert!(error.to_string().contains("User cancelled operation"));

    Ok(())
}