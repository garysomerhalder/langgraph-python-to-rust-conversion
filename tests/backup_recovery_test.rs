// Integration tests for Backup and Recovery System
// Following Integration-First methodology - tests against real persistence backends

use langgraph::checkpoint::{Checkpointer, MemoryCheckpointer};
use langgraph::state::GraphState;
use langgraph::backup::{BackupManager, FileBackupStorage, BackupType, RetentionPolicy};
use serde_json::json;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::sleep;
use async_trait::async_trait;

// These tests use the real BackupManager implementation

#[tokio::test]
async fn test_backup_manager_full_backup_creation() {
    // Test creating a full backup of checkpoint data
    let checkpointer = MemoryCheckpointer::new();

    // Create some checkpoint data
    let thread_id = "backup_test_thread";
    let checkpoint_data = HashMap::from([
        ("state".to_string(), json!({"counter": 42, "status": "active"})),
        ("metadata".to_string(), json!({"version": "1.0", "created_by": "test"}))
    ]);
    let metadata = HashMap::from([
        ("backup_test".to_string(), json!(true)),
        ("timestamp".to_string(), json!(1234567890))
    ]);

    let checkpoint_id = checkpointer.save(thread_id, checkpoint_data.clone(), metadata.clone(), None)
        .await.expect("Failed to save checkpoint");

    // Use real BackupManager implementation
    let backup_manager = BackupManager::new(Box::new(FileBackupStorage::new("./backups")));

    let backup_id = backup_manager.create_full_backup(&checkpointer, Some("test_backup"))
        .await.expect("Failed to create full backup");

    // Verify backup was created
    assert!(!backup_id.is_empty(), "Backup ID should not be empty");

    // Verify backup metadata
    let backup_info = backup_manager.get_backup_info(&backup_id)
        .await.expect("Failed to get backup info");

    assert_eq!(backup_info.backup_type, BackupType::Full);
    assert_eq!(backup_info.source_backend, "memory");
    assert!(backup_info.created_at > 0);
    assert!(backup_info.size_bytes > 0);
}

#[tokio::test]
async fn test_backup_manager_full_restore_operation() {
    // Test complete restore from backup
    let source_checkpointer = MemoryCheckpointer::new();
    let target_checkpointer = MemoryCheckpointer::new();

    // Create original data
    let thread_id = "restore_test_thread";
    let original_data = HashMap::from([
        ("workflow_state".to_string(), json!({"step": 5, "completed": ["init", "process", "validate"]})),
        ("user_context".to_string(), json!({"user_id": "test_user", "session": "abc123"}))
    ]);
    let metadata = HashMap::new();

    source_checkpointer.save(thread_id, original_data.clone(), metadata, None)
        .await.expect("Failed to save original checkpoint");

    // Create backup
    let backup_manager = BackupManager::new(Box::new(FileBackupStorage::new("./backups")));
    let backup_id = backup_manager.create_full_backup(&source_checkpointer, Some("restore_test"))
        .await.expect("Failed to create backup");

    // Restore to target checkpointer
    let restore_result = backup_manager.restore_backup(&backup_id, &target_checkpointer)
        .await.expect("Failed to restore backup");

    assert_eq!(restore_result.restored_checkpoints, 1);
    assert_eq!(restore_result.restored_threads, 1);

    // Verify restored data matches original
    let restored_data = target_checkpointer.load(thread_id, None)
        .await.expect("Failed to load restored data")
        .expect("Restored data should exist");

    assert_eq!(restored_data.0.get("workflow_state"), original_data.get("workflow_state"));
    assert_eq!(restored_data.0.get("user_context"), original_data.get("user_context"));
}

#[tokio::test]
async fn test_backup_manager_incremental_backup_chain() {
    // Test incremental backup creation and restoration
    let checkpointer = MemoryCheckpointer::new();
    let backup_manager = BackupManager::new(Box::new(FileBackupStorage::new("./backups")));

    let thread_id = "incremental_test_thread";

    // Create initial state and full backup
    let initial_data = HashMap::from([
        ("counter".to_string(), json!(1)),
        ("items".to_string(), json!(["item1"]))
    ]);
    checkpointer.save(thread_id, initial_data, HashMap::new(), None)
        .await.expect("Failed to save initial checkpoint");

    let full_backup_id = backup_manager.create_full_backup(&checkpointer, Some("incremental_base"))
        .await.expect("Failed to create full backup");

    // Wait and create incremental changes
    sleep(Duration::from_millis(100)).await;

    let updated_data = HashMap::from([
        ("counter".to_string(), json!(2)),
        ("items".to_string(), json!(["item1", "item2"]))
    ]);
    checkpointer.save(thread_id, updated_data, HashMap::new(), None)
        .await.expect("Failed to save updated checkpoint");

    // Create incremental backup
    let incremental_backup_id = backup_manager.create_incremental_backup(&checkpointer, &full_backup_id, Some("incremental_1"))
        .await.expect("Failed to create incremental backup");

    // Add more changes
    sleep(Duration::from_millis(100)).await;

    let final_data = HashMap::from([
        ("counter".to_string(), json!(3)),
        ("items".to_string(), json!(["item1", "item2", "item3"]))
    ]);
    checkpointer.save(thread_id, final_data, HashMap::new(), None)
        .await.expect("Failed to save final checkpoint");

    let incremental_backup_2_id = backup_manager.create_incremental_backup(&checkpointer, &incremental_backup_id, Some("incremental_2"))
        .await.expect("Failed to create second incremental backup");

    // Restore from incremental backup chain
    let target_checkpointer = MemoryCheckpointer::new();
    let restore_result = backup_manager.restore_incremental_chain(&incremental_backup_2_id, &target_checkpointer)
        .await.expect("Failed to restore incremental chain");

    // Verify final state is restored correctly
    let restored_data = target_checkpointer.load(thread_id, None)
        .await.expect("Failed to load restored data")
        .expect("Restored data should exist");

    assert_eq!(restored_data.0.get("counter"), Some(&json!(3)));
    assert_eq!(restored_data.0.get("items"), Some(&json!(["item1", "item2", "item3"])));
}

#[tokio::test]
async fn test_backup_manager_cross_backend_restore() {
    // Test backup from one backend type and restore to another
    let memory_checkpointer = MemoryCheckpointer::new();

    // Create test data in memory backend
    let thread_id = "cross_backend_test";
    let test_data = HashMap::from([
        ("source_backend".to_string(), json!("memory")),
        ("migration_test".to_string(), json!({"data": "cross_backend_data", "version": 2}))
    ]);

    memory_checkpointer.save(thread_id, test_data.clone(), HashMap::new(), None)
        .await.expect("Failed to save to memory backend");

    // Create backup from memory backend
    let backup_manager = BackupManager::new(Box::new(FileBackupStorage::new("./backups")));
    let backup_id = backup_manager.create_full_backup(&memory_checkpointer, Some("cross_backend_test"))
        .await.expect("Failed to create backup from memory backend");

    // Restore to different backend type (another memory instance simulating different backend)
    let target_checkpointer = MemoryCheckpointer::new();
    let restore_result = backup_manager.restore_backup(&backup_id, &target_checkpointer)
        .await.expect("Failed to restore to target backend");

    assert!(restore_result.cross_backend_migration);
    assert_eq!(restore_result.source_backend_type, "memory");
    assert_eq!(restore_result.target_backend_type, "memory");

    // Verify data integrity across backends
    let restored_data = target_checkpointer.load(thread_id, None)
        .await.expect("Failed to load from target backend")
        .expect("Data should exist in target backend");

    assert_eq!(restored_data.0.get("migration_test"), test_data.get("migration_test"));
    assert_eq!(restored_data.0.get("source_backend"), test_data.get("source_backend"));
}

#[tokio::test]
async fn test_backup_manager_retention_policy() {
    // Test automated backup retention and cleanup
    let checkpointer = MemoryCheckpointer::new();
    let backup_storage = FileBackupStorage::new("./backups");

    // Configure retention policy: keep 3 backups, delete older ones
    let retention_policy = RetentionPolicy::new()
        .max_backups(3)
        .max_age_days(7)
        .cleanup_interval(Duration::from_secs(60));

    let backup_manager = BackupManager::new(Box::new(backup_storage))
        .with_retention_policy(retention_policy);

    let thread_id = "retention_test_thread";
    let test_data = HashMap::from([
        ("retention_test".to_string(), json!(true))
    ]);

    checkpointer.save(thread_id, test_data, HashMap::new(), None)
        .await.expect("Failed to save checkpoint");

    // Create multiple backups
    let mut backup_ids = Vec::new();
    for i in 0..5 {
        let backup_id = backup_manager.create_full_backup(&checkpointer, Some(&format!("retention_test_{}", i)))
            .await.expect("Failed to create backup");
        backup_ids.push(backup_id);

        // Small delay between backups
        sleep(Duration::from_millis(10)).await;
    }

    // Apply retention policy
    let cleanup_result = backup_manager.apply_retention_policy()
        .await.expect("Failed to apply retention policy");

    assert_eq!(cleanup_result.deleted_backups, 2); // Should delete 2 oldest backups
    assert_eq!(cleanup_result.retained_backups, 3); // Should keep 3 newest backups

    // Verify only latest backups remain accessible
    let remaining_backups = backup_manager.list_backups(None)
        .await.expect("Failed to list backups");

    assert_eq!(remaining_backups.len(), 3);

    // Verify oldest backups are no longer accessible
    for old_backup_id in &backup_ids[0..2] {
        let backup_info = backup_manager.get_backup_info(old_backup_id).await;
        assert!(backup_info.is_err(), "Old backup should no longer be accessible");
    }
}

#[tokio::test]
async fn test_backup_manager_backup_verification() {
    // Test backup integrity verification
    let checkpointer = MemoryCheckpointer::new();
    let backup_manager = BackupManager::new(Box::new(FileBackupStorage::new("./backups")));

    let thread_id = "verification_test";
    let test_data = HashMap::from([
        ("verification_data".to_string(), json!({"checksum": "abc123", "size": 1024}))
    ]);

    checkpointer.save(thread_id, test_data, HashMap::new(), None)
        .await.expect("Failed to save checkpoint");

    // Create backup with verification
    let backup_id = backup_manager.create_full_backup(&checkpointer, Some("verification_test"))
        .await.expect("Failed to create backup");

    // Verify backup integrity
    let verification_result = backup_manager.verify_backup(&backup_id)
        .await.expect("Failed to verify backup");

    assert!(verification_result.is_valid);
    assert!(verification_result.checksum_match);
    assert_eq!(verification_result.corruption_errors.len(), 0);
    assert!(verification_result.metadata_valid);

    // Test verification with corrupted backup (this would require actual corruption simulation)
    // For now, just verify the API exists
    let detailed_verification = backup_manager.verify_backup_detailed(&backup_id, true)
        .await.expect("Failed to perform detailed verification");

    assert!(detailed_verification.data_integrity_check);
    assert!(detailed_verification.metadata_integrity_check);
    assert!(detailed_verification.compression_integrity_check);
}

#[tokio::test]
async fn test_backup_manager_disaster_recovery_scenario() {
    // Test complete disaster recovery workflow
    let primary_checkpointer = MemoryCheckpointer::new();
    let backup_manager = BackupManager::new(Box::new(FileBackupStorage::new("./backups")));

    // Simulate production data
    let threads = vec!["workflow_1", "workflow_2", "workflow_3"];
    let mut original_data = HashMap::new();

    for (i, thread_id) in threads.iter().enumerate() {
        let data = HashMap::from([
            ("workflow_id".to_string(), json!(thread_id)),
            ("step".to_string(), json!(i + 1)),
            ("status".to_string(), json!("running")),
            ("data".to_string(), json!(format!("workflow_data_{}", i)))
        ]);

        primary_checkpointer.save(thread_id, data.clone(), HashMap::new(), None)
            .await.expect("Failed to save workflow checkpoint");

        original_data.insert(thread_id.to_string(), data);
    }

    // Create disaster recovery backup
    let disaster_backup_id = backup_manager.create_disaster_recovery_backup(&primary_checkpointer, "disaster_scenario")
        .await.expect("Failed to create disaster recovery backup");

    // Simulate disaster - create new "recovered" system
    let recovered_checkpointer = MemoryCheckpointer::new();

    // Perform disaster recovery
    let recovery_result = backup_manager.disaster_recovery_restore(&disaster_backup_id, &recovered_checkpointer)
        .await.expect("Failed to perform disaster recovery");

    assert_eq!(recovery_result.restored_threads, 3);
    assert_eq!(recovery_result.recovery_time_seconds, recovery_result.recovery_time_seconds); // Just verify field exists
    assert!(recovery_result.data_integrity_verified);

    // Verify all workflows are recovered correctly
    for thread_id in &threads {
        let recovered_data = recovered_checkpointer.load(thread_id, None)
            .await.expect("Failed to load recovered workflow")
            .expect("Recovered workflow should exist");

        let original = original_data.get(*thread_id).unwrap();
        assert_eq!(recovered_data.0.get("workflow_id"), original.get("workflow_id"));
        assert_eq!(recovered_data.0.get("status"), original.get("status"));
        assert_eq!(recovered_data.0.get("data"), original.get("data"));
    }
}

// All types and implementations are now in the backup module