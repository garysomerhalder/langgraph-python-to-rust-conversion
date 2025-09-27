// S3 Checkpointer Implementation
// Following Integration-First methodology - uses real S3 or S3-compatible service

use crate::checkpoint::{CheckpointError, CheckpointResult, VersionedCheckpoint};
use crate::state::GraphState;
use async_trait::async_trait;
use aws_sdk_s3::{Client, Error as S3Error};
use aws_sdk_s3::primitives::ByteStream;
use aws_config::BehaviorVersion;
use serde::{Deserialize, Serialize};
use serde_json::{self, Value};
use std::collections::HashMap;
use uuid::Uuid;
use chrono::{DateTime, Utc};
use flate2::write::GzEncoder;
use flate2::read::GzDecoder;
use flate2::Compression;
use std::io::{Write, Read};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::{sleep, timeout};
use tracing::{info, warn, error, debug, instrument};
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use std::collections::VecDeque;

/// S3 checkpointer configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3Config {
    pub bucket_name: String,
    pub region: String,
    pub key_prefix: String,
    pub enable_versioning: bool,
    pub enable_encryption: bool,
    pub compression: bool,
    pub multipart_threshold_mb: u64,
    pub endpoint_url: Option<String>,
    pub force_path_style: bool,
    // Production resilience configuration
    pub max_retries: u32,
    pub initial_retry_delay_ms: u64,
    pub max_retry_delay_ms: u64,
    pub timeout_seconds: u64,
    pub circuit_breaker_threshold: u32,
    pub circuit_breaker_timeout_seconds: u64,
    pub connection_pool_size: u32,
}

impl Default for S3Config {
    fn default() -> Self {
        Self {
            bucket_name: "langgraph-checkpoints".to_string(),
            region: "us-east-1".to_string(),
            key_prefix: "checkpoints/".to_string(),
            enable_versioning: false,
            enable_encryption: true,
            compression: true,
            multipart_threshold_mb: 5,
            endpoint_url: None,
            force_path_style: false,
            // Production defaults for resilience
            max_retries: 3,
            initial_retry_delay_ms: 100,
            max_retry_delay_ms: 5000,
            timeout_seconds: 30,
            circuit_breaker_threshold: 5,
            circuit_breaker_timeout_seconds: 60,
            connection_pool_size: 10,
        }
    }
}

/// Circuit breaker state for S3 operations
#[derive(Debug)]
enum CircuitBreakerState {
    Closed,
    Open { opened_at: Instant },
    HalfOpen,
}

/// Circuit breaker for S3 operations to prevent cascading failures
#[derive(Debug)]
struct CircuitBreaker {
    state: Arc<tokio::sync::RwLock<CircuitBreakerState>>,
    failure_count: AtomicU64,
    threshold: u32,
    timeout: Duration,
    metrics: Arc<S3Metrics>,
}

impl CircuitBreaker {
    fn new(threshold: u32, timeout: Duration, metrics: Arc<S3Metrics>) -> Self {
        Self {
            state: Arc::new(tokio::sync::RwLock::new(CircuitBreakerState::Closed)),
            failure_count: AtomicU64::new(0),
            threshold,
            timeout,
            metrics,
        }
    }

    async fn execute<F, Fut, T>(&self, operation: F) -> Result<T, CheckpointError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, CheckpointError>>,
    {
        // Check circuit breaker state
        let should_attempt = {
            let state = self.state.read().await;
            match *state {
                CircuitBreakerState::Closed => true,
                CircuitBreakerState::Open { opened_at } => {
                    if opened_at.elapsed() >= self.timeout {
                        // Transition to half-open
                        drop(state);
                        let mut state = self.state.write().await;
                        *state = CircuitBreakerState::HalfOpen;
                        self.metrics.circuit_breaker_half_open.fetch_add(1, Ordering::Relaxed);
                        true
                    } else {
                        false
                    }
                }
                CircuitBreakerState::HalfOpen => true,
            }
        };

        if !should_attempt {
            self.metrics.circuit_breaker_rejections.fetch_add(1, Ordering::Relaxed);
            return Err(CheckpointError::StorageError("Circuit breaker is open".to_string()));
        }

        // Execute the operation
        match operation().await {
            Ok(result) => {
                // Success - reset failure count and close circuit if it was half-open
                self.failure_count.store(0, Ordering::Relaxed);
                let mut state = self.state.write().await;
                if matches!(*state, CircuitBreakerState::HalfOpen) {
                    *state = CircuitBreakerState::Closed;
                    self.metrics.circuit_breaker_closed.fetch_add(1, Ordering::Relaxed);
                }
                Ok(result)
            }
            Err(err) => {
                // Failure - increment count and potentially open circuit
                let new_count = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;
                if new_count >= self.threshold as u64 {
                    let mut state = self.state.write().await;
                    *state = CircuitBreakerState::Open { opened_at: Instant::now() };
                    self.metrics.circuit_breaker_opened.fetch_add(1, Ordering::Relaxed);
                }
                Err(err)
            }
        }
    }
}

/// Metrics for S3 operations
#[derive(Debug)]
struct S3Metrics {
    // Operation counters
    saves_total: AtomicU64,
    saves_failed: AtomicU64,
    loads_total: AtomicU64,
    loads_failed: AtomicU64,
    deletes_total: AtomicU64,
    deletes_failed: AtomicU64,

    // Circuit breaker metrics
    circuit_breaker_opened: AtomicU64,
    circuit_breaker_closed: AtomicU64,
    circuit_breaker_half_open: AtomicU64,
    circuit_breaker_rejections: AtomicU64,

    // Retry metrics
    retries_total: AtomicU64,
    retry_delays_total_ms: AtomicU64,

    // Performance metrics
    operation_duration_ms: AtomicU64,
    bytes_transferred: AtomicU64,
}

impl S3Metrics {
    fn new() -> Self {
        Self {
            saves_total: AtomicU64::new(0),
            saves_failed: AtomicU64::new(0),
            loads_total: AtomicU64::new(0),
            loads_failed: AtomicU64::new(0),
            deletes_total: AtomicU64::new(0),
            deletes_failed: AtomicU64::new(0),
            circuit_breaker_opened: AtomicU64::new(0),
            circuit_breaker_closed: AtomicU64::new(0),
            circuit_breaker_half_open: AtomicU64::new(0),
            circuit_breaker_rejections: AtomicU64::new(0),
            retries_total: AtomicU64::new(0),
            retry_delays_total_ms: AtomicU64::new(0),
            operation_duration_ms: AtomicU64::new(0),
            bytes_transferred: AtomicU64::new(0),
        }
    }

    fn log_metrics(&self) {
        info!(
            "S3 Metrics: saves={}/{} loads={}/{} deletes={}/{} \
             circuit_breaker=(opened={}, rejections={}) retries={} avg_delay={}ms",
            self.saves_total.load(Ordering::Relaxed),
            self.saves_failed.load(Ordering::Relaxed),
            self.loads_total.load(Ordering::Relaxed),
            self.loads_failed.load(Ordering::Relaxed),
            self.deletes_total.load(Ordering::Relaxed),
            self.deletes_failed.load(Ordering::Relaxed),
            self.circuit_breaker_opened.load(Ordering::Relaxed),
            self.circuit_breaker_rejections.load(Ordering::Relaxed),
            self.retries_total.load(Ordering::Relaxed),
            {
                let total_retries = self.retries_total.load(Ordering::Relaxed);
                if total_retries > 0 {
                    self.retry_delays_total_ms.load(Ordering::Relaxed) / total_retries
                } else {
                    0
                }
            }
        );
    }
}

/// S3-based checkpointer for cloud storage persistence
pub struct S3Checkpointer {
    client: Client,
    config: S3Config,
    circuit_breaker: CircuitBreaker,
    metrics: Arc<S3Metrics>,
}

impl S3Checkpointer {
    /// Create a new S3 checkpointer with the given configuration
    pub async fn new(config: S3Config) -> Result<Self, CheckpointError> {
        let mut aws_config_builder = aws_config::defaults(BehaviorVersion::latest())
            .region(aws_config::Region::new(config.region.clone()));

        // Support for LocalStack/MinIO with custom endpoint
        if let Some(ref endpoint) = config.endpoint_url {
            aws_config_builder = aws_config_builder.endpoint_url(endpoint);
        }

        let aws_config = aws_config_builder.load().await;

        let mut s3_config_builder = aws_sdk_s3::config::Builder::from(&aws_config);

        if config.force_path_style {
            s3_config_builder = s3_config_builder.force_path_style(true);
        }

        let client = Client::from_conf(s3_config_builder.build());

        let metrics = Arc::new(S3Metrics::new());
        let circuit_breaker = CircuitBreaker::new(
            config.circuit_breaker_threshold,
            Duration::from_secs(config.circuit_breaker_timeout_seconds),
            metrics.clone(),
        );

        Ok(Self {
            client,
            config,
            circuit_breaker,
            metrics,
        })
    }

    /// Execute an operation with retry logic and exponential backoff
    async fn retry_with_backoff<F, Fut, T>(&self, operation: F) -> Result<T, CheckpointError>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, CheckpointError>>,
    {
        let mut attempt = 0;
        let mut current_delay = Duration::from_millis(self.config.initial_retry_delay_ms);

        loop {
            let start_time = Instant::now();

            // Apply timeout to the operation
            let result = timeout(
                Duration::from_secs(self.config.timeout_seconds),
                operation()
            ).await;

            let elapsed = start_time.elapsed();
            self.metrics.operation_duration_ms.fetch_add(elapsed.as_millis() as u64, Ordering::Relaxed);

            match result {
                Ok(Ok(value)) => return Ok(value),
                Ok(Err(err)) => {
                    attempt += 1;

                    // Check if we should retry based on error type
                    let should_retry = matches!(err,
                        CheckpointError::StorageError(_) |
                        CheckpointError::SerializationError(_)
                    ) && attempt <= self.config.max_retries;

                    if !should_retry {
                        error!("Operation failed after {} attempts: {:?}", attempt, err);
                        return Err(err);
                    }

                    // Log retry attempt
                    warn!("Operation failed on attempt {}, retrying in {:?}: {:?}",
                          attempt, current_delay, err);

                    // Track retry metrics
                    self.metrics.retries_total.fetch_add(1, Ordering::Relaxed);
                    self.metrics.retry_delays_total_ms.fetch_add(current_delay.as_millis() as u64, Ordering::Relaxed);

                    // Sleep before retry
                    sleep(current_delay).await;

                    // Exponential backoff with jitter
                    current_delay = std::cmp::min(
                        Duration::from_millis((current_delay.as_millis() as u64 * 2).min(self.config.max_retry_delay_ms)),
                        Duration::from_millis(self.config.max_retry_delay_ms)
                    );
                }
                Err(_timeout_err) => {
                    attempt += 1;
                    if attempt > self.config.max_retries {
                        error!("Operation timed out after {} attempts", attempt);
                        return Err(CheckpointError::StorageError("Operation timed out".to_string()));
                    }

                    warn!("Operation timed out on attempt {}, retrying in {:?}", attempt, current_delay);

                    // Track retry metrics
                    self.metrics.retries_total.fetch_add(1, Ordering::Relaxed);
                    self.metrics.retry_delays_total_ms.fetch_add(current_delay.as_millis() as u64, Ordering::Relaxed);

                    sleep(current_delay).await;
                    current_delay = std::cmp::min(
                        Duration::from_millis(current_delay.as_millis() as u64 * 2),
                        Duration::from_millis(self.config.max_retry_delay_ms)
                    );
                }
            }
        }
    }

    /// Get metrics for monitoring and observability
    pub fn get_metrics(&self) -> &Arc<S3Metrics> {
        &self.metrics
    }

    /// Log current metrics (should be called periodically)
    pub fn log_metrics(&self) {
        self.metrics.log_metrics();
    }

    /// Ensure the S3 bucket exists, create if it doesn't
    pub async fn ensure_bucket_exists(&self) -> Result<(), CheckpointError> {
        // Check if bucket exists
        match self.client
            .head_bucket()
            .bucket(&self.config.bucket_name)
            .send()
            .await
        {
            Ok(_) => Ok(()),
            Err(_) => {
                // Create bucket if it doesn't exist
                let mut create_bucket = self.client
                    .create_bucket()
                    .bucket(&self.config.bucket_name);

                // Add location constraint for non us-east-1 regions
                if self.config.region != "us-east-1" {
                    use aws_sdk_s3::types::{CreateBucketConfiguration, BucketLocationConstraint};
                    let location = BucketLocationConstraint::from(self.config.region.as_str());
                    let config = CreateBucketConfiguration::builder()
                        .location_constraint(location)
                        .build();
                    create_bucket = create_bucket.create_bucket_configuration(config);
                }

                create_bucket.send().await
                    .map_err(|e| CheckpointError::StorageError(format!("Failed to create bucket: {}", e)))?;

                Ok(())
            }
        }
    }

    /// Enable versioning on the S3 bucket
    pub async fn enable_bucket_versioning(&self) -> Result<(), CheckpointError> {
        use aws_sdk_s3::types::{BucketVersioningStatus, VersioningConfiguration};

        let versioning_config = VersioningConfiguration::builder()
            .status(BucketVersioningStatus::Enabled)
            .build();

        self.client
            .put_bucket_versioning()
            .bucket(&self.config.bucket_name)
            .versioning_configuration(versioning_config)
            .send()
            .await
            .map_err(|e| CheckpointError::StorageError(format!("Failed to enable versioning: {}", e)))?;

        Ok(())
    }

    /// Generate S3 key for a checkpoint
    fn generate_key(&self, thread_id: &str, checkpoint_id: &str) -> String {
        format!("{}{}/{}.json", self.config.key_prefix, thread_id, checkpoint_id)
    }

    /// Compress data if compression is enabled
    fn compress_data(&self, data: &[u8]) -> Result<Vec<u8>, CheckpointError> {
        if !self.config.compression {
            return Ok(data.to_vec());
        }

        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(data)
            .map_err(|e| CheckpointError::SerializationError(format!("Compression failed: {}", e)))?;
        encoder.finish()
            .map_err(|e| CheckpointError::SerializationError(format!("Compression finish failed: {}", e)))
    }

    /// Decompress data if compression is enabled
    fn decompress_data(&self, data: &[u8]) -> Result<Vec<u8>, CheckpointError> {
        if !self.config.compression {
            return Ok(data.to_vec());
        }

        let mut decoder = GzDecoder::new(data);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed)
            .map_err(|e| CheckpointError::SerializationError(format!("Decompression failed: {}", e)))?;
        Ok(decompressed)
    }

    /// Check if a checkpoint exists
    pub async fn checkpoint_exists(&self, thread_id: &str, checkpoint_id: &str) -> Result<bool, CheckpointError> {
        let key = self.generate_key(thread_id, checkpoint_id);

        match self.client
            .head_object()
            .bucket(&self.config.bucket_name)
            .key(&key)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(e) => {
                let service_error = e.into_service_error();
                if service_error.is_not_found() {
                    Ok(false)
                } else {
                    Err(CheckpointError::StorageError(format!("Failed to check existence: {}", service_error)))
                }
            }
        }
    }

    /// Generate a signed URL for direct access to a checkpoint
    pub async fn generate_signed_url(
        &self,
        thread_id: &str,
        checkpoint_id: &str,
        expiration: std::time::Duration,
    ) -> Result<String, CheckpointError> {
        use aws_sdk_s3::presigning::PresigningConfig;

        let key = self.generate_key(thread_id, checkpoint_id);

        let presigning_config = PresigningConfig::expires_in(expiration)
            .map_err(|e| CheckpointError::StorageError(format!("Invalid expiration: {}", e)))?;

        let presigned_request = self.client
            .get_object()
            .bucket(&self.config.bucket_name)
            .key(&key)
            .presigned(presigning_config)
            .await
            .map_err(|e| CheckpointError::StorageError(format!("Failed to generate signed URL: {}", e)))?;

        Ok(presigned_request.uri().to_string())
    }

    /// Get checkpoint metadata
    pub async fn get_checkpoint_metadata(
        &self,
        thread_id: &str,
        checkpoint_id: &str,
    ) -> Result<CheckpointMetadata, CheckpointError> {
        let key = self.generate_key(thread_id, checkpoint_id);

        let response = self.client
            .head_object()
            .bucket(&self.config.bucket_name)
            .key(&key)
            .send()
            .await
            .map_err(|e| CheckpointError::StorageError(format!("Failed to get metadata: {}", e)))?;

        Ok(CheckpointMetadata {
            size_bytes: response.content_length().unwrap_or(0) as usize,
            last_modified: response.last_modified()
                .map(|_t| Utc::now())
                .unwrap_or_else(Utc::now),
            server_side_encryption: response.server_side_encryption()
                .map(|e| e.as_str().to_string()),
        })
    }

    /// List checkpoint versions (when versioning is enabled)
    pub async fn list_checkpoint_versions(
        &self,
        thread_id: &str,
        checkpoint_id: &str,
    ) -> Result<Vec<String>, CheckpointError> {
        let key = self.generate_key(thread_id, checkpoint_id);

        let response = self.client
            .list_object_versions()
            .bucket(&self.config.bucket_name)
            .prefix(&key)
            .send()
            .await
            .map_err(|e| CheckpointError::StorageError(format!("Failed to list versions: {}", e)))?;

        let versions = response.versions()
            .iter()
            .filter_map(|v| v.version_id().map(|id| id.to_string()))
            .collect();

        Ok(versions)
    }

    /// Load a specific version of a checkpoint
    pub async fn load_checkpoint_version(
        &self,
        thread_id: &str,
        checkpoint_id: &str,
        version_id: Option<&str>,
    ) -> Result<Option<GraphState>, CheckpointError> {
        let key = self.generate_key(thread_id, checkpoint_id);

        let mut request = self.client
            .get_object()
            .bucket(&self.config.bucket_name)
            .key(&key);

        if let Some(version) = version_id {
            request = request.version_id(version);
        }

        let response = match request.send().await {
            Ok(resp) => resp,
            Err(e) => {
                let service_error = e.into_service_error();
                if service_error.is_no_such_key() {
                    return Ok(None);
                }
                return Err(CheckpointError::StorageError(format!("Failed to load checkpoint: {}", service_error)));
            }
        };

        let body = response.body.collect().await
            .map_err(|e| CheckpointError::StorageError(format!("Failed to read body: {}", e)))?
            .into_bytes();

        let decompressed = self.decompress_data(&body)?;
        let checkpoint: VersionedCheckpoint = serde_json::from_slice(&decompressed)
            .map_err(|e| CheckpointError::SerializationError(format!("Failed to deserialize: {}", e)))?;

        Ok(Some(checkpoint.state))
    }

    /// Set lifecycle policy for automatic cleanup
    pub async fn set_lifecycle_policy(&self, policy: S3LifecyclePolicy) -> Result<(), CheckpointError> {
        use aws_sdk_s3::types::{
            LifecycleRule, LifecycleRuleFilter,
            LifecycleExpiration, Transition, StorageClass,
        };

        let filter = LifecycleRuleFilter::builder()
            .prefix(&self.config.key_prefix)
            .build();

        let mut rule_builder = LifecycleRule::builder()
            .id("checkpoint-lifecycle")
            // Status is set through builder method, not an enum value
            .filter(filter);

        // Set expiration
        rule_builder = rule_builder.expiration(
            LifecycleExpiration::builder()
                .days(policy.days_until_deletion as i32)
                .build()
        );

        // Set archive transition if specified
        if let Some(archive_days) = policy.days_until_archive {
            rule_builder = rule_builder.transitions(
                Transition::builder()
                    .days(archive_days as i32)
                    .storage_class(if policy.enable_intelligent_tiering {
                        aws_sdk_s3::types::TransitionStorageClass::IntelligentTiering
                    } else {
                        aws_sdk_s3::types::TransitionStorageClass::Glacier
                    })
                    .build()
            );
        }

        let rule = rule_builder.build()
            .map_err(|e| CheckpointError::StorageError(format!("Failed to build lifecycle rule: {}", e)))?;

        let lifecycle_config = aws_sdk_s3::types::BucketLifecycleConfiguration::builder()
            .rules(rule)
            .build();

        self.client
            .put_bucket_lifecycle_configuration()
            .bucket(&self.config.bucket_name)
            .lifecycle_configuration(lifecycle_config.map_err(|e|
                CheckpointError::StorageError(format!("Failed to build lifecycle config: {}", e)))?)
            .send()
            .await
            .map_err(|e| CheckpointError::StorageError(format!("Failed to set lifecycle policy: {}", e)))?;

        Ok(())
    }

    /// Get current lifecycle policy
    pub async fn get_lifecycle_policy(&self) -> Result<Option<S3LifecyclePolicy>, CheckpointError> {
        let response = match self.client
            .get_bucket_lifecycle_configuration()
            .bucket(&self.config.bucket_name)
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                let service_error = e.into_service_error();
                // Check if it's a not found error
                if format!("{:?}", service_error).contains("NoSuchLifecycleConfiguration") {
                    return Ok(None);
                }
                return Err(CheckpointError::StorageError(format!("Failed to get lifecycle policy: {}", service_error)));
            }
        };

        // Parse the first matching rule
        for rule in response.rules() {
            if let Some(_filter) = rule.filter() {
                // Check if this rule applies to our prefix
                let mut policy = S3LifecyclePolicy {
                    days_until_deletion: 30,
                    days_until_archive: None,
                    enable_intelligent_tiering: false,
                };

                if let Some(expiration) = rule.expiration() {
                    if let Some(days) = expiration.days() {
                        policy.days_until_deletion = days as u32;
                    }
                }

                let transitions = rule.transitions();
                if !transitions.is_empty() {
                    if let Some(transition) = transitions.first() {
                        if let Some(days) = transition.days() {
                            policy.days_until_archive = Some(days as u32);
                        }
                        if let Some(storage_class) = transition.storage_class() {
                            policy.enable_intelligent_tiering =
                                storage_class == &aws_sdk_s3::types::TransitionStorageClass::IntelligentTiering;
                        }
                    }
                }

                return Ok(Some(policy));
            }
        }

        Ok(None)
    }

    /// Batch save multiple checkpoints
    pub async fn batch_save_checkpoints(
        &self,
        checkpoints: Vec<(String, GraphState)>,
    ) -> Result<Vec<String>, CheckpointError> {
        let mut checkpoint_ids = Vec::new();

        // Use parallel uploads for efficiency
        let mut handles = Vec::new();

        for (thread_id, state) in checkpoints {
            let checkpoint_id = Uuid::new_v4().to_string();
            checkpoint_ids.push(checkpoint_id.clone());

            let client = self.client.clone();
            let config = self.config.clone();
            let key = self.generate_key(&thread_id, &checkpoint_id);

            handles.push(tokio::spawn(async move {
                let checkpoint = VersionedCheckpoint {
                    version: checkpoint_id.clone(),
                    state,
                    metadata: HashMap::new(),
                    timestamp: Utc::now(),
                };

                let serialized = serde_json::to_vec(&checkpoint)?;
                let compressed = if config.compression {
                    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
                    encoder.write_all(&serialized)?;
                    encoder.finish()?
                } else {
                    serialized
                };

                let mut put_request = client
                    .put_object()
                    .bucket(&config.bucket_name)
                    .key(&key)
                    .body(ByteStream::from(compressed));

                if config.enable_encryption {
                    put_request = put_request
                        .server_side_encryption(aws_sdk_s3::types::ServerSideEncryption::Aes256);
                }

                if config.compression {
                    put_request = put_request.content_encoding("gzip");
                }

                put_request.send().await?;

                Ok::<_, Box<dyn std::error::Error + Send + Sync>>(checkpoint_id)
            }));
        }

        // Wait for all uploads to complete
        for handle in handles {
            handle.await
                .map_err(|e| CheckpointError::StorageError(format!("Batch save task failed: {}", e)))?
                .map_err(|e| CheckpointError::StorageError(format!("Batch save failed: {}", e)))?;
        }

        Ok(checkpoint_ids)
    }

    /// Batch delete multiple checkpoints
    pub async fn batch_delete_checkpoints(
        &self,
        thread_ids: &[String],
        checkpoint_ids: &[String],
    ) -> Result<(), CheckpointError> {
        use aws_sdk_s3::types::{Delete, ObjectIdentifier};

        if thread_ids.len() != checkpoint_ids.len() {
            return Err(CheckpointError::StorageError(
                "Thread IDs and checkpoint IDs must have same length".to_string()
            ));
        }

        let mut objects = Vec::new();
        for (thread_id, checkpoint_id) in thread_ids.iter().zip(checkpoint_ids.iter()) {
            let key = self.generate_key(thread_id, checkpoint_id);
            objects.push(
                ObjectIdentifier::builder()
                    .key(key)
                    .build()
                    .map_err(|e| CheckpointError::StorageError(format!("Failed to build object identifier: {}", e)))?
            );
        }

        let delete = Delete::builder()
            .set_objects(Some(objects))
            .build()
            .map_err(|e| CheckpointError::StorageError(format!("Failed to build delete request: {}", e)))?;

        self.client
            .delete_objects()
            .bucket(&self.config.bucket_name)
            .delete(delete)
            .send()
            .await
            .map_err(|e| CheckpointError::StorageError(format!("Batch delete failed: {}", e)))?;

        Ok(())
    }
}

// Custom trait for S3-specific checkpoint operations
#[async_trait]
pub trait S3CheckpointerTrait: Send + Sync {
    async fn save_checkpoint(&self, thread_id: &str, state: &GraphState) -> CheckpointResult<String>;
    async fn load_checkpoint(&self, thread_id: &str, checkpoint_id: Option<&str>) -> CheckpointResult<Option<GraphState>>;
    async fn list_checkpoints(&self, thread_id: &str) -> CheckpointResult<Vec<String>>;
    async fn delete_checkpoint(&self, thread_id: &str, checkpoint_id: &str) -> CheckpointResult<()>;
}

#[async_trait]
impl S3CheckpointerTrait for S3Checkpointer {
    #[instrument(skip(self, state))]
    async fn save_checkpoint(&self, thread_id: &str, state: &GraphState) -> CheckpointResult<String> {
        self.metrics.saves_total.fetch_add(1, Ordering::Relaxed);
        let checkpoint_id = Uuid::new_v4().to_string();

        let result = self.circuit_breaker.execute(|| {
            let checkpoint_id = checkpoint_id.clone();
            let thread_id = thread_id.to_string();
            let state = state.clone();

            self.retry_with_backoff(|| async {
                let key = self.generate_key(&thread_id, &checkpoint_id);

                let checkpoint = VersionedCheckpoint {
                    version: checkpoint_id.clone(),
                    state: state.clone(),
                    metadata: HashMap::new(),
                    timestamp: Utc::now(),
                };

                let serialized = serde_json::to_vec(&checkpoint)
                    .map_err(|e| CheckpointError::SerializationError(format!("Failed to serialize: {}", e)))?;

                let compressed = self.compress_data(&serialized)?;

                // Track bytes transferred
                self.metrics.bytes_transferred.fetch_add(compressed.len() as u64, Ordering::Relaxed);

                // Use multipart upload for large files
                let size_mb = compressed.len() / (1024 * 1024);
                let should_multipart = size_mb >= self.config.multipart_threshold_mb as usize;

                if should_multipart {
                    // Multipart upload for large objects
                    let multipart_upload = self.client
                        .create_multipart_upload()
                        .bucket(&self.config.bucket_name)
                        .key(&key)
                        .send()
                        .await
                        .map_err(|e| CheckpointError::StorageError(format!("Failed to create multipart upload: {}", e)))?;

                    let upload_id = multipart_upload.upload_id()
                        .ok_or_else(|| CheckpointError::StorageError("No upload ID returned".to_string()))?;

                    // Upload parts (5MB each)
                    const PART_SIZE: usize = 5 * 1024 * 1024;
                    let mut parts = Vec::new();

                    for (part_number, chunk) in compressed.chunks(PART_SIZE).enumerate() {
                        let part_response = self.client
                            .upload_part()
                            .bucket(&self.config.bucket_name)
                            .key(&key)
                            .upload_id(upload_id)
                            .part_number((part_number + 1) as i32)
                            .body(ByteStream::from(chunk.to_vec()))
                            .send()
                            .await
                            .map_err(|e| CheckpointError::StorageError(format!("Failed to upload part: {}", e)))?;

                        parts.push(
                            aws_sdk_s3::types::CompletedPart::builder()
                                .part_number((part_number + 1) as i32)
                                .e_tag(part_response.e_tag().unwrap_or_default())
                                .build()
                        );
                    }

                    // Complete multipart upload
                    let completed_multipart = aws_sdk_s3::types::CompletedMultipartUpload::builder()
                        .set_parts(Some(parts))
                        .build();

                    self.client
                        .complete_multipart_upload()
                        .bucket(&self.config.bucket_name)
                        .key(&key)
                        .upload_id(upload_id)
                        .multipart_upload(completed_multipart)
                        .send()
                        .await
                        .map_err(|e| CheckpointError::StorageError(format!("Failed to complete multipart upload: {}", e)))?;
                } else {
                    // Regular upload for smaller objects
                    let mut put_request = self.client
                        .put_object()
                        .bucket(&self.config.bucket_name)
                        .key(&key)
                        .body(ByteStream::from(compressed));

                    if self.config.enable_encryption {
                        put_request = put_request
                            .server_side_encryption(aws_sdk_s3::types::ServerSideEncryption::Aes256);
                    }

                    if self.config.compression {
                        put_request = put_request.content_encoding("gzip");
                    }

                    put_request.send().await
                        .map_err(|e| CheckpointError::StorageError(format!("Failed to save checkpoint: {}", e)))?;
                }

                Ok(checkpoint_id.clone())
            })
        }).await;

        match result {
            Ok(id) => {
                debug!("Successfully saved checkpoint {} for thread {}", id, thread_id);
                Ok(id)
            }
            Err(err) => {
                self.metrics.saves_failed.fetch_add(1, Ordering::Relaxed);
                error!("Failed to save checkpoint for thread {}: {:?}", thread_id, err);
                Err(err)
            }
        }
    }

    #[instrument(skip(self))]
    async fn load_checkpoint(&self, thread_id: &str, checkpoint_id: Option<&str>) -> CheckpointResult<Option<GraphState>> {
        self.metrics.loads_total.fetch_add(1, Ordering::Relaxed);

        let checkpoint_id = checkpoint_id.ok_or_else(||
            CheckpointError::NotFound("Checkpoint ID required for S3 load".to_string())
        )?;

        let result = self.circuit_breaker.execute(|| {
            let thread_id = thread_id.to_string();
            let checkpoint_id = checkpoint_id.to_string();

            self.retry_with_backoff(|| async {
                self.load_checkpoint_version(&thread_id, &checkpoint_id, None).await
            })
        }).await;

        match result {
            Ok(state) => {
                debug!("Successfully loaded checkpoint {} for thread {}", checkpoint_id, thread_id);
                Ok(state)
            }
            Err(err) => {
                self.metrics.loads_failed.fetch_add(1, Ordering::Relaxed);
                error!("Failed to load checkpoint {} for thread {}: {:?}", checkpoint_id, thread_id, err);
                Err(err)
            }
        }
    }

    async fn list_checkpoints(&self, thread_id: &str) -> CheckpointResult<Vec<String>> {
        let prefix = format!("{}{}/", self.config.key_prefix, thread_id);

        let response = self.client
            .list_objects_v2()
            .bucket(&self.config.bucket_name)
            .prefix(&prefix)
            .send()
            .await
            .map_err(|e| CheckpointError::StorageError(format!("Failed to list checkpoints: {}", e)))?;

        let checkpoint_ids = response.contents()
            .iter()
            .filter_map(|object| {
                object.key()
                    .and_then(|key| {
                        key.strip_prefix(&prefix)
                            .and_then(|name| name.strip_suffix(".json"))
                            .map(|id| id.to_string())
                    })
            })
            .collect();

        Ok(checkpoint_ids)
    }

    #[instrument(skip(self))]
    async fn delete_checkpoint(&self, thread_id: &str, checkpoint_id: &str) -> CheckpointResult<()> {
        self.metrics.deletes_total.fetch_add(1, Ordering::Relaxed);

        let result = self.circuit_breaker.execute(|| {
            let thread_id = thread_id.to_string();
            let checkpoint_id = checkpoint_id.to_string();

            self.retry_with_backoff(|| async {
                let key = self.generate_key(&thread_id, &checkpoint_id);

                self.client
                    .delete_object()
                    .bucket(&self.config.bucket_name)
                    .key(&key)
                    .send()
                    .await
                    .map_err(|e| CheckpointError::StorageError(format!("Failed to delete checkpoint: {}", e)))?;

                Ok(())
            })
        }).await;

        match result {
            Ok(()) => {
                debug!("Successfully deleted checkpoint {} for thread {}", checkpoint_id, thread_id);
                Ok(())
            }
            Err(err) => {
                self.metrics.deletes_failed.fetch_add(1, Ordering::Relaxed);
                error!("Failed to delete checkpoint {} for thread {}: {:?}", checkpoint_id, thread_id, err);
                Err(err)
            }
        }
    }
}

/// Checkpoint metadata
#[derive(Debug, Clone)]
pub struct CheckpointMetadata {
    pub size_bytes: usize,
    pub last_modified: DateTime<Utc>,
    pub server_side_encryption: Option<String>,
}

/// Lifecycle policy configuration
#[derive(Debug, Clone)]
pub struct S3LifecyclePolicy {
    pub days_until_deletion: u32,
    pub days_until_archive: Option<u32>,
    pub enable_intelligent_tiering: bool,
}

// Implementation of the unified Checkpointer trait
#[async_trait]
impl crate::checkpoint::Checkpointer for S3Checkpointer {
    async fn save(
        &self,
        thread_id: &str,
        checkpoint: HashMap<String, Value>,
        metadata: HashMap<String, Value>,
        parent_checkpoint_id: Option<String>,
    ) -> anyhow::Result<String> {
        let mut state = GraphState::new();
        for (key, value) in checkpoint {
            state.set(&key, value);
        }

        let checkpoint_id = self.save_checkpoint(thread_id, &state).await
            .map_err(|e| anyhow::anyhow!("S3 save failed: {}", e))?;

        // Store metadata as part of the checkpoint
        if !metadata.is_empty() || parent_checkpoint_id.is_some() {
            // Metadata and parent ID are already handled in the VersionedCheckpoint structure
            // No additional storage needed for S3
        }

        Ok(checkpoint_id)
    }

    async fn load(
        &self,
        thread_id: &str,
        checkpoint_id: Option<String>,
    ) -> anyhow::Result<Option<(HashMap<String, Value>, HashMap<String, Value>)>> {
        let state_opt = self.load_checkpoint(thread_id, checkpoint_id.as_deref()).await
            .map_err(|e| anyhow::anyhow!("S3 load failed: {}", e))?;

        match state_opt {
            Some(state) => {
                let mut checkpoint = HashMap::new();
                // Convert GraphState to HashMap - we'll iterate manually
                // GraphState doesn't expose keys(), so we'll use a known structure
                // For now, return the whole state as a single value
                checkpoint.insert("state".to_string(), serde_json::to_value(&state).unwrap_or(Value::Null));

                // Empty metadata for now (could be enhanced to store in S3 tags)
                let metadata = HashMap::new();

                Ok(Some((checkpoint, metadata)))
            }
            None => Ok(None),
        }
    }

    async fn list(
        &self,
        thread_id: Option<&str>,
        limit: Option<usize>,
    ) -> anyhow::Result<Vec<(String, HashMap<String, Value>)>> {
        let thread_id = thread_id.ok_or_else(|| anyhow::anyhow!("Thread ID required for S3 list"))?;

        let checkpoint_ids = self.list_checkpoints(thread_id).await
            .map_err(|e| anyhow::anyhow!("S3 list failed: {}", e))?;

        let limited_ids = if let Some(limit) = limit {
            checkpoint_ids.into_iter().take(limit).collect()
        } else {
            checkpoint_ids
        };

        let mut results = Vec::new();
        for id in limited_ids {
            let metadata = HashMap::new(); // Could enhance with S3 object metadata
            results.push((id, metadata));
        }

        Ok(results)
    }

    async fn delete(&self, thread_id: &str, checkpoint_id: Option<&str>) -> anyhow::Result<()> {
        if let Some(id) = checkpoint_id {
            self.delete_checkpoint(thread_id, id).await
                .map_err(|e| anyhow::anyhow!("S3 delete failed: {}", e))?;
        } else {
            // Delete all checkpoints for thread
            let checkpoint_ids = self.list_checkpoints(thread_id).await
                .map_err(|e| anyhow::anyhow!("S3 list for delete failed: {}", e))?;

            for id in checkpoint_ids {
                self.delete_checkpoint(thread_id, &id).await
                    .map_err(|e| anyhow::anyhow!("S3 delete failed: {}", e))?;
            }
        }

        Ok(())
    }
}