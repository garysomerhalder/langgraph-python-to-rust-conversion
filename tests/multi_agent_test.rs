//! Integration tests for multi-agent system

use langgraph::{
    agents::{
        AgentMessage, AgentRole, ArchitectAgent, CodeAgent, DataAgent, DevOpsAgent, MessageType,
        MultiAgentSystem, OrchestratorAgent, ProductAgent, QAAgent, ResearchAgent, SecurityAgent,
        SpecializedAgent,
    },
    Result,
};
use serde_json::json;

#[tokio::test]
async fn test_multi_agent_system_initialization() {
    let system = MultiAgentSystem::new();

    // Test all agents are properly initialized
    assert!(system.get_agent(AgentRole::Research).is_some());
    assert!(system.get_agent(AgentRole::Architect).is_some());
    assert!(system.get_agent(AgentRole::Code).is_some());
    assert!(system.get_agent(AgentRole::QA).is_some());
    assert!(system.get_agent(AgentRole::DevOps).is_some());
    assert!(system.get_agent(AgentRole::Security).is_some());
    assert!(system.get_agent(AgentRole::Data).is_some());
    assert!(system.get_agent(AgentRole::Product).is_some());
}

#[tokio::test]
async fn test_agent_direct_communication() {
    let system = MultiAgentSystem::new();

    // Test direct message passing between agents
    let message = AgentMessage {
        from: AgentRole::Orchestrator,
        to: AgentRole::Research,
        message_type: MessageType::Request,
        payload: json!("rust async programming"),
        priority: 2,
        correlation_id: "test-001".to_string(),
    };

    let response = system.send_message(message).await.unwrap();

    assert_eq!(response.from, AgentRole::Research);
    assert_eq!(response.to, AgentRole::Orchestrator);
    assert_eq!(response.message_type, MessageType::Response);
    assert!(response.payload["findings"].is_array());
}

#[tokio::test]
async fn test_feature_development_workflow() {
    let system = MultiAgentSystem::new();

    let workflow = json!({
        "type": "feature_development",
        "name": "user_authentication",
        "requirements": [
            "OAuth2 support",
            "JWT tokens",
            "Rate limiting",
            "Secure password hashing"
        ]
    });

    let result = system.execute_workflow(workflow).await.unwrap();

    // Verify workflow execution
    assert_eq!(result["status"], "completed");
    assert!(result["workflow_id"].is_string());
    assert!(result["stages"].is_array());

    // Verify all required agents participated
    let stages = result["stages"].as_array().unwrap();
    assert!(stages.len() >= 6); // Research, Architect, Code, QA, Security, DevOps

    // Verify final output contains expected elements
    assert!(result["final_output"]["status"].is_string());
}

#[tokio::test]
async fn test_data_analysis_workflow() {
    let system = MultiAgentSystem::new();

    let workflow = json!({
        "type": "data_analysis",
        "name": "user_behavior_analysis",
        "dataset": {
            "source": "clickstream_data",
            "size": "10GB",
            "format": "parquet"
        }
    });

    let result = system.execute_workflow(workflow).await.unwrap();

    assert_eq!(result["status"], "completed");

    // Verify data analysis agents were involved
    let stages = result["stages"].as_array().unwrap();
    let agent_roles: Vec<String> = stages
        .iter()
        .map(|s| s["agent"].as_str().unwrap_or("").to_string())
        .collect();

    assert!(agent_roles.contains(&"Data".to_string()));
    assert!(agent_roles.contains(&"Research".to_string()));
    assert!(agent_roles.contains(&"Product".to_string()));
}

#[tokio::test]
async fn test_security_audit_workflow() {
    let system = MultiAgentSystem::new();

    let workflow = json!({
        "type": "security_audit",
        "name": "api_security_review",
        "target": {
            "service": "payment_api",
            "endpoints": 15,
            "authentication": "OAuth2"
        }
    });

    let result = system.execute_workflow(workflow).await.unwrap();

    assert_eq!(result["status"], "completed");

    // Verify security-focused agents participated
    let stages = result["stages"].as_array().unwrap();
    let agent_roles: Vec<String> = stages
        .iter()
        .map(|s| s["agent"].as_str().unwrap_or("").to_string())
        .collect();

    assert!(agent_roles.contains(&"Security".to_string()));
    assert!(agent_roles.contains(&"Code".to_string()));
    assert!(agent_roles.contains(&"DevOps".to_string()));
}

#[tokio::test]
async fn test_parallel_agent_execution() {
    let system = MultiAgentSystem::new();

    // Test parallel execution of multiple independent tasks
    let tasks = vec![
        (
            AgentRole::Research,
            json!({"topic": "microservices architecture"}),
        ),
        (AgentRole::Security, json!({"name": "vulnerability_scan"})),
        (AgentRole::Data, json!({"name": "performance_metrics"})),
        (
            AgentRole::Product,
            json!({"features": ["feature_a", "feature_b"]}),
        ),
    ];

    let results = system.parallel_execution(tasks).await.unwrap();

    // Verify all tasks completed
    assert_eq!(results.len(), 4);

    // Verify each result has expected structure
    for result in &results {
        assert!(result.is_object());
    }
}

#[tokio::test]
async fn test_agent_collaboration() {
    let system = MultiAgentSystem::new();

    // Test complex multi-agent collaboration
    let workflow = json!({
        "type": "feature_development",
        "name": "real_time_analytics",
        "complexity": "high",
        "requirements": {
            "performance": "sub-100ms latency",
            "scale": "1M requests/sec",
            "reliability": "99.99% uptime"
        }
    });

    let result = system.execute_workflow(workflow).await.unwrap();

    // Verify comprehensive collaboration
    let stages = result["stages"].as_array().unwrap();

    // Research phase
    let research_stage = &stages[0];
    assert!(research_stage["result"]["findings"].is_array());
    assert!(research_stage["result"]["confidence"].as_f64().unwrap() > 0.0);

    // Architecture phase
    let architect_stage = &stages[1];
    assert!(architect_stage["result"]["architecture"].is_string());
    assert!(architect_stage["result"]["components"].is_array());

    // Implementation phase
    let code_stage = &stages[2];
    assert!(code_stage["result"]["status"].as_str() == Some("implemented"));

    // Quality assurance phase
    let qa_stage = &stages[3];
    assert_eq!(qa_stage["result"]["tests_run"].as_i64(), Some(142));
    assert_eq!(qa_stage["result"]["passed"].as_i64(), Some(142));

    // Security review
    let security_stage = &stages[4];
    assert!(security_stage["result"]["compliance"].is_string());
    assert_eq!(security_stage["result"]["risk_level"], "low");

    // Deployment phase
    let devops_stage = &stages[5];
    assert_eq!(devops_stage["result"]["status"], "deployed");
    assert_eq!(devops_stage["result"]["health"], "healthy");
}

#[tokio::test]
async fn test_orchestrator_agent_delegation() {
    let system = MultiAgentSystem::new();

    // Test orchestrator's ability to delegate tasks
    let delegation_message = AgentMessage {
        from: AgentRole::Product,
        to: AgentRole::Orchestrator,
        message_type: MessageType::Delegation,
        payload: json!({
            "target": "Research",
            "task": "competitive_analysis"
        }),
        priority: 1,
        correlation_id: "delegate-001".to_string(),
    };

    // Send to orchestrator (which should delegate to Research)
    if let Some(orchestrator) = system.get_agent(AgentRole::Orchestrator) {
        let response = orchestrator
            .process_message(delegation_message)
            .await
            .unwrap();
        assert_eq!(response.from, AgentRole::Orchestrator);
        assert!(response.payload["findings"].is_array()); // Research agent's response
    }
}

#[tokio::test]
async fn test_product_planning_workflow() {
    let system = MultiAgentSystem::new();

    let workflow = json!({
        "type": "product_planning",
        "name": "q1_roadmap",
        "features": [
            "advanced_search",
            "real_time_collaboration",
            "mobile_app",
            "api_v2"
        ],
        "constraints": {
            "timeline": "3_months",
            "team_size": 10,
            "budget": "500k"
        }
    });

    let result = system.execute_workflow(workflow).await.unwrap();

    assert_eq!(result["status"], "completed");

    // Verify product planning agents were involved
    let stages = result["stages"].as_array().unwrap();
    let agent_roles: Vec<String> = stages
        .iter()
        .map(|s| s["agent"].as_str().unwrap_or("").to_string())
        .collect();

    assert!(agent_roles.contains(&"Product".to_string()));
    assert!(agent_roles.contains(&"Research".to_string()));
    assert!(agent_roles.contains(&"Architect".to_string()));

    // Verify prioritization was done
    let final_output = &result["final_output"];
    if let Some(priorities) = final_output.get("high_priority") {
        assert!(priorities.is_array());
    }
}

#[tokio::test]
async fn test_agent_memory_persistence() {
    let research_agent = ResearchAgent::new();

    // Test that agents maintain memory across interactions
    let context = langgraph::agents::multi_agent::AgentContext::new("test");
    let input1 = json!({"topic": "rust_safety"});
    let result1 = research_agent.execute_task(&context, input1).await.unwrap();

    // Second interaction should have access to previous knowledge
    let input2 = json!({"topic": "rust_safety"}); // Same topic
    let result2 = research_agent.execute_task(&context, input2).await.unwrap();

    // Knowledge base should have retained information
    assert_eq!(result1["topic"], result2["topic"]);
    assert_eq!(result1["confidence"], result2["confidence"]);
}

#[tokio::test]
async fn test_error_handling_in_workflow() {
    let system = MultiAgentSystem::new();

    // Test with invalid/unknown workflow type
    let invalid_workflow = json!({
        "type": "unknown_workflow_type",
        "name": "test"
    });

    // Should fall back to default workflow
    let result = system.execute_workflow(invalid_workflow).await.unwrap();
    assert_eq!(result["status"], "completed");

    // Verify it used the default agent sequence
    let stages = result["stages"].as_array().unwrap();
    assert!(stages.len() >= 3); // Default: Research, Architect, Code
}

#[tokio::test]
async fn test_agent_capability_matching() {
    // Test that agents have appropriate capabilities
    let research = ResearchAgent::new();
    let capabilities = research.capabilities();

    assert!(capabilities
        .skills
        .contains(&"information_gathering".to_string()));
    assert!(capabilities
        .skills
        .contains(&"technology_evaluation".to_string()));
    assert!(capabilities.complexity_level > 0);
    assert!(!capabilities.tools.is_empty());

    // Test architect capabilities
    let architect = ArchitectAgent::new();
    let arch_capabilities = architect.capabilities();

    assert!(arch_capabilities
        .skills
        .contains(&"system_design".to_string()));
    assert!(arch_capabilities
        .skills
        .contains(&"scalability_planning".to_string()));
    assert!(arch_capabilities.integrations.contains(&AgentRole::Code));
}

#[tokio::test]
async fn test_multi_agent_stress_test() {
    let system = MultiAgentSystem::new();

    // Run multiple workflows concurrently
    let mut handles = Vec::new();

    for i in 0..10 {
        let system_clone = MultiAgentSystem::new();
        let handle = tokio::spawn(async move {
            let workflow = json!({
                "type": if i % 2 == 0 { "feature_development" } else { "data_analysis" },
                "name": format!("workflow_{}", i),
                "id": i
            });

            system_clone.execute_workflow(workflow).await
        });
        handles.push(handle);
    }

    // Collect all results
    let mut all_completed = true;
    for handle in handles {
        if let Ok(Ok(result)) = handle.await {
            if result["status"] != "completed" {
                all_completed = false;
            }
        } else {
            all_completed = false;
        }
    }

    assert!(all_completed);
}
