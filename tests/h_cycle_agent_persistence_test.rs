//! H Cycle Iteration 3 - Agent State Persistence
//!
//! RED Phase: Tests that agents maintain memory across multiple nodes

use langgraph::graph::{GraphBuilder, NodeType};
use langgraph::engine::ExecutionEngine;
use langgraph::state::StateData;
use serde_json::json;

#[tokio::test]
async fn test_agent_memory_persists_across_nodes() {
    // RED PHASE: This test will fail - agents currently lose memory between nodes

    // Build graph with TWO agent nodes using the SAME agent name
    let graph = GraphBuilder::new("memory_persistence_test")
        .add_node("__start__", NodeType::Start)
        .add_node("observer", NodeType::Agent("persistent_agent".to_string()))
        .add_node("recaller", NodeType::Agent("persistent_agent".to_string()))
        .add_node("__end__", NodeType::End)
        .set_entry_point("__start__")
        .add_edge("__start__", "observer")
        .add_edge("observer", "recaller")
        .add_edge("recaller", "__end__")
        .build().expect("Failed to build graph")
        .compile().expect("Failed to compile graph");

    // Initial state: First node observes important information
    let mut initial_state = StateData::new();
    initial_state.insert("observation".to_string(), json!("Remember: the password is 'blue42'"));

    // Execute graph
    let engine = ExecutionEngine::new();
    let result = engine.execute(graph, initial_state).await
        .expect("Execution should succeed");

    // ASSERTION: Second node should have access to first node's memory
    let final_memory = result.get("agent_memory")
        .and_then(|v| v.as_array())
        .expect("Agent memory should exist");

    // Should have at least 2 entries: one from observer, one from recaller
    assert!(
        final_memory.len() >= 2,
        "Agent memory should accumulate across nodes, got {} entries",
        final_memory.len()
    );

    // Check that password observation is in memory
    let memory_json = serde_json::to_string(&final_memory).unwrap();
    assert!(
        memory_json.contains("blue42"),
        "Agent memory should contain the password observation from first node"
    );
}

#[tokio::test]
async fn test_different_agents_have_separate_memory() {
    // RED PHASE: Test that different agent names maintain separate contexts

    let graph = GraphBuilder::new("separate_memory_test")
        .add_node("__start__", NodeType::Start)
        .add_node("agent_a", NodeType::Agent("agent_a".to_string()))
        .add_node("agent_b", NodeType::Agent("agent_b".to_string()))
        .add_node("agent_a_2", NodeType::Agent("agent_a".to_string()))
        .add_node("__end__", NodeType::End)
        .set_entry_point("__start__")
        .add_edge("__start__", "agent_a")
        .add_edge("agent_a", "agent_b")
        .add_edge("agent_b", "agent_a_2")
        .add_edge("agent_a_2", "__end__")
        .build().expect("Failed to build graph")
        .compile().expect("Failed to compile graph");

    let mut initial_state = StateData::new();
    initial_state.insert("observation".to_string(), json!("Agent A sees this"));

    let engine = ExecutionEngine::new();
    let result = engine.execute(graph, initial_state).await
        .expect("Execution should succeed");

    // ASSERTION: Should have separate memory storage for agent_a and agent_b
    let agent_a_state = result.get("agent_state_agent_a");
    let agent_b_state = result.get("agent_state_agent_b");

    assert!(
        agent_a_state.is_some(),
        "Should have persistent state for agent_a"
    );
    assert!(
        agent_b_state.is_some(),
        "Should have persistent state for agent_b"
    );

    // agent_a should have 2 observations (from agent_a and agent_a_2)
    // agent_b should have 1 observation
    let a_memory_str = serde_json::to_string(&agent_a_state).unwrap();
    let b_memory_str = serde_json::to_string(&agent_b_state).unwrap();

    assert!(
        a_memory_str.contains("Agent A sees this"),
        "agent_a memory should contain original observation"
    );

    // Memories should be independent
    assert_ne!(
        agent_a_state, agent_b_state,
        "Different agents should have different memory states"
    );
}

#[tokio::test]
async fn test_agent_memory_accumulates_correctly() {
    // RED PHASE: Test that memory entries accumulate in order

    let graph = GraphBuilder::new("accumulation_test")
        .add_node("__start__", NodeType::Start)
        .add_node("step1", NodeType::Agent("sequential_agent".to_string()))
        .add_node("step2", NodeType::Agent("sequential_agent".to_string()))
        .add_node("step3", NodeType::Agent("sequential_agent".to_string()))
        .add_node("__end__", NodeType::End)
        .set_entry_point("__start__")
        .add_edge("__start__", "step1")
        .add_edge("step1", "step2")
        .add_edge("step2", "step3")
        .add_edge("step3", "__end__")
        .build().expect("Failed to build graph")
        .compile().expect("Failed to compile graph");

    let mut initial_state = StateData::new();
    initial_state.insert("observation".to_string(), json!("Step 1 observation"));

    let engine = ExecutionEngine::new();
    let result = engine.execute(graph, initial_state).await
        .expect("Execution should succeed");

    // ASSERTION: Final memory should show progression through all 3 steps
    let final_memory = result.get("agent_memory")
        .and_then(|v| v.as_array())
        .expect("Agent memory should exist");

    // Should have at least 6 entries: 2 per step (observation + decision)
    assert!(
        final_memory.len() >= 6,
        "Agent should accumulate memory from all steps, got {} entries",
        final_memory.len()
    );

    // Verify memory is from same agent across all nodes
    let agent_state = result.get("agent_state_sequential_agent");
    assert!(
        agent_state.is_some(),
        "Should have persistent state for sequential_agent"
    );
}
