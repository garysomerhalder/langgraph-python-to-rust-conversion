//! Human-in-the-Loop functionality for graph execution
//! YELLOW Phase: Minimal implementation to make tests pass

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{RwLock, mpsc, oneshot};
use uuid::Uuid;
use serde::{Deserialize, Serialize};
use dashmap::DashMap;
use thiserror::Error;

use crate::state::StateData;
use crate::Result;

/// Errors specific to human-in-the-loop operations
#[derive(Error, Debug)]
pub enum InterruptError {
    #[error("Interrupt timeout after {0:?}")]
    Timeout(Duration),

    #[error("Interrupt cancelled: {0}")]
    Cancelled(String),

    #[error("Invalid interrupt handle")]
    InvalidHandle,

    #[error("State modification failed: {0}")]
    StateModificationFailed(String),
}

/// Mode of interruption for a node
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum InterruptMode {
    /// Interrupt before node execution
    Before,
    /// Interrupt after node execution
    After,
    /// No interruption
    None,
}

/// Handle for an active interrupt
#[derive(Debug)]
pub struct InterruptHandle {
    /// Unique identifier for this interrupt
    pub id: Uuid,

    /// Node that triggered the interrupt
    pub node_id: String,

    /// When the interrupt was triggered
    pub timestamp: SystemTime,

    /// Snapshot of state at interrupt time
    pub state_snapshot: StateData,

    /// Optional timeout for this interrupt
    pub timeout: Option<Duration>,

    /// Channel to send approval decision
    response_tx: Option<oneshot::Sender<ApprovalDecision>>,
}

// Manual Clone implementation that doesn't clone the sender
impl Clone for InterruptHandle {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            node_id: self.node_id.clone(),
            timestamp: self.timestamp,
            state_snapshot: self.state_snapshot.clone(),
            timeout: self.timeout,
            response_tx: None,  // Don't clone the sender
        }
    }
}

impl InterruptHandle {
    /// Create a new interrupt handle
    pub fn new(node_id: String, state_snapshot: StateData, timeout: Option<Duration>) -> (Self, oneshot::Receiver<ApprovalDecision>) {
        let (tx, rx) = oneshot::channel();

        let handle = Self {
            id: Uuid::new_v4(),
            node_id,
            timestamp: SystemTime::now(),
            state_snapshot,
            timeout,
            response_tx: Some(tx),
        };

        (handle, rx)
    }

    /// Modify state during interrupt
    pub async fn modify_state(&mut self, changes: StateData) -> Result<()> {
        // Merge changes into state snapshot
        for (key, value) in changes {
            self.state_snapshot.insert(key, value);
        }
        Ok(())
    }

    /// Send approval decision
    pub fn approve(mut self, decision: ApprovalDecision) -> Result<()> {
        if let Some(tx) = self.response_tx.take() {
            tx.send(decision)
                .map_err(|_| InterruptError::InvalidHandle)?;
        }
        Ok(())
    }
}

/// Decision made during an interrupt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApprovalDecision {
    /// Continue normal execution
    Continue,

    /// Retry the current node
    Retry,

    /// Skip the current node
    Skip,

    /// Abort execution with reason
    Abort(String),

    /// Redirect to a different node
    Redirect(String),
}

/// Callback for handling interrupts
pub type InterruptCallback = Box<dyn Fn(InterruptHandle) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<ApprovalDecision>> + Send>> + Send + Sync>;

/// Manager for handling interrupts during execution
pub struct InterruptManager {
    /// Currently pending interrupts
    pending: DashMap<Uuid, InterruptHandle>,

    /// Registered callbacks for handling interrupts
    callbacks: Arc<RwLock<Vec<InterruptCallback>>>,

    /// Node interrupt configurations
    node_configs: HashMap<String, InterruptMode>,
}

impl InterruptManager {
    /// Create a new interrupt manager
    pub fn new() -> Self {
        Self {
            pending: DashMap::new(),
            callbacks: Arc::new(RwLock::new(Vec::new())),
            node_configs: HashMap::new(),
        }
    }

    /// Add an interrupt configuration for a node
    pub fn add_node_config(&mut self, node_id: String, mode: InterruptMode) {
        self.node_configs.insert(node_id, mode);
    }

    /// Register a callback for handling interrupts
    pub async fn register_callback(&self, callback: InterruptCallback) {
        let mut callbacks = self.callbacks.write().await;
        callbacks.push(callback);
    }

    /// Check if a node should interrupt
    pub fn should_interrupt(&self, node_id: &str, mode: InterruptMode) -> bool {
        self.node_configs.get(node_id)
            .map(|&config_mode| config_mode == mode)
            .unwrap_or(false)
    }

    /// Create an interrupt for a node
    pub async fn create_interrupt(
        &self,
        node_id: String,
        state: StateData,
        timeout: Option<Duration>,
    ) -> Result<ApprovalDecision> {
        let (handle, rx) = InterruptHandle::new(node_id.clone(), state, timeout);
        let handle_id = handle.id;

        // Store the pending interrupt
        self.pending.insert(handle_id, handle.clone());

        // Call all registered callbacks
        let callbacks = self.callbacks.read().await;
        if callbacks.is_empty() {
            // No callbacks registered, default to Continue
            return Ok(ApprovalDecision::Continue);
        }

        // Use the first callback (for simplicity in YELLOW phase)
        let callback = &callbacks[0];
        let future = callback(handle);

        // Handle timeout if specified
        let decision = if let Some(timeout) = timeout {
            match tokio::time::timeout(timeout, future).await {
                Ok(result) => result?,
                Err(_) => return Err(InterruptError::Timeout(timeout).into()),
            }
        } else {
            future.await?
        };

        // Remove from pending
        self.pending.remove(&handle_id);

        Ok(decision)
    }

    /// Get all pending interrupts
    pub fn get_pending(&self) -> Vec<InterruptHandle> {
        self.pending.iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Clear all pending interrupts
    pub fn clear_pending(&self) {
        self.pending.clear();
    }
}

/// Extension trait for adding human-in-the-loop to execution
#[async_trait::async_trait]
pub trait HumanInLoopExecution {
    /// Execute with interrupt support
    async fn execute_with_interrupts(
        &self,
        input: StateData,
        callback: InterruptCallback,
    ) -> Result<ExecutionHandle>;

    /// Execute with interrupts and timeout
    async fn execute_with_interrupts_and_timeout(
        &self,
        input: StateData,
        callback: InterruptCallback,
        timeout: Duration,
    ) -> Result<ExecutionHandle>;
}

/// Handle for an execution that may be interrupted
pub struct ExecutionHandle {
    /// The final result receiver
    result_rx: oneshot::Receiver<Result<StateData>>,

    /// Whether execution was interrupted
    interrupted: Arc<RwLock<bool>>,
}

impl ExecutionHandle {
    /// Create a new execution handle
    pub fn new(result_rx: oneshot::Receiver<Result<StateData>>) -> Self {
        Self {
            result_rx,
            interrupted: Arc::new(RwLock::new(false)),
        }
    }

    /// Check if execution is interrupted
    pub async fn is_interrupted(&self) -> bool {
        *self.interrupted.read().await
    }

    /// Mark as interrupted
    pub async fn mark_interrupted(&self) {
        *self.interrupted.write().await = true;
    }

    /// Wait for execution to complete
    pub async fn await_result(self) -> Result<StateData> {
        self.result_rx.await
            .map_err(|_| InterruptError::Cancelled("Execution cancelled".to_string()))?
    }
}

// Implement Future for ExecutionHandle to allow direct await
impl std::future::Future for ExecutionHandle {
    type Output = Result<StateData>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        // Poll the receiver
        match self.result_rx.try_recv() {
            Ok(result) => std::task::Poll::Ready(result),
            Err(oneshot::error::TryRecvError::Empty) => {
                cx.waker().wake_by_ref();
                std::task::Poll::Pending
            },
            Err(oneshot::error::TryRecvError::Closed) => {
                std::task::Poll::Ready(Err(InterruptError::Cancelled("Channel closed".to_string()).into()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_interrupt_handle_creation() {
        let mut state = StateData::new();
        state.insert("test".to_string(), json!("value"));
        let (handle, _rx) = InterruptHandle::new("test_node".to_string(), state.clone(), None);

        assert_eq!(handle.node_id, "test_node");
        assert!(handle.state_snapshot.contains_key("test"));
    }

    #[tokio::test]
    async fn test_interrupt_manager() {
        let mut manager = InterruptManager::new();
        manager.add_node_config("test_node".to_string(), InterruptMode::Before);

        assert!(manager.should_interrupt("test_node", InterruptMode::Before));
        assert!(!manager.should_interrupt("test_node", InterruptMode::After));
    }
}