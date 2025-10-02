//! Simple tool implementations compatible with current ToolSpec

use super::{Tool, ToolSpec, ToolResult, ToolContext, ToolError, ToolParameter};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
use crate::Result;

/// Simple calculator tool for basic arithmetic
pub struct CalculatorTool;

#[async_trait]
impl Tool for CalculatorTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "calculator".to_string(),
            description: "Performs basic arithmetic calculations".to_string(),
            parameters: vec![
                ToolParameter {
                    name: "operation".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                    description: Some("Operation: add, subtract, multiply, divide".to_string()),
                    default: None,
                    schema: None,
                },
                ToolParameter {
                    name: "operands".to_string(),
                    param_type: "array".to_string(),
                    required: true,
                    description: Some("Array of numbers to operate on".to_string()),
                    default: None,
                    schema: None,
                },
            ],
            returns: Some("number".to_string()),
            tags: vec!["math".to_string()],
            examples: vec![],
        }
    }

    async fn validate(&self, params: &Value) -> Result<()> {
        // GREEN PHASE: Enhanced validation with detailed error messages
        if !params.is_object() {
            return Err(ToolError::InvalidParameters(
                "Parameters must be a JSON object. Example: {\"operation\": \"add\", \"operands\": [5, 3]}".to_string()
            ).into());
        }

        let operation = params.get("operation")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParameters(
                "Missing 'operation' field. Must be one of: add, subtract, multiply, divide".to_string()
            ))?;

        if !["add", "subtract", "multiply", "divide"].contains(&operation) {
            return Err(ToolError::InvalidParameters(
                format!("Invalid operation '{}'. Supported operations: add, subtract, multiply, divide", operation)
            ).into());
        }

        let operands = params.get("operands")
            .and_then(|v| v.as_array())
            .ok_or_else(|| ToolError::InvalidParameters(
                "Missing 'operands' field. Must be an array of numbers. Example: [5, 3]".to_string()
            ))?;

        if operands.is_empty() {
            return Err(ToolError::InvalidParameters("Operands array cannot be empty".to_string()).into());
        }

        if operands.len() < 2 {
            return Err(ToolError::InvalidParameters(
                format!("Need at least 2 operands, got {}. Example: [5, 3]", operands.len())
            ).into());
        }

        // GREEN: Validate that all operands are actually numbers
        for (i, operand) in operands.iter().enumerate() {
            if !operand.is_f64() && !operand.is_i64() && !operand.is_u64() {
                return Err(ToolError::InvalidParameters(
                    format!("Operand at index {} is not a number: {:?}", i, operand)
                ).into());
            }
        }

        // GREEN: Check for division by zero upfront
        if operation == "divide" && operands.len() >= 2 {
            if let Some(divisor) = operands[1].as_f64() {
                if divisor == 0.0 {
                    return Err(ToolError::InvalidParameters(
                        "Cannot divide by zero".to_string()
                    ).into());
                }
            }
        }

        Ok(())
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

        if operands.len() < 2 {
            return Ok(ToolResult {
                success: false,
                data: None,
                error: Some("Not enough numeric operands".to_string()),
                metadata: HashMap::new(),
            });
        }

        let result = match operation {
            "add" => operands.iter().sum(),
            "subtract" => operands.iter().skip(1).fold(operands[0], |acc, x| acc - x),
            "multiply" => operands.iter().product(),
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
            _ => {
                return Ok(ToolResult {
                    success: false,
                    data: None,
                    error: Some(format!("Unknown operation: {}", operation)),
                    metadata: HashMap::new(),
                });
            }
        };

        Ok(ToolResult {
            success: true,
            data: Some(json!({"result": result})),
            error: None,
            metadata: HashMap::from([
                ("operation".to_string(), json!(operation)),
                ("operand_count".to_string(), json!(operands.len())),
            ]),
        })
    }
}

/// Simple echo/string tool
pub struct StringTool;

#[async_trait]
impl Tool for StringTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "echo".to_string(),
            description: "Echoes back the input message".to_string(),
            parameters: vec![
                ToolParameter {
                    name: "message".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                    description: Some("Message to echo".to_string()),
                    default: None,
                    schema: None,
                },
            ],
            returns: Some("string".to_string()),
            tags: vec!["utility".to_string()],
            examples: vec![],
        }
    }

    async fn validate(&self, params: &Value) -> Result<()> {
        if !params.is_object() {
            return Err(ToolError::InvalidParameters("Parameters must be an object".to_string()).into());
        }

        params.get("message")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParameters("Missing message parameter".to_string()))?;

        Ok(())
    }

    async fn execute(&self, params: Value, _context: ToolContext) -> Result<ToolResult> {
        self.validate(&params).await?;

        let message = params["message"].as_str().unwrap();

        Ok(ToolResult {
            success: true,
            data: Some(json!({"message": message})),
            error: None,
            metadata: HashMap::from([
                ("length".to_string(), json!(message.len())),
            ]),
        })
    }
}
