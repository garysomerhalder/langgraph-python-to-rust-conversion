//! H Cycle Iteration 2 - Real Agent Reasoning Integration
//!
//! RED Phase: Tests that will fail until we integrate actual Agent trait usage

use langgraph::graph::{GraphBuilder, NodeType};
use langgraph::engine::ExecutionEngine;
use langgraph::state::StateData;
use langgraph::agents::{AgentConfig, ReasoningStrategy};
use serde_json::json;

#[tokio::test]
async fn test_agent_should_use_reasoning_strategy() {
    // RED PHASE: This test will fail - we need to integrate actual Agent trait

    // Build graph with agent node
    let graph = GraphBuilder::new("reasoning_test")
        .add_node("__start__", NodeType::Start)
        .add_node("reasoning_agent", NodeType::Agent("reasoning_agent".to_string()))
        .add_node("__end__", NodeType::End)
        .set_entry_point("__start__")
        .add_edge("__start__", "reasoning_agent")
        .add_edge("reasoning_agent", "__end__")
        .build().expect("Failed to build graph")
        .compile().expect("Failed to compile graph");

    let mut initial_state = StateData::new();
    initial_state.insert("input".to_string(), json!("What is 2+2?"));

    // Execute graph
    let engine = ExecutionEngine::new();
    let result = engine.execute(graph, initial_state).await
        .expect("Execution should succeed");

    // ASSERTION: Agent should have produced reasoning output
    // Currently this will fail because we don't have actual agent reasoning
    let reasoning = result.get("agent_reasoning")
        .and_then(|v| v.as_str());

    assert!(
        reasoning.is_some(),
        "Agent should produce reasoning output, but none found"
    );

    let reasoning_text = reasoning.unwrap();
    assert!(
        !reasoning_text.is_empty(),
        "Agent reasoning should not be empty"
    );
}

#[tokio::test]
async fn test_agent_should_make_decisions() {
    // RED PHASE: Test that agent uses reason() to make decisions

    let graph = GraphBuilder::new("decision_test")
        .add_node("__start__", NodeType::Start)
        .add_node("decision_agent", NodeType::Agent("decision_agent".to_string()))
        .add_node("__end__", NodeType::End)
        .set_entry_point("__start__")
        .add_edge("__start__", "decision_agent")
        .add_edge("decision_agent", "__end__")
        .build().expect("Failed to build graph")
        .compile().expect("Failed to compile graph");

    let mut initial_state = StateData::new();
    initial_state.insert("question".to_string(), json!("Should I proceed?"));

    let engine = ExecutionEngine::new();
    let result = engine.execute(graph, initial_state).await
        .expect("Execution should succeed");

    // ASSERTION: Agent should have made a decision
    let decision = result.get("agent_decision")
        .and_then(|v| v.as_object());

    assert!(
        decision.is_some(),
        "Agent should make a decision, but none found"
    );

    let decision_obj = decision.unwrap();
    assert!(
        decision_obj.contains_key("action"),
        "Decision should contain 'action' field"
    );
    assert!(
        decision_obj.contains_key("reasoning"),
        "Decision should contain 'reasoning' field"
    );
}

#[tokio::test]
async fn test_agent_should_use_memory() {
    // RED PHASE: Test that agent maintains memory across observations

    let graph = GraphBuilder::new("memory_test")
        .add_node("__start__", NodeType::Start)
        .add_node("memory_agent", NodeType::Agent("memory_agent".to_string()))
        .add_node("__end__", NodeType::End)
        .set_entry_point("__start__")
        .add_edge("__start__", "memory_agent")
        .add_edge("memory_agent", "__end__")
        .build().expect("Failed to build graph")
        .compile().expect("Failed to compile graph");

    let mut initial_state = StateData::new();
    initial_state.insert("observation".to_string(), json!("User said: Remember this"));

    let engine = ExecutionEngine::new();
    let result = engine.execute(graph, initial_state).await
        .expect("Execution should succeed");

    // ASSERTION: Agent should have stored observation in memory
    let memory = result.get("agent_memory")
        .and_then(|v| v.as_array());

    assert!(
        memory.is_some(),
        "Agent should have memory output, but none found"
    );

    let memory_entries = memory.unwrap();
    assert!(
        !memory_entries.is_empty(),
        "Agent memory should contain at least one entry"
    );
}
