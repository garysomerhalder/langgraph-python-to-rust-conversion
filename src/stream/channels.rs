use crate::graph::GraphError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, oneshot, RwLock};

#[derive(Debug, Clone)]
pub enum ChannelType {
    Broadcast(usize),
    Mpsc(usize),
    Oneshot,
}

#[derive(Clone)]
pub struct ChannelRegistry {
    channels: Arc<RwLock<HashMap<String, ChannelEndpoint>>>,
}

impl ChannelRegistry {
    pub fn new() -> Self {
        Self {
            channels: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn create_channel(
        &self,
        name: String,
        channel_type: ChannelType,
    ) -> Result<(), GraphError> {
        let mut channels = self.channels.write().await;

        if channels.contains_key(&name) {
            return Err(GraphError::EdgeError(format!(
                "Channel {} already exists",
                name
            )));
        }

        let endpoint = match channel_type {
            ChannelType::Broadcast(capacity) => {
                let (tx, _) = broadcast::channel(capacity);
                ChannelEndpoint::Broadcast(Arc::new(tx))
            }
            ChannelType::Mpsc(capacity) => {
                let (tx, rx) = mpsc::channel(capacity);
                ChannelEndpoint::Mpsc {
                    sender: Arc::new(tx),
                    receiver: Arc::new(RwLock::new(Some(rx))),
                }
            }
            ChannelType::Oneshot => ChannelEndpoint::Oneshot {
                pending: Arc::new(RwLock::new(Vec::new())),
            },
        };

        channels.insert(name, endpoint);
        Ok(())
    }

    pub async fn get_sender<T: Clone + Send + Sync + 'static>(
        &self,
        name: &str,
    ) -> Result<ChannelSender<T>, GraphError> {
        let channels = self.channels.read().await;

        channels
            .get(name)
            .ok_or_else(|| GraphError::EdgeError(format!("Channel {} not found", name)))
            .map(|endpoint| ChannelSender::new(name.to_string(), endpoint.clone()))
    }

    pub async fn get_receiver<T: Clone + Send + Sync + 'static>(
        &self,
        name: &str,
    ) -> Result<ChannelReceiver<T>, GraphError> {
        let channels = self.channels.read().await;

        channels
            .get(name)
            .ok_or_else(|| GraphError::EdgeError(format!("Channel {} not found", name)))
            .map(|endpoint| ChannelReceiver::new(name.to_string(), endpoint.clone()))
    }

    pub async fn remove_channel(&self, name: &str) -> Result<(), GraphError> {
        let mut channels = self.channels.write().await;
        channels
            .remove(name)
            .ok_or_else(|| GraphError::EdgeError(format!("Channel {} not found", name)))
            .map(|_| ())
    }
}

#[derive(Clone)]
enum ChannelEndpoint {
    Broadcast(Arc<broadcast::Sender<Vec<u8>>>),
    Mpsc {
        sender: Arc<mpsc::Sender<Vec<u8>>>,
        receiver: Arc<RwLock<Option<mpsc::Receiver<Vec<u8>>>>>,
    },
    Oneshot {
        pending: Arc<RwLock<Vec<oneshot::Sender<Vec<u8>>>>>,
    },
}

pub struct ChannelSender<T> {
    name: String,
    endpoint: ChannelEndpoint,
    _phantom: std::marker::PhantomData<T>,
}

impl<T> ChannelSender<T> {
    fn new(name: String, endpoint: ChannelEndpoint) -> Self {
        Self {
            name,
            endpoint,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<T: Serialize + Clone + Send + Sync> ChannelSender<T> {
    pub async fn send(&self, value: T) -> Result<(), GraphError> {
        let bytes = bincode::serialize(&value)
            .map_err(|e| GraphError::SerializationError(e.to_string()))?;

        match &self.endpoint {
            ChannelEndpoint::Broadcast(tx) => {
                tx.send(bytes).map_err(|_| {
                    GraphError::EdgeError("Failed to send on broadcast channel".to_string())
                })?;
                Ok(())
            }
            ChannelEndpoint::Mpsc { sender, .. } => {
                sender.send(bytes).await.map_err(|_| {
                    GraphError::EdgeError("Failed to send on mpsc channel".to_string())
                })?;
                Ok(())
            }
            ChannelEndpoint::Oneshot { pending } => {
                let mut pending_lock = pending.write().await;
                if let Some(tx) = pending_lock.pop() {
                    tx.send(bytes).map_err(|_| {
                        GraphError::EdgeError("Failed to send on oneshot channel".to_string())
                    })?;
                    Ok(())
                } else {
                    Err(GraphError::EdgeError(
                        "No receiver for oneshot channel".to_string(),
                    ))
                }
            }
        }
    }
}

pub struct ChannelReceiver<T> {
    name: String,
    endpoint: ChannelEndpoint,
    _phantom: std::marker::PhantomData<T>,
}

impl<T> ChannelReceiver<T> {
    fn new(name: String, endpoint: ChannelEndpoint) -> Self {
        Self {
            name,
            endpoint,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<T: for<'de> Deserialize<'de> + Clone + Send + Sync> ChannelReceiver<T> {
    pub async fn recv(&mut self) -> Result<T, GraphError> {
        match &self.endpoint {
            ChannelEndpoint::Broadcast(tx) => {
                let mut rx = tx.subscribe();
                let bytes = rx.recv().await.map_err(|_| {
                    GraphError::EdgeError("Failed to receive from broadcast channel".to_string())
                })?;

                bincode::deserialize(&bytes)
                    .map_err(|e| GraphError::SerializationError(e.to_string()))
            }
            ChannelEndpoint::Mpsc { receiver, .. } => {
                let mut receiver_lock = receiver.write().await;
                if let Some(ref mut rx) = *receiver_lock {
                    let bytes = rx
                        .recv()
                        .await
                        .ok_or_else(|| GraphError::EdgeError("Channel closed".to_string()))?;

                    bincode::deserialize(&bytes)
                        .map_err(|e| GraphError::SerializationError(e.to_string()))
                } else {
                    Err(GraphError::EdgeError("Receiver already taken".to_string()))
                }
            }
            ChannelEndpoint::Oneshot { pending } => {
                let (tx, rx) = oneshot::channel();
                pending.write().await.push(tx);

                let bytes = rx.await.map_err(|_| {
                    GraphError::EdgeError("Failed to receive from oneshot channel".to_string())
                })?;

                bincode::deserialize(&bytes)
                    .map_err(|e| GraphError::SerializationError(e.to_string()))
            }
        }
    }
}

#[async_trait]
pub trait ChannelSelector: Send + Sync {
    async fn select(&self, channels: Vec<String>) -> Result<String, GraphError>;
}

pub struct RoundRobinSelector {
    index: Arc<RwLock<usize>>,
}

impl RoundRobinSelector {
    pub fn new() -> Self {
        Self {
            index: Arc::new(RwLock::new(0)),
        }
    }
}

#[async_trait]
impl ChannelSelector for RoundRobinSelector {
    async fn select(&self, channels: Vec<String>) -> Result<String, GraphError> {
        if channels.is_empty() {
            return Err(GraphError::EdgeError(
                "No channels to select from".to_string(),
            ));
        }

        let mut index = self.index.write().await;
        let selected = channels[*index % channels.len()].clone();
        *index += 1;

        Ok(selected)
    }
}
