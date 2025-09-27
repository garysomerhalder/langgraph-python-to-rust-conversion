// Integration tests for PostgreSQL Checkpointer
// Following Integration-First methodology - uses real PostgreSQL database

use langgraph::checkpoint::{Checkpointer, PostgresCheckpointer, PostgresConfig};
use langgraph::state::GraphState;
use serde_json::json;
use std::env;

/// Get PostgreSQL connection URL from environment or use default
fn get_postgres_url() -> String {
    env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgresql://postgres:postgres@localhost:5432/langgraph_test".to_string()
    })
}

#[tokio::test]
async fn test_postgres_checkpointer_save_and_load() {
    // Real PostgreSQL connection required
    let config = PostgresConfig {
        database_url: get_postgres_url(),
        max_connections: 5,
        min_connections: 1,
        max_lifetime_secs: 3600,
        idle_timeout_secs: 600,
        table_prefix: "test_".to_string(),
        auto_cleanup: true,
        cleanup_interval_secs: 3600,
        retention_days: 7,
    };

    let checkpointer = PostgresCheckpointer::new(config)
        .await
        .expect("Failed to create PostgreSQL checkpointer");

    // Initialize schema
    checkpointer
        .initialize_schema()
        .await
        .expect("Failed to initialize database schema");

    // Create test state
    let mut state = GraphState::new();
    state.set("test_key", json!("test_value"));
    state.set("counter", json!(42));

    let thread_id = "test_thread_001";

    // Save checkpoint
    let checkpoint_id = checkpointer
        .save_checkpoint(thread_id, &state)
        .await
        .expect("Failed to save checkpoint");

    assert!(!checkpoint_id.is_empty());

    // Load checkpoint
    let loaded_state = checkpointer
        .load_checkpoint(thread_id, Some(&checkpoint_id))
        .await
        .expect("Failed to load checkpoint")
        .expect("Checkpoint not found");

    // Verify state
    assert_eq!(loaded_state.get("test_key"), Some(&json!("test_value")));
    assert_eq!(loaded_state.get("counter"), Some(&json!(42)));
}

#[tokio::test]
async fn test_postgres_checkpointer_list_checkpoints() {
    let config = PostgresConfig {
        database_url: get_postgres_url(),
        max_connections: 5,
        min_connections: 1,
        max_lifetime_secs: 3600,
        idle_timeout_secs: 600,
        table_prefix: "test_".to_string(),
        auto_cleanup: false,
        cleanup_interval_secs: 3600,
        retention_days: 7,
    };

    let checkpointer = PostgresCheckpointer::new(config)
        .await
        .expect("Failed to create PostgreSQL checkpointer");

    checkpointer
        .initialize_schema()
        .await
        .expect("Failed to initialize database schema");

    let thread_id = "test_thread_list";

    // Create multiple checkpoints
    for i in 0..3 {
        let mut state = GraphState::new();
        state.set("iteration", json!(i));

        checkpointer
            .save_checkpoint(thread_id, &state)
            .await
            .expect("Failed to save checkpoint");

        // Small delay to ensure different timestamps
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }

    // List checkpoints
    let checkpoints = checkpointer
        .list_checkpoints(thread_id, Some(10))
        .await
        .expect("Failed to list checkpoints");

    assert_eq!(checkpoints.len(), 3);

    // Verify ordering (newest first)
    for i in 0..checkpoints.len() - 1 {
        assert!(checkpoints[i].created_at >= checkpoints[i + 1].created_at);
    }
}

#[tokio::test]
async fn test_postgres_checkpointer_delete_checkpoint() {
    let config = PostgresConfig {
        database_url: get_postgres_url(),
        max_connections: 5,
        min_connections: 1,
        max_lifetime_secs: 3600,
        idle_timeout_secs: 600,
        table_prefix: "test_".to_string(),
        auto_cleanup: false,
        cleanup_interval_secs: 3600,
        retention_days: 7,
    };

    let checkpointer = PostgresCheckpointer::new(config)
        .await
        .expect("Failed to create PostgreSQL checkpointer");

    checkpointer
        .initialize_schema()
        .await
        .expect("Failed to initialize database schema");

    let thread_id = "test_thread_delete";
    let mut state = GraphState::new();
    state.set("data", json!("to_be_deleted"));

    // Save checkpoint
    let checkpoint_id = checkpointer
        .save_checkpoint(thread_id, &state)
        .await
        .expect("Failed to save checkpoint");

    // Verify it exists
    let loaded = checkpointer
        .load_checkpoint(thread_id, Some(&checkpoint_id))
        .await
        .expect("Failed to load checkpoint");
    assert!(loaded.is_some());

    // Delete checkpoint
    checkpointer
        .delete_checkpoint(thread_id, &checkpoint_id)
        .await
        .expect("Failed to delete checkpoint");

    // Verify it's gone
    let loaded_after_delete = checkpointer
        .load_checkpoint(thread_id, Some(&checkpoint_id))
        .await
        .expect("Failed to load checkpoint");
    assert!(loaded_after_delete.is_none());
}

#[tokio::test]
async fn test_postgres_checkpointer_auto_cleanup() {
    let config = PostgresConfig {
        database_url: get_postgres_url(),
        max_connections: 5,
        min_connections: 1,
        max_lifetime_secs: 3600,
        idle_timeout_secs: 600,
        table_prefix: "test_".to_string(),
        auto_cleanup: true,
        cleanup_interval_secs: 1, // Fast cleanup for testing
        retention_days: 0,        // Delete immediately for testing
    };

    let checkpointer = PostgresCheckpointer::new(config)
        .await
        .expect("Failed to create PostgreSQL checkpointer");

    checkpointer
        .initialize_schema()
        .await
        .expect("Failed to initialize database schema");

    let thread_id = "test_thread_cleanup";
    let mut state = GraphState::new();
    state.set("old_data", json!("should_be_cleaned"));

    // Save checkpoint
    let checkpoint_id = checkpointer
        .save_checkpoint(thread_id, &state)
        .await
        .expect("Failed to save checkpoint");

    // Manually trigger cleanup (in production this would be automatic)
    checkpointer
        .cleanup_old_checkpoints()
        .await
        .expect("Failed to run cleanup");

    // Verify checkpoint is cleaned up
    let checkpoints = checkpointer
        .list_checkpoints(thread_id, None)
        .await
        .expect("Failed to list checkpoints");

    // With 0 retention days, old checkpoints should be deleted
    assert_eq!(checkpoints.len(), 0);
}

#[tokio::test]
async fn test_postgres_checkpointer_transaction_rollback() {
    let config = PostgresConfig {
        database_url: get_postgres_url(),
        max_connections: 5,
        min_connections: 1,
        max_lifetime_secs: 3600,
        idle_timeout_secs: 600,
        table_prefix: "test_".to_string(),
        auto_cleanup: false,
        cleanup_interval_secs: 3600,
        retention_days: 7,
    };

    let checkpointer = PostgresCheckpointer::new(config)
        .await
        .expect("Failed to create PostgreSQL checkpointer");

    checkpointer
        .initialize_schema()
        .await
        .expect("Failed to initialize database schema");

    // Test transaction rollback on error
    let thread_id = "test_thread_transaction";

    // This should simulate a failed transaction
    // Implementation would handle this internally
    let result = checkpointer
        .save_checkpoint_transactional(thread_id, None)
        .await;

    // Transaction should rollback on error
    assert!(result.is_err());

    // Verify no partial data was saved
    let checkpoints = checkpointer
        .list_checkpoints(thread_id, None)
        .await
        .expect("Failed to list checkpoints");
    assert_eq!(checkpoints.len(), 0);
}
