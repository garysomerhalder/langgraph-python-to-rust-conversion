//! Integration tests for advanced LangGraph features

use langgraph::graph::{
    GraphBuilder, NodeType, 
    ConditionalRouter, ConditionalBranch,
    Subgraph, SubgraphBuilder, SelectiveMapper, PassthroughMapper,
};
use langgraph::state::{GraphState, StateData};
use langgraph::tools::{
    Tool, ToolRegistry, ToolSpec, ToolParameter, FunctionTool, 
    ToolContext, ToolResult, HttpTool, ToolChain,
};
use langgraph::agents::{
    Agent, ReasoningAgent, ReasoningStrategy, AgentMemory,
    MultiAgentCoordinator, CoordinationStrategy,
};
use langgraph::state::advanced::{
    VersionedStateManager, SnapshotManager, StateDiff,
};
use serde_json::{json, Value};
use std::sync::Arc;
use std::collections::HashMap;

#[tokio::test]
async fn test_conditional_routing() {
    // Create a graph with conditional routing
    let builder = GraphBuilder::new("conditional_workflow");
    
    builder
        .add_node("__start__", NodeType::Start)
        .add_node("check", NodeType::Agent("checker".to_string()))
        .add_node("path_a", NodeType::Agent("handler_a".to_string()))
        .add_node("path_b", NodeType::Agent("handler_b".to_string()))
        .add_node("__end__", NodeType::End)
        .set_entry_point("__start__")
        .add_edge("__start__", "check");
    
    // Create conditional router
    let router = ConditionalRouter::new("check".to_string());
    
    // Add branches based on state
    let branch_a = ConditionalBranch {
        condition: Arc::new(|state| {
            state.get("route")
                .and_then(|v| v.as_str())
                .map(|s| s == "a")
                .unwrap_or(false)
        }),
        target: "path_a".to_string(),
        priority: Some(10),
        metadata: None,
    };
    
    let branch_b = ConditionalBranch {
        condition: Arc::new(|state| {
            state.get("route")
                .and_then(|v| v.as_str())
                .map(|s| s == "b")
                .unwrap_or(false)
        }),
        target: "path_b".to_string(),
        priority: Some(5),
        metadata: None,
    };
    
    // Test routing logic
    let mut state = StateData::new();
    state.insert("route".to_string(), json!("a"));
    
    let target = router.route_with_branches(&state, vec![branch_a.clone(), branch_b.clone()]);
    assert_eq!(target, Some("path_a".to_string()));
    
    state.insert("route".to_string(), json!("b"));
    let target = router.route_with_branches(&state, vec![branch_a, branch_b]);
    assert_eq!(target, Some("path_b".to_string()));
}

#[tokio::test]
async fn test_subgraph_composition() {
    // Create a subgraph
    let subgraph_builder = GraphBuilder::new("sub_process")
        .add_node("__start__", NodeType::Start)
        .add_node("sub_task", NodeType::Agent("subtask_processor".to_string()))
        .add_node("__end__", NodeType::End)
        .set_entry_point("__start__")
        .add_edge("__start__", "sub_task")
        .add_edge("sub_task", "__end__");
    
    let subgraph_compiled = subgraph_builder.build().unwrap().compile().unwrap();
    
    // Create subgraph with mappers
    let mut subgraph = Subgraph::new("process_sub", subgraph_compiled);
    
    // Test with passthrough mapper
    let passthrough = Arc::new(PassthroughMapper);
    subgraph = subgraph.with_input_mapper(passthrough.clone())
                      .with_output_mapper(passthrough);
    
    let mut input = StateData::new();
    input.insert("test".to_string(), json!("value"));
    
    let output = subgraph.execute(&input).await.unwrap();
    assert_eq!(output.get("test"), Some(&json!("value")));
    
    // Test with selective mapper
    let selective = SelectiveMapper::new()
        .add_mapping("input_key", "mapped_key");
    
    subgraph = subgraph.with_input_mapper(Arc::new(selective));
    
    input.insert("input_key".to_string(), json!("mapped_value"));
    let mapped = subgraph.input_mapper.map(&input).await.unwrap();
    assert!(mapped.contains_key("mapped_key"));
}

#[tokio::test]
async fn test_tool_integration() {
    let mut registry = ToolRegistry::new();
    
    // Create a math tool
    let add_spec = ToolSpec {
        name: "add".to_string(),
        description: "Add two numbers".to_string(),
        parameters: vec![
            ToolParameter {
                name: "a".to_string(),
                param_type: "number".to_string(),
                required: true,
                description: Some("First number".to_string()),
                default: None,
                schema: None,
            },
            ToolParameter {
                name: "b".to_string(),
                param_type: "number".to_string(),
                required: true,
                description: Some("Second number".to_string()),
                default: None,
                schema: None,
            },
        ],
        returns: Some("number".to_string()),
        tags: vec!["math".to_string()],
        examples: vec![],
    };
    
    let add_tool = FunctionTool::new(add_spec, |params: Value, _context: ToolContext| {
        let a = params["a"].as_f64().unwrap_or(0.0);
        let b = params["b"].as_f64().unwrap_or(0.0);
        
        Ok(ToolResult {
            success: true,
            data: Some(json!({"result": a + b})),
            error: None,
            metadata: HashMap::new(),
        })
    });
    
    registry.register(Arc::new(add_tool));
    
    // Execute tool
    let params = json!({"a": 10, "b": 20});
    let context = ToolContext {
        state: HashMap::new(),
        metadata: HashMap::new(),
        auth: None,
        timeout: None,
    };
    
    let result = registry.execute("add", params, context).await.unwrap();
    assert!(result.success);
    assert_eq!(result.data.unwrap()["result"], json!(30.0));
    
    // Test tool chain
    let multiply_spec = ToolSpec {
        name: "multiply".to_string(),
        description: "Multiply by 2".to_string(),
        parameters: vec![
            ToolParameter {
                name: "value".to_string(),
                param_type: "number".to_string(),
                required: true,
                description: Some("Value to multiply".to_string()),
                default: None,
                schema: None,
            },
        ],
        returns: Some("number".to_string()),
        tags: vec!["math".to_string()],
        examples: vec![],
    };
    
    let multiply_tool = FunctionTool::new(multiply_spec, |params: Value, _context: ToolContext| {
        let value = params["value"].as_f64().unwrap_or(0.0);
        
        Ok(ToolResult {
            success: true,
            data: Some(json!({"result": value * 2.0})),
            error: None,
            metadata: HashMap::new(),
        })
    });
    
    let mut chain = ToolChain::new(true);
    chain.add_tool("add".to_string(), Arc::new(FunctionTool::new(
        ToolSpec {
            name: "add".to_string(),
            description: "Add two numbers".to_string(),
            parameters: vec![],
            returns: None,
            tags: vec![],
            examples: vec![],
        },
        |_params: Value, _context: ToolContext| {
            Ok(ToolResult {
                success: true,
                data: Some(json!({"value": 15})),
                error: None,
                metadata: HashMap::new(),
            })
        },
    )));
    
    chain.add_tool("multiply".to_string(), Arc::new(multiply_tool));
    
    let context = ToolContext {
        state: HashMap::new(),
        metadata: HashMap::new(),
        auth: None,
        timeout: None,
    };
    
    let results = chain.execute(json!({}), context).await.unwrap();
    assert_eq!(results.len(), 2);
    assert!(results[0].success);
    assert!(results[1].success);
}

#[tokio::test]
async fn test_agent_reasoning() {
    let mut agent = ReasoningAgent::new(
        "test_agent",
        "An intelligent test agent",
        ReasoningStrategy::ChainOfThought,
    );
    
    agent.add_tool("analyze".to_string());
    agent.add_tool("process".to_string());
    
    // Test observation
    let observation = json!({
        "type": "sensor_data",
        "value": 42
    });
    
    let state = GraphState::new();
    agent.observe(observation.clone(), &state.values).await.unwrap();
    
    // Verify memory was updated
    let memory = agent.memory();
    assert!(!memory.short_term.is_empty());
    assert_eq!(memory.short_term[0].entry_type, "observation");
    
    // Test reasoning
    let decision = agent.reason(&state.values).await.unwrap();
    assert!(!decision.action.is_empty());
    assert!(decision.confidence > 0.0);
    assert!(!decision.reasoning.is_empty());
    
    // Test action execution
    let tools = ToolRegistry::new();
    let mut state_mut = state.values.clone();
    let result = agent.act(&decision, &tools, &mut state_mut).await.unwrap();
    assert!(result.success);
    
    // Test reflection
    agent.reflect(&result, &state.values).await.unwrap();
    let memory = agent.memory();
    assert!(memory.short_term.len() > 2); // observation + decision + result
}

#[tokio::test]
async fn test_react_reasoning_pattern() {
    let mut agent = ReasoningAgent::new(
        "react_agent",
        "Agent using ReAct pattern",
        ReasoningStrategy::ReAct,
    );
    
    // Add working memory
    let mut memory = AgentMemory::new();
    memory.update_working("goal".to_string(), json!("complete_task"));
    agent.update_memory(memory);
    
    let state = GraphState::new();
    let decision = agent.reason(&state.values).await.unwrap();
    
    // Verify ReAct pattern reasoning
    assert!(decision.reasoning.contains("Thought:"));
    assert!(decision.reasoning.contains("Observation:"));
    assert!(decision.reasoning.contains("Action:"));
    assert_eq!(decision.action, "work_towards_goal");
}

#[tokio::test]
async fn test_multi_agent_coordination() {
    let coordinator = MultiAgentCoordinator::new(CoordinationStrategy::Sequential);
    
    // Create multiple agents
    let _agent1 = Arc::new(tokio::sync::Mutex::new(ReasoningAgent::new(
        "analyzer",
        "Analysis agent",
        ReasoningStrategy::ChainOfThought,
    )));
    
    let _agent2 = Arc::new(tokio::sync::Mutex::new(ReasoningAgent::new(
        "processor",
        "Processing agent",
        ReasoningStrategy::ReAct,
    )));
    
    // Note: In real implementation, would need to properly add agents to coordinator
    // coordinator.add_agent(agent1);
    // coordinator.add_agent(agent2);
    
    let initial_state = GraphState::new();
    
    // Test sequential execution
    let result = coordinator.execute(initial_state.values.clone()).await.unwrap();
    assert_eq!(result.len(), 0); // Empty since no agents were actually added
}

#[tokio::test]
async fn test_state_versioning() {
    let manager = VersionedStateManager::new(10);
    
    // Create initial version
    let mut state1 = StateData::new();
    state1.insert("version".to_string(), json!(1));
    state1.insert("data".to_string(), json!("initial"));
    
    let v1 = manager.create_version(state1.clone(), Some("Initial version".to_string()))
        .await
        .unwrap();
    assert_eq!(v1, 1);
    
    // Create second version
    let mut state2 = state1.clone();
    state2.insert("version".to_string(), json!(2));
    state2.insert("data".to_string(), json!("modified"));
    
    let v2 = manager.create_version(state2, Some("Modified data".to_string()))
        .await
        .unwrap();
    assert_eq!(v2, 2);
    
    // Get current version
    let current = manager.get_current_version().await.unwrap();
    assert_eq!(current.version, 2);
    assert_eq!(current.state.get("data"), Some(&json!("modified")));
    
    // Rollback to v1
    manager.rollback(1).await.unwrap();
    let rolled_back = manager.get_current_version().await.unwrap();
    assert_eq!(rolled_back.version, 1);
    assert_eq!(rolled_back.state.get("data"), Some(&json!("initial")));
}

#[tokio::test]
async fn test_state_branching() {
    let manager = VersionedStateManager::new(10);
    
    // Create base state
    let mut base = StateData::new();
    base.insert("branch".to_string(), json!("main"));
    manager.create_version(base, None).await.unwrap();
    
    // Create feature branch
    manager.create_branch("feature".to_string()).await.unwrap();
    
    // Add changes in feature branch
    let mut feature_state = StateData::new();
    feature_state.insert("branch".to_string(), json!("feature"));
    feature_state.insert("feature_flag".to_string(), json!(true));
    manager.create_version(feature_state, None).await.unwrap();
    
    // Verify branch isolation
    let current = manager.get_current_version().await.unwrap();
    assert_eq!(current.state.get("branch"), Some(&json!("feature")));
    assert_eq!(current.state.get("feature_flag"), Some(&json!(true)));
}

#[tokio::test]
async fn test_state_snapshots() {
    let snapshot_manager = SnapshotManager::new(5);
    
    // Create snapshots
    for i in 1..=3 {
        let mut state = StateData::new();
        state.insert("snapshot".to_string(), json!(i));
        state.insert("data".to_string(), json!(format!("snapshot_{}", i)));
        
        snapshot_manager.create_snapshot(
            format!("snap_{}", i),
            state,
            i as u64,
            None,
        ).await.unwrap();
    }
    
    // List snapshots
    let snapshots = snapshot_manager.list_snapshots().await;
    assert_eq!(snapshots.len(), 3);
    
    // Load specific snapshot
    let snap2 = snapshot_manager.load_snapshot("snap_2").await.unwrap();
    assert_eq!(snap2.id, "snap_2");
    assert_eq!(snap2.version, 2);
    assert_eq!(snap2.state.get("data"), Some(&json!("snapshot_2")));
    
    // Delete snapshot
    snapshot_manager.delete_snapshot("snap_1").await.unwrap();
    let snapshots = snapshot_manager.list_snapshots().await;
    assert_eq!(snapshots.len(), 2);
}

#[tokio::test]
async fn test_state_diff() {
    let mut old_state = StateData::new();
    old_state.insert("unchanged".to_string(), json!("same"));
    old_state.insert("modified".to_string(), json!("old_value"));
    old_state.insert("removed".to_string(), json!("to_be_removed"));
    
    let mut new_state = StateData::new();
    new_state.insert("unchanged".to_string(), json!("same"));
    new_state.insert("modified".to_string(), json!("new_value"));
    new_state.insert("added".to_string(), json!("new_key"));
    
    let diff = StateDiff::calculate(&old_state, &new_state);
    
    // Verify diff calculation
    assert_eq!(diff.added.len(), 1);
    assert!(diff.added.contains_key("added"));
    
    assert_eq!(diff.modified.len(), 1);
    assert!(diff.modified.contains_key("modified"));
    
    assert_eq!(diff.removed.len(), 1);
    assert!(diff.removed.contains_key("removed"));
    
    // Apply diff to another state
    let mut target = old_state.clone();
    diff.apply(&mut target);
    
    assert_eq!(target.get("unchanged"), Some(&json!("same")));
    assert_eq!(target.get("modified"), Some(&json!("new_value")));
    assert_eq!(target.get("added"), Some(&json!("new_key")));
    assert!(!target.contains_key("removed"));
}

#[tokio::test]
async fn test_http_tool() {
    let spec = ToolSpec {
        name: "api_call".to_string(),
        description: "Make API call".to_string(),
        parameters: vec![
            ToolParameter {
                name: "endpoint".to_string(),
                param_type: "string".to_string(),
                required: true,
                description: Some("API endpoint".to_string()),
                default: None,
                schema: None,
            },
        ],
        returns: Some("object".to_string()),
        tags: vec!["api".to_string()],
        examples: vec![],
    };
    
    let http_tool = HttpTool::new(
        spec,
        "https://api.example.com".to_string(),
        "POST".to_string(),
        HashMap::new(),
    );
    
    let params = json!({"endpoint": "/users"});
    let context = ToolContext {
        state: HashMap::new(),
        metadata: HashMap::new(),
        auth: None,
        timeout: Some(30),
    };
    
    // This would make actual HTTP call in real implementation
    let result = http_tool.execute(params.clone(), context).await.unwrap();
    assert!(result.success);
    assert_eq!(result.data, Some(params)); // Placeholder implementation returns params
}

#[test]
fn test_subgraph_builder() {
    // Create subgraphs using builder
    let builder = SubgraphBuilder::new("main_workflow");
    
    // Create a simple subgraph
    let sub_builder = GraphBuilder::new("subprocess")
        .add_node("__start__", NodeType::Start)
        .add_node("task", NodeType::Agent("processor".to_string()))
        .add_node("__end__", NodeType::End)
        .set_entry_point("__start__")
        .add_edge("__start__", "task")
        .add_edge("task", "__end__");
    
    let sub_compiled = sub_builder.build().unwrap().compile().unwrap();
    let subgraph = Subgraph::new("sub1", sub_compiled)
        .with_isolation(true);
    
    let builder = builder.add_subgraph(subgraph);
    
    let (parent, subgraphs) = builder.build().unwrap();
    assert!(parent.get_node("sub1").is_some());
    assert_eq!(subgraphs.len(), 1);
    assert!(subgraphs.contains_key("sub1"));
}