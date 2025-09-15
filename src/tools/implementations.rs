//! Concrete tool implementations

use super::{Tool, ToolSpec, ToolResult, ToolContext, ToolError};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
use crate::Result;

/// Calculator tool for mathematical operations
pub struct CalculatorTool;

#[async_trait]
impl Tool for CalculatorTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "calculator".to_string(),
            description: "Performs mathematical calculations".to_string(),
            version: "1.0.0".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "enum": ["add", "subtract", "multiply", "divide", "power", "sqrt"],
                        "description": "The mathematical operation to perform"
                    },
                    "operands": {
                        "type": "array",
                        "items": {"type": "number"},
                        "description": "The numbers to operate on"
                    }
                },
                "required": ["operation", "operands"]
            }),
            tags: vec!["math".to_string(), "calculation".to_string()],
            metadata: HashMap::new(),
        }
    }
    
    async fn validate(&self, params: &Value) -> Result<()> {
        let operation = params.get("operation")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParameters("Missing operation".to_string()))?;
        
        let operands = params.get("operands")
            .and_then(|v| v.as_array())
            .ok_or_else(|| ToolError::InvalidParameters("Missing operands".to_string()))?;
        
        match operation {
            "add" | "subtract" | "multiply" if operands.len() >= 2 => Ok(()),
            "divide" | "power" if operands.len() == 2 => Ok(()),
            "sqrt" if operands.len() == 1 => Ok(()),
            _ => Err(ToolError::InvalidParameters(
                format!("Invalid operation '{}' or wrong number of operands", operation)
            ).into()),
        }
    }
    
    async fn execute(&self, params: Value, _context: ToolContext) -> Result<ToolResult> {
        self.validate(&params).await?;
        
        let operation = params["operation"].as_str().unwrap();
        let operands: Vec<f64> = params["operands"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_f64())
            .collect();
        
        let result = match operation {
            "add" => operands.iter().sum::<f64>(),
            "subtract" => operands.iter().skip(1).fold(operands[0], |acc, x| acc - x),
            "multiply" => operands.iter().product::<f64>(),
            "divide" => {
                if operands[1] == 0.0 {
                    return Ok(ToolResult {
                        success: false,
                        data: None,
                        error: Some("Division by zero".to_string()),
                        metadata: HashMap::new(),
                    });
                }
                operands[0] / operands[1]
            }
            "power" => operands[0].powf(operands[1]),
            "sqrt" => {
                if operands[0] < 0.0 {
                    return Ok(ToolResult {
                        success: false,
                        data: None,
                        error: Some("Cannot take square root of negative number".to_string()),
                        metadata: HashMap::new(),
                    });
                }
                operands[0].sqrt()
            }
            _ => unreachable!(),
        };
        
        Ok(ToolResult {
            success: true,
            data: Some(json!(result)),
            error: None,
            metadata: HashMap::from([
                ("operation".to_string(), json!(operation)),
                ("operands".to_string(), json!(operands)),
            ]),
        })
    }
}

/// String manipulation tool
pub struct StringTool;

#[async_trait]
impl Tool for StringTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "string_tool".to_string(),
            description: "Performs string operations".to_string(),
            version: "1.0.0".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "enum": ["concat", "split", "upper", "lower", "trim", "replace", "length"],
                        "description": "The string operation to perform"
                    },
                    "input": {
                        "type": ["string", "array"],
                        "description": "The input string(s)"
                    },
                    "separator": {
                        "type": "string",
                        "description": "Separator for split/join operations"
                    },
                    "pattern": {
                        "type": "string",
                        "description": "Pattern for replace operations"
                    },
                    "replacement": {
                        "type": "string",
                        "description": "Replacement for replace operations"
                    }
                },
                "required": ["operation", "input"]
            }),
            tags: vec!["text".to_string(), "string".to_string()],
            metadata: HashMap::new(),
        }
    }
    
    async fn validate(&self, params: &Value) -> Result<()> {
        params.get("operation")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParameters("Missing operation".to_string()))?;
        
        params.get("input")
            .ok_or_else(|| ToolError::InvalidParameters("Missing input".to_string()))?;
        
        Ok(())
    }
    
    async fn execute(&self, params: Value, _context: ToolContext) -> Result<ToolResult> {
        self.validate(&params).await?;
        
        let operation = params["operation"].as_str().unwrap();
        
        let result = match operation {
            "concat" => {
                let inputs = params["input"].as_array()
                    .ok_or_else(|| ToolError::InvalidParameters("Input must be array for concat".to_string()))?;
                let separator = params.get("separator")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let concatenated: Vec<String> = inputs.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect();
                json!(concatenated.join(separator))
            }
            "split" => {
                let input = params["input"].as_str()
                    .ok_or_else(|| ToolError::InvalidParameters("Input must be string for split".to_string()))?;
                let separator = params.get("separator")
                    .and_then(|v| v.as_str())
                    .unwrap_or(" ");
                let parts: Vec<&str> = input.split(separator).collect();
                json!(parts)
            }
            "upper" => {
                let input = params["input"].as_str()
                    .ok_or_else(|| ToolError::InvalidParameters("Input must be string".to_string()))?;
                json!(input.to_uppercase())
            }
            "lower" => {
                let input = params["input"].as_str()
                    .ok_or_else(|| ToolError::InvalidParameters("Input must be string".to_string()))?;
                json!(input.to_lowercase())
            }
            "trim" => {
                let input = params["input"].as_str()
                    .ok_or_else(|| ToolError::InvalidParameters("Input must be string".to_string()))?;
                json!(input.trim())
            }
            "replace" => {
                let input = params["input"].as_str()
                    .ok_or_else(|| ToolError::InvalidParameters("Input must be string".to_string()))?;
                let pattern = params.get("pattern")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidParameters("Missing pattern".to_string()))?;
                let replacement = params.get("replacement")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                json!(input.replace(pattern, replacement))
            }
            "length" => {
                let input = params["input"].as_str()
                    .ok_or_else(|| ToolError::InvalidParameters("Input must be string".to_string()))?;
                json!(input.len())
            }
            _ => {
                return Err(ToolError::InvalidParameters(
                    format!("Unknown operation: {}", operation)
                ).into());
            }
        };
        
        Ok(ToolResult {
            success: true,
            data: Some(result),
            error: None,
            metadata: HashMap::from([
                ("operation".to_string(), json!(operation)),
            ]),
        })
    }
}

/// HTTP request tool
pub struct HttpTool {
    client: reqwest::Client,
}

impl HttpTool {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl Tool for HttpTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "http_request".to_string(),
            description: "Makes HTTP requests".to_string(),
            version: "1.0.0".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "method": {
                        "type": "string",
                        "enum": ["GET", "POST", "PUT", "DELETE", "PATCH"],
                        "description": "HTTP method"
                    },
                    "url": {
                        "type": "string",
                        "description": "The URL to request"
                    },
                    "headers": {
                        "type": "object",
                        "description": "HTTP headers"
                    },
                    "body": {
                        "type": ["object", "string"],
                        "description": "Request body"
                    },
                    "timeout": {
                        "type": "number",
                        "description": "Request timeout in seconds"
                    }
                },
                "required": ["method", "url"]
            }),
            tags: vec!["http".to_string(), "network".to_string(), "api".to_string()],
            metadata: HashMap::new(),
        }
    }
    
    async fn validate(&self, params: &Value) -> Result<()> {
        params.get("method")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParameters("Missing method".to_string()))?;
        
        params.get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParameters("Missing url".to_string()))?;
        
        Ok(())
    }
    
    async fn execute(&self, params: Value, context: ToolContext) -> Result<ToolResult> {
        self.validate(&params).await?;
        
        let method = params["method"].as_str().unwrap();
        let url = params["url"].as_str().unwrap();
        
        // Build request
        let mut request = match method {
            "GET" => self.client.get(url),
            "POST" => self.client.post(url),
            "PUT" => self.client.put(url),
            "DELETE" => self.client.delete(url),
            "PATCH" => self.client.patch(url),
            _ => {
                return Err(ToolError::InvalidParameters(
                    format!("Invalid method: {}", method)
                ).into());
            }
        };
        
        // Add headers
        if let Some(headers) = params.get("headers").and_then(|v| v.as_object()) {
            for (key, value) in headers {
                if let Some(value_str) = value.as_str() {
                    request = request.header(key, value_str);
                }
            }
        }
        
        // Add body
        if let Some(body) = params.get("body") {
            if let Some(body_str) = body.as_str() {
                request = request.body(body_str.to_string());
            } else {
                request = request.json(body);
            }
        }
        
        // Set timeout
        let timeout = params.get("timeout")
            .and_then(|v| v.as_u64())
            .or(context.timeout)
            .unwrap_or(30);
        
        request = request.timeout(std::time::Duration::from_secs(timeout));
        
        // Execute request
        match request.send().await {
            Ok(response) => {
                let status = response.status();
                let headers: HashMap<String, String> = response.headers()
                    .iter()
                    .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
                    .collect();
                
                let body = response.text().await.unwrap_or_default();
                
                // Try to parse as JSON, fallback to string
                let body_value = serde_json::from_str::<Value>(&body)
                    .unwrap_or_else(|_| json!(body));
                
                Ok(ToolResult {
                    success: status.is_success(),
                    data: Some(json!({
                        "status": status.as_u16(),
                        "headers": headers,
                        "body": body_value,
                    })),
                    error: if !status.is_success() {
                        Some(format!("HTTP {}", status))
                    } else {
                        None
                    },
                    metadata: HashMap::from([
                        ("method".to_string(), json!(method)),
                        ("url".to_string(), json!(url)),
                    ]),
                })
            }
            Err(e) => Ok(ToolResult {
                success: false,
                data: None,
                error: Some(e.to_string()),
                metadata: HashMap::from([
                    ("method".to_string(), json!(method)),
                    ("url".to_string(), json!(url)),
                ]),
            }),
        }
    }
}

/// Tool chain for executing multiple tools in sequence
pub struct ToolChain {
    tools: Vec<(String, Value)>,
}

impl ToolChain {
    pub fn new() -> Self {
        Self {
            tools: Vec::new(),
        }
    }
    
    pub fn add_tool(&mut self, tool_name: String, params: Value) {
        self.tools.push((tool_name, params));
    }
}

#[async_trait]
impl Tool for ToolChain {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "tool_chain".to_string(),
            description: "Executes multiple tools in sequence".to_string(),
            version: "1.0.0".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "tools": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "name": {"type": "string"},
                                "params": {"type": "object"}
                            },
                            "required": ["name", "params"]
                        },
                        "description": "Tools to execute in order"
                    }
                },
                "required": ["tools"]
            }),
            tags: vec!["chain".to_string(), "composite".to_string()],
            metadata: HashMap::new(),
        }
    }
    
    async fn validate(&self, params: &Value) -> Result<()> {
        params.get("tools")
            .and_then(|v| v.as_array())
            .ok_or_else(|| ToolError::InvalidParameters("Missing tools array".to_string()))?;
        
        Ok(())
    }
    
    async fn execute(&self, params: Value, context: ToolContext) -> Result<ToolResult> {
        let tools = params["tools"].as_array().unwrap();
        let mut results = Vec::new();
        let mut last_result = None;
        
        for tool_config in tools {
            let tool_name = tool_config.get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidParameters("Missing tool name".to_string()))?;
            
            let mut tool_params = tool_config.get("params")
                .cloned()
                .unwrap_or_else(|| json!({}));
            
            // Pass previous result as input if available
            if let Some(ref prev) = last_result {
                if let Some(data) = prev.data.as_ref() {
                    if let Some(obj) = tool_params.as_object_mut() {
                        obj.insert("_previous".to_string(), data.clone());
                    }
                }
            }
            
            // Execute tool (would need registry access in real implementation)
            // For now, return a mock result
            let result = ToolResult {
                success: true,
                data: Some(json!({
                    "tool": tool_name,
                    "params": tool_params,
                    "mock": "This would execute the actual tool"
                })),
                error: None,
                metadata: HashMap::new(),
            };
            
            results.push(json!({
                "tool": tool_name,
                "result": result,
            }));
            
            if !result.success {
                return Ok(ToolResult {
                    success: false,
                    data: Some(json!(results)),
                    error: result.error,
                    metadata: HashMap::from([
                        ("failed_at".to_string(), json!(tool_name)),
                    ]),
                });
            }
            
            last_result = Some(result);
        }
        
        Ok(ToolResult {
            success: true,
            data: Some(json!(results)),
            error: None,
            metadata: HashMap::from([
                ("tools_executed".to_string(), json!(tools.len())),
            ]),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_calculator_tool() {
        let tool = CalculatorTool;
        let context = ToolContext {
            request_id: "test".to_string(),
            timestamp: 0,
            metadata: HashMap::new(),
            auth: None,
            timeout: None,
        };
        
        // Test addition
        let params = json!({
            "operation": "add",
            "operands": [10, 20, 30]
        });
        
        let result = tool.execute(params, context.clone()).await.unwrap();
        assert!(result.success);
        assert_eq!(result.data, Some(json!(60.0)));
        
        // Test division by zero
        let params = json!({
            "operation": "divide",
            "operands": [10, 0]
        });
        
        let result = tool.execute(params, context.clone()).await.unwrap();
        assert!(!result.success);
        assert_eq!(result.error, Some("Division by zero".to_string()));
    }
    
    #[tokio::test]
    async fn test_string_tool() {
        let tool = StringTool;
        let context = ToolContext {
            request_id: "test".to_string(),
            timestamp: 0,
            metadata: HashMap::new(),
            auth: None,
            timeout: None,
        };
        
        // Test concatenation
        let params = json!({
            "operation": "concat",
            "input": ["hello", "world"],
            "separator": " "
        });
        
        let result = tool.execute(params, context.clone()).await.unwrap();
        assert!(result.success);
        assert_eq!(result.data, Some(json!("hello world")));
        
        // Test uppercase
        let params = json!({
            "operation": "upper",
            "input": "hello world"
        });
        
        let result = tool.execute(params, context).await.unwrap();
        assert!(result.success);
        assert_eq!(result.data, Some(json!("HELLO WORLD")));
    }
}