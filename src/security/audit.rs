//! Audit logging for security events

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

/// Audit logger for security events
#[derive(Clone)]
pub struct AuditLogger {
    /// In-memory event storage (for testing/development)
    events: Arc<RwLock<Vec<AuditEvent>>>,
}

/// Audit event record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    /// Event type (e.g., "authentication_success", "permission_denied")
    pub event_type: String,

    /// User ID associated with the event
    pub user_id: String,

    /// When the event occurred
    pub timestamp: DateTime<Utc>,

    /// Additional event metadata
    pub metadata: HashMap<String, String>,
}

impl AuditLogger {
    /// Create a new audit logger
    pub fn new() -> Self {
        Self {
            events: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Log an audit event
    pub async fn log_event(
        &self,
        event_type: impl Into<String>,
        user_id: impl Into<String>,
        metadata: HashMap<String, String>,
    ) {
        let event = AuditEvent {
            event_type: event_type.into(),
            user_id: user_id.into(),
            timestamp: Utc::now(),
            metadata,
        };

        info!(
            "Audit event: {} for user {} at {}",
            event.event_type, event.user_id, event.timestamp
        );

        let mut events = self.events.write().await;
        events.push(event);
    }

    /// Get all audit events
    pub async fn get_events(&self) -> Vec<AuditEvent> {
        self.events.read().await.clone()
    }

    /// Get events for a specific user
    pub async fn get_user_events(&self, user_id: &str) -> Vec<AuditEvent> {
        self.events
            .read()
            .await
            .iter()
            .filter(|e| e.user_id == user_id)
            .cloned()
            .collect()
    }

    /// Clear all events (for testing)
    pub async fn clear(&self) {
        self.events.write().await.clear();
    }
}

impl Default for AuditLogger {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_audit_logging() {
        let logger = AuditLogger::new();

        logger.log_event(
            "test_event",
            "user123",
            HashMap::new(),
        ).await;

        let events = logger.get_events().await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "test_event");
        assert_eq!(events[0].user_id, "user123");
    }
}
