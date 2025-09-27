// Distributed Checkpointer Implementation
// YELLOW PHASE: Simplified implementation without etcd (GREEN phase will add full etcd support)
// Following Integration-First methodology - simulates distributed behavior for testing

use crate::checkpoint::{CheckpointError, CheckpointResult, Checkpointer};
use crate::state::GraphState;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{self, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, RwLock, Mutex};
use tokio::time::{sleep, timeout};
use tracing::{info, warn, error, debug, instrument};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use std::sync::atomic::{AtomicBool, Ordering};

/// Configuration for distributed checkpointer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributedConfig {
    pub node_id: String,
    pub etcd_endpoints: Vec<String>,
    pub cluster_name: String,
    pub key_prefix: String,
    pub consensus_timeout: Duration,
    pub sync_interval: Duration,
    pub lock_timeout: Duration,
    pub enable_leader_election: bool,
    pub enable_conflict_resolution: bool,
}

impl Default for DistributedConfig {
    fn default() -> Self {
        Self {
            node_id: format!("node_{}", Uuid::new_v4()),
            etcd_endpoints: vec!["localhost:2379".to_string()],
            cluster_name: "langgraph".to_string(),
            key_prefix: "checkpoints/".to_string(),
            consensus_timeout: Duration::from_secs(5),
            sync_interval: Duration::from_millis(100),
            lock_timeout: Duration::from_secs(10),
            enable_leader_election: true,
            enable_conflict_resolution: true,
        }
    }
}

/// Events broadcast across the distributed cluster
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StateEvent {
    CheckpointSaved { thread_id: String, checkpoint_id: String, node_id: String },
    CheckpointDeleted { thread_id: String, checkpoint_id: String, node_id: String },
    NodeJoined { node_id: String },
    NodeLeft { node_id: String },
    LeaderChanged { old_leader: Option<String>, new_leader: String },
}

/// Node information for cluster membership
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub node_id: String,
    pub joined_at: DateTime<Utc>,
    pub last_heartbeat: DateTime<Utc>,
    pub is_leader: bool,
    pub version: String,
}

/// Performance metrics for distributed operations
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    pub average_save_latency: Duration,
    pub average_sync_latency: Duration,
    pub throughput_ops_per_second: f64,
    pub sync_success_rate: f64,
    pub leader_election_time: Duration,
}

/// Distributed lock structure
#[derive(Debug, Clone)]
pub struct DistributedLock {
    pub key: String,
    pub node_id: String,
    pub lease_id: u64,
    pub acquired_at: DateTime<Utc>,
}

/// Distributed lock manager using in-memory simulation (YELLOW phase)
#[derive(Debug)]
struct DistributedLockManager {
    locks: Arc<RwLock<HashMap<String, DistributedLock>>>,
    lock_prefix: String,
    lease_ttl: Duration,
}

impl DistributedLockManager {
    fn new(etcd_client: EtcdClient, lock_prefix: String, lease_ttl: Duration) -> Self {
        Self {
            etcd_client,
            lock_prefix,
            lease_ttl,
        }
    }

    async fn acquire_lock(&self, key: &str, timeout: Duration) -> Result<Option<DistributedLock>, CheckpointError> {
        let lock_key = format!("{}{}", self.lock_prefix, key);
        let node_id = format!("node_{}", Uuid::new_v4());

        // Create lease
        let lease_result = self.etcd_client.lease_grant(self.lease_ttl.as_secs() as i64).await
            .map_err(|e| CheckpointError::StorageError(format!("Failed to create lease: {}", e)))?;

        let lease_id = lease_result.id();

        // Try to acquire lock with lease
        let put_request = PutRequest::new(lock_key.clone(), node_id.clone())
            .with_lease(lease_id)
            .with_prev_kv();

        let start_time = Instant::now();
        let result = timeout(timeout, async {
            loop {
                let response = self.etcd_client.put(put_request.clone()).await
                    .map_err(|e| CheckpointError::StorageError(format!("Failed to put lock: {}", e)))?;

                // Check if we got the lock (no previous value means we got it)
                if response.prev_kv().is_none() {
                    return Ok(Some(DistributedLock {
                        key: lock_key.clone(),
                        node_id: node_id.clone(),
                        lease_id,
                        acquired_at: Utc::now(),
                    }));
                }

                // Wait a bit before retrying
                sleep(Duration::from_millis(10)).await;
            }
        }).await;

        match result {
            Ok(lock_result) => lock_result,
            Err(_) => {
                // Timeout - revoke lease and return None
                let _ = self.etcd_client.lease_revoke(lease_id).await;
                Ok(None)
            }
        }
    }

    async fn release_lock(&self, lock: DistributedLock) -> Result<(), CheckpointError> {
        self.etcd_client.lease_revoke(lock.lease_id).await
            .map_err(|e| CheckpointError::StorageError(format!("Failed to release lock: {}", e)))?;
        Ok(())
    }
}

/// Cluster membership manager
#[derive(Debug)]
struct ClusterMembership {
    etcd_client: EtcdClient,
    node_id: String,
    cluster_prefix: String,
    members: Arc<RwLock<HashMap<String, NodeInfo>>>,
    leader_id: Arc<RwLock<Option<String>>>,
    event_tx: broadcast::Sender<StateEvent>,
}

impl ClusterMembership {
    fn new(
        etcd_client: EtcdClient,
        node_id: String,
        cluster_prefix: String,
        event_tx: broadcast::Sender<StateEvent>,
    ) -> Self {
        Self {
            etcd_client,
            node_id,
            cluster_prefix,
            members: Arc::new(RwLock::new(HashMap::new())),
            leader_id: Arc::new(RwLock::new(None)),
            event_tx,
        }
    }

    async fn join_cluster(&self) -> Result<(), CheckpointError> {
        let member_key = format!("{}/members/{}", self.cluster_prefix, self.node_id);
        let node_info = NodeInfo {
            node_id: self.node_id.clone(),
            joined_at: Utc::now(),
            last_heartbeat: Utc::now(),
            is_leader: false,
            version: "0.1.0".to_string(),
        };

        let node_info_json = serde_json::to_string(&node_info)
            .map_err(|e| CheckpointError::SerializationError(format!("Failed to serialize node info: {}", e)))?;

        // Create lease for membership
        let lease_result = self.etcd_client.lease_grant(30).await // 30 seconds TTL
            .map_err(|e| CheckpointError::StorageError(format!("Failed to create membership lease: {}", e)))?;

        let lease_id = lease_result.id();

        // Register as cluster member
        let put_request = PutRequest::new(member_key, node_info_json).with_lease(lease_id);
        self.etcd_client.put(put_request).await
            .map_err(|e| CheckpointError::StorageError(format!("Failed to join cluster: {}", e)))?;

        // Broadcast join event
        let _ = self.event_tx.send(StateEvent::NodeJoined { node_id: self.node_id.clone() });

        info!("Node {} joined cluster", self.node_id);
        Ok(())
    }

    async fn leave_cluster(&self) -> Result<(), CheckpointError> {
        let member_key = format!("{}/members/{}", self.cluster_prefix, self.node_id);

        self.etcd_client.delete(member_key).await
            .map_err(|e| CheckpointError::StorageError(format!("Failed to leave cluster: {}", e)))?;

        // Broadcast leave event
        let _ = self.event_tx.send(StateEvent::NodeLeft { node_id: self.node_id.clone() });

        info!("Node {} left cluster", self.node_id);
        Ok(())
    }

    async fn get_cluster_members(&self) -> Result<HashMap<String, NodeInfo>, CheckpointError> {
        let members_prefix = format!("{}/members/", self.cluster_prefix);
        let get_request = GetRequest::new(members_prefix).with_prefix();

        let response = self.etcd_client.get(get_request).await
            .map_err(|e| CheckpointError::StorageError(format!("Failed to get cluster members: {}", e)))?;

        let mut members = HashMap::new();

        for kv in response.kvs() {
            if let (Some(key), Some(value)) = (kv.key_str(), kv.value_str()) {
                if let Some(node_id) = key.split('/').last() {
                    match serde_json::from_str::<NodeInfo>(value) {
                        Ok(node_info) => {
                            members.insert(node_id.to_string(), node_info);
                        }
                        Err(e) => {
                            warn!("Failed to deserialize node info for {}: {}", node_id, e);
                        }
                    }
                }
            }
        }

        // Update local cache
        *self.members.write().await = members.clone();

        Ok(members)
    }

    async fn elect_leader(&self) -> Result<(), CheckpointError> {
        let leader_key = format!("{}/leader", self.cluster_prefix);

        // Simple leader election: first to write wins
        let put_request = PutRequest::new(leader_key.clone(), self.node_id.clone())
            .with_prev_kv();

        let response = self.etcd_client.put(put_request).await
            .map_err(|e| CheckpointError::StorageError(format!("Failed to elect leader: {}", e)))?;

        // Check if we became leader (no previous value)
        let became_leader = response.prev_kv().is_none();

        if became_leader {
            *self.leader_id.write().await = Some(self.node_id.clone());

            // Broadcast leader change
            let _ = self.event_tx.send(StateEvent::LeaderChanged {
                old_leader: None,
                new_leader: self.node_id.clone(),
            });

            info!("Node {} elected as leader", self.node_id);
        }

        Ok(())
    }

    async fn get_current_leader(&self) -> Result<Option<String>, CheckpointError> {
        let leader_key = format!("{}/leader", self.cluster_prefix);
        let get_request = GetRequest::new(leader_key);

        let response = self.etcd_client.get(get_request).await
            .map_err(|e| CheckpointError::StorageError(format!("Failed to get leader: {}", e)))?;

        if let Some(kv) = response.kvs().first() {
            if let Some(leader_id) = kv.value_str() {
                return Ok(Some(leader_id.to_string()));
            }
        }

        Ok(None)
    }

    async fn is_leader(&self) -> Result<bool, CheckpointError> {
        let current_leader = self.get_current_leader().await?;
        Ok(current_leader.as_ref() == Some(&self.node_id))
    }
}

/// Distributed checkpointer for multi-node deployments
pub struct DistributedCheckpointer {
    config: DistributedConfig,
    etcd_client: EtcdClient,
    cluster_membership: ClusterMembership,
    lock_manager: DistributedLockManager,
    event_tx: broadcast::Sender<StateEvent>,
    event_rx: Arc<Mutex<broadcast::Receiver<StateEvent>>>,
    local_checkpointer: Arc<dyn Checkpointer + Send + Sync>,
    performance_metrics: Arc<RwLock<PerformanceMetrics>>,
}

impl DistributedCheckpointer {
    /// Create a new distributed checkpointer
    pub async fn new(config: DistributedConfig) -> Result<Self, CheckpointError> {
        // Connect to etcd cluster
        let etcd_client = EtcdClient::connect(config.etcd_endpoints.clone(), None).await
            .map_err(|e| CheckpointError::StorageError(format!("Failed to connect to etcd: {}", e)))?;

        // Create event channel
        let (event_tx, event_rx) = broadcast::channel(1000);

        // Create cluster membership manager
        let cluster_prefix = format!("{}/{}", config.key_prefix, config.cluster_name);
        let cluster_membership = ClusterMembership::new(
            etcd_client.clone(),
            config.node_id.clone(),
            cluster_prefix.clone(),
            event_tx.clone(),
        );

        // Create distributed lock manager
        let lock_prefix = format!("{}/locks/", cluster_prefix);
        let lock_manager = DistributedLockManager::new(
            etcd_client.clone(),
            lock_prefix,
            config.lock_timeout,
        );

        // Use memory checkpointer as local backend (could be configurable)
        let local_checkpointer: Arc<dyn Checkpointer + Send + Sync> =
            Arc::new(crate::checkpoint::MemoryCheckpointer::new());

        // Initialize performance metrics
        let performance_metrics = Arc::new(RwLock::new(PerformanceMetrics {
            average_save_latency: Duration::from_millis(0),
            average_sync_latency: Duration::from_millis(0),
            throughput_ops_per_second: 0.0,
            sync_success_rate: 100.0,
            leader_election_time: Duration::from_millis(0),
        }));

        Ok(Self {
            config,
            etcd_client,
            cluster_membership,
            lock_manager,
            event_tx,
            event_rx: Arc::new(Mutex::new(event_rx)),
            local_checkpointer,
            performance_metrics,
        })
    }

    /// Join the distributed cluster
    pub async fn join_cluster(&self) -> Result<(), CheckpointError> {
        self.cluster_membership.join_cluster().await?;

        // Trigger leader election if enabled
        if self.config.enable_leader_election {
            self.cluster_membership.elect_leader().await?;
        }

        Ok(())
    }

    /// Leave the distributed cluster
    pub async fn leave_cluster(&self) -> Result<(), CheckpointError> {
        self.cluster_membership.leave_cluster().await
    }

    /// Get cluster members
    pub async fn get_cluster_members(&self) -> Result<HashMap<String, NodeInfo>, CheckpointError> {
        self.cluster_membership.get_cluster_members().await
    }

    /// Get current cluster leader
    pub async fn get_current_leader(&self) -> Result<Option<String>, CheckpointError> {
        self.cluster_membership.get_current_leader().await
    }

    /// Check if this node is the leader
    pub async fn is_leader(&self) -> Result<bool, CheckpointError> {
        self.cluster_membership.is_leader().await
    }

    /// Acquire a distributed lock
    pub async fn acquire_lock(&self, key: &str, timeout: Duration) -> Result<Option<DistributedLock>, CheckpointError> {
        self.lock_manager.acquire_lock(key, timeout).await
    }

    /// Release a distributed lock
    pub async fn release_lock(&self, lock: DistributedLock) -> Result<(), CheckpointError> {
        self.lock_manager.release_lock(lock).await
    }

    /// Subscribe to cluster events
    pub async fn subscribe_to_events(&self) -> Result<broadcast::Receiver<StateEvent>, CheckpointError> {
        Ok(self.event_tx.subscribe())
    }

    /// Get performance metrics
    pub async fn get_performance_metrics(&self) -> Result<PerformanceMetrics, CheckpointError> {
        let metrics = self.performance_metrics.read().await;
        Ok(metrics.clone())
    }

    /// Simulate network partition (for testing)
    pub async fn simulate_network_partition(&self, _isolated_nodes: Vec<&str>) -> Result<(), CheckpointError> {
        // This is a test-only method - in real implementation would affect networking
        warn!("Simulating network partition for testing");
        Ok(())
    }

    /// Heal network partition (for testing)
    pub async fn heal_network_partition(&self) -> Result<(), CheckpointError> {
        // This is a test-only method - in real implementation would restore networking
        info!("Healing network partition");
        Ok(())
    }

    /// Synchronize state across cluster nodes
    async fn sync_state(&self, thread_id: &str, checkpoint_id: &str, operation: &str) -> Result<(), CheckpointError> {
        let sync_key = format!("{}/sync/{}/{}", self.config.key_prefix, thread_id, checkpoint_id);
        let sync_data = serde_json::json!({
            "node_id": self.config.node_id,
            "operation": operation,
            "timestamp": Utc::now().to_rfc3339()
        });

        let sync_data_str = serde_json::to_string(&sync_data)
            .map_err(|e| CheckpointError::SerializationError(format!("Failed to serialize sync data: {}", e)))?;

        // Write synchronization record to etcd
        let put_request = PutRequest::new(sync_key, sync_data_str);
        self.etcd_client.put(put_request).await
            .map_err(|e| CheckpointError::StorageError(format!("Failed to sync state: {}", e)))?;

        Ok(())
    }
}

#[async_trait]
impl Checkpointer for DistributedCheckpointer {
    #[instrument(skip(self, checkpoint, metadata))]
    async fn save(
        &self,
        thread_id: &str,
        checkpoint: HashMap<String, Value>,
        metadata: HashMap<String, Value>,
        parent_checkpoint_id: Option<String>,
    ) -> anyhow::Result<String> {
        let start_time = Instant::now();

        // Save to local checkpointer first
        let checkpoint_id = self.local_checkpointer
            .save(thread_id, checkpoint, metadata, parent_checkpoint_id)
            .await?;

        // Synchronize across cluster
        if let Err(e) = self.sync_state(thread_id, &checkpoint_id, "save").await {
            warn!("Failed to sync save operation: {}", e);
        }

        // Broadcast save event
        let _ = self.event_tx.send(StateEvent::CheckpointSaved {
            thread_id: thread_id.to_string(),
            checkpoint_id: checkpoint_id.clone(),
            node_id: self.config.node_id.clone(),
        });

        // Update performance metrics
        let elapsed = start_time.elapsed();
        let mut metrics = self.performance_metrics.write().await;
        metrics.average_save_latency = (metrics.average_save_latency + elapsed) / 2;

        debug!("Distributed save completed for checkpoint {} in thread {}", checkpoint_id, thread_id);

        Ok(checkpoint_id)
    }

    #[instrument(skip(self))]
    async fn load(
        &self,
        thread_id: &str,
        checkpoint_id: Option<String>,
    ) -> anyhow::Result<Option<(HashMap<String, Value>, HashMap<String, Value>)>> {
        let start_time = Instant::now();

        // Load from local checkpointer
        let result = self.local_checkpointer.load(thread_id, checkpoint_id).await?;

        // Update performance metrics
        let elapsed = start_time.elapsed();
        let mut metrics = self.performance_metrics.write().await;
        metrics.average_sync_latency = (metrics.average_sync_latency + elapsed) / 2;

        Ok(result)
    }

    #[instrument(skip(self))]
    async fn list(
        &self,
        thread_id: Option<&str>,
        limit: Option<usize>,
    ) -> anyhow::Result<Vec<(String, HashMap<String, Value>)>> {
        // Delegate to local checkpointer
        self.local_checkpointer.list(thread_id, limit).await
    }

    #[instrument(skip(self))]
    async fn delete(&self, thread_id: &str, checkpoint_id: Option<&str>) -> anyhow::Result<()> {
        // Delete from local checkpointer
        self.local_checkpointer.delete(thread_id, checkpoint_id).await?;

        // Synchronize deletion across cluster
        if let Some(id) = checkpoint_id {
            if let Err(e) = self.sync_state(thread_id, id, "delete").await {
                warn!("Failed to sync delete operation: {}", e);
            }

            // Broadcast delete event
            let _ = self.event_tx.send(StateEvent::CheckpointDeleted {
                thread_id: thread_id.to_string(),
                checkpoint_id: id.to_string(),
                node_id: self.config.node_id.clone(),
            });
        }

        Ok(())
    }
}

// Extension trait for GraphState to convert to checkpoint format
impl GraphState {
    pub fn to_checkpoint(&self) -> HashMap<String, Value> {
        // Convert GraphState to HashMap<String, Value>
        // This is a simplified implementation - in reality would need proper serialization
        let mut checkpoint = HashMap::new();
        checkpoint.insert("state".to_string(), serde_json::to_value(self).unwrap_or(Value::Null));
        checkpoint
    }
}