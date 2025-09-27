// Integration tests for Redis Checkpointer
// Following Integration-First methodology - uses real Redis instance

use langgraph::checkpoint::{Checkpointer, RedisCheckpointer, RedisConfig};
use langgraph::state::GraphState;
use serde_json::json;
use std::env;
use std::time::Duration;

/// Get Redis connection URL from environment or use default
fn get_redis_url() -> String {
    env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379/0".to_string())
}

#[tokio::test]
async fn test_redis_checkpointer_save_and_load() {
    // Real Redis connection required
    let config = RedisConfig {
        redis_url: get_redis_url(),
        pool_size: 5,
        default_ttl_secs: 3600,
        key_prefix: "test:".to_string(),
        enable_cluster: false,
        compression: false,
        enable_pubsub: false,
        max_retries: 3,
        retry_delay_ms: 100,
    };

    let checkpointer = RedisCheckpointer::new(config)
        .await
        .expect("Failed to create Redis checkpointer");

    // Create test state
    let mut state = GraphState::new();
    state.set("test_key", json!("test_value"));
    state.set("counter", json!(42));
    state.set(
        "nested",
        json!({
            "field1": "value1",
            "field2": 123
        }),
    );

    let thread_id = "test_thread_001";

    // Save checkpoint
    let checkpoint_id = checkpointer
        .save_checkpoint(thread_id, &state)
        .await
        .expect("Failed to save checkpoint");

    assert!(!checkpoint_id.is_empty());

    // Load checkpoint immediately
    let loaded_state = checkpointer
        .load_checkpoint(thread_id, Some(&checkpoint_id))
        .await
        .expect("Failed to load checkpoint")
        .expect("Checkpoint not found");

    // Verify state
    assert_eq!(loaded_state.get("test_key"), Some(&json!("test_value")));
    assert_eq!(loaded_state.get("counter"), Some(&json!(42)));
    assert_eq!(
        loaded_state.get("nested"),
        Some(&json!({
            "field1": "value1",
            "field2": 123
        }))
    );
}

#[tokio::test]
async fn test_redis_checkpointer_ttl_expiry() {
    let config = RedisConfig {
        redis_url: get_redis_url(),
        pool_size: 5,
        default_ttl_secs: 1, // 1 second TTL for testing
        key_prefix: "test_ttl:".to_string(),
        enable_cluster: false,
        compression: false,
        enable_pubsub: false,
        max_retries: 3,
        retry_delay_ms: 100,
    };

    let checkpointer = RedisCheckpointer::new(config)
        .await
        .expect("Failed to create Redis checkpointer");

    let thread_id = "test_thread_ttl";
    let mut state = GraphState::new();
    state.set("ephemeral", json!("should expire"));

    // Save with TTL
    let checkpoint_id = checkpointer
        .save_checkpoint(thread_id, &state)
        .await
        .expect("Failed to save checkpoint");

    // Load immediately - should exist
    let loaded = checkpointer
        .load_checkpoint(thread_id, Some(&checkpoint_id))
        .await
        .expect("Failed to load checkpoint");
    assert!(loaded.is_some());

    // Wait for TTL to expire
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Try to load - should be gone
    let expired = checkpointer
        .load_checkpoint(thread_id, Some(&checkpoint_id))
        .await
        .expect("Failed to load checkpoint");
    assert!(expired.is_none());
}

#[tokio::test]
async fn test_redis_checkpointer_batch_operations() {
    let config = RedisConfig {
        redis_url: get_redis_url(),
        pool_size: 5,
        default_ttl_secs: 3600,
        key_prefix: "test_batch:".to_string(),
        enable_cluster: false,
        compression: false,
        enable_pubsub: false,
        max_retries: 3,
        retry_delay_ms: 100,
    };

    let checkpointer = RedisCheckpointer::new(config)
        .await
        .expect("Failed to create Redis checkpointer");

    // Prepare multiple checkpoints
    let mut checkpoints = Vec::new();
    for i in 0..5 {
        let mut state = GraphState::new();
        state.set("batch_id", json!(i));
        state.set("data", json!(format!("batch_data_{}", i)));
        checkpoints.push((format!("thread_batch_{}", i), state));
    }

    // Batch save using pipeline
    let ids = checkpointer
        .batch_save_checkpoints(checkpoints.clone())
        .await
        .expect("Failed to batch save checkpoints");

    assert_eq!(ids.len(), 5);

    // Batch load
    let loaded_states = checkpointer
        .batch_load_checkpoints(&ids)
        .await
        .expect("Failed to batch load checkpoints");

    assert_eq!(loaded_states.len(), 5);

    // Verify each state
    for (i, state) in loaded_states.iter().enumerate() {
        if let Some(state) = state {
            assert_eq!(state.get("batch_id"), Some(&json!(i)));
        } else {
            panic!("Checkpoint {} not found", i);
        }
    }
}

#[tokio::test]
async fn test_redis_checkpointer_compression() {
    let config = RedisConfig {
        redis_url: get_redis_url(),
        pool_size: 5,
        default_ttl_secs: 3600,
        key_prefix: "test_compress:".to_string(),
        enable_cluster: false,
        compression: true, // Enable compression
        enable_pubsub: false,
        max_retries: 3,
        retry_delay_ms: 100,
    };

    let checkpointer = RedisCheckpointer::new(config)
        .await
        .expect("Failed to create Redis checkpointer");

    let thread_id = "test_thread_compress";
    let mut state = GraphState::new();

    // Create large state to benefit from compression
    let large_data = vec!["test_data"; 1000].join(" ");
    state.set("large_field", json!(large_data));

    // Save with compression
    let checkpoint_id = checkpointer
        .save_checkpoint(thread_id, &state)
        .await
        .expect("Failed to save checkpoint");

    // Load and verify
    let loaded_state = checkpointer
        .load_checkpoint(thread_id, Some(&checkpoint_id))
        .await
        .expect("Failed to load checkpoint")
        .expect("Checkpoint not found");

    assert_eq!(loaded_state.get("large_field"), Some(&json!(large_data)));

    // Verify compression actually happened (would need Redis inspection)
    let stats = checkpointer
        .get_checkpoint_stats(&checkpoint_id)
        .await
        .expect("Failed to get stats");
    assert!(stats.compressed);
    assert!(stats.compression_ratio > 1.0);
}

#[tokio::test]
async fn test_redis_checkpointer_pubsub() {
    let config = RedisConfig {
        redis_url: get_redis_url(),
        pool_size: 5,
        default_ttl_secs: 3600,
        key_prefix: "test_pubsub:".to_string(),
        enable_cluster: false,
        compression: false,
        enable_pubsub: true, // Enable pub/sub
        max_retries: 3,
        retry_delay_ms: 100,
    };

    let checkpointer = RedisCheckpointer::new(config)
        .await
        .expect("Failed to create Redis checkpointer");

    let thread_id = "test_thread_pubsub";

    // Subscribe to state changes
    let mut subscriber = checkpointer
        .subscribe_to_changes(thread_id)
        .await
        .expect("Failed to subscribe");

    // Save checkpoint in another task
    let checkpointer_clone = checkpointer.clone();
    let thread_id_clone = thread_id.to_string();
    let save_task = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut state = GraphState::new();
        state.set("pubsub_test", json!("notification"));

        checkpointer_clone
            .save_checkpoint(&thread_id_clone, &state)
            .await
            .expect("Failed to save checkpoint")
    });

    // Wait for notification
    let notification = tokio::time::timeout(Duration::from_secs(2), subscriber.recv())
        .await
        .expect("Timeout waiting for notification")
        .expect("Failed to receive notification");

    assert_eq!(notification.thread_id, thread_id);
    assert!(notification.checkpoint_id.len() > 0);

    save_task.await.unwrap();
}

#[tokio::test]
async fn test_redis_checkpointer_atomic_operations() {
    let config = RedisConfig {
        redis_url: get_redis_url(),
        pool_size: 5,
        default_ttl_secs: 3600,
        key_prefix: "test_atomic:".to_string(),
        enable_cluster: false,
        compression: false,
        enable_pubsub: false,
        max_retries: 3,
        retry_delay_ms: 100,
    };

    let checkpointer = RedisCheckpointer::new(config)
        .await
        .expect("Failed to create Redis checkpointer");

    let thread_id = "test_thread_atomic";

    // Test compare-and-swap operation
    let mut state1 = GraphState::new();
    state1.set("version", json!(1));

    let checkpoint_id = checkpointer
        .save_checkpoint(thread_id, &state1)
        .await
        .expect("Failed to save checkpoint");

    // Try to update with CAS
    let mut state2 = GraphState::new();
    state2.set("version", json!(2));

    let updated = checkpointer
        .compare_and_swap(thread_id, Some(&checkpoint_id), &state2)
        .await
        .expect("Failed to CAS");

    assert!(updated);

    // Try to update with wrong checkpoint ID (should fail)
    let mut state3 = GraphState::new();
    state3.set("version", json!(3));

    let failed_update = checkpointer
        .compare_and_swap(thread_id, Some("wrong_id"), &state3)
        .await
        .expect("Failed to CAS");

    assert!(!failed_update);

    // Verify final state
    let final_state = checkpointer
        .load_checkpoint(thread_id, None)
        .await
        .expect("Failed to load")
        .expect("Not found");

    assert_eq!(final_state.get("version"), Some(&json!(2)));
}

#[tokio::test]
async fn test_redis_checkpointer_health_check() {
    let config = RedisConfig {
        redis_url: get_redis_url(),
        pool_size: 5,
        default_ttl_secs: 3600,
        key_prefix: "test_health:".to_string(),
        enable_cluster: false,
        compression: false,
        enable_pubsub: false,
        max_retries: 3,
        retry_delay_ms: 100,
    };

    let checkpointer = RedisCheckpointer::new(config)
        .await
        .expect("Failed to create Redis checkpointer");

    // Health check should pass
    let healthy = checkpointer
        .health_check()
        .await
        .expect("Health check failed");

    assert!(healthy);

    // Get connection stats
    let stats = checkpointer
        .get_connection_stats()
        .await
        .expect("Failed to get stats");

    assert!(stats.active_connections > 0);
    assert_eq!(stats.pool_size, 5);
}
