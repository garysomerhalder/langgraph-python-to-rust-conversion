//! Channel definitions for state management

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

use crate::state::reducer::Reducer;

/// Represents a state channel with type and reducer information
pub struct Channel {
    /// Type of data in the channel
    pub channel_type: ChannelType,
    
    /// Whether this channel is required
    pub required: bool,
    
    /// Optional reducer for merging updates
    pub reducer: Option<Box<dyn Reducer>>,
    
    /// Default value if not provided
    pub default: Option<Value>,
    
    /// Channel metadata
    pub metadata: Option<Value>,
}

/// Types of channels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ChannelType {
    /// String channel
    String,
    
    /// Number channel (integer or float)
    Number,
    
    /// Boolean channel
    Boolean,
    
    /// Array channel
    Array,
    
    /// Object channel
    Object,
    
    /// Any type allowed
    Any,
    
    /// Custom type with validation
    Custom(String),
}

impl Channel {
    /// Create a new channel
    pub fn new(channel_type: ChannelType) -> Self {
        Self {
            channel_type,
            required: false,
            reducer: None,
            default: None,
            metadata: None,
        }
    }
    
    /// Create a required channel
    pub fn required(channel_type: ChannelType) -> Self {
        Self {
            channel_type,
            required: true,
            reducer: None,
            default: None,
            metadata: None,
        }
    }
    
    /// Set the reducer for this channel
    pub fn with_reducer(mut self, reducer: Box<dyn Reducer>) -> Self {
        self.reducer = Some(reducer);
        self
    }
    
    /// Set default value for this channel
    pub fn with_default(mut self, default: Value) -> Self {
        self.default = Some(default);
        self
    }
    
    /// Add metadata to the channel
    pub fn with_metadata(mut self, metadata: Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
    
    /// Validate that a value matches the channel type
    pub fn validate_type(&self, value: &Value) -> bool {
        match &self.channel_type {
            ChannelType::String => value.is_string(),
            ChannelType::Number => value.is_number(),
            ChannelType::Boolean => value.is_boolean(),
            ChannelType::Array => value.is_array(),
            ChannelType::Object => value.is_object(),
            ChannelType::Any => true,
            ChannelType::Custom(_) => {
                // Custom validation would go here
                // For now, accept any value
                true
            }
        }
    }
    
    /// Apply the channel's reducer to merge values
    pub fn reduce(&self, existing: Option<&Value>, new: Value) -> Value {
        if let Some(reducer) = &self.reducer {
            reducer.reduce(existing, new)
        } else {
            // Default behavior: last write wins
            new
        }
    }
}

/// Builder for creating channels
pub struct ChannelBuilder {
    channel: Channel,
}

impl ChannelBuilder {
    /// Create a new channel builder
    pub fn new(channel_type: ChannelType) -> Self {
        Self {
            channel: Channel::new(channel_type),
        }
    }
    
    /// Make the channel required
    pub fn required(mut self) -> Self {
        self.channel.required = true;
        self
    }
    
    /// Set the reducer
    pub fn reducer(mut self, reducer: Box<dyn Reducer>) -> Self {
        self.channel.reducer = Some(reducer);
        self
    }
    
    /// Set default value
    pub fn default(mut self, default: Value) -> Self {
        self.channel.default = Some(default);
        self
    }
    
    /// Set metadata
    pub fn metadata(mut self, metadata: Value) -> Self {
        self.channel.metadata = Some(metadata);
        self
    }
    
    /// Build the channel
    pub fn build(self) -> Channel {
        self.channel
    }
}

impl fmt::Debug for Channel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Channel")
            .field("channel_type", &self.channel_type)
            .field("required", &self.required)
            .field("reducer", &self.reducer.is_some())
            .field("default", &self.default)
            .field("metadata", &self.metadata)
            .finish()
    }
}

impl Clone for Channel {
    fn clone(&self) -> Self {
        Self {
            channel_type: self.channel_type.clone(),
            required: self.required,
            reducer: None, // Can't clone Box<dyn Reducer>
            default: self.default.clone(),
            metadata: self.metadata.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::reducer::AppendReducer;
    use serde_json::json;
    
    #[test]
    fn test_channel_creation() {
        let channel = Channel::new(ChannelType::String);
        assert_eq!(channel.channel_type, ChannelType::String);
        assert!(!channel.required);
        assert!(channel.reducer.is_none());
        assert!(channel.default.is_none());
    }
    
    #[test]
    fn test_required_channel() {
        let channel = Channel::required(ChannelType::Number);
        assert_eq!(channel.channel_type, ChannelType::Number);
        assert!(channel.required);
    }
    
    #[test]
    fn test_channel_with_default() {
        let channel = Channel::new(ChannelType::String)
            .with_default(json!("default_value"));
        
        assert_eq!(channel.default, Some(json!("default_value")));
    }
    
    #[test]
    fn test_channel_type_validation() {
        let string_channel = Channel::new(ChannelType::String);
        assert!(string_channel.validate_type(&json!("test")));
        assert!(!string_channel.validate_type(&json!(42)));
        
        let number_channel = Channel::new(ChannelType::Number);
        assert!(number_channel.validate_type(&json!(42)));
        assert!(number_channel.validate_type(&json!(3.14)));
        assert!(!number_channel.validate_type(&json!("not a number")));
        
        let array_channel = Channel::new(ChannelType::Array);
        assert!(array_channel.validate_type(&json!([1, 2, 3])));
        assert!(!array_channel.validate_type(&json!("not an array")));
        
        let any_channel = Channel::new(ChannelType::Any);
        assert!(any_channel.validate_type(&json!("anything")));
        assert!(any_channel.validate_type(&json!(42)));
        assert!(any_channel.validate_type(&json!([1, 2, 3])));
    }
    
    #[test]
    fn test_channel_builder() {
        let channel = ChannelBuilder::new(ChannelType::Array)
            .required()
            .reducer(Box::new(AppendReducer))
            .default(json!([]))
            .metadata(json!({"description": "Message list"}))
            .build();
        
        assert_eq!(channel.channel_type, ChannelType::Array);
        assert!(channel.required);
        assert!(channel.reducer.is_some());
        assert_eq!(channel.default, Some(json!([])));
        assert!(channel.metadata.is_some());
    }
}