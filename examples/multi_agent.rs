//! Example demonstrating multi-agent collaboration with reasoning

use langgraph::agents::{Agent, ReasoningAgent, ReasoningStrategy, AgentMemory};
use langgraph::state::GraphState;
use langgraph::tools::ToolRegistry;
use serde_json::json;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("🚀 LangGraph Rust - Multi-Agent Collaboration Example\n");
    
    // Create specialized agents
    let mut research_agent = ReasoningAgent::new(
        "researcher",
        "Gathers and analyzes information",
        ReasoningStrategy::ChainOfThought,
    );
    
    let mut analysis_agent = ReasoningAgent::new(
        "analyst",
        "Processes and evaluates data",
        ReasoningStrategy::ReAct,
    );
    
    let mut decision_agent = ReasoningAgent::new(
        "decision_maker",
        "Makes final decisions based on analysis",
        ReasoningStrategy::ChainOfThought,
    );
    
    // Add available tools to agents
    research_agent.add_tool("web_search".to_string());
    research_agent.add_tool("document_retrieval".to_string());
    
    analysis_agent.add_tool("data_analysis".to_string());
    analysis_agent.add_tool("pattern_recognition".to_string());
    
    decision_agent.add_tool("risk_assessment".to_string());
    decision_agent.add_tool("decision_tree".to_string());
    
    // Simulate a collaborative workflow
    println!("Starting multi-agent collaboration on project evaluation...\n");
    
    // Step 1: Research phase
    println!("📚 Research Agent:");
    let research_observation = json!({
        "project": "New AI Product Launch",
        "market_size": "$2.5B",
        "competitors": 5,
        "technology_readiness": "High"
    });
    
    let state = GraphState::new();
    research_agent.observe(research_observation.clone(), &state.values).await?;
    
    let research_decision = research_agent.reason(&state.values).await?;
    println!("  Decision: {}", research_decision.action);
    println!("  Confidence: {:.1}%", research_decision.confidence * 100.0);
    println!("  Reasoning: {}\n", research_decision.reasoning);
    
    // Update agent memory with findings
    let mut research_memory = research_agent.memory().clone();
    research_memory.store_long_term(
        "market_analysis".to_string(),
        json!({
            "viable": true,
            "opportunity_score": 85
        }),
    );
    research_agent.update_memory(research_memory);
    
    // Step 2: Analysis phase
    println!("📊 Analysis Agent:");
    let analysis_observation = json!({
        "research_findings": "Positive market indicators",
        "risk_factors": ["Competition", "Time to market"],
        "opportunity_score": 85
    });
    
    analysis_agent.observe(analysis_observation, &state.values).await?;
    
    // Set goal in working memory for ReAct strategy
    let mut analysis_memory = AgentMemory::new();
    analysis_memory.update_working("goal".to_string(), json!("evaluate_project_viability"));
    analysis_agent.update_memory(analysis_memory);
    
    let analysis_decision = analysis_agent.reason(&state.values).await?;
    println!("  Decision: {}", analysis_decision.action);
    println!("  Confidence: {:.1}%", analysis_decision.confidence * 100.0);
    println!("  Reasoning: {}\n", analysis_decision.reasoning);
    
    // Step 3: Decision phase
    println!("🎯 Decision Agent:");
    let decision_observation = json!({
        "research_score": 85,
        "analysis_result": "Favorable",
        "risk_level": "Moderate"
    });
    
    decision_agent.observe(decision_observation, &state.values).await?;
    
    let final_decision = decision_agent.reason(&state.values).await?;
    println!("  Decision: {}", final_decision.action);
    println!("  Confidence: {:.1}%", final_decision.confidence * 100.0);
    println!("  Reasoning: {}\n", final_decision.reasoning);
    
    // Simulate reflection on outcomes
    let tools = ToolRegistry::new();
    let mut final_state = state.values.clone();
    
    let result = decision_agent.act(&final_decision, &tools, &mut final_state).await?;
    decision_agent.reflect(&result, &final_state).await?;
    
    // Display collaboration summary
    println!("=" .repeat(50));
    println!("\n📋 Collaboration Summary:");
    println!("  Research Agent: Gathered market data and competitive analysis");
    println!("  Analysis Agent: Evaluated viability and identified risks");
    println!("  Decision Agent: Made final go/no-go decision");
    println!("\n✅ Multi-agent collaboration completed successfully!");
    
    // Show agent memories
    println!("\n🧠 Agent Memory States:");
    println!("  Research Agent: {} short-term entries", research_agent.memory().short_term.len());
    println!("  Analysis Agent: {} short-term entries", analysis_agent.memory().short_term.len());
    println!("  Decision Agent: {} short-term entries", decision_agent.memory().short_term.len());
    
    Ok(())
}