// Backup System Types - Core data structures and enums

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Backup {
    pub id: String,
    pub backup_type: BackupType,
    pub data: Vec<u8>,
    pub metadata: BackupMetadata,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BackupType {
    Full,
    Incremental,
    Differential,
    DisasterRecovery,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupMetadata {
    pub backup_type: BackupType,
    pub source_backend: String,
    pub created_at: u64,
    pub size_bytes: u64,
    pub checksum: String,
    pub compression_ratio: f64,
    pub name: Option<String>,
    pub parent_backup_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RestoreResult {
    pub restored_checkpoints: usize,
    pub restored_threads: usize,
    pub cross_backend_migration: bool,
    pub source_backend_type: String,
    pub target_backend_type: String,
    pub recovery_time_seconds: f64,
    pub data_integrity_verified: bool,
}

#[derive(Debug, Clone)]
pub struct RetentionPolicy {
    pub max_backups: usize,
    pub max_age_days: usize,
    pub cleanup_interval: Duration,
}

#[derive(Debug, Clone)]
pub struct CleanupResult {
    pub deleted_backups: usize,
    pub retained_backups: usize,
}

#[derive(Debug, Clone)]
pub struct VerificationResult {
    pub is_valid: bool,
    pub checksum_match: bool,
    pub corruption_errors: Vec<String>,
    pub metadata_valid: bool,
    pub data_integrity_check: bool,
    pub metadata_integrity_check: bool,
    pub compression_integrity_check: bool,
}

#[derive(Debug, Clone)]
pub struct BackupFilter {
    pub backup_type: Option<BackupType>,
    pub date_range: Option<(u64, u64)>,
    pub source_backend: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    #[error("Storage error: {0}")]
    StorageError(String),
    #[error("Compression error: {0}")]
    CompressionError(String),
    #[error("Encryption error: {0}")]
    EncryptionError(String),
    #[error("Verification error: {0}")]
    VerificationError(String),
    #[error("Restore error: {0}")]
    RestoreError(String),
    #[error("IO error: {0}")]
    IoError(String),
    #[error("Serialization error: {0}")]
    SerializationError(String),
}

impl RetentionPolicy {
    pub fn new() -> Self {
        Self {
            max_backups: 10,
            max_age_days: 30,
            cleanup_interval: Duration::from_secs(3600), // 1 hour
        }
    }

    pub fn max_backups(mut self, count: usize) -> Self {
        self.max_backups = count;
        self
    }

    pub fn max_age_days(mut self, days: usize) -> Self {
        self.max_age_days = days;
        self
    }

    pub fn cleanup_interval(mut self, interval: Duration) -> Self {
        self.cleanup_interval = interval;
        self
    }
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self::new()
    }
}