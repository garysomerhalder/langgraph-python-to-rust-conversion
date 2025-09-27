// Integration tests for Distributed Checkpointer
// Following Integration-First methodology - uses real etcd cluster and multiple nodes

use langgraph::checkpoint::{Checkpointer, DistributedCheckpointer, DistributedConfig, StateEvent, PerformanceMetrics, DistributedLock};
use langgraph::state::GraphState;
use serde_json::json;
use std::env;
use std::time::Duration;
use tokio::time::sleep;

/// Get distributed configuration from environment
fn get_distributed_config(node_id: &str) -> DistributedConfig {
    DistributedConfig {
        node_id: node_id.to_string(),
        etcd_endpoints: env::var("ETCD_ENDPOINTS")
            .unwrap_or_else(|_| "localhost:2379".to_string())
            .split(',')
            .map(|s| s.trim().to_string())
            .collect(),
        cluster_name: "langgraph-test".to_string(),
        key_prefix: format!("test/distributed/{}/", node_id),
        consensus_timeout: Duration::from_secs(5),
        sync_interval: Duration::from_millis(100),
        lock_timeout: Duration::from_secs(10),
        enable_leader_election: true,
        enable_conflict_resolution: true,
    }
}

#[tokio::test]
async fn test_distributed_checkpointer_multi_node_basic() {
    // RED PHASE: This test WILL FAIL until we implement DistributedCheckpointer

    // Create two distributed checkpointer nodes
    let config_node1 = get_distributed_config("node1");
    let config_node2 = get_distributed_config("node2");

    let node1 = DistributedCheckpointer::new(config_node1).await
        .expect("Failed to create distributed checkpointer node1");

    let node2 = DistributedCheckpointer::new(config_node2).await
        .expect("Failed to create distributed checkpointer node2");

    // Join both nodes to cluster
    node1.join_cluster().await
        .expect("Node1 failed to join cluster");

    node2.join_cluster().await
        .expect("Node2 failed to join cluster");

    // Wait for cluster formation
    sleep(Duration::from_secs(2)).await;

    // Verify cluster membership
    let cluster_members = node1.get_cluster_members().await
        .expect("Failed to get cluster members");
    assert_eq!(cluster_members.len(), 2);
    assert!(cluster_members.contains_key("node1"));
    assert!(cluster_members.contains_key("node2"));

    // Test basic save/load across nodes
    let thread_id = "test_distributed_thread";
    let mut state = GraphState::new();
    state.set("distributed_test", json!("multi_node_value"));

    // Save checkpoint on node1
    let checkpoint_id = node1.save(
        thread_id,
        state.to_checkpoint(),
        std::collections::HashMap::new(),
        None,
    ).await.expect("Failed to save checkpoint on node1");

    // Wait for synchronization
    sleep(Duration::from_millis(500)).await;

    // Load checkpoint from node2 (should be synchronized)
    let loaded = node2.load(thread_id, Some(checkpoint_id.clone())).await
        .expect("Failed to load from node2")
        .expect("Checkpoint not found on node2");

    assert_eq!(loaded.0.get("distributed_test"), Some(&json!("multi_node_value")));

    // Cleanup
    node1.leave_cluster().await.expect("Node1 failed to leave cluster");
    node2.leave_cluster().await.expect("Node2 failed to leave cluster");
}

#[tokio::test]
async fn test_distributed_checkpointer_leader_election() {
    // RED PHASE: This test WILL FAIL until we implement leader election

    let config_node1 = get_distributed_config("leader_test_node1");
    let config_node2 = get_distributed_config("leader_test_node2");

    let node1 = DistributedCheckpointer::new(config_node1).await
        .expect("Failed to create node1");

    let node2 = DistributedCheckpointer::new(config_node2).await
        .expect("Failed to create node2");

    // Join cluster
    node1.join_cluster().await.expect("Node1 join failed");
    node2.join_cluster().await.expect("Node2 join failed");

    // Wait for leader election
    sleep(Duration::from_secs(3)).await;

    // Verify exactly one leader exists
    let leader1 = node1.get_current_leader().await
        .expect("Failed to get leader from node1");
    let leader2 = node2.get_current_leader().await
        .expect("Failed to get leader from node2");

    assert!(leader1.is_some());
    assert!(leader2.is_some());
    assert_eq!(leader1, leader2, "Nodes disagree on leader");

    // Verify one node is leader
    let node1_is_leader = node1.is_leader().await.expect("Failed to check if node1 is leader");
    let node2_is_leader = node2.is_leader().await.expect("Failed to check if node2 is leader");

    assert!(
        (node1_is_leader && !node2_is_leader) || (!node1_is_leader && node2_is_leader),
        "Exactly one node should be leader"
    );

    // Cleanup
    node1.leave_cluster().await.expect("Node1 leave failed");
    node2.leave_cluster().await.expect("Node2 leave failed");
}

#[tokio::test]
async fn test_distributed_checkpointer_conflict_resolution() {
    // RED PHASE: This test WILL FAIL until we implement conflict resolution

    let config_node1 = get_distributed_config("conflict_node1");
    let config_node2 = get_distributed_config("conflict_node2");

    let node1 = DistributedCheckpointer::new(config_node1).await
        .expect("Failed to create node1");

    let node2 = DistributedCheckpointer::new(config_node2).await
        .expect("Failed to create node2");

    // Join cluster
    node1.join_cluster().await.expect("Node1 join failed");
    node2.join_cluster().await.expect("Node2 join failed");

    sleep(Duration::from_secs(1)).await;

    let thread_id = "conflict_test_thread";

    // Create conflicting state updates
    let mut state1 = GraphState::new();
    state1.set("counter", json!(1));
    state1.set("source", json!("node1"));

    let mut state2 = GraphState::new();
    state2.set("counter", json!(2));
    state2.set("source", json!("node2"));

    // Concurrent saves to create conflict
    let (result1, result2) = tokio::join!(
        node1.save(thread_id, state1.to_checkpoint(), std::collections::HashMap::new(), None),
        node2.save(thread_id, state2.to_checkpoint(), std::collections::HashMap::new(), None)
    );

    let checkpoint_id1 = result1.expect("Node1 save failed");
    let checkpoint_id2 = result2.expect("Node2 save failed");

    // Wait for conflict resolution
    sleep(Duration::from_secs(2)).await;

    // Both nodes should have consistent final state
    let final_state1 = node1.load(thread_id, None).await
        .expect("Failed to load final state from node1")
        .expect("Final state not found on node1");

    let final_state2 = node2.load(thread_id, None).await
        .expect("Failed to load final state from node2")
        .expect("Final state not found on node2");

    // States should be consistent (conflict resolved)
    assert_eq!(final_state1, final_state2, "Conflict resolution failed - states are inconsistent");

    // Cleanup
    node1.leave_cluster().await.expect("Node1 leave failed");
    node2.leave_cluster().await.expect("Node2 leave failed");
}

#[tokio::test]
async fn test_distributed_checkpointer_distributed_locking() {
    // RED PHASE: This test WILL FAIL until we implement distributed locking

    let config_node1 = get_distributed_config("lock_node1");
    let config_node2 = get_distributed_config("lock_node2");

    let node1 = DistributedCheckpointer::new(config_node1).await
        .expect("Failed to create node1");

    let node2 = DistributedCheckpointer::new(config_node2).await
        .expect("Failed to create node2");

    // Join cluster
    node1.join_cluster().await.expect("Node1 join failed");
    node2.join_cluster().await.expect("Node2 join failed");

    sleep(Duration::from_secs(1)).await;

    let lock_key = "test_lock_key";

    // Node1 acquires lock
    let lock1 = node1.acquire_lock(lock_key, Duration::from_secs(5)).await
        .expect("Node1 failed to acquire lock");

    assert!(lock1.is_some(), "Node1 should have acquired the lock");

    // Node2 attempts to acquire same lock (should fail or wait)
    let lock2_result = tokio::time::timeout(
        Duration::from_millis(100),
        node2.acquire_lock(lock_key, Duration::from_secs(5))
    ).await;

    match lock2_result {
        Ok(lock2_option) => match lock2_option {
            Ok(lock2) => assert!(lock2.is_none(), "Node2 should not acquire lock while node1 holds it"),
            Err(_) => {}, // Lock error is expected when waiting for lock
        },
        Err(_) => {}, // Timeout is expected when waiting for lock
    }

    // Release lock from node1
    if let Some(lock) = lock1 {
        node1.release_lock(lock).await.expect("Failed to release lock");
    }

    // Node2 should now be able to acquire lock
    let lock2 = node2.acquire_lock(lock_key, Duration::from_secs(5)).await
        .expect("Node2 failed to acquire lock after release");

    assert!(lock2.is_some(), "Node2 should acquire lock after node1 released it");

    // Cleanup
    if let Some(lock) = lock2 {
        node2.release_lock(lock).await.expect("Failed to release lock2");
    }
    node1.leave_cluster().await.expect("Node1 leave failed");
    node2.leave_cluster().await.expect("Node2 leave failed");
}

#[tokio::test]
async fn test_distributed_checkpointer_partition_tolerance() {
    // RED PHASE: This test WILL FAIL until we implement partition tolerance

    let config_node1 = get_distributed_config("partition_node1");
    let config_node2 = get_distributed_config("partition_node2");
    let config_node3 = get_distributed_config("partition_node3");

    let node1 = DistributedCheckpointer::new(config_node1).await
        .expect("Failed to create node1");

    let node2 = DistributedCheckpointer::new(config_node2).await
        .expect("Failed to create node2");

    let node3 = DistributedCheckpointer::new(config_node3).await
        .expect("Failed to create node3");

    // Join all nodes to cluster
    node1.join_cluster().await.expect("Node1 join failed");
    node2.join_cluster().await.expect("Node2 join failed");
    node3.join_cluster().await.expect("Node3 join failed");

    sleep(Duration::from_secs(2)).await;

    // Verify all nodes see 3-node cluster
    let members = node1.get_cluster_members().await.expect("Failed to get members");
    assert_eq!(members.len(), 3);

    // Simulate network partition: isolate node3
    node3.simulate_network_partition(vec!["partition_node1", "partition_node2"]).await
        .expect("Failed to simulate partition");

    sleep(Duration::from_secs(3)).await;

    // Majority partition (node1 + node2) should continue operating
    let thread_id = "partition_test_thread";
    let mut state = GraphState::new();
    state.set("partition_test", json!("majority_partition"));

    let checkpoint_id = node1.save(
        thread_id,
        state.to_checkpoint(),
        std::collections::HashMap::new(),
        None,
    ).await.expect("Majority partition should accept writes");

    // Load from node2 in majority partition
    let loaded = node2.load(thread_id, Some(checkpoint_id.clone())).await
        .expect("Failed to load in majority partition")
        .expect("Checkpoint not found in majority partition");

    assert_eq!(loaded.0.get("partition_test"), Some(&json!("majority_partition")));

    // Minority partition (node3) should reject writes or enter read-only mode
    let minority_result = node3.save(
        thread_id,
        state.to_checkpoint(),
        std::collections::HashMap::new(),
        None,
    ).await;

    assert!(minority_result.is_err(), "Minority partition should reject writes");

    // Heal partition
    node3.heal_network_partition().await.expect("Failed to heal partition");
    sleep(Duration::from_secs(2)).await;

    // Node3 should rejoin and sync state
    let synced_state = node3.load(thread_id, Some(checkpoint_id)).await
        .expect("Failed to load after partition heal")
        .expect("State not synced after partition heal");

    assert_eq!(synced_state.0.get("partition_test"), Some(&json!("majority_partition")));

    // Cleanup
    node1.leave_cluster().await.expect("Node1 leave failed");
    node2.leave_cluster().await.expect("Node2 leave failed");
    node3.leave_cluster().await.expect("Node3 leave failed");
}

#[tokio::test]
async fn test_distributed_checkpointer_event_propagation() {
    // RED PHASE: This test WILL FAIL until we implement event propagation

    let config_node1 = get_distributed_config("event_node1");
    let config_node2 = get_distributed_config("event_node2");

    let node1 = DistributedCheckpointer::new(config_node1).await
        .expect("Failed to create node1");

    let node2 = DistributedCheckpointer::new(config_node2).await
        .expect("Failed to create node2");

    // Subscribe to events on node2
    let mut event_stream = node2.subscribe_to_events().await
        .expect("Failed to subscribe to events");

    // Join cluster
    node1.join_cluster().await.expect("Node1 join failed");
    node2.join_cluster().await.expect("Node2 join failed");

    sleep(Duration::from_secs(1)).await;

    let thread_id = "event_test_thread";
    let mut state = GraphState::new();
    state.set("event_test", json!("propagation_value"));

    // Save checkpoint on node1
    let checkpoint_id = node1.save(
        thread_id,
        state.to_checkpoint(),
        std::collections::HashMap::new(),
        None,
    ).await.expect("Failed to save checkpoint");

    // Node2 should receive save event
    let event = tokio::time::timeout(Duration::from_secs(5), event_stream.recv()).await
        .expect("Timeout waiting for event")
        .expect("Failed to receive event");

    match event {
        StateEvent::CheckpointSaved { thread_id: evt_thread, checkpoint_id: evt_checkpoint, node_id } => {
            assert_eq!(evt_thread, thread_id);
            assert_eq!(evt_checkpoint, checkpoint_id);
            assert_eq!(node_id, "event_node1");
        }
        _ => panic!("Expected CheckpointSaved event, got: {:?}", event),
    }

    // Delete checkpoint and verify delete event
    node1.delete(thread_id, Some(&checkpoint_id)).await
        .expect("Failed to delete checkpoint");

    let delete_event = tokio::time::timeout(Duration::from_secs(5), event_stream.recv()).await
        .expect("Timeout waiting for delete event")
        .expect("Failed to receive delete event");

    match delete_event {
        StateEvent::CheckpointDeleted { thread_id: evt_thread, checkpoint_id: evt_checkpoint, node_id } => {
            assert_eq!(evt_thread, thread_id);
            assert_eq!(evt_checkpoint, checkpoint_id);
            assert_eq!(node_id, "event_node1");
        }
        _ => panic!("Expected CheckpointDeleted event, got: {:?}", delete_event),
    }

    // Cleanup
    node1.leave_cluster().await.expect("Node1 leave failed");
    node2.leave_cluster().await.expect("Node2 leave failed");
}

#[tokio::test]
async fn test_distributed_checkpointer_performance_benchmark() {
    // RED PHASE: This test WILL FAIL until we implement performance monitoring

    let config_node1 = get_distributed_config("perf_node1");
    let config_node2 = get_distributed_config("perf_node2");

    let node1 = DistributedCheckpointer::new(config_node1).await
        .expect("Failed to create node1");

    let node2 = DistributedCheckpointer::new(config_node2).await
        .expect("Failed to create node2");

    // Join cluster
    node1.join_cluster().await.expect("Node1 join failed");
    node2.join_cluster().await.expect("Node2 join failed");

    sleep(Duration::from_secs(1)).await;

    let thread_id = "perf_test_thread";
    let num_operations = 100;

    // Benchmark save operations
    let start_time = std::time::Instant::now();

    for i in 0..num_operations {
        let mut state = GraphState::new();
        state.set("iteration", json!(i));
        state.set("timestamp", json!(chrono::Utc::now().to_rfc3339()));

        let _checkpoint_id = node1.save(
            &format!("{}_{}", thread_id, i),
            state.to_checkpoint(),
            std::collections::HashMap::new(),
            None,
        ).await.expect("Failed to save checkpoint during benchmark");
    }

    let save_duration = start_time.elapsed();
    let saves_per_second = num_operations as f64 / save_duration.as_secs_f64();

    // Verify synchronization performance
    sleep(Duration::from_secs(2)).await;

    let sync_start = std::time::Instant::now();
    let mut sync_count = 0;

    for i in 0..num_operations {
        if let Ok(Some(_)) = node2.load(&format!("{}_{}", thread_id, i), None).await {
            sync_count += 1;
        }
    }

    let sync_duration = sync_start.elapsed();
    let sync_rate = sync_count as f64 / sync_duration.as_secs_f64();

    // Performance assertions
    assert!(saves_per_second > 10.0, "Save performance too low: {} ops/sec", saves_per_second);
    assert!(sync_rate > 50.0, "Sync performance too low: {} ops/sec", sync_rate);
    assert_eq!(sync_count, num_operations, "Not all checkpoints synchronized");

    // Get performance metrics
    let metrics = node1.get_performance_metrics().await
        .expect("Failed to get performance metrics");

    assert!(metrics.average_save_latency.as_millis() < 100, "Save latency too high");
    assert!(metrics.average_sync_latency.as_millis() < 50, "Sync latency too high");
    assert!(metrics.throughput_ops_per_second > 10.0, "Throughput too low");

    println!("Performance Results:");
    println!("  Saves per second: {:.2}", saves_per_second);
    println!("  Sync rate: {:.2}", sync_rate);
    println!("  Average save latency: {:?}", metrics.average_save_latency);
    println!("  Average sync latency: {:?}", metrics.average_sync_latency);
    println!("  Throughput: {:.2} ops/sec", metrics.throughput_ops_per_second);

    // Cleanup
    node1.leave_cluster().await.expect("Node1 leave failed");
    node2.leave_cluster().await.expect("Node2 leave failed");
}

// Types that need to be implemented (will cause compilation failures)
// Use DistributedConfig from library - removed duplicate definition

// Use StateEvent from library - removed duplicate definition

// Use PerformanceMetrics and DistributedLock from library - removed duplicate definitions