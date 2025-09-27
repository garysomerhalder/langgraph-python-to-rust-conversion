// Backup and Recovery System - Minimal Implementation (YELLOW Phase)
// Production-grade backup management for all checkpoint backends

use crate::checkpoint::Checkpointer;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::fs;
use uuid::Uuid;
use anyhow::Result;

pub mod manager;
pub mod storage;
pub mod types;

pub use manager::BackupManager;
pub use storage::{BackupStorage, FileBackupStorage};
pub use types::{
    Backup, BackupError, BackupFilter, BackupMetadata, BackupType, CleanupResult,
    RestoreResult, RetentionPolicy, VerificationResult,
};

// Re-export for convenience
pub use manager::*;
pub use storage::*;
pub use types::*;