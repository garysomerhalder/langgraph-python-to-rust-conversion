// Backup Storage Backends - File system and future cloud storage

use super::types::{Backup, BackupError, BackupFilter, BackupMetadata};
use async_trait::async_trait;
use serde_json;
use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncWriteExt;

#[async_trait]
pub trait BackupStorage: Send + Sync {
    async fn store_backup(&self, backup: &Backup) -> Result<String, BackupError>;
    async fn retrieve_backup(&self, backup_id: &str) -> Result<Backup, BackupError>;
    async fn list_backups(&self, filter: Option<BackupFilter>) -> Result<Vec<BackupMetadata>, BackupError>;
    async fn delete_backup(&self, backup_id: &str) -> Result<(), BackupError>;
}

pub struct FileBackupStorage {
    backup_directory: PathBuf,
}

impl FileBackupStorage {
    pub fn new(directory: &str) -> Self {
        Self {
            backup_directory: PathBuf::from(directory),
        }
    }

    async fn ensure_directory_exists(&self) -> Result<(), BackupError> {
        if !self.backup_directory.exists() {
            fs::create_dir_all(&self.backup_directory)
                .await
                .map_err(|e| BackupError::IoError(format!("Failed to create backup directory: {}", e)))?;
        }
        Ok(())
    }

    fn backup_path(&self, backup_id: &str) -> PathBuf {
        self.backup_directory.join(format!("{}.backup", backup_id))
    }

    fn metadata_path(&self, backup_id: &str) -> PathBuf {
        self.backup_directory.join(format!("{}.metadata", backup_id))
    }
}

#[async_trait]
impl BackupStorage for FileBackupStorage {
    async fn store_backup(&self, backup: &Backup) -> Result<String, BackupError> {
        self.ensure_directory_exists().await?;

        let backup_path = self.backup_path(&backup.id);
        let metadata_path = self.metadata_path(&backup.id);

        // Store backup data
        let mut backup_file = fs::File::create(&backup_path)
            .await
            .map_err(|e| BackupError::IoError(format!("Failed to create backup file: {}", e)))?;

        backup_file.write_all(&backup.data)
            .await
            .map_err(|e| BackupError::IoError(format!("Failed to write backup data: {}", e)))?;

        // Store metadata
        let metadata_json = serde_json::to_string_pretty(&backup.metadata)
            .map_err(|e| BackupError::SerializationError(format!("Failed to serialize metadata: {}", e)))?;

        fs::write(&metadata_path, metadata_json)
            .await
            .map_err(|e| BackupError::IoError(format!("Failed to write metadata: {}", e)))?;

        Ok(backup.id.clone())
    }

    async fn retrieve_backup(&self, backup_id: &str) -> Result<Backup, BackupError> {
        let backup_path = self.backup_path(backup_id);
        let metadata_path = self.metadata_path(backup_id);

        // Check if files exist
        if !backup_path.exists() || !metadata_path.exists() {
            return Err(BackupError::StorageError(format!("Backup {} not found", backup_id)));
        }

        // Read backup data
        let data = fs::read(&backup_path)
            .await
            .map_err(|e| BackupError::IoError(format!("Failed to read backup data: {}", e)))?;

        // Read metadata
        let metadata_json = fs::read_to_string(&metadata_path)
            .await
            .map_err(|e| BackupError::IoError(format!("Failed to read metadata: {}", e)))?;

        let metadata: BackupMetadata = serde_json::from_str(&metadata_json)
            .map_err(|e| BackupError::SerializationError(format!("Failed to deserialize metadata: {}", e)))?;

        Ok(Backup {
            id: backup_id.to_string(),
            backup_type: metadata.backup_type.clone(),
            data,
            metadata,
        })
    }

    async fn list_backups(&self, filter: Option<BackupFilter>) -> Result<Vec<BackupMetadata>, BackupError> {
        if !self.backup_directory.exists() {
            return Ok(Vec::new());
        }

        let mut entries = fs::read_dir(&self.backup_directory)
            .await
            .map_err(|e| BackupError::IoError(format!("Failed to read backup directory: {}", e)))?;

        let mut metadata_list = Vec::new();

        while let Some(entry) = entries.next_entry()
            .await
            .map_err(|e| BackupError::IoError(format!("Failed to read directory entry: {}", e)))?
        {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("metadata") {
                let metadata_json = fs::read_to_string(&path)
                    .await
                    .map_err(|e| BackupError::IoError(format!("Failed to read metadata file: {}", e)))?;

                if let Ok(metadata) = serde_json::from_str::<BackupMetadata>(&metadata_json) {
                    // Apply filter if provided
                    if let Some(ref filter) = filter {
                        let mut include = true;

                        if let Some(ref backup_type) = filter.backup_type {
                            if &metadata.backup_type != backup_type {
                                include = false;
                            }
                        }

                        if let Some(ref source_backend) = filter.source_backend {
                            if &metadata.source_backend != source_backend {
                                include = false;
                            }
                        }

                        if let Some((start, end)) = filter.date_range {
                            if metadata.created_at < start || metadata.created_at > end {
                                include = false;
                            }
                        }

                        if include {
                            metadata_list.push(metadata);
                        }
                    } else {
                        metadata_list.push(metadata);
                    }
                }
            }
        }

        // Sort by creation time (newest first)
        metadata_list.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(metadata_list)
    }

    async fn delete_backup(&self, backup_id: &str) -> Result<(), BackupError> {
        let backup_path = self.backup_path(backup_id);
        let metadata_path = self.metadata_path(backup_id);

        // Remove backup file if it exists
        if backup_path.exists() {
            fs::remove_file(&backup_path)
                .await
                .map_err(|e| BackupError::IoError(format!("Failed to delete backup file: {}", e)))?;
        }

        // Remove metadata file if it exists
        if metadata_path.exists() {
            fs::remove_file(&metadata_path)
                .await
                .map_err(|e| BackupError::IoError(format!("Failed to delete metadata file: {}", e)))?;
        }

        Ok(())
    }
}