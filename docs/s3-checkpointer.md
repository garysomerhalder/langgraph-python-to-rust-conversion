# S3 Checkpointer Documentation

## Overview
The S3 Checkpointer provides cloud-based persistence for LangGraph state using Amazon S3 or S3-compatible storage services.

## Features
- ✅ Full Checkpointer trait implementation
- ✅ AWS SDK v2 integration
- ✅ S3-compatible API support (MinIO, LocalStack)
- ✅ Compression with gzip
- ✅ Multipart upload for large states (>5MB)
- ✅ Versioning support
- ✅ Lifecycle policies for automatic cleanup
- ✅ Signed URL generation for direct access
- ✅ Server-side encryption (AES256)
- ✅ Batch operations for efficiency

## Configuration
```rust
use langgraph::checkpoint::{S3Checkpointer, S3Config};

let config = S3Config {
    bucket_name: "langgraph-checkpoints".to_string(),
    region: "us-east-1".to_string(),
    key_prefix: "checkpoints/".to_string(),
    enable_versioning: true,
    enable_encryption: true,
    compression: true,
    multipart_threshold_mb: 5,
    endpoint_url: None, // Or Some("http://localhost:4566") for LocalStack
    force_path_style: false,
};

let checkpointer = S3Checkpointer::new(config).await?;
```

## Environment Variables
- `S3_BUCKET_NAME` - S3 bucket name (default: "langgraph-test")
- `AWS_REGION` - AWS region (default: "us-east-1")
- `S3_ENDPOINT_URL` - Optional endpoint for S3-compatible services
- `S3_FORCE_PATH_STYLE` - Use path-style URLs (for MinIO/LocalStack)

## Usage Examples

### Basic Save and Load
```rust
use langgraph::state::GraphState;

// Create state
let mut state = GraphState::new();
state.set("key", json!("value"));

// Save checkpoint
let checkpoint_id = checkpointer.save_checkpoint("thread_123", &state).await?;

// Load checkpoint
let loaded_state = checkpointer.load_checkpoint("thread_123", Some(&checkpoint_id)).await?
    .expect("Checkpoint not found");
```

### Versioning
```rust
// Enable versioning on bucket
checkpointer.enable_bucket_versioning().await?;

// List versions
let versions = checkpointer.list_checkpoint_versions("thread_123", &checkpoint_id).await?;

// Load specific version
let state = checkpointer.load_checkpoint_version("thread_123", &checkpoint_id, Some(&version_id)).await?;
```

### Lifecycle Policies
```rust
use langgraph::checkpoint::S3LifecyclePolicy;

let policy = S3LifecyclePolicy {
    days_until_deletion: 30,
    days_until_archive: Some(7),
    enable_intelligent_tiering: true,
};

checkpointer.set_lifecycle_policy(policy).await?;
```

### Signed URLs
```rust
use std::time::Duration;

let signed_url = checkpointer.generate_signed_url(
    "thread_123",
    &checkpoint_id,
    Duration::from_secs(3600), // 1 hour expiry
).await?;
```

### Batch Operations
```rust
// Batch save
let checkpoints = vec![
    ("thread_1".to_string(), state1),
    ("thread_2".to_string(), state2),
];
let ids = checkpointer.batch_save_checkpoints(checkpoints).await?;

// Batch delete
let thread_ids = vec!["thread_1".to_string(), "thread_2".to_string()];
checkpointer.batch_delete_checkpoints(&thread_ids, &ids).await?;
```

## Testing with LocalStack
```bash
# Start LocalStack
docker run -d \
  --name localstack \
  -p 4566:4566 \
  -e SERVICES=s3 \
  localstack/localstack

# Set environment variables
export S3_ENDPOINT_URL=http://localhost:4566
export S3_FORCE_PATH_STYLE=1
export AWS_ACCESS_KEY_ID=test
export AWS_SECRET_ACCESS_KEY=test

# Run tests
cargo test s3_checkpointer
```

## Production Considerations
- Use IAM roles/policies for authentication
- Enable versioning for critical applications
- Set up lifecycle policies for cost optimization
- Consider cross-region replication for HA
- Monitor with CloudWatch metrics
- Use Transfer Acceleration for global users
- Implement client-side encryption for sensitive data

## Status
**Current Phase:** YELLOW - Minimal implementation complete
**Next Steps:** GREEN phase hardening with retry logic, circuit breakers, and enhanced error handling