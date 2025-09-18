use langgraph::engine::human_in_loop::{
    ApprovalDecision, HumanInLoop, InterruptHandle, InterruptManager, InterruptMode,
};
use langgraph::graph::{GraphBuilder, NodeType};
use langgraph::state::GraphState;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

#[tokio::test]
async fn test_interrupt_before_node() {
    // Create graph with interrupt point
    let mut graph = GraphBuilder::new("interrupt_test");

    // Add nodes with interrupt configuration
    graph.add_node("start", NodeType::Start);
    graph.add_node_with_interrupt(
        "review",
        NodeType::Process(Arc::new(|state| {
            Box::pin(async move {
                state.update("reviewed", true.into()).await;
                Ok(state)
            })
        })),
        InterruptMode::Before,
    );
    graph.add_node("end", NodeType::End);

    // Add edges
    graph.add_edge("start", "review");
    graph.add_edge("review", "end");

    let compiled = graph.compile().unwrap();
    let interrupt_manager = Arc::new(InterruptManager::new());

    // Execute with interrupt handling
    let state = GraphState::new();
    state.update("input", "test_data".into()).await;

    let execution_handle = compiled
        .execute_with_interrupt(state.clone(), interrupt_manager.clone())
        .await
        .unwrap();

    // Should be interrupted at review node
    let interrupt_handle = timeout(Duration::from_secs(1), interrupt_manager.wait_for_interrupt())
        .await
        .expect("Should interrupt within timeout")
        .expect("Should have interrupt handle");

    assert_eq!(interrupt_handle.node_id, "review");
    assert!(interrupt_handle.state_snapshot.contains_key("input"));

    // Approve continuation
    interrupt_manager
        .approve(interrupt_handle.id, ApprovalDecision::Continue)
        .await
        .expect("Should approve successfully");

    // Execution should complete
    let result = timeout(Duration::from_secs(1), execution_handle)
        .await
        .expect("Should complete within timeout")
        .expect("Execution should succeed");

    assert!(result.get("reviewed").is_some());
    assert_eq!(result.get("reviewed").unwrap().as_bool(), Some(true));
}

#[tokio::test]
async fn test_interrupt_after_node() {
    let mut graph = GraphBuilder::new("interrupt_after_test");

    graph.add_node("start", NodeType::Start);
    graph.add_node_with_interrupt(
        "process",
        NodeType::Process(Arc::new(|state| {
            Box::pin(async move {
                state.update("processed", true.into()).await;
                Ok(state)
            })
        })),
        InterruptMode::After,
    );
    graph.add_node("end", NodeType::End);

    graph.add_edge("start", "process");
    graph.add_edge("process", "end");

    let compiled = graph.compile().unwrap();
    let interrupt_manager = Arc::new(InterruptManager::new());

    let state = GraphState::new();
    let execution_handle = compiled
        .execute_with_interrupt(state.clone(), interrupt_manager.clone())
        .await
        .unwrap();

    // Should interrupt after process node
    let interrupt_handle = timeout(Duration::from_secs(1), interrupt_manager.wait_for_interrupt())
        .await
        .expect("Should interrupt")
        .expect("Should have handle");

    // State should include processed=true since interrupt is after
    assert!(interrupt_handle.state_snapshot.get("processed").is_some());

    // Approve
    interrupt_manager
        .approve(interrupt_handle.id, ApprovalDecision::Continue)
        .await
        .unwrap();

    let result = timeout(Duration::from_secs(1), execution_handle)
        .await
        .unwrap()
        .unwrap();

    assert!(result.get("processed").is_some());
}

#[tokio::test]
async fn test_reject_with_redirect() {
    let mut graph = GraphBuilder::new("redirect_test");

    graph.add_node("start", NodeType::Start);
    graph.add_node_with_interrupt(
        "risky_operation",
        NodeType::Process(Arc::new(|state| {
            Box::pin(async move {
                state.update("risk_taken", true.into()).await;
                Ok(state)
            })
        })),
        InterruptMode::Before,
    );
    graph.add_node("safe_path", NodeType::Process(Arc::new(|state| {
        Box::pin(async move {
            state.update("safe_choice", true.into()).await;
            Ok(state)
        })
    })));
    graph.add_node("end", NodeType::End);

    graph.add_edge("start", "risky_operation");
    graph.add_edge("risky_operation", "end");
    graph.add_edge("safe_path", "end");

    let compiled = graph.compile().unwrap();
    let interrupt_manager = Arc::new(InterruptManager::new());

    let state = GraphState::new();
    let execution_handle = compiled
        .execute_with_interrupt(state.clone(), interrupt_manager.clone())
        .await
        .unwrap();

    // Get interrupt at risky_operation
    let interrupt_handle = timeout(Duration::from_secs(1), interrupt_manager.wait_for_interrupt())
        .await
        .unwrap()
        .unwrap();

    // Redirect to safe_path instead
    interrupt_manager
        .approve(interrupt_handle.id, ApprovalDecision::Redirect("safe_path".to_string()))
        .await
        .unwrap();

    let result = timeout(Duration::from_secs(1), execution_handle)
        .await
        .unwrap()
        .unwrap();

    // Should have taken safe path, not risky
    assert!(result.get("safe_choice").is_some());
    assert!(result.get("risk_taken").is_none());
}

#[tokio::test]
async fn test_modify_state_during_interrupt() {
    let mut graph = GraphBuilder::new("modify_state_test");

    graph.add_node("start", NodeType::Start);
    graph.add_node_with_interrupt(
        "process",
        NodeType::Process(Arc::new(|state| {
            Box::pin(async move {
                let value = state.get("counter")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                state.update("counter", (value * 2).into()).await;
                Ok(state)
            })
        })),
        InterruptMode::Before,
    );
    graph.add_node("end", NodeType::End);

    graph.add_edge("start", "process");
    graph.add_edge("process", "end");

    let compiled = graph.compile().unwrap();
    let interrupt_manager = Arc::new(InterruptManager::new());

    let state = GraphState::new();
    state.update("counter", 5i64.into()).await;

    let execution_handle = compiled
        .execute_with_interrupt(state.clone(), interrupt_manager.clone())
        .await
        .unwrap();

    let interrupt_handle = timeout(Duration::from_secs(1), interrupt_manager.wait_for_interrupt())
        .await
        .unwrap()
        .unwrap();

    // Modify state before approval
    let mut modified_state = interrupt_handle.state_snapshot.clone();
    modified_state.insert("counter".to_string(), 10i64.into());
    modified_state.insert("modified".to_string(), true.into());

    interrupt_manager
        .modify_and_approve(
            interrupt_handle.id,
            modified_state,
            ApprovalDecision::Continue,
        )
        .await
        .unwrap();

    let result = timeout(Duration::from_secs(1), execution_handle)
        .await
        .unwrap()
        .unwrap();

    // Counter should be 20 (10 * 2)
    assert_eq!(result.get("counter").unwrap().as_i64(), Some(20));
    assert_eq!(result.get("modified").unwrap().as_bool(), Some(true));
}

#[tokio::test]
async fn test_interrupt_timeout() {
    let mut graph = GraphBuilder::new("timeout_test");

    graph.add_node("start", NodeType::Start);
    graph.add_node_with_interrupt(
        "waiting",
        NodeType::Process(Arc::new(|state| {
            Box::pin(async move { Ok(state) })
        })),
        InterruptMode::Before,
    );
    graph.add_node("end", NodeType::End);

    graph.add_edge("start", "waiting");
    graph.add_edge("waiting", "end");

    let compiled = graph.compile().unwrap();
    let interrupt_manager = Arc::new(InterruptManager::new());

    // Set short timeout for interrupts
    interrupt_manager.set_default_timeout(Duration::from_millis(100));

    let state = GraphState::new();
    let execution_handle = compiled
        .execute_with_interrupt(state.clone(), interrupt_manager.clone())
        .await
        .unwrap();

    // Don't approve, let it timeout
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Execution should fail with timeout
    let result = execution_handle.await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("timeout"));
}

#[tokio::test]
async fn test_multiple_interrupt_points() {
    let mut graph = GraphBuilder::new("multiple_interrupts");

    graph.add_node("start", NodeType::Start);
    graph.add_node_with_interrupt(
        "step1",
        NodeType::Process(Arc::new(|state| {
            Box::pin(async move {
                state.update("step1_done", true.into()).await;
                Ok(state)
            })
        })),
        InterruptMode::After,
    );
    graph.add_node_with_interrupt(
        "step2",
        NodeType::Process(Arc::new(|state| {
            Box::pin(async move {
                state.update("step2_done", true.into()).await;
                Ok(state)
            })
        })),
        InterruptMode::Before,
    );
    graph.add_node("end", NodeType::End);

    graph.add_edge("start", "step1");
    graph.add_edge("step1", "step2");
    graph.add_edge("step2", "end");

    let compiled = graph.compile().unwrap();
    let interrupt_manager = Arc::new(InterruptManager::new());

    let state = GraphState::new();
    let execution_handle = compiled
        .execute_with_interrupt(state.clone(), interrupt_manager.clone())
        .await
        .unwrap();

    // First interrupt after step1
    let interrupt1 = timeout(Duration::from_secs(1), interrupt_manager.wait_for_interrupt())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(interrupt1.node_id, "step1");
    assert!(interrupt1.state_snapshot.get("step1_done").is_some());

    interrupt_manager
        .approve(interrupt1.id, ApprovalDecision::Continue)
        .await
        .unwrap();

    // Second interrupt before step2
    let interrupt2 = timeout(Duration::from_secs(1), interrupt_manager.wait_for_interrupt())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(interrupt2.node_id, "step2");
    assert!(interrupt2.state_snapshot.get("step2_done").is_none());

    interrupt_manager
        .approve(interrupt2.id, ApprovalDecision::Continue)
        .await
        .unwrap();

    let result = timeout(Duration::from_secs(1), execution_handle)
        .await
        .unwrap()
        .unwrap();

    assert!(result.get("step1_done").is_some());
    assert!(result.get("step2_done").is_some());
}

#[tokio::test]
async fn test_abort_execution() {
    let mut graph = GraphBuilder::new("abort_test");

    graph.add_node("start", NodeType::Start);
    graph.add_node_with_interrupt(
        "dangerous",
        NodeType::Process(Arc::new(|state| {
            Box::pin(async move {
                state.update("danger_executed", true.into()).await;
                Ok(state)
            })
        })),
        InterruptMode::Before,
    );
    graph.add_node("end", NodeType::End);

    graph.add_edge("start", "dangerous");
    graph.add_edge("dangerous", "end");

    let compiled = graph.compile().unwrap();
    let interrupt_manager = Arc::new(InterruptManager::new());

    let state = GraphState::new();
    let execution_handle = compiled
        .execute_with_interrupt(state.clone(), interrupt_manager.clone())
        .await
        .unwrap();

    let interrupt_handle = timeout(Duration::from_secs(1), interrupt_manager.wait_for_interrupt())
        .await
        .unwrap()
        .unwrap();

    // Abort the execution
    interrupt_manager
        .approve(
            interrupt_handle.id,
            ApprovalDecision::Abort("Too dangerous to continue".to_string()),
        )
        .await
        .unwrap();

    let result = execution_handle.await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Too dangerous"));
}