// Integration tests for S3 Checkpointer
// Following Integration-First methodology - uses real S3 or S3-compatible service

use langgraph::checkpoint::{Checkpointer, S3Checkpointer, S3Config};
use langgraph::state::GraphState;
use serde_json::json;
use std::env;

/// Get S3 configuration from environment
fn get_s3_config() -> S3Config {
    S3Config {
        bucket_name: env::var("S3_BUCKET_NAME").unwrap_or_else(|_| "langgraph-test".to_string()),
        region: env::var("AWS_REGION").unwrap_or_else(|_| "us-east-1".to_string()),
        key_prefix: "test/checkpoints/".to_string(),
        enable_versioning: false,
        enable_encryption: true,
        compression: true,
        multipart_threshold_mb: 5,
        endpoint_url: env::var("S3_ENDPOINT_URL").ok(), // For LocalStack/MinIO
        force_path_style: env::var("S3_FORCE_PATH_STYLE").is_ok(),
    }
}

#[tokio::test]
async fn test_s3_checkpointer_save_and_load() {
    // Real S3 or LocalStack connection required
    let config = get_s3_config();

    let checkpointer = S3Checkpointer::new(config).await
        .expect("Failed to create S3 checkpointer");

    // Create bucket if it doesn't exist
    checkpointer.ensure_bucket_exists().await
        .expect("Failed to ensure bucket exists");

    // Create test state
    let mut state = GraphState::new();
    state.set("s3_test_key", json!("s3_test_value"));
    state.set("metadata", json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "version": "1.0.0"
    }));

    let thread_id = "test_s3_thread_001";

    // Save checkpoint
    let checkpoint_id = checkpointer.save_checkpoint(thread_id, &state).await
        .expect("Failed to save checkpoint to S3");

    assert!(!checkpoint_id.is_empty());

    // Load checkpoint
    let loaded_state = checkpointer.load_checkpoint(thread_id, Some(&checkpoint_id)).await
        .expect("Failed to load checkpoint from S3")
        .expect("Checkpoint not found");

    // Verify state
    assert_eq!(loaded_state.get("s3_test_key"), Some(&json!("s3_test_value")));
    assert!(loaded_state.get("metadata").is_some());
}

#[tokio::test]
async fn test_s3_checkpointer_multipart_upload() {
    let config = get_s3_config();

    let checkpointer = S3Checkpointer::new(config).await
        .expect("Failed to create S3 checkpointer");

    checkpointer.ensure_bucket_exists().await
        .expect("Failed to ensure bucket exists");

    let thread_id = "test_s3_multipart";
    let mut state = GraphState::new();

    // Create large state to trigger multipart upload
    let large_data = vec!["x"; 1024 * 1024].join(""); // 1MB+ of data
    for i in 0..10 {
        state.set(format!("large_field_{}", i), json!(large_data.clone()));
    }

    // Save with multipart
    let checkpoint_id = checkpointer.save_checkpoint(thread_id, &state).await
        .expect("Failed to save large checkpoint");

    // Verify upload completed
    let exists = checkpointer.checkpoint_exists(thread_id, &checkpoint_id).await
        .expect("Failed to check existence");
    assert!(exists);

    // Load and verify
    let loaded_state = checkpointer.load_checkpoint(thread_id, Some(&checkpoint_id)).await
        .expect("Failed to load")
        .expect("Not found");

    assert_eq!(loaded_state.get("large_field_0"), state.get("large_field_0"));
}

#[tokio::test]
async fn test_s3_checkpointer_versioning() {
    let mut config = get_s3_config();
    config.enable_versioning = true;

    let checkpointer = S3Checkpointer::new(config).await
        .expect("Failed to create S3 checkpointer");

    checkpointer.ensure_bucket_exists().await
        .expect("Failed to ensure bucket exists");

    // Enable versioning on bucket
    checkpointer.enable_bucket_versioning().await
        .expect("Failed to enable versioning");

    let thread_id = "test_s3_versioning";

    // Save multiple versions
    let mut version_ids = Vec::new();
    for i in 0..3 {
        let mut state = GraphState::new();
        state.set("version", json!(i));
        state.set("data", json!(format!("version_{}", i)));

        let checkpoint_id = checkpointer.save_checkpoint(thread_id, &state).await
            .expect("Failed to save checkpoint");
        version_ids.push(checkpoint_id);
    }

    // List all versions
    let versions = checkpointer.list_checkpoint_versions(thread_id, &version_ids[2]).await
        .expect("Failed to list versions");

    assert!(versions.len() >= 1); // Should have at least one version

    // Load specific version
    let state_v1 = checkpointer.load_checkpoint_version(thread_id, &version_ids[0], None).await
        .expect("Failed to load version")
        .expect("Version not found");

    assert_eq!(state_v1.get("version"), Some(&json!(0)));
}

#[tokio::test]
async fn test_s3_checkpointer_signed_url() {
    let config = get_s3_config();

    let checkpointer = S3Checkpointer::new(config).await
        .expect("Failed to create S3 checkpointer");

    checkpointer.ensure_bucket_exists().await
        .expect("Failed to ensure bucket exists");

    let thread_id = "test_s3_signed_url";
    let mut state = GraphState::new();
    state.set("signed_test", json!("data"));

    // Save checkpoint
    let checkpoint_id = checkpointer.save_checkpoint(thread_id, &state).await
        .expect("Failed to save");

    // Generate signed URL for direct access
    let signed_url = checkpointer.generate_signed_url(
        thread_id,
        &checkpoint_id,
        std::time::Duration::from_secs(3600) // 1 hour expiry
    ).await.expect("Failed to generate signed URL");

    assert!(signed_url.starts_with("https://"));

    // Verify URL contains necessary parameters
    assert!(signed_url.contains("X-Amz-Signature") || signed_url.contains("signature"));
}

#[tokio::test]
async fn test_s3_checkpointer_lifecycle_policy() {
    let config = get_s3_config();

    let checkpointer = S3Checkpointer::new(config).await
        .expect("Failed to create S3 checkpointer");

    checkpointer.ensure_bucket_exists().await
        .expect("Failed to ensure bucket exists");

    // Set lifecycle policy for automatic deletion
    let policy = S3LifecyclePolicy {
        days_until_deletion: 30,
        days_until_archive: Some(7),
        enable_intelligent_tiering: true,
    };

    checkpointer.set_lifecycle_policy(policy).await
        .expect("Failed to set lifecycle policy");

    // Verify policy was set
    let current_policy = checkpointer.get_lifecycle_policy().await
        .expect("Failed to get lifecycle policy");

    assert!(current_policy.is_some());
}

#[tokio::test]
async fn test_s3_checkpointer_batch_operations() {
    let config = get_s3_config();

    let checkpointer = S3Checkpointer::new(config).await
        .expect("Failed to create S3 checkpointer");

    checkpointer.ensure_bucket_exists().await
        .expect("Failed to ensure bucket exists");

    // Prepare multiple checkpoints
    let mut checkpoints = Vec::new();
    for i in 0..5 {
        let mut state = GraphState::new();
        state.set("batch_id", json!(i));
        checkpoints.push((format!("thread_s3_batch_{}", i), state));
    }

    // Batch save
    let ids = checkpointer.batch_save_checkpoints(checkpoints.clone()).await
        .expect("Failed to batch save");

    assert_eq!(ids.len(), 5);

    // Batch delete
    let thread_ids: Vec<String> = (0..5)
        .map(|i| format!("thread_s3_batch_{}", i))
        .collect();

    checkpointer.batch_delete_checkpoints(&thread_ids, &ids).await
        .expect("Failed to batch delete");

    // Verify deletion
    for (thread_id, checkpoint_id) in thread_ids.iter().zip(ids.iter()) {
        let exists = checkpointer.checkpoint_exists(thread_id, checkpoint_id).await
            .expect("Failed to check existence");
        assert!(!exists);
    }
}

#[tokio::test]
async fn test_s3_checkpointer_compression() {
    let mut config = get_s3_config();
    config.compression = true;

    let checkpointer = S3Checkpointer::new(config).await
        .expect("Failed to create S3 checkpointer");

    checkpointer.ensure_bucket_exists().await
        .expect("Failed to ensure bucket exists");

    let thread_id = "test_s3_compression";
    let mut state = GraphState::new();

    // Create compressible data
    let repeated_data = vec!["test_pattern"; 1000].join(" ");
    state.set("compressible", json!(repeated_data.clone()));

    // Save with compression
    let checkpoint_id = checkpointer.save_checkpoint(thread_id, &state).await
        .expect("Failed to save");

    // Get object metadata to check size
    let metadata = checkpointer.get_checkpoint_metadata(thread_id, &checkpoint_id).await
        .expect("Failed to get metadata");

    // Compressed size should be significantly smaller
    let uncompressed_size = repeated_data.len();
    assert!(metadata.size_bytes < uncompressed_size / 2);

    // Load and verify decompression
    let loaded_state = checkpointer.load_checkpoint(thread_id, Some(&checkpoint_id)).await
        .expect("Failed to load")
        .expect("Not found");

    assert_eq!(loaded_state.get("compressible"), Some(&json!(repeated_data)));
}

#[tokio::test]
async fn test_s3_checkpointer_encryption() {
    let mut config = get_s3_config();
    config.enable_encryption = true;

    let checkpointer = S3Checkpointer::new(config).await
        .expect("Failed to create S3 checkpointer");

    checkpointer.ensure_bucket_exists().await
        .expect("Failed to ensure bucket exists");

    let thread_id = "test_s3_encryption";
    let mut state = GraphState::new();
    state.set("sensitive_data", json!("encrypted_content"));

    // Save with encryption
    let checkpoint_id = checkpointer.save_checkpoint(thread_id, &state).await
        .expect("Failed to save");

    // Verify encryption was applied
    let metadata = checkpointer.get_checkpoint_metadata(thread_id, &checkpoint_id).await
        .expect("Failed to get metadata");

    assert!(metadata.server_side_encryption.is_some());

    // Load and verify decryption
    let loaded_state = checkpointer.load_checkpoint(thread_id, Some(&checkpoint_id)).await
        .expect("Failed to load")
        .expect("Not found");

    assert_eq!(loaded_state.get("sensitive_data"), Some(&json!("encrypted_content")));
}

/// Lifecycle policy configuration
#[derive(Debug, Clone)]
pub struct S3LifecyclePolicy {
    pub days_until_deletion: u32,
    pub days_until_archive: Option<u32>,
    pub enable_intelligent_tiering: bool,
}