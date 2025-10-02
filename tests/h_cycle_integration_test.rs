//! H Cycle Integration Tests - Testing Real Node Execution
//!
//! These tests verify that the H cycle actually executes nodes and transforms state,
//! rather than just passing through input unchanged (stubbed behavior).

use langgraph::graph::{GraphBuilder, NodeType};
use langgraph::state::StateData;
use langgraph::engine::ExecutionEngine;
use serde_json::json;

#[tokio::test]
async fn test_agent_node_should_transform_state() {
    // RED PHASE: This test MUST FAIL with current stubbed implementation
    // The stub returns input unchanged, but this test expects transformation

    // Build graph with agent node
    let graph = GraphBuilder::new("test_graph")
        .add_node("__start__", NodeType::Start)
        .add_node("test_agent", NodeType::Agent("test_agent".to_string()))
        .add_node("__end__", NodeType::End)
        .set_entry_point("__start__")
        .add_edge("__start__", "test_agent")
        .add_edge("test_agent", "__end__")
        .build().expect("Failed to build graph")
        .compile().expect("Failed to compile graph");

    // Initial state with a counter
    let mut initial_state = StateData::new();
    initial_state.insert("counter".to_string(), json!(1));
    initial_state.insert("agent_executed".to_string(), json!(false));

    // Execute graph
    let engine = ExecutionEngine::new();
    let result = engine.execute(graph, initial_state.clone()).await
        .expect("Execution should succeed");

    // ASSERTION: State should be transformed by agent
    // With STUB: result == initial_state (unchanged)
    // With REAL: result should have agent_executed = true

    let agent_executed = result.get("agent_executed")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // This WILL FAIL with stub implementation
    assert!(
        agent_executed,
        "Agent should have set agent_executed to true, but state was unchanged (stub behavior)"
    );
}

#[tokio::test]
async fn test_tool_node_should_transform_state() {
    // RED PHASE: This test MUST FAIL with current stubbed implementation

    // Build graph with tool node
    let graph = GraphBuilder::new("test_graph")
        .add_node("__start__", NodeType::Start)
        .add_node("test_tool", NodeType::Tool("test_tool".to_string()))
        .add_node("__end__", NodeType::End)
        .set_entry_point("__start__")
        .add_edge("__start__", "test_tool")
        .add_edge("test_tool", "__end__")
        .build().expect("Failed to build graph")
        .compile().expect("Failed to compile graph");

    // Initial state with counter
    let mut initial_state = StateData::new();
    initial_state.insert("counter".to_string(), json!(5));

    // Execute graph
    let engine = ExecutionEngine::new();
    let result = engine.execute(graph, initial_state.clone()).await
        .expect("Execution should succeed");

    // ASSERTION: Counter should be modified by tool
    let counter = result.get("counter")
        .and_then(|v| v.as_i64())
        .expect("counter should exist");

    // This WILL FAIL with stub - stub returns input unchanged (counter=5)
    // We expect the tool to increment it to 6
    assert_ne!(
        counter, 5,
        "Tool should have modified counter, but state was unchanged (stub behavior)"
    );
}

#[tokio::test]
async fn test_sequential_nodes_should_accumulate_changes() {
    // RED PHASE: Test that state changes accumulate across multiple nodes

    // Build graph with three nodes in sequence
    let graph = GraphBuilder::new("test_graph")
        .add_node("__start__", NodeType::Start)
        .add_node("node1", NodeType::Agent("node1".to_string()))
        .add_node("node2", NodeType::Agent("node2".to_string()))
        .add_node("node3", NodeType::Agent("node3".to_string()))
        .add_node("__end__", NodeType::End)
        .set_entry_point("__start__")
        .add_edge("__start__", "node1")
        .add_edge("node1", "node2")
        .add_edge("node2", "node3")
        .add_edge("node3", "__end__")
        .build().expect("Failed to build graph")
        .compile().expect("Failed to compile graph");

    // Initial state with execution count
    let mut initial_state = StateData::new();
    initial_state.insert("execution_count".to_string(), json!(0));

    // Execute graph
    let engine = ExecutionEngine::new();
    let result = engine.execute(graph, initial_state.clone()).await
        .expect("Execution should succeed");

    // ASSERTION: Each node should increment execution_count
    let count = result.get("execution_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    // This WILL FAIL with stub - stub keeps count at 0
    // We expect count to be 3 (one increment per node)
    assert!(
        count > 0,
        "Nodes should have incremented execution_count, but it remained 0 (stub behavior)"
    );
}
