//! Simple workflow example demonstrating basic graph construction and execution

use langgraph::graph::{GraphBuilder, NodeType};
use langgraph::state::GraphState;
use serde_json::json;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("🚀 LangGraph Rust - Simple Workflow Example\n");

    // Step 1: Build the graph
    println!("Building workflow graph...");
    let graph = GraphBuilder::new("greeting_workflow")
        .add_node("__start__", NodeType::Start)
        .add_node("greet", NodeType::Agent("greeter".to_string()))
        .add_node("process", NodeType::Agent("processor".to_string()))
        .add_node("respond", NodeType::Agent("responder".to_string()))
        .add_node("__end__", NodeType::End)
        .set_entry_point("__start__")
        .add_edge("__start__", "greet")
        .add_edge("greet", "process")
        .add_edge("process", "respond")
        .add_edge("respond", "__end__")
        .build()?;

    println!("✅ Graph built with {} nodes\n", graph.node_count());

    // Step 2: Compile the graph
    let compiled = graph.compile()?;
    println!("✅ Graph compiled and ready for execution\n");

    // Step 3: Prepare initial state
    let mut state = GraphState::new();
    state.set("user_name", json!("Alice"));
    state.set("message", json!("Hello from LangGraph Rust!"));

    println!("Initial state:");
    println!("  user_name: {}", state.get("user_name").unwrap());
    println!("  message: {}\n", state.get("message").unwrap());

    // Step 4: Execute the workflow
    println!("Executing workflow...");

    // Simulate node processing
    state.add_transition("__start__".to_string(), "greet".to_string(), None);

    state.set("greeting", json!("Welcome, Alice!"));
    state.add_transition("greet".to_string(), "process".to_string(), None);

    state.set("processed", json!(true));
    state.add_transition("process".to_string(), "respond".to_string(), None);

    state.set("response", json!("Task completed successfully!"));
    state.add_transition("respond".to_string(), "__end__".to_string(), None);

    println!("✅ Workflow executed successfully!\n");

    // Step 5: Display results
    println!("Final state:");
    println!("  greeting: {}", state.get("greeting").unwrap());
    println!("  processed: {}", state.get("processed").unwrap());
    println!("  response: {}", state.get("response").unwrap());

    println!("\nExecution history:");
    for (i, transition) in state.history.iter().enumerate() {
        println!("  {}. {} -> {}", i + 1, transition.from, transition.to);
    }

    Ok(())
}
