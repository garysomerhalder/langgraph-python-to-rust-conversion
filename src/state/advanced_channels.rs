//! Advanced channel implementations for LangGraph
//! CHAN-001 through CHAN-005: LastValue, Topic, Context channels and custom reducers

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::state::reducer::Reducer;

/// LastValue channel - keeps only the most recent value
#[derive(Debug, Clone)]
pub struct LastValueChannel {
    name: String,
    value: Arc<RwLock<Option<Value>>>,
    timestamp: Arc<RwLock<Option<DateTime<Utc>>>>,
}

impl LastValueChannel {
    /// Create a new LastValue channel
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: Arc::new(RwLock::new(None)),
            timestamp: Arc::new(RwLock::new(None)),
        }
    }

    /// Set the value
    pub async fn set(&self, value: Value) {
        *self.value.write().await = Some(value);
        *self.timestamp.write().await = Some(Utc::now());
    }

    /// Get the current value
    pub async fn get(&self) -> Option<Value> {
        self.value.read().await.clone()
    }

    /// Get the value with timestamp
    pub async fn get_with_timestamp(&self) -> Option<(Value, DateTime<Utc>)> {
        let value = self.value.read().await.clone()?;
        let timestamp = *self.timestamp.read().await;
        timestamp.map(|ts| (value, ts))
    }

    /// Clear the value
    pub async fn clear(&self) {
        *self.value.write().await = None;
        *self.timestamp.write().await = None;
    }
}

/// Topic channel - pub/sub pattern with multiple subscribers
#[derive(Clone)]
pub struct TopicChannel {
    name: String,
    subscribers: Arc<RwLock<Vec<Arc<dyn Fn(Value) + Send + Sync>>>>,
    history: Arc<RwLock<VecDeque<(Value, DateTime<Utc>)>>>,
    max_history: usize,
}

impl TopicChannel {
    /// Create a new Topic channel
    pub fn new(name: impl Into<String>) -> Self {
        Self::with_history(name, 100)
    }

    /// Create with custom history size
    pub fn with_history(name: impl Into<String>, max_history: usize) -> Self {
        Self {
            name: name.into(),
            subscribers: Arc::new(RwLock::new(Vec::new())),
            history: Arc::new(RwLock::new(VecDeque::with_capacity(max_history))),
            max_history,
        }
    }

    /// Publish a value to all subscribers
    pub async fn publish(&self, value: Value) {
        // Store in history
        {
            let mut history = self.history.write().await;
            history.push_back((value.clone(), Utc::now()));
            if history.len() > self.max_history {
                history.pop_front();
            }
        }

        // Notify subscribers
        let subscribers = self.subscribers.read().await;
        for subscriber in subscribers.iter() {
            subscriber(value.clone());
        }
    }

    /// Subscribe to the channel
    pub async fn subscribe<F>(&self, callback: F)
    where
        F: Fn(Value) + Send + Sync + 'static,
    {
        let mut subscribers = self.subscribers.write().await;
        subscribers.push(Arc::new(callback));
    }

    /// Get recent history
    pub async fn get_history(&self, count: usize) -> Vec<(Value, DateTime<Utc>)> {
        let history = self.history.read().await;
        history
            .iter()
            .rev()
            .take(count)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    /// Clear history
    pub async fn clear_history(&self) {
        self.history.write().await.clear();
    }
}

/// Context channel - hierarchical context with inheritance
#[derive(Debug, Clone)]
pub struct ContextChannel {
    name: String,
    local_context: Arc<RwLock<HashMap<String, Value>>>,
    parent: Option<Arc<ContextChannel>>,
}

impl ContextChannel {
    /// Create a new Context channel
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            local_context: Arc::new(RwLock::new(HashMap::new())),
            parent: None,
        }
    }

    /// Create a child context
    pub fn create_child(&self, name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            local_context: Arc::new(RwLock::new(HashMap::new())),
            parent: Some(Arc::new(self.clone())),
        }
    }

    /// Set a value in the context
    pub async fn set(&self, key: impl Into<String>, value: Value) {
        self.local_context.write().await.insert(key.into(), value);
    }

    /// Get a value from the context (checks parent if not found locally)
    pub fn get<'a>(
        &'a self,
        key: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<Value>> + Send + 'a>> {
        Box::pin(async move {
            // Check local context first
            if let Some(value) = self.local_context.read().await.get(key) {
                return Some(value.clone());
            }

            // Check parent context
            if let Some(parent) = &self.parent {
                parent.get(key).await
            } else {
                None
            }
        })
    }

    /// Get all values (including inherited from parent)
    pub fn get_all(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = HashMap<String, Value>> + Send + '_>>
    {
        Box::pin(async move {
            let mut result = HashMap::new();

            // Get parent values first
            if let Some(parent) = &self.parent {
                result = parent.get_all().await;
            }

            // Override with local values
            let local = self.local_context.read().await;
            for (key, value) in local.iter() {
                result.insert(key.clone(), value.clone());
            }

            result
        })
    }

    /// Clear local context (doesn't affect parent)
    pub async fn clear_local(&self) {
        self.local_context.write().await.clear();
    }

    /// Check if key exists (locally or in parent)
    pub fn contains<'a>(
        &'a self,
        key: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send + 'a>> {
        Box::pin(async move {
            if self.local_context.read().await.contains_key(key) {
                return true;
            }

            if let Some(parent) = &self.parent {
                parent.contains(key).await
            } else {
                false
            }
        })
    }
}

/// Custom reducer for advanced channel operations
pub struct CustomReducer {
    name: String,
    reducer_fn: Arc<dyn Fn(Option<&Value>, Value) -> Value + Send + Sync>,
}

impl CustomReducer {
    /// Create a new custom reducer
    pub fn new<F>(name: impl Into<String>, reducer_fn: F) -> Self
    where
        F: Fn(Option<&Value>, Value) -> Value + Send + Sync + 'static,
    {
        Self {
            name: name.into(),
            reducer_fn: Arc::new(reducer_fn),
        }
    }

    /// Create a sum reducer
    pub fn sum() -> Self {
        Self::new("sum", |existing, new| match (existing, &new) {
            (Some(Value::Number(e)), Value::Number(n)) => {
                let sum = e.as_f64().unwrap_or(0.0) + n.as_f64().unwrap_or(0.0);
                Value::Number(serde_json::Number::from_f64(sum).unwrap())
            }
            (None, Value::Number(n)) => Value::Number(n.clone()),
            _ => new,
        })
    }

    /// Create a max reducer
    pub fn max() -> Self {
        Self::new("max", |existing, new| match (existing, &new) {
            (Some(Value::Number(e)), Value::Number(n)) => {
                let e_val = e.as_f64().unwrap_or(f64::MIN);
                let n_val = n.as_f64().unwrap_or(f64::MIN);
                Value::Number(serde_json::Number::from_f64(e_val.max(n_val)).unwrap())
            }
            (None, Value::Number(n)) => Value::Number(n.clone()),
            _ => new,
        })
    }

    /// Create a min reducer
    pub fn min() -> Self {
        Self::new("min", |existing, new| match (existing, &new) {
            (Some(Value::Number(e)), Value::Number(n)) => {
                let e_val = e.as_f64().unwrap_or(f64::MAX);
                let n_val = n.as_f64().unwrap_or(f64::MAX);
                Value::Number(serde_json::Number::from_f64(e_val.min(n_val)).unwrap())
            }
            (None, Value::Number(n)) => Value::Number(n.clone()),
            _ => new,
        })
    }

    /// Create an append reducer for arrays
    pub fn append() -> Self {
        Self::new("append", |existing, new| match existing {
            Some(Value::Array(arr)) => {
                let mut result = arr.clone();
                if let Value::Array(new_arr) = &new {
                    result.extend(new_arr.clone());
                } else {
                    result.push(new);
                }
                Value::Array(result)
            }
            None => {
                if matches!(new, Value::Array(_)) {
                    new
                } else {
                    Value::Array(vec![new])
                }
            }
            _ => new,
        })
    }

    /// Create a merge reducer for objects
    pub fn merge() -> Self {
        Self::new("merge", |existing, new| match (existing, &new) {
            (Some(Value::Object(e)), Value::Object(n)) => {
                let mut result = e.clone();
                for (key, value) in n {
                    result.insert(key.clone(), value.clone());
                }
                Value::Object(result)
            }
            (None, Value::Object(n)) => Value::Object(n.clone()),
            _ => new,
        })
    }
}

impl Reducer for CustomReducer {
    fn reduce(&self, existing: Option<&Value>, new: Value) -> Value {
        (self.reducer_fn)(existing, new)
    }
}

/// Channel composition patterns
pub struct ChannelComposer {
    channels: HashMap<String, ChannelType>,
}

#[derive(Clone)]
pub enum ChannelType {
    LastValue(LastValueChannel),
    Topic(TopicChannel),
    Context(ContextChannel),
}

impl ChannelComposer {
    /// Create a new channel composer
    pub fn new() -> Self {
        Self {
            channels: HashMap::new(),
        }
    }

    /// Add a LastValue channel
    pub fn add_last_value(&mut self, name: impl Into<String>) -> LastValueChannel {
        let name = name.into();
        let channel = LastValueChannel::new(name.clone());
        self.channels
            .insert(name, ChannelType::LastValue(channel.clone()));
        channel
    }

    /// Add a Topic channel
    pub fn add_topic(&mut self, name: impl Into<String>) -> TopicChannel {
        let name = name.into();
        let channel = TopicChannel::new(name.clone());
        self.channels
            .insert(name, ChannelType::Topic(channel.clone()));
        channel
    }

    /// Add a Context channel
    pub fn add_context(&mut self, name: impl Into<String>) -> ContextChannel {
        let name = name.into();
        let channel = ContextChannel::new(name.clone());
        self.channels
            .insert(name, ChannelType::Context(channel.clone()));
        channel
    }

    /// Get a channel by name
    pub fn get(&self, name: &str) -> Option<&ChannelType> {
        self.channels.get(name)
    }

    /// Compose channels into a pipeline
    pub async fn compose_pipeline(&self, channel_names: Vec<&str>) -> Value {
        let mut result = Value::Null;

        for name in channel_names {
            if let Some(channel) = self.channels.get(name) {
                match channel {
                    ChannelType::LastValue(ch) => {
                        if let Some(value) = ch.get().await {
                            result = value;
                        }
                    }
                    ChannelType::Topic(ch) => {
                        let history = ch.get_history(1).await;
                        if let Some((value, _)) = history.first() {
                            result = value.clone();
                        }
                    }
                    ChannelType::Context(ch) => {
                        result = Value::Object(
                            ch.get_all()
                                .await
                                .into_iter()
                                .map(|(k, v)| (k, v))
                                .collect(),
                        );
                    }
                }
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_last_value_channel() {
        let channel = LastValueChannel::new("test");

        assert!(channel.get().await.is_none());

        channel.set(json!(42)).await;
        assert_eq!(channel.get().await, Some(json!(42)));

        channel.set(json!("new value")).await;
        assert_eq!(channel.get().await, Some(json!("new value")));

        channel.clear().await;
        assert!(channel.get().await.is_none());
    }

    #[tokio::test]
    async fn test_topic_channel() {
        let channel = TopicChannel::new("test");

        let received = Arc::new(RwLock::new(Vec::new()));
        let received_clone = received.clone();

        channel
            .subscribe(move |value| {
                let received = received_clone.clone();
                tokio::spawn(async move {
                    received.write().await.push(value);
                });
            })
            .await;

        channel.publish(json!("message 1")).await;
        channel.publish(json!("message 2")).await;

        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        let history = channel.get_history(10).await;
        assert_eq!(history.len(), 2);
    }

    #[tokio::test]
    async fn test_context_channel() {
        let parent = ContextChannel::new("parent");
        parent.set("global", json!("parent value")).await;
        parent.set("override", json!("parent override")).await;

        let child = parent.create_child("child");
        child.set("local", json!("child value")).await;
        child.set("override", json!("child override")).await;

        assert_eq!(child.get("global").await, Some(json!("parent value")));
        assert_eq!(child.get("local").await, Some(json!("child value")));
        assert_eq!(child.get("override").await, Some(json!("child override")));
        assert_eq!(child.get("missing").await, None);

        let all = child.get_all().await;
        assert_eq!(all.get("global"), Some(&json!("parent value")));
        assert_eq!(all.get("local"), Some(&json!("child value")));
        assert_eq!(all.get("override"), Some(&json!("child override")));
    }

    #[test]
    fn test_custom_reducers() {
        let sum = CustomReducer::sum();
        assert_eq!(sum.reduce(Some(&json!(5)), json!(3)), json!(8));

        let max = CustomReducer::max();
        assert_eq!(max.reduce(Some(&json!(5)), json!(10)), json!(10));

        let append = CustomReducer::append();
        assert_eq!(
            append.reduce(Some(&json!([1, 2])), json!([3, 4])),
            json!([1, 2, 3, 4])
        );
    }
}
