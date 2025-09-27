//! MessageGraph implementation for message-based workflows
//! YELLOW Phase: Minimal implementation to make tests pass

use crate::{LangGraphError, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Role of a message sender
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Tool,
}

/// Type of message
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageType {
    Text,
    Image,
    Audio,
    Video,
    File,
    Command,
}

/// A message in the graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: uuid::Uuid,
    pub role: MessageRole,
    pub content: String,
    pub message_type: MessageType,
    pub metadata: serde_json::Value,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl Message {
    /// Create a new text message
    pub fn new(role: MessageRole, content: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            role,
            content: content.into(),
            message_type: MessageType::Text,
            metadata: serde_json::json!({}),
            timestamp: chrono::Utc::now(),
        }
    }

    /// Create a message with specific type
    pub fn with_type(
        role: MessageRole,
        content: impl Into<String>,
        message_type: MessageType,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            role,
            content: content.into(),
            message_type,
            metadata: serde_json::json!({}),
            timestamp: chrono::Utc::now(),
        }
    }

    /// Add metadata to the message
    pub fn add_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        if !self.metadata.is_object() {
            self.metadata = serde_json::json!({});
        }
        if let serde_json::Value::Object(ref mut map) = self.metadata {
            map.insert(key.into(), value);
        }
        self
    }

    /// Get metadata from the message
    pub fn get_metadata(&self, key: &str) -> Option<&serde_json::Value> {
        self.metadata.as_object()?.get(key)
    }

    /// Get state from message metadata
    pub fn get_state(&self) -> Option<&serde_json::Value> {
        self.get_metadata("state")
    }

    /// Set state in message metadata
    pub fn set_state(mut self, state: serde_json::Value) -> Self {
        self.add_metadata("state", state)
    }
}

/// Message handler function type
pub type MessageHandler =
    Arc<dyn Fn(Message) -> Pin<Box<dyn Future<Output = Message> + Send>> + Send + Sync>;

/// Router function type
pub type RouterFn = Arc<dyn Fn(&Message) -> &str + Send + Sync>;

/// MessageGraph for message-based workflows
pub struct MessageGraph {
    name: String,
    nodes: HashMap<String, MessageHandler>,
    routers: HashMap<String, RouterFn>,
    content_routers: HashMap<String, Vec<(String, Vec<String>)>>,
    entry_point: Option<String>,
    default_route: Option<String>,
    history: Arc<RwLock<Vec<Message>>>,
    max_history: usize,
}

impl MessageGraph {
    /// Create a new MessageGraph
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            nodes: HashMap::new(),
            routers: HashMap::new(),
            content_routers: HashMap::new(),
            entry_point: None,
            default_route: None,
            history: Arc::new(RwLock::new(Vec::new())),
            max_history: 0,
        }
    }

    /// Enable history tracking
    pub fn with_history_enabled(mut self, max_messages: usize) -> Self {
        self.max_history = max_messages;
        self
    }

    /// Add a message processing node
    pub fn add_message_node<F, Fut>(mut self, name: impl Into<String>, handler: F) -> Self
    where
        F: Fn(Message) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Message> + Send + 'static,
    {
        let handler = Arc::new(move |msg: Message| {
            Box::pin(handler(msg)) as Pin<Box<dyn Future<Output = Message> + Send>>
        });
        self.nodes.insert(name.into(), handler);
        self
    }

    /// Add a router node
    pub fn add_router<F>(mut self, name: impl Into<String>, router: F) -> Self
    where
        F: Fn(&Message) -> &str + Send + Sync + 'static,
    {
        self.routers.insert(name.into(), Arc::new(router));
        self
    }

    /// Add a content-based router
    pub fn add_content_router(
        mut self,
        name: impl Into<String>,
        routes: Vec<(&str, Vec<&str>)>,
    ) -> Self {
        let routes = routes
            .into_iter()
            .map(|(node, keywords)| {
                (
                    node.to_string(),
                    keywords.iter().map(|k| k.to_string()).collect(),
                )
            })
            .collect();
        self.content_routers.insert(name.into(), routes);
        self
    }

    /// Set the entry point
    pub fn set_entry_point(mut self, node: impl Into<String>) -> Self {
        self.entry_point = Some(node.into());
        self
    }

    /// Set default route for content router
    pub fn set_default_route(mut self, node: impl Into<String>) -> Self {
        self.default_route = Some(node.into());
        self
    }

    /// Build the message graph
    pub fn build(self) -> Result<Self> {
        if self.entry_point.is_none() && !self.nodes.is_empty() {
            return Err(LangGraphError::Graph(
                crate::graph::GraphError::InvalidStructure(
                    "MessageGraph requires an entry point".to_string(),
                ),
            ));
        }
        Ok(self)
    }

    /// Process a message through the graph
    pub async fn process_message(&self, message: Message) -> Result<Message> {
        // Store in history if enabled
        if self.max_history > 0 {
            let mut history = self.history.write().await;
            history.push(message.clone());
            if history.len() > self.max_history {
                history.remove(0);
            }
        }

        // Route message
        let target_node = self.route_message(&message)?;

        // Process through target node
        if let Some(handler) = self.nodes.get(&target_node) {
            let response = handler(message).await;

            // Store response in history
            if self.max_history > 0 {
                let mut history = self.history.write().await;
                history.push(response.clone());
                if history.len() > self.max_history {
                    history.remove(0);
                }
            }

            Ok(response)
        } else {
            Err(LangGraphError::Execution(format!(
                "Node '{}' not found",
                target_node
            )))
        }
    }

    /// Route a message to appropriate node
    fn route_message(&self, message: &Message) -> Result<String> {
        // If entry point is a router
        if let Some(entry) = &self.entry_point {
            // Check if it's a router
            if let Some(router) = self.routers.get(entry) {
                return Ok(router(message).to_string());
            }

            // Check if it's a content router
            if let Some(routes) = self.content_routers.get(entry) {
                let content_lower = message.content.to_lowercase();

                for (node, keywords) in routes {
                    for keyword in keywords {
                        if content_lower.contains(keyword) {
                            return Ok(node.clone());
                        }
                    }
                }

                // Use default route if no match
                if let Some(default) = &self.default_route {
                    return Ok(default.clone());
                }
            }

            // Direct node
            return Ok(entry.clone());
        }

        Err(LangGraphError::Execution(
            "No entry point defined".to_string(),
        ))
    }

    /// Get conversation history
    pub async fn get_history(&self) -> Vec<Message> {
        self.history.read().await.clone()
    }

    /// Get conversation history (alias)
    pub async fn get_conversation_history(&self) -> Vec<Message> {
        self.get_history().await
    }

    /// Clear conversation history
    pub async fn clear_history(&self) {
        self.history.write().await.clear();
    }

    /// Get history size
    pub async fn history_size(&self) -> usize {
        self.history.read().await.len()
    }

    /// Enable state management
    pub fn with_state_management(self) -> Self {
        // State management is implicitly available through message metadata
        self
    }

    /// Process a batch of messages
    pub async fn process_batch(&self, messages: Vec<Message>) -> Result<Vec<Message>> {
        let mut responses = Vec::new();
        for msg in messages {
            responses.push(self.process_message(msg).await?);
        }
        Ok(responses)
    }

    /// Get graph statistics
    pub fn stats(&self) -> MessageGraphStats {
        MessageGraphStats {
            node_count: self.nodes.len(),
            router_count: self.routers.len() + self.content_routers.len(),
            has_entry_point: self.entry_point.is_some(),
            history_enabled: self.max_history > 0,
            max_history_size: self.max_history,
        }
    }
}

/// Statistics about a MessageGraph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageGraphStats {
    pub node_count: usize,
    pub router_count: usize,
    pub has_entry_point: bool,
    pub history_enabled: bool,
    pub max_history_size: usize,
}
