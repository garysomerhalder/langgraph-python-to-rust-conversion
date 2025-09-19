//! Integration tests for User Feedback System
//! RED Phase: Writing failing tests for HIL-004

use langgraph::{
    engine::{ExecutionEngine, UserFeedback, FeedbackManager, FeedbackType, FeedbackHistory, FeedbackRequestStatus},
    graph::GraphBuilder,
    state::StateData,
    Result,
};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Test basic feedback collection
#[tokio::test]
async fn test_feedback_collection() -> Result<()> {
    let manager = FeedbackManager::new();

    // Create feedback
    let feedback = UserFeedback::new(
        "node_123",
        FeedbackType::Approval,
        Some("Looks good to proceed".to_string()),
    );

    // Submit feedback
    let feedback_id = manager.submit_feedback(feedback).await;

    // Retrieve feedback
    let retrieved = manager.get_feedback(&feedback_id).await;
    assert!(retrieved.is_some());

    let fb = retrieved.unwrap();
    assert_eq!(fb.node_id, "node_123");
    assert_eq!(fb.feedback_type, FeedbackType::Approval);
    assert_eq!(fb.comment, Some("Looks good to proceed".to_string()));

    Ok(())
}

/// Test different feedback types
#[tokio::test]
async fn test_feedback_types() -> Result<()> {
    let manager = FeedbackManager::new();

    // Approval feedback
    let approval = UserFeedback::new("node_1", FeedbackType::Approval, None);
    let id1 = manager.submit_feedback(approval).await;

    // Rejection feedback
    let rejection = UserFeedback::new(
        "node_2",
        FeedbackType::Rejection,
        Some("Invalid data".to_string()),
    );
    let id2 = manager.submit_feedback(rejection).await;

    // Modification feedback with updated state
    let mut modified_state = StateData::new();
    modified_state.insert("corrected".to_string(), json!(true));

    let modification = UserFeedback::with_modification(
        "node_3",
        Some("Fixed the issue".to_string()),
        modified_state,
    );
    let id3 = manager.submit_feedback(modification).await;

    // Verify all feedbacks
    assert!(manager.get_feedback(&id1).await.is_some());
    assert!(manager.get_feedback(&id2).await.is_some());

    let mod_feedback = manager.get_feedback(&id3).await.unwrap();
    assert_eq!(mod_feedback.feedback_type, FeedbackType::Modification);
    assert!(mod_feedback.modified_state.is_some());

    Ok(())
}

/// Test feedback history tracking
#[tokio::test]
async fn test_feedback_history() -> Result<()> {
    let manager = FeedbackManager::new();

    // Submit multiple feedbacks
    for i in 0..5 {
        let feedback = UserFeedback::new(
            &format!("node_{}", i),
            FeedbackType::Approval,
            Some(format!("Feedback {}", i)),
        );
        manager.submit_feedback(feedback).await;
    }

    // Get history
    let history = manager.get_history(None, None).await;
    assert_eq!(history.len(), 5);

    // Get history for specific node
    let node_history = manager.get_history_for_node("node_2").await;
    assert_eq!(node_history.len(), 1);

    // Get history by type
    let approvals = manager.get_history_by_type(FeedbackType::Approval).await;
    assert_eq!(approvals.len(), 5);

    Ok(())
}

/// Test feedback with state modifications
#[tokio::test]
async fn test_feedback_state_modification() -> Result<()> {
    let manager = FeedbackManager::new();

    // Original state
    let mut original_state = StateData::new();
    original_state.insert("value".to_string(), json!(100));
    original_state.insert("status".to_string(), json!("pending"));

    // Modified state
    let mut modified_state = StateData::new();
    modified_state.insert("value".to_string(), json!(200));
    modified_state.insert("status".to_string(), json!("approved"));
    modified_state.insert("reviewed_by".to_string(), json!("user123"));

    // Create modification feedback
    let feedback = UserFeedback::with_modification(
        "approval_node",
        Some("Increased value and approved".to_string()),
        modified_state.clone(),
    );

    let feedback_id = manager.submit_feedback(feedback).await;

    // Apply modification
    let result = manager.apply_modification(&feedback_id, &mut original_state).await?;

    assert!(result);
    assert_eq!(original_state.get("value"), Some(&json!(200)));
    assert_eq!(original_state.get("status"), Some(&json!("approved")));
    assert_eq!(original_state.get("reviewed_by"), Some(&json!("user123")));

    Ok(())
}

/// Test feedback aggregation
#[tokio::test]
async fn test_feedback_aggregation() -> Result<()> {
    let manager = FeedbackManager::new();

    // Submit various feedbacks
    for _ in 0..3 {
        manager.submit_feedback(
            UserFeedback::new("node_a", FeedbackType::Approval, None)
        ).await;
    }

    for _ in 0..2 {
        manager.submit_feedback(
            UserFeedback::new("node_b", FeedbackType::Rejection, None)
        ).await;
    }

    manager.submit_feedback(
        UserFeedback::with_modification(
            "node_c",
            None,
            StateData::new(),
        )
    ).await;

    // Get aggregated stats
    let stats = manager.get_feedback_stats().await;

    assert_eq!(stats.total_count, 6);
    assert_eq!(stats.approval_count, 3);
    assert_eq!(stats.rejection_count, 2);
    assert_eq!(stats.modification_count, 1);

    // Get node-specific stats
    let node_stats = manager.get_node_feedback_stats("node_a").await;
    assert_eq!(node_stats.total_count, 3);
    assert_eq!(node_stats.approval_count, 3);

    Ok(())
}

/// Test feedback timeout handling
#[tokio::test]
async fn test_feedback_timeout() -> Result<()> {
    let manager = FeedbackManager::new();

    // Request feedback with timeout
    let request = manager.request_feedback(
        "timeout_node",
        "Please approve this action",
        std::time::Duration::from_millis(100),
    ).await;

    // Wait for timeout
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    // Check if timeout was recorded
    let status = manager.get_request_status(&request.id).await;
    assert_eq!(status, FeedbackRequestStatus::TimedOut);

    // Verify timeout feedback was auto-generated
    let history = manager.get_history_for_node("timeout_node").await;
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].feedback_type, FeedbackType::Timeout);

    Ok(())
}

/// Test concurrent feedback submission
#[tokio::test]
async fn test_concurrent_feedback() -> Result<()> {
    let manager = Arc::new(FeedbackManager::new());

    let mut handles = Vec::new();

    // Submit feedbacks concurrently
    for i in 0..10 {
        let mgr = manager.clone();
        let handle = tokio::spawn(async move {
            let feedback = UserFeedback::new(
                &format!("concurrent_{}", i),
                if i % 2 == 0 { FeedbackType::Approval } else { FeedbackType::Rejection },
                Some(format!("Concurrent feedback {}", i)),
            );
            mgr.submit_feedback(feedback).await
        });
        handles.push(handle);
    }

    // Collect all feedback IDs
    let mut feedback_ids = Vec::new();
    for handle in handles {
        feedback_ids.push(handle.await.unwrap());
    }

    // Verify all feedbacks were recorded
    assert_eq!(feedback_ids.len(), 10);

    let history = manager.get_history(None, None).await;
    assert_eq!(history.len(), 10);

    Ok(())
}

/// Test feedback persistence
#[tokio::test]
async fn test_feedback_persistence() -> Result<()> {
    let manager = FeedbackManager::new();

    // Submit feedback
    let feedback = UserFeedback::new(
        "persist_node",
        FeedbackType::Approval,
        Some("Persisted feedback".to_string()),
    );

    let feedback_id = manager.submit_feedback(feedback).await;

    // Export feedback
    let export = manager.export_feedback().await?;
    assert!(export.iter().any(|f| f.id == feedback_id));

    // Clear and import
    manager.clear_all_feedback().await;
    assert_eq!(manager.get_history(None, None).await.len(), 0);

    manager.import_feedback_by_ids(vec![feedback_id]).await?;

    // Verify imported feedback
    let imported = manager.get_feedback(&feedback_id).await;
    assert!(imported.is_some());
    assert_eq!(imported.unwrap().comment, Some("Persisted feedback".to_string()));

    Ok(())
}

// Test feedback integration with execution engine is commented out
// as it requires ExecutionEngine methods not yet implemented.
// This will be implemented in a future task when ExecutionEngine
// gains set_feedback_manager and execute_with_feedback methods.
/*
/// Test feedback integration with execution engine
#[tokio::test]
#[ignore] // TODO: Implement after ExecutionEngine integration
async fn test_execution_integration() -> Result<()> {
    let mut engine = ExecutionEngine::new();
    let feedback_manager = FeedbackManager::new();
    engine.set_feedback_manager(feedback_manager.clone());

    // Create graph with feedback points
    let graph = GraphBuilder::new("feedback_test")
        .add_node("start", langgraph::graph::NodeType::Start)
        .add_node("review", langgraph::graph::NodeType::HumanFeedback)
        .add_node("process", langgraph::graph::NodeType::Agent("processor".to_string()))
        .add_node("end", langgraph::graph::NodeType::End)
        .set_entry_point("start")
        .add_edge("start", "review")
        .add_edge("review", "process")
        .add_edge("process", "end")
        .build()?
        .compile()?;

    let mut input = StateData::new();
    input.insert("data".to_string(), json!("test"));

    // Execute with feedback handling
    let result = engine.execute_with_feedback(graph, input, |request| async {
        // Auto-approve for testing
        UserFeedback::new(&request.node_id, FeedbackType::Approval, None)
    }).await?;

    // Verify feedback was collected
    let history = feedback_manager.get_history(None, None).await;
    assert!(!history.is_empty());

    Ok(())
}
*/

/// Test feedback filtering and search
#[tokio::test]
async fn test_feedback_search() -> Result<()> {
    let manager = FeedbackManager::new();

    // Submit various feedbacks
    for i in 0..10 {
        let feedback = UserFeedback::new(
            &format!("search_node_{}", i % 3),
            if i < 5 { FeedbackType::Approval } else { FeedbackType::Rejection },
            Some(format!("Comment {}", i)),
        );
        manager.submit_feedback(feedback).await;
    }

    // Search by node pattern
    let results = manager.search_feedback("search_node_1").await;
    assert!(results.len() > 0);

    // Filter by time range
    let now = chrono::Utc::now();
    let results = manager.get_history(
        Some(now - chrono::Duration::hours(1)),
        Some(now),
    ).await;
    assert_eq!(results.len(), 10);

    // Complex filter
    let results = manager.search_with_filter(|f| {
        f.node_id.contains("node_0") && f.feedback_type == FeedbackType::Approval
    }).await;
    assert!(results.len() > 0);

    Ok(())
}