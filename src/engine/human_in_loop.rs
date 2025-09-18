use crate::state::GraphState;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use thiserror::Error;
use tokio::sync::{oneshot, RwLock};
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum HumanInLoopError {
    #[error("Interrupt handle not found: {0}")]
    HandleNotFound(Uuid),

    #[error("Interrupt timeout after {0:?}")]
    Timeout(Duration),

    #[error("Execution aborted: {0}")]
    Aborted(String),

    #[error("Invalid redirect target: {0}")]
    InvalidRedirect(String),

    #[error("State modification failed: {0}")]
    StateModificationError(String),
}

pub type Result<T> = std::result::Result<T, HumanInLoopError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InterruptMode {
    Before,
    After,
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApprovalDecision {
    Continue,
    Retry,
    Skip,
    Abort(String),
    Redirect(String),
}

#[derive(Debug, Clone)]
pub struct InterruptHandle {
    pub id: Uuid,
    pub node_id: String,
    pub timestamp: SystemTime,
    pub state_snapshot: HashMap<String, serde_json::Value>,
    pub timeout: Option<Duration>,
    pub mode: InterruptMode,
}

type InterruptCallback = Box<dyn Fn(&InterruptHandle) + Send + Sync>;

pub struct InterruptManager {
    pending: DashMap<Uuid, InterruptHandle>,
    callbacks: Arc<RwLock<HashMap<String, InterruptCallback>>>,
    waiting_receivers: Arc<RwLock<Vec<oneshot::Sender<InterruptHandle>>>>,
    default_timeout: Arc<RwLock<Option<Duration>>>,
}

impl InterruptManager {
    pub fn new() -> Self {
        Self {
            pending: DashMap::new(),
            callbacks: Arc::new(RwLock::new(HashMap::new())),
            waiting_receivers: Arc::new(RwLock::new(Vec::new())),
            default_timeout: Arc::new(RwLock::new(Some(Duration::from_secs(300)))),
        }
    }

    pub async fn create_interrupt(
        &self,
        node_id: String,
        state: &GraphState,
        mode: InterruptMode,
    ) -> InterruptHandle {
        let handle = InterruptHandle {
            id: Uuid::new_v4(),
            node_id,
            timestamp: SystemTime::now(),
            state_snapshot: self.snapshot_state(state).await,
            timeout: *self.default_timeout.read().await,
            mode,
        };

        self.pending.insert(handle.id, handle.clone());

        // Notify any waiting receivers
        let mut receivers = self.waiting_receivers.write().await;
        if let Some(receiver) = receivers.pop() {
            let _ = receiver.send(handle.clone());
        }

        // Call any registered callbacks
        let callbacks = self.callbacks.read().await;
        if let Some(callback) = callbacks.get(&handle.node_id) {
            callback(&handle);
        }

        handle
    }

    pub async fn wait_for_interrupt(&self) -> Option<InterruptHandle> {
        // Check if there's already a pending interrupt
        if let Some(entry) = self.pending.iter().next() {
            return Some(entry.value().clone());
        }

        // Wait for next interrupt
        let (tx, rx) = oneshot::channel();
        self.waiting_receivers.write().await.push(tx);
        rx.await.ok()
    }

    pub async fn approve(&self, handle_id: Uuid, decision: ApprovalDecision) -> Result<()> {
        let _handle = self.pending.remove(&handle_id)
            .ok_or(HumanInLoopError::HandleNotFound(handle_id))?;

        match decision {
            ApprovalDecision::Abort(reason) => {
                return Err(HumanInLoopError::Aborted(reason));
            }
            _ => {
                // Decision is stored and will be used by executor
                Ok(())
            }
        }
    }

    pub async fn modify_and_approve(
        &self,
        handle_id: Uuid,
        modified_state: HashMap<String, serde_json::Value>,
        decision: ApprovalDecision,
    ) -> Result<()> {
        let mut handle = self.pending.get_mut(&handle_id)
            .ok_or(HumanInLoopError::HandleNotFound(handle_id))?;

        handle.state_snapshot = modified_state;
        drop(handle); // Release the lock

        self.approve(handle_id, decision).await
    }

    pub async fn set_default_timeout(&self, timeout: Duration) {
        *self.default_timeout.write().await = Some(timeout);
    }

    async fn snapshot_state(&self, state: &GraphState) -> HashMap<String, serde_json::Value> {
        // Clone the values HashMap directly
        state.values.clone()
    }

    pub async fn register_callback(&self, node_id: String, callback: InterruptCallback) {
        self.callbacks.write().await.insert(node_id, callback);
    }

    pub fn get_pending_count(&self) -> usize {
        self.pending.len()
    }

    pub async fn clear_pending(&self) {
        self.pending.clear();
    }
}

#[async_trait::async_trait]
pub trait HumanInLoop: Send + Sync {
    async fn interrupt(
        &self,
        node: &str,
        state: &GraphState,
        mode: InterruptMode,
    ) -> InterruptHandle;

    async fn approve(&self, handle: InterruptHandle) -> Result<ApprovalDecision>;

    async fn reject(&self, handle: InterruptHandle, reason: &str) -> Result<()>;

    async fn modify_state(
        &self,
        handle: InterruptHandle,
        changes: HashMap<String, serde_json::Value>,
    ) -> Result<()>;
}

impl Default for InterruptManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_interrupt_manager_creation() {
        let manager = InterruptManager::new();
        assert_eq!(manager.get_pending_count(), 0);
    }

    #[tokio::test]
    async fn test_create_and_approve_interrupt() {
        let manager = InterruptManager::new();
        let state = GraphState::new();
        state.set("test_key", "test_value".into());

        let handle = manager.create_interrupt(
            "test_node".to_string(),
            &state,
            InterruptMode::Before,
        ).await;

        assert_eq!(handle.node_id, "test_node");
        assert_eq!(manager.get_pending_count(), 1);

        manager.approve(handle.id, ApprovalDecision::Continue).await.unwrap();
        assert_eq!(manager.get_pending_count(), 0);
    }
}