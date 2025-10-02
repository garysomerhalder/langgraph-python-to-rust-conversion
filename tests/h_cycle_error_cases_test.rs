//! H Cycle Error Case Tests
//!
//! Tests for GREEN phase hardening - error handling, validation, and resilience.

use langgraph::graph::{GraphBuilder, NodeType};
use langgraph::engine::ExecutionEngine;
use langgraph::state::StateData;
use serde_json::json;

#[tokio::test]
async fn test_empty_agent_name_should_fail() {
    // GREEN PHASE: Test validation of empty agent name

    // Build graph with agent that has empty name
    let graph = GraphBuilder::new("test_graph")
        .add_node("__start__", NodeType::Start)
        .add_node("empty_agent", NodeType::Agent("".to_string()))
        .add_node("__end__", NodeType::End)
        .set_entry_point("__start__")
        .add_edge("__start__", "empty_agent")
        .add_edge("empty_agent", "__end__")
        .build().expect("Failed to build graph")
        .compile().expect("Failed to compile graph");

    let mut initial_state = StateData::new();
    initial_state.insert("test_key".to_string(), json!("test_value"));

    // Execute graph - should fail due to empty agent name
    let engine = ExecutionEngine::new();
    let result = engine.execute(graph, initial_state).await;

    // ASSERTION: Execution should fail with validation error
    assert!(
        result.is_err(),
        "Execution should fail with empty agent name, but succeeded"
    );

    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Agent name cannot be empty") || err_msg.contains("empty"),
        "Error message should mention empty agent name, got: {}",
        err_msg
    );
}

#[tokio::test]
async fn test_empty_tool_name_should_fail() {
    // GREEN PHASE: Test validation of empty tool name

    // Build graph with tool that has empty name
    let graph = GraphBuilder::new("test_graph")
        .add_node("__start__", NodeType::Start)
        .add_node("empty_tool", NodeType::Tool("".to_string()))
        .add_node("__end__", NodeType::End)
        .set_entry_point("__start__")
        .add_edge("__start__", "empty_tool")
        .add_edge("empty_tool", "__end__")
        .build().expect("Failed to build graph")
        .compile().expect("Failed to compile graph");

    let mut initial_state = StateData::new();
    initial_state.insert("counter".to_string(), json!(5));

    // Execute graph - should fail due to empty tool name
    let engine = ExecutionEngine::new();
    let result = engine.execute(graph, initial_state).await;

    // ASSERTION: Execution should fail with validation error
    assert!(
        result.is_err(),
        "Execution should fail with empty tool name, but succeeded"
    );

    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Tool name cannot be empty") || err_msg.contains("empty"),
        "Error message should mention empty tool name, got: {}",
        err_msg
    );
}

#[tokio::test]
async fn test_agent_with_invalid_state_types() {
    // GREEN PHASE: Test handling of invalid state value types

    let graph = GraphBuilder::new("test_graph")
        .add_node("__start__", NodeType::Start)
        .add_node("test_agent", NodeType::Agent("test_agent".to_string()))
        .add_node("__end__", NodeType::End)
        .set_entry_point("__start__")
        .add_edge("__start__", "test_agent")
        .add_edge("test_agent", "__end__")
        .build().expect("Failed to build graph")
        .compile().expect("Failed to compile graph");

    // Initial state with execution_count as string (wrong type)
    let mut initial_state = StateData::new();
    initial_state.insert("execution_count".to_string(), json!("not_a_number"));

    // Execute graph - should succeed but log warning
    let engine = ExecutionEngine::new();
    let result = engine.execute(graph, initial_state.clone()).await
        .expect("Execution should succeed despite wrong type");

    // ASSERTION: execution_count should remain unchanged (not incremented)
    let count = result.get("execution_count")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    assert_eq!(
        count, "not_a_number",
        "Invalid type should be ignored, execution_count should remain unchanged"
    );
}

#[tokio::test]
async fn test_tool_with_invalid_counter_type() {
    // GREEN PHASE: Test handling of invalid counter type

    let graph = GraphBuilder::new("test_graph")
        .add_node("__start__", NodeType::Start)
        .add_node("test_tool", NodeType::Tool("test_tool".to_string()))
        .add_node("__end__", NodeType::End)
        .set_entry_point("__start__")
        .add_edge("__start__", "test_tool")
        .add_edge("test_tool", "__end__")
        .build().expect("Failed to build graph")
        .compile().expect("Failed to compile graph");

    // Initial state with counter as boolean (wrong type)
    let mut initial_state = StateData::new();
    initial_state.insert("counter".to_string(), json!(true));

    // Execute graph - should succeed but log warning
    let engine = ExecutionEngine::new();
    let result = engine.execute(graph, initial_state.clone()).await
        .expect("Execution should succeed despite wrong type");

    // ASSERTION: counter should remain unchanged (not incremented)
    let counter = result.get("counter")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    assert!(
        counter,
        "Invalid type should be ignored, counter should remain unchanged as true"
    );
}

#[tokio::test]
async fn test_sequential_agents_accumulate_correctly() {
    // GREEN PHASE: Test that multiple agent nodes accumulate state correctly

    let graph = GraphBuilder::new("test_graph")
        .add_node("__start__", NodeType::Start)
        .add_node("agent1", NodeType::Agent("agent1".to_string()))
        .add_node("agent2", NodeType::Agent("agent2".to_string()))
        .add_node("agent3", NodeType::Agent("agent3".to_string()))
        .add_node("__end__", NodeType::End)
        .set_entry_point("__start__")
        .add_edge("__start__", "agent1")
        .add_edge("agent1", "agent2")
        .add_edge("agent2", "agent3")
        .add_edge("agent3", "__end__")
        .build().expect("Failed to build graph")
        .compile().expect("Failed to compile graph");

    let mut initial_state = StateData::new();
    initial_state.insert("execution_count".to_string(), json!(0));

    let engine = ExecutionEngine::new();
    let result = engine.execute(graph, initial_state).await
        .expect("Execution should succeed");

    // ASSERTION: All 3 agents should have incremented execution_count
    let count = result.get("execution_count")
        .and_then(|v| v.as_i64())
        .expect("execution_count should exist as integer");

    assert_eq!(
        count, 3,
        "All 3 agent nodes should have incremented execution_count to 3, got: {}",
        count
    );
}

#[tokio::test]
async fn test_mixed_node_types_accumulate_correctly() {
    // GREEN PHASE: Test that mixed agent and tool nodes both increment counters

    let graph = GraphBuilder::new("test_graph")
        .add_node("__start__", NodeType::Start)
        .add_node("agent1", NodeType::Agent("agent1".to_string()))
        .add_node("tool1", NodeType::Tool("tool1".to_string()))
        .add_node("agent2", NodeType::Agent("agent2".to_string()))
        .add_node("__end__", NodeType::End)
        .set_entry_point("__start__")
        .add_edge("__start__", "agent1")
        .add_edge("agent1", "tool1")
        .add_edge("tool1", "agent2")
        .add_edge("agent2", "__end__")
        .build().expect("Failed to build graph")
        .compile().expect("Failed to compile graph");

    let mut initial_state = StateData::new();
    initial_state.insert("execution_count".to_string(), json!(0));

    let engine = ExecutionEngine::new();
    let result = engine.execute(graph, initial_state).await
        .expect("Execution should succeed");

    // ASSERTION: 2 agents + 1 tool = 3 increments
    let count = result.get("execution_count")
        .and_then(|v| v.as_i64())
        .expect("execution_count should exist as integer");

    assert_eq!(
        count, 3,
        "Mixed node types (2 agents + 1 tool) should increment to 3, got: {}",
        count
    );
}
