//! H Cycle Iteration 4 - Tool Integration
//!
//! RED Phase: Tests that agents can actually execute tools and receive results

use langgraph::graph::{GraphBuilder, NodeType};
use langgraph::engine::ExecutionEngine;
use langgraph::state::StateData;
use serde_json::json;

#[tokio::test]
async fn test_agent_executes_calculator_tool() {
    // RED PHASE: This test will fail - no calculator tool registered

    // Build graph with agent that should use calculator
    let graph = GraphBuilder::new("calculator_test")
        .add_node("__start__", NodeType::Start)
        .add_node("calculator_agent", NodeType::Agent("calculator_agent".to_string()))
        .add_node("__end__", NodeType::End)
        .set_entry_point("__start__")
        .add_edge("__start__", "calculator_agent")
        .add_edge("calculator_agent", "__end__")
        .build().expect("Failed to build graph")
        .compile().expect("Failed to compile graph");

    // State: Ask agent to perform calculation
    let mut initial_state = StateData::new();
    initial_state.insert("input".to_string(), json!("Calculate 5 + 3"));

    // Execute graph
    let engine = ExecutionEngine::new();
    let result = engine.execute(graph, initial_state).await
        .expect("Execution should succeed");

    // ASSERTION: Agent should have executed calculator tool
    let action_result = result.get("agent_action_result")
        .expect("Agent should have action result from tool execution");

    // Tool result should be an object with success=true
    let tool_result = action_result.as_object()
        .expect("Action result should be an object");

    assert_eq!(
        tool_result.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "Tool execution should succeed"
    );

    // Tool result should contain the calculation answer
    let data = tool_result.get("data")
        .expect("Tool result should have data field");

    // Check that the result is 8 (5 + 3)
    let answer = data.get("result")
        .and_then(|v| v.as_f64())
        .expect("Tool result should contain numeric answer");

    assert_eq!(
        answer, 8.0,
        "Calculator tool should return 8 for '5 + 3', got {}",
        answer
    );
}

#[tokio::test]
async fn test_agent_executes_echo_tool() {
    // RED PHASE: Test simple echo tool execution

    let graph = GraphBuilder::new("echo_test")
        .add_node("__start__", NodeType::Start)
        .add_node("echo_agent", NodeType::Agent("echo_agent".to_string()))
        .add_node("__end__", NodeType::End)
        .set_entry_point("__start__")
        .add_edge("__start__", "echo_agent")
        .add_edge("echo_agent", "__end__")
        .build().expect("Failed to build graph")
        .compile().expect("Failed to compile graph");

    let mut initial_state = StateData::new();
    initial_state.insert("input".to_string(), json!("Echo this message"));

    let engine = ExecutionEngine::new();
    let result = engine.execute(graph, initial_state).await
        .expect("Execution should succeed");

    // ASSERTION: Echo tool should return the input unchanged
    let action_result = result.get("agent_action_result")
        .expect("Agent should have action result");

    let tool_result = action_result.as_object()
        .expect("Action result should be an object");

    assert_eq!(
        tool_result.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "Echo tool execution should succeed"
    );

    // Echo should return the input message
    let data = tool_result.get("data")
        .expect("Tool result should have data");

    let message = data.get("message")
        .and_then(|v| v.as_str())
        .expect("Echo result should contain message");

    assert_eq!(
        message, "Echo this message",
        "Echo tool should return original message"
    );
}

#[tokio::test]
async fn test_tool_execution_appears_in_memory() {
    // RED PHASE: Test that tool execution is recorded in agent memory

    let graph = GraphBuilder::new("memory_test")
        .add_node("__start__", NodeType::Start)
        .add_node("tool_agent", NodeType::Agent("tool_agent".to_string()))
        .add_node("__end__", NodeType::End)
        .set_entry_point("__start__")
        .add_edge("__start__", "tool_agent")
        .add_edge("tool_agent", "__end__")
        .build().expect("Failed to build graph")
        .compile().expect("Failed to compile graph");

    let mut initial_state = StateData::new();
    initial_state.insert("input".to_string(), json!("Use calculator to add 10 + 20"));

    let engine = ExecutionEngine::new();
    let result = engine.execute(graph, initial_state).await
        .expect("Execution should succeed");

    // ASSERTION: Agent memory should contain tool execution
    let memory = result.get("agent_memory")
        .and_then(|v| v.as_array())
        .expect("Agent should have memory");

    // Memory should include: observation, decision, result (from reflection)
    assert!(
        memory.len() >= 3,
        "Agent memory should have at least 3 entries (observation, decision, result), got {}",
        memory.len()
    );

    // Check that memory contains a "result" entry (from reflection)
    let memory_json = serde_json::to_string(&memory).unwrap();
    assert!(
        memory_json.contains("result") || memory_json.contains("observation"),
        "Agent memory should contain result entry from tool execution"
    );
}
