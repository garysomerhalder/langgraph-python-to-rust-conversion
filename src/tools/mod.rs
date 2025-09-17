//! Tool integration framework for LangGraph
//!
//! This module provides abstractions for integrating external tools
//! and functions into LangGraph workflows.

use std::sync::Arc;
use std::collections::HashMap;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::Result;

/// Errors related to tool operations
#[derive(Error, Debug)]
pub enum ToolError {
    #[error("Tool not found: {0}")]
    NotFound(String),
    
    #[error("Invalid parameters: {0}")]
    InvalidParameters(String),
    
    #[error("Tool execution failed: {0}")]
    ExecutionFailed(String),
    
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    
    #[error("Tool timeout: {0}")]
    Timeout(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
}

/// Tool parameter specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParameter {
    /// Parameter name
    pub name: String,
    
    /// Parameter type (string, number, boolean, object, array)
    pub param_type: String,
    
    /// Whether the parameter is required
    pub required: bool,
    
    /// Parameter description
    pub description: Option<String>,
    
    /// Default value if not provided
    pub default: Option<Value>,
    
    /// Validation schema (JSON Schema format)
    pub schema: Option<Value>,
}

/// Tool specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    /// Tool name
    pub name: String,
    
    /// Tool description
    pub description: String,
    
    /// Parameters accepted by the tool
    pub parameters: Vec<ToolParameter>,
    
    /// Return type description
    pub returns: Option<String>,
    
    /// Categories or tags
    pub tags: Vec<String>,
    
    /// Usage examples
    pub examples: Vec<ToolExample>,
}

/// Tool usage example
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExample {
    /// Example description
    pub description: String,
    
    /// Input parameters
    pub input: Value,
    
    /// Expected output
    pub output: Option<Value>,
}

/// Tool execution context
#[derive(Debug, Clone)]
pub struct ToolContext {
    /// Current state data
    pub state: HashMap<String, Value>,
    
    /// Execution metadata
    pub metadata: HashMap<String, Value>,
    
    /// Authentication/authorization info
    pub auth: Option<ToolAuth>,
    
    /// Execution timeout in seconds
    pub timeout: Option<u64>,
}

/// Tool authentication information
#[derive(Debug, Clone)]
pub struct ToolAuth {
    /// User or service identifier
    pub principal: String,
    
    /// Permissions granted
    pub permissions: Vec<String>,
    
    /// Additional auth metadata
    pub metadata: HashMap<String, Value>,
}

/// Tool result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Success flag
    pub success: bool,
    
    /// Result data
    pub data: Option<Value>,
    
    /// Error message if failed
    pub error: Option<String>,
    
    /// Execution metadata
    pub metadata: HashMap<String, Value>,
}

/// Trait for implementing tools
#[async_trait]
pub trait Tool: Send + Sync {
    /// Get tool specification
    fn spec(&self) -> ToolSpec;
    
    /// Validate parameters before execution
    async fn validate(&self, params: &Value) -> Result<()>;
    
    /// Execute the tool
    async fn execute(&self, params: Value, context: ToolContext) -> Result<ToolResult>;
    
    /// Check if the tool can be executed in the given context
    async fn can_execute(&self, _context: &ToolContext) -> bool {
        true
    }
}

/// Tool registry for managing available tools
pub struct ToolRegistry {
    /// Registered tools
    tools: HashMap<String, Arc<dyn Tool>>,
    
    /// Tool categories
    categories: HashMap<String, Vec<String>>,
}

impl ToolRegistry {
    /// Create a new tool registry
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            categories: HashMap::new(),
        }
    }
    
    /// Register a tool
    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        let spec = tool.spec();
        let name = spec.name.clone();
        
        // Register tool
        self.tools.insert(name.clone(), tool);
        
        // Update categories
        for tag in spec.tags {
            self.categories
                .entry(tag)
                .or_insert_with(Vec::new)
                .push(name.clone());
        }
    }
    
    /// Get a tool by name
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }
    
    /// List all available tools
    pub fn list(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }
    
    /// Get tools by category
    pub fn get_by_category(&self, category: &str) -> Vec<String> {
        self.categories
            .get(category)
            .cloned()
            .unwrap_or_default()
    }
    
    /// Execute a tool
    pub async fn execute(
        &self,
        name: &str,
        params: Value,
        context: ToolContext,
    ) -> Result<ToolResult> {
        let tool = self.get(name)
            .ok_or_else(|| ToolError::NotFound(name.to_string()))?;
        
        // Check permissions
        if !tool.can_execute(&context).await {
            return Err(ToolError::PermissionDenied(name.to_string()).into());
        }
        
        // Validate parameters
        tool.validate(&params).await?;
        
        // Execute tool
        tool.execute(params, context).await
    }
}

/// Function tool - wraps a function as a tool
pub struct FunctionTool<F> {
    spec: ToolSpec,
    function: F,
}

impl<F> FunctionTool<F> {
    /// Create a new function tool
    pub fn new(spec: ToolSpec, function: F) -> Self {
        Self { spec, function }
    }
}

#[async_trait]
impl<F> Tool for FunctionTool<F>
where
    F: Fn(Value, ToolContext) -> Result<ToolResult> + Send + Sync,
{
    fn spec(&self) -> ToolSpec {
        self.spec.clone()
    }
    
    async fn validate(&self, params: &Value) -> Result<()> {
        // Basic validation based on spec
        for param in &self.spec.parameters {
            if param.required && !params.get(&param.name).is_some() {
                return Err(ToolError::InvalidParameters(
                    format!("Missing required parameter: {}", param.name)
                ).into());
            }
        }
        Ok(())
    }
    
    async fn execute(&self, params: Value, context: ToolContext) -> Result<ToolResult> {
        (self.function)(params, context)
    }
}

/// HTTP tool - calls an HTTP API
pub struct HttpTool {
    spec: ToolSpec,
    base_url: String,
    method: String,
    headers: HashMap<String, String>,
}

impl HttpTool {
    /// Create a new HTTP tool
    pub fn new(
        spec: ToolSpec,
        base_url: String,
        method: String,
        headers: HashMap<String, String>,
    ) -> Self {
        Self {
            spec,
            base_url,
            method,
            headers,
        }
    }
}

#[async_trait]
impl Tool for HttpTool {
    fn spec(&self) -> ToolSpec {
        self.spec.clone()
    }
    
    async fn validate(&self, params: &Value) -> Result<()> {
        // Validate required parameters
        for param in &self.spec.parameters {
            if param.required && !params.get(&param.name).is_some() {
                return Err(ToolError::InvalidParameters(
                    format!("Missing required parameter: {}", param.name)
                ).into());
            }
        }
        Ok(())
    }
    
    async fn execute(&self, params: Value, _context: ToolContext) -> Result<ToolResult> {
        // TODO: Implement actual HTTP call
        // This would use reqwest or similar to make the actual API call
        Ok(ToolResult {
            success: true,
            data: Some(params),
            error: None,
            metadata: HashMap::new(),
        })
    }
}

/// Tool chain - combines multiple tools in sequence
pub struct ToolChain {
    /// Tools to execute in order
    tools: Vec<(String, Arc<dyn Tool>)>,
    
    /// Whether to stop on first failure
    stop_on_error: bool,
}

impl ToolChain {
    /// Create a new tool chain
    pub fn new(stop_on_error: bool) -> Self {
        Self {
            tools: Vec::new(),
            stop_on_error,
        }
    }
    
    /// Add a tool to the chain
    pub fn add_tool(&mut self, name: String, tool: Arc<dyn Tool>) {
        self.tools.push((name, tool));
    }
    
    /// Execute the tool chain
    pub async fn execute(
        &self,
        initial_params: Value,
        context: ToolContext,
    ) -> Result<Vec<ToolResult>> {
        let mut results = Vec::new();
        let mut current_params = initial_params;
        
        for (name, tool) in &self.tools {
            let result = tool.execute(current_params.clone(), context.clone()).await?;
            
            if !result.success && self.stop_on_error {
                return Err(ToolError::ExecutionFailed(
                    format!("Tool {} failed: {:?}", name, result.error)
                ).into());
            }
            
            // Pass output of previous tool as input to next
            if let Some(data) = &result.data {
                current_params = data.clone();
            }
            
            results.push(result);
        }
        
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    
    #[tokio::test]
    async fn test_tool_registry() {
        let mut registry = ToolRegistry::new();
        
        // Create a simple function tool
        let spec = ToolSpec {
            name: "test_tool".to_string(),
            description: "A test tool".to_string(),
            parameters: vec![],
            returns: None,
            tags: vec!["test".to_string()],
            examples: vec![],
        };
        
        let tool = FunctionTool::new(spec, |_params, _context| {
            Ok(ToolResult {
                success: true,
                data: Some(json!({"result": "success"})),
                error: None,
                metadata: HashMap::new(),
            })
        });
        
        registry.register(Arc::new(tool));
        
        assert!(registry.get("test_tool").is_some());
        assert_eq!(registry.list(), vec!["test_tool"]);
        assert_eq!(registry.get_by_category("test"), vec!["test_tool"]);
    }
    
    #[tokio::test]
    async fn test_function_tool_execution() {
        let spec = ToolSpec {
            name: "add".to_string(),
            description: "Add two numbers".to_string(),
            parameters: vec![
                ToolParameter {
                    name: "a".to_string(),
                    param_type: "number".to_string(),
                    required: true,
                    description: Some("First number".to_string()),
                    default: None,
                    schema: None,
                },
                ToolParameter {
                    name: "b".to_string(),
                    param_type: "number".to_string(),
                    required: true,
                    description: Some("Second number".to_string()),
                    default: None,
                    schema: None,
                },
            ],
            returns: Some("number".to_string()),
            tags: vec!["math".to_string()],
            examples: vec![],
        };
        
        let tool = FunctionTool::new(spec, |params: Value, _context: ToolContext| {
            let a = params["a"].as_f64().unwrap_or(0.0);
            let b = params["b"].as_f64().unwrap_or(0.0);
            
            Ok(ToolResult {
                success: true,
                data: Some(json!({"result": a + b})),
                error: None,
                metadata: HashMap::new(),
            })
        });
        
        let params = json!({"a": 5, "b": 3});
        let context = ToolContext {
            state: HashMap::new(),
            metadata: HashMap::new(),
            auth: None,
            timeout: None,
        };
        
        let result = tool.execute(params, context).await.unwrap();
        assert!(result.success);
        assert_eq!(result.data.unwrap()["result"], json!(8.0));
    }
}