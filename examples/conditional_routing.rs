//! Example demonstrating conditional routing based on state evaluation

use langgraph::graph::{GraphBuilder, NodeType, ConditionalRouter, ConditionalBranch};
use langgraph::state::{GraphState, StateData};
use serde_json::json;
use std::sync::Arc;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("🚀 LangGraph Rust - Conditional Routing Example\n");
    
    // Build a graph with conditional routing
    let graph = GraphBuilder::new("decision_workflow")
        .add_node("__start__", NodeType::Start)
        .add_node("evaluate", NodeType::Agent("evaluator".to_string()))
        .add_node("high_priority", NodeType::Agent("high_handler".to_string()))
        .add_node("medium_priority", NodeType::Agent("medium_handler".to_string()))
        .add_node("low_priority", NodeType::Agent("low_handler".to_string()))
        .add_node("__end__", NodeType::End)
        .set_entry_point("__start__")
        .add_edge("__start__", "evaluate")
        .add_edge("high_priority", "__end__")
        .add_edge("medium_priority", "__end__")
        .add_edge("low_priority", "__end__")
        .build()?;
    
    // Create conditional router
    let router = ConditionalRouter::new("evaluate");
    
    // Define routing branches based on priority score
    let high_branch = ConditionalBranch {
        condition: Arc::new(|state: &StateData| {
            state.get("priority_score")
                .and_then(|v| v.as_i64())
                .map(|score| score >= 80)
                .unwrap_or(false)
        }),
        target: "high_priority".to_string(),
        priority: Some(10),
        metadata: None,
    };
    
    let medium_branch = ConditionalBranch {
        condition: Arc::new(|state: &StateData| {
            state.get("priority_score")
                .and_then(|v| v.as_i64())
                .map(|score| score >= 50 && score < 80)
                .unwrap_or(false)
        }),
        target: "medium_priority".to_string(),
        priority: Some(5),
        metadata: None,
    };
    
    let low_branch = ConditionalBranch {
        condition: Arc::new(|state: &StateData| {
            state.get("priority_score")
                .and_then(|v| v.as_i64())
                .map(|score| score < 50)
                .unwrap_or(false)
        }),
        target: "low_priority".to_string(),
        priority: Some(1),
        metadata: None,
    };
    
    // Test different scenarios
    println!("Testing conditional routing with different priority scores:\n");
    
    // Scenario 1: High priority
    let mut state1 = StateData::new();
    state1.insert("task".to_string(), json!("Critical bug fix"));
    state1.insert("priority_score".to_string(), json!(90));
    
    let route1 = router.route_with_branches(&state1, vec![
        high_branch.clone(),
        medium_branch.clone(),
        low_branch.clone(),
    ]);
    
    println!("Task: {}", state1.get("task").unwrap());
    println!("Priority Score: {}", state1.get("priority_score").unwrap());
    println!("→ Routed to: {}\n", route1.unwrap_or("unknown".to_string()));
    
    // Scenario 2: Medium priority
    let mut state2 = StateData::new();
    state2.insert("task".to_string(), json!("Feature enhancement"));
    state2.insert("priority_score".to_string(), json!(65));
    
    let route2 = router.route_with_branches(&state2, vec![
        high_branch.clone(),
        medium_branch.clone(),
        low_branch.clone(),
    ]);
    
    println!("Task: {}", state2.get("task").unwrap());
    println!("Priority Score: {}", state2.get("priority_score").unwrap());
    println!("→ Routed to: {}\n", route2.unwrap_or("unknown".to_string()));
    
    // Scenario 3: Low priority
    let mut state3 = StateData::new();
    state3.insert("task".to_string(), json!("Documentation update"));
    state3.insert("priority_score".to_string(), json!(30));
    
    let route3 = router.route_with_branches(&state3, vec![
        high_branch,
        medium_branch,
        low_branch,
    ]);
    
    println!("Task: {}", state3.get("task").unwrap());
    println!("Priority Score: {}", state3.get("priority_score").unwrap());
    println!("→ Routed to: {}\n", route3.unwrap_or("unknown".to_string()));
    
    println!("✅ Conditional routing demonstration complete!");
    
    Ok(())
}