use async_trait::async_trait;
use futures::stream::{Stream, StreamExt};
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::graph::{Graph, GraphError};
use crate::state::State;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamOutput<T> {
    pub value: T,
    pub timestamp: std::time::SystemTime,
    pub node_id: String,
    pub sequence: usize,
}

#[async_trait]
pub trait StreamingEngine: Send + Sync {
    type Output: Send + Sync;
    
    async fn stream_execution<S: State>(
        &self,
        graph: Arc<Graph>,
        initial_state: S,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamOutput<Self::Output>> + Send>>, GraphError>;
    
    async fn stream_with_backpressure<S: State>(
        &self,
        graph: Arc<Graph>,
        initial_state: S,
        buffer_size: usize,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamOutput<Self::Output>> + Send>>, GraphError>;
}

pub struct DefaultStreamingEngine {
    buffer_size: usize,
    enable_metrics: bool,
}

impl DefaultStreamingEngine {
    pub fn new(buffer_size: usize) -> Self {
        Self {
            buffer_size,
            enable_metrics: true,
        }
    }
    
    pub fn with_metrics(mut self, enable: bool) -> Self {
        self.enable_metrics = enable;
        self
    }
}

#[async_trait]
impl StreamingEngine for DefaultStreamingEngine {
    type Output = serde_json::Value;
    
    async fn stream_execution<S: State>(
        &self,
        graph: Arc<Graph>,
        initial_state: S,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamOutput<Self::Output>> + Send>>, GraphError> {
        let (tx, rx) = tokio::sync::mpsc::channel(self.buffer_size);
        
        let graph_ref = graph.graph();
        let node_count = graph_ref.node_count();
        
        tokio::spawn(async move {
            let mut sequence = 0;
            
            // Process each node in the graph
            for i in 0..node_count {
                let output = StreamOutput {
                    value: serde_json::json!({
                        "node_index": i,
                        "state": "processing"
                    }),
                    timestamp: std::time::SystemTime::now(),
                    node_id: format!("node_{}", i),
                    sequence,
                };
                
                sequence += 1;
                let _ = tx.send(output).await;
            }
        });
        
        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }
    
    async fn stream_with_backpressure<S: State>(
        &self,
        graph: Arc<Graph>,
        initial_state: S,
        buffer_size: usize,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamOutput<Self::Output>> + Send>>, GraphError> {
        let (tx, rx) = tokio::sync::mpsc::channel(buffer_size);
        
        let graph_ref = graph.graph();
        let node_count = graph_ref.node_count();
        
        tokio::spawn(async move {
            let mut sequence = 0;
            
            // Process each node with backpressure
            for i in 0..node_count {
                let output = StreamOutput {
                    value: serde_json::json!({
                        "node_index": i,
                        "state": "processing",
                        "backpressure": true
                    }),
                    timestamp: std::time::SystemTime::now(),
                    node_id: format!("node_{}", i),
                    sequence,
                };
                
                sequence += 1;
                
                if tx.send(output).await.is_err() {
                    break;
                }
                
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            }
        });
        
        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }
}

pub struct StreamingNode<T> {
    inner: T,
    stream_enabled: bool,
}

impl<T> StreamingNode<T> {
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            stream_enabled: true,
        }
    }
    
    pub fn disable_streaming(mut self) -> Self {
        self.stream_enabled = false;
        self
    }
}