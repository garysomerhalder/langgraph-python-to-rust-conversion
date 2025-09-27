//! Multi-agent collaboration example
//!
//! This example demonstrates a sophisticated 9-agent system working together
//! to develop a complete feature from research to deployment.

use langgraph::{
    agents::{AgentMessage, AgentRole, MessageType, MultiAgentSystem},
    Result,
};
use serde_json::json;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("langgraph=debug")
        .init();

    println!("🤖 Multi-Agent System Demonstration");
    println!("===================================");
    println!("Demonstrating 9 specialized agents collaborating on complex workflows\n");

    // Initialize the multi-agent system
    let system = MultiAgentSystem::new();

    // Example 1: Feature Development Workflow
    println!("📋 Workflow 1: Full-Stack Feature Development");
    println!("----------------------------------------------");

    let feature_workflow = json!({
        "type": "feature_development",
        "name": "Real-Time Chat System",
        "requirements": [
            "WebSocket support for real-time messaging",
            "End-to-end encryption",
            "Message persistence with PostgreSQL",
            "Rate limiting to prevent spam",
            "React frontend with TypeScript",
            "Horizontal scaling support"
        ],
        "constraints": {
            "timeline": "2 weeks",
            "team_size": 5,
            "technologies": ["Rust", "React", "PostgreSQL", "Redis"]
        }
    });

    let start = Instant::now();
    let result = system.execute_workflow(feature_workflow).await?;
    let elapsed = start.elapsed();

    println!("✅ Workflow completed in {:?}", elapsed);
    println!("📊 Workflow ID: {}", result["workflow_id"]);

    // Display results from each agent
    if let Some(stages) = result["stages"].as_array() {
        for (i, stage) in stages.iter().enumerate() {
            println!("\n  Stage {}: {}", i + 1, stage["agent"]);
            if let Some(result) = stage.get("result") {
                match stage["agent"].as_str().unwrap_or("") {
                    "Research" => {
                        println!("    📚 Findings: {:?}", result["findings"]);
                        println!("    💡 Recommendations: {:?}", result["recommendations"]);
                        println!("    🎯 Confidence: {}", result["confidence"]);
                    }
                    "Architect" => {
                        println!("    🏗️ Architecture: {}", result["architecture"]);
                        println!("    🧩 Components: {:?}", result["components"]);
                        println!("    📈 Scalability: {}", result["scalability"]);
                    }
                    "Code" => {
                        println!("    💻 Language: {}", result["language"]);
                        println!("    📦 Modules: {:?}", result["modules"]);
                        println!("    ✅ Status: {}", result["status"]);
                        println!("    📊 Coverage: {}", result["coverage"]);
                    }
                    "QA" => {
                        println!("    🧪 Tests Run: {}", result["tests_run"]);
                        println!("    ✅ Passed: {}", result["passed"]);
                        println!("    ❌ Failed: {}", result["failed"]);
                        println!("    📊 Coverage: {}", result["coverage"]);
                    }
                    "Security" => {
                        println!("    🔒 Compliance: {}", result["compliance"]);
                        println!("    ⚠️ Risk Level: {}", result["risk_level"]);
                        println!("    📋 Recommendations: {:?}", result["recommendations"]);
                    }
                    "DevOps" => {
                        println!("    🚀 Environment: {}", result["environment"]);
                        println!("    📈 Status: {}", result["status"]);
                        println!("    💚 Health: {}", result["health"]);
                    }
                    _ => {
                        println!("    📋 Result: {:?}", result);
                    }
                }
            }
        }
    }

    // Example 2: Data Analysis Workflow
    println!("\n\n📋 Workflow 2: Data Analysis Pipeline");
    println!("----------------------------------------");

    let data_workflow = json!({
        "type": "data_analysis",
        "name": "Customer Behavior Analysis",
        "dataset": {
            "source": "user_interactions",
            "size": "500GB",
            "format": "parquet",
            "timeframe": "last_6_months"
        },
        "objectives": [
            "Identify user segments",
            "Predict churn probability",
            "Recommend personalization strategies"
        ]
    });

    let start = Instant::now();
    let result = system.execute_workflow(data_workflow).await?;
    let elapsed = start.elapsed();

    println!("✅ Analysis completed in {:?}", elapsed);
    display_workflow_summary(&result);

    // Example 3: Security Audit Workflow
    println!("\n📋 Workflow 3: Security Audit");
    println!("--------------------------------");

    let security_workflow = json!({
        "type": "security_audit",
        "name": "API Security Review",
        "target": {
            "service": "payment_gateway",
            "endpoints": 25,
            "authentication": "OAuth2 + API Keys",
            "compliance_requirements": ["PCI-DSS", "SOC2", "GDPR"]
        }
    });

    let start = Instant::now();
    let result = system.execute_workflow(security_workflow).await?;
    let elapsed = start.elapsed();

    println!("✅ Audit completed in {:?}", elapsed);
    display_workflow_summary(&result);

    // Example 4: Parallel Agent Execution
    println!("\n📋 Example 4: Parallel Agent Execution");
    println!("----------------------------------------");
    println!("Running 4 agents in parallel on independent tasks...\n");

    let parallel_tasks = vec![
        (
            AgentRole::Research,
            json!({
                "topic": "Latest trends in distributed systems"
            }),
        ),
        (
            AgentRole::Security,
            json!({
                "name": "vulnerability_scan",
                "target": "web_application"
            }),
        ),
        (
            AgentRole::Data,
            json!({
                "name": "performance_metrics",
                "period": "last_30_days"
            }),
        ),
        (
            AgentRole::Product,
            json!({
                "features": ["search", "filtering", "sorting", "export"],
                "user_feedback": "positive"
            }),
        ),
    ];

    let start = Instant::now();
    let results = system.parallel_execution(parallel_tasks).await?;
    let elapsed = start.elapsed();

    println!("✅ Parallel execution completed in {:?}", elapsed);
    println!("📊 Results from {} agents:", results.len());

    for (i, result) in results.iter().enumerate() {
        println!(
            "  Agent {}: {:?}",
            i + 1,
            result
                .get("topic")
                .or(result.get("name"))
                .or(result.get("features"))
                .unwrap_or(&json!("completed"))
        );
    }

    // Example 5: Inter-Agent Communication
    println!("\n📋 Example 5: Direct Inter-Agent Communication");
    println!("------------------------------------------------");

    // Product agent requests research from Research agent
    let research_request = AgentMessage {
        from: AgentRole::Product,
        to: AgentRole::Research,
        message_type: MessageType::Request,
        payload: json!({
            "topic": "User preferences for real-time features",
            "depth": "comprehensive"
        }),
        priority: 1,
        correlation_id: "prod-research-001".to_string(),
    };

    println!("📤 Product Agent → Research Agent: Research request");
    let response = system.send_message(research_request).await?;
    println!("📥 Research Agent → Product Agent: Research complete");
    println!("   Findings: {:?}", response.payload["findings"]);

    // Architect requests security review from Security agent
    let security_review = AgentMessage {
        from: AgentRole::Architect,
        to: AgentRole::Security,
        message_type: MessageType::Request,
        payload: json!({
            "name": "microservices_architecture",
            "components": ["api_gateway", "auth_service", "data_service"],
            "review_type": "design_review"
        }),
        priority: 2,
        correlation_id: "arch-sec-001".to_string(),
    };

    println!("\n📤 Architect Agent → Security Agent: Security review request");
    let response = system.send_message(security_review).await?;
    println!("📥 Security Agent → Architect Agent: Review complete");
    println!("   Compliance: {}", response.payload["compliance"]);
    println!("   Risk Level: {}", response.payload["risk_level"]);

    // Example 6: Product Planning Workflow
    println!("\n📋 Workflow 6: Quarterly Product Planning");
    println!("-------------------------------------------");

    let planning_workflow = json!({
        "type": "product_planning",
        "name": "Q2 2024 Roadmap",
        "features": [
            "AI-powered search",
            "Mobile app v2",
            "Enterprise SSO",
            "Advanced analytics dashboard",
            "API rate limiting",
            "Multi-language support"
        ],
        "constraints": {
            "timeline": "3 months",
            "team_size": 15,
            "budget": "1.5M",
            "tech_debt_allocation": "20%"
        },
        "market_data": {
            "competitor_features": ["AI search", "mobile"],
            "user_requests": ["SSO", "analytics", "API"],
            "retention_impact": {
                "AI_search": 0.15,
                "mobile": 0.25,
                "SSO": 0.10,
                "analytics": 0.20
            }
        }
    });

    let start = Instant::now();
    let result = system.execute_workflow(planning_workflow).await?;
    let elapsed = start.elapsed();

    println!("✅ Planning completed in {:?}", elapsed);

    if let Some(final_output) = result.get("final_output") {
        if let Some(high_priority) = final_output.get("high_priority") {
            println!("\n🎯 High Priority Features:");
            for (i, feature) in high_priority
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .enumerate()
            {
                println!("  {}. {}", i + 1, feature);
            }
        }

        if let Some(rationale) = final_output.get("rationale") {
            println!("\n📝 Prioritization Rationale: {}", rationale);
        }
    }

    // Summary
    println!("\n========================================");
    println!("🎉 Multi-Agent System Demonstration Complete!");
    println!("========================================");
    println!("\n📊 System Capabilities Demonstrated:");
    println!("  ✅ 9 specialized agents working in coordination");
    println!("  ✅ Complex workflow orchestration");
    println!("  ✅ Parallel agent execution");
    println!("  ✅ Inter-agent communication");
    println!("  ✅ Adaptive workflow routing");
    println!("  ✅ Memory persistence across interactions");
    println!("\n💡 Use Cases:");
    println!("  • Full-stack feature development");
    println!("  • Data analysis and insights");
    println!("  • Security auditing and compliance");
    println!("  • Product planning and prioritization");
    println!("  • DevOps automation and deployment");

    Ok(())
}

fn display_workflow_summary(result: &serde_json::Value) {
    println!("  Workflow ID: {}", result["workflow_id"]);
    println!("  Status: {}", result["status"]);

    if let Some(stages) = result["stages"].as_array() {
        println!("  Stages Completed: {}", stages.len());

        let agents: Vec<String> = stages
            .iter()
            .filter_map(|s| s["agent"].as_str())
            .map(|s| s.to_string())
            .collect();

        println!("  Agents Involved: {}", agents.join(" → "));
    }
}
