//! State Schema Framework
//!
//! Provides compile-time and runtime type-safe state validation
//! using schema definitions similar to Python's Pydantic.

use std::collections::HashMap;
use serde_json::Value;
use thiserror::Error;
use super::StateData;
use crate::Result;

/// Schema validation errors
#[derive(Error, Debug)]
pub enum SchemaError {
    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Field '{field}' has invalid type: expected {expected}, got {actual}")]
    InvalidType {
        field: String,
        expected: String,
        actual: String,
    },

    #[error("Field '{field}' value '{value}' is too short (min: {min})")]
    TooShort {
        field: String,
        value: String,
        min: usize,
    },

    #[error("Field '{field}' value length {length} exceeds max: {max}")]
    TooLong {
        field: String,
        length: usize,
        max: usize,
    },

    #[error("Field '{field}' value {value} is below minimum: {min}")]
    BelowMinimum {
        field: String,
        value: i64,
        min: i64,
    },

    #[error("Field '{field}' value {value} exceeds maximum: {max}")]
    AboveMaximum {
        field: String,
        value: i64,
        max: i64,
    },

    #[error("Field '{field}' value '{value}' is not a valid enum option. Valid: {valid_options:?}")]
    InvalidEnum {
        field: String,
        value: String,
        valid_options: Vec<String>,
    },

    #[error("Nested validation failed for field '{field}': {error}")]
    NestedError {
        field: String,
        error: Box<SchemaError>,
    },

    #[error("Custom validation failed for field '{field}': {message}")]
    CustomValidation {
        field: String,
        message: String,
    },
}

/// Field type definition
#[derive(Debug, Clone)]
pub enum FieldType {
    String,
    Integer,
    Float,
    Boolean,
    Array(Box<FieldType>),
    Object(Box<Schema>),
    Enum(Vec<String>),
    Any,
}

impl FieldType {
    /// Check if a value matches this field type
    pub fn matches(&self, value: &Value) -> bool {
        match (self, value) {
            (FieldType::String, Value::String(_)) => true,
            (FieldType::Integer, Value::Number(n)) if n.is_i64() => true,
            (FieldType::Float, Value::Number(_)) => true,
            (FieldType::Boolean, Value::Bool(_)) => true,
            (FieldType::Array(inner_type), Value::Array(arr)) => {
                arr.iter().all(|v| inner_type.matches(v))
            }
            (FieldType::Object(_), Value::Object(_)) => true,
            (FieldType::Enum(options), Value::String(s)) => options.contains(s),
            (FieldType::Any, _) => true,
            _ => false,
        }
    }

    /// Get human-readable type name
    pub fn type_name(&self) -> String {
        match self {
            FieldType::String => "string".to_string(),
            FieldType::Integer => "integer".to_string(),
            FieldType::Float => "float".to_string(),
            FieldType::Boolean => "boolean".to_string(),
            FieldType::Array(inner) => format!("array<{}>", inner.type_name()),
            FieldType::Object(schema) => format!("object<{}>", schema.name),
            FieldType::Enum(opts) => format!("enum({:?})", opts),
            FieldType::Any => "any".to_string(),
        }
    }
}

/// Field definition with validation rules
#[derive(Debug, Clone)]
pub struct FieldDefinition {
    pub name: String,
    pub field_type: FieldType,
    pub required: bool,
    pub default: Option<Value>,
    pub min_length: Option<usize>,
    pub max_length: Option<usize>,
    pub min_value: Option<i64>,
    pub max_value: Option<i64>,
    pub validators: Vec<String>, // Custom validator names
}

impl FieldDefinition {
    /// Builder for creating FieldDefinition with fluent API
    pub fn builder(name: impl Into<String>, field_type: FieldType) -> FieldDefinitionBuilder {
        FieldDefinitionBuilder::new(name, field_type)
    }

    /// Validate a value against this field definition
    pub fn validate(&self, value: Option<&Value>) -> std::result::Result<(), SchemaError> {
        // Check required
        let val = match value {
            Some(v) => v,
            None => {
                if self.required {
                    return Err(SchemaError::MissingField(self.name.clone()));
                } else {
                    return Ok(()); // Optional field not present is OK
                }
            }
        };

        // Check type
        if !self.field_type.matches(val) {
            return Err(SchemaError::InvalidType {
                field: self.name.clone(),
                expected: self.field_type.type_name(),
                actual: match val {
                    Value::Null => "null".to_string(),
                    Value::Bool(_) => "boolean".to_string(),
                    Value::Number(_) => "number".to_string(),
                    Value::String(_) => "string".to_string(),
                    Value::Array(_) => "array".to_string(),
                    Value::Object(_) => "object".to_string(),
                },
            });
        }

        // Type-specific validations
        match (&self.field_type, val) {
            (FieldType::String, Value::String(s)) => {
                // Length checks
                if let Some(min) = self.min_length {
                    if s.len() < min {
                        return Err(SchemaError::TooShort {
                            field: self.name.clone(),
                            value: s.clone(),
                            min,
                        });
                    }
                }
                if let Some(max) = self.max_length {
                    if s.len() > max {
                        return Err(SchemaError::TooLong {
                            field: self.name.clone(),
                            length: s.len(),
                            max,
                        });
                    }
                }
            }
            (FieldType::Integer, Value::Number(n)) if n.is_i64() => {
                let num = n.as_i64().unwrap();
                if let Some(min) = self.min_value {
                    if num < min {
                        return Err(SchemaError::BelowMinimum {
                            field: self.name.clone(),
                            value: num,
                            min,
                        });
                    }
                }
                if let Some(max) = self.max_value {
                    if num > max {
                        return Err(SchemaError::AboveMaximum {
                            field: self.name.clone(),
                            value: num,
                            max,
                        });
                    }
                }
            }
            (FieldType::Array(inner_type), Value::Array(arr)) => {
                // Array length checks
                if let Some(min) = self.min_length {
                    if arr.len() < min {
                        return Err(SchemaError::TooShort {
                            field: self.name.clone(),
                            value: format!("array of length {}", arr.len()),
                            min,
                        });
                    }
                }
                if let Some(max) = self.max_length {
                    if arr.len() > max {
                        return Err(SchemaError::TooLong {
                            field: self.name.clone(),
                            length: arr.len(),
                            max,
                        });
                    }
                }

                // Validate each element
                for (i, elem) in arr.iter().enumerate() {
                    if !inner_type.matches(elem) {
                        return Err(SchemaError::InvalidType {
                            field: format!("{}[{}]", self.name, i),
                            expected: inner_type.type_name(),
                            actual: format!("{:?}", elem),
                        });
                    }
                }
            }
            (FieldType::Object(schema), Value::Object(obj)) => {
                // Validate nested object
                let nested_state: StateData = obj.iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();

                schema.validate(&nested_state).map_err(|e| {
                    SchemaError::NestedError {
                        field: self.name.clone(),
                        error: Box::new(e),
                    }
                })?;
            }
            (FieldType::Enum(options), Value::String(s)) => {
                if !options.contains(s) {
                    return Err(SchemaError::InvalidEnum {
                        field: self.name.clone(),
                        value: s.clone(),
                        valid_options: options.clone(),
                    });
                }
            }
            _ => {}
        }

        Ok(())
    }
}

/// Schema for state validation
#[derive(Debug, Clone)]
pub struct Schema {
    pub name: String,
    fields: HashMap<String, FieldDefinition>,
}

impl Schema {
    /// Create a new schema
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            fields: HashMap::new(),
        }
    }

    /// Add a field definition
    pub fn add_field(&mut self, field: FieldDefinition) {
        self.fields.insert(field.name.clone(), field);
    }

    /// Validate state data against this schema
    pub fn validate(&self, state: &StateData) -> std::result::Result<(), SchemaError> {
        // Check all fields
        for (field_name, field_def) in &self.fields {
            let value = state.get(field_name);
            field_def.validate(value)?;
        }

        Ok(())
    }

    /// Apply default values to state
    pub fn apply_defaults(&self, state: &StateData) -> std::result::Result<StateData, SchemaError> {
        let mut result = state.clone();

        for (field_name, field_def) in &self.fields {
            // If field is missing and has a default, apply it
            if !result.contains_key(field_name) {
                if let Some(ref default_value) = field_def.default {
                    result.insert(field_name.clone(), default_value.clone());
                }
            }
        }

        Ok(result)
    }

    /// Get field definition
    pub fn get_field(&self, name: &str) -> Option<&FieldDefinition> {
        self.fields.get(name)
    }

    /// Get all field names
    pub fn field_names(&self) -> Vec<String> {
        self.fields.keys().cloned().collect()
    }
}

/// Trait for types that can act as state validators
pub trait StateValidator: Send + Sync {
    /// Validate state against this schema
    fn validate_state(&self, state: &StateData) -> Result<()>;

    /// Apply defaults to state
    fn apply_defaults(&self, state: &StateData) -> Result<StateData>;
}

impl StateValidator for Schema {
    fn validate_state(&self, state: &StateData) -> Result<()> {
        self.validate(state).map_err(|e| e.into())
    }

    fn apply_defaults(&self, state: &StateData) -> Result<StateData> {
        Self::apply_defaults(self, state).map_err(|e| e.into())
    }
}

/// Builder for creating FieldDefinition with fluent API
pub struct FieldDefinitionBuilder {
    name: String,
    field_type: FieldType,
    required: bool,
    default: Option<Value>,
    min_length: Option<usize>,
    max_length: Option<usize>,
    min_value: Option<i64>,
    max_value: Option<i64>,
    validators: Vec<String>,
}

impl FieldDefinitionBuilder {
    /// Create new builder
    pub fn new(name: impl Into<String>, field_type: FieldType) -> Self {
        Self {
            name: name.into(),
            field_type,
            required: false,
            default: None,
            min_length: None,
            max_length: None,
            min_value: None,
            max_value: None,
            validators: vec![],
        }
    }

    /// Mark field as required
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Set default value
    pub fn default_value(mut self, value: Value) -> Self {
        self.default = Some(value);
        self
    }

    /// Set minimum length (for strings/arrays)
    pub fn min_length(mut self, min: usize) -> Self {
        self.min_length = Some(min);
        self
    }

    /// Set maximum length (for strings/arrays)
    pub fn max_length(mut self, max: usize) -> Self {
        self.max_length = Some(max);
        self
    }

    /// Set minimum value (for numbers)
    pub fn min_value(mut self, min: i64) -> Self {
        self.min_value = Some(min);
        self
    }

    /// Set maximum value (for numbers)
    pub fn max_value(mut self, max: i64) -> Self {
        self.max_value = Some(max);
        self
    }

    /// Add custom validator
    pub fn validator(mut self, name: impl Into<String>) -> Self {
        self.validators.push(name.into());
        self
    }

    /// Build the FieldDefinition
    pub fn build(self) -> FieldDefinition {
        FieldDefinition {
            name: self.name,
            field_type: self.field_type,
            required: self.required,
            default: self.default,
            min_length: self.min_length,
            max_length: self.max_length,
            min_value: self.min_value,
            max_value: self.max_value,
            validators: self.validators,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_field_type_matching() {
        assert!(FieldType::String.matches(&json!("hello")));
        assert!(!FieldType::String.matches(&json!(123)));

        assert!(FieldType::Integer.matches(&json!(42)));
        assert!(!FieldType::Integer.matches(&json!(42.5)));

        assert!(FieldType::Boolean.matches(&json!(true)));
        assert!(!FieldType::Boolean.matches(&json!("true")));
    }

    #[test]
    fn test_basic_field_validation() {
        let field = FieldDefinition {
            name: "test".to_string(),
            field_type: FieldType::String,
            required: true,
            default: None,
            min_length: Some(3),
            max_length: Some(10),
            min_value: None,
            max_value: None,
            validators: vec![],
        };

        // Valid
        assert!(field.validate(Some(&json!("hello"))).is_ok());

        // Too short
        assert!(field.validate(Some(&json!("hi"))).is_err());

        // Too long
        assert!(field.validate(Some(&json!("hello world!"))).is_err());

        // Missing required
        assert!(field.validate(None).is_err());
    }

    #[test]
    fn test_field_builder() {
        let field = FieldDefinition::builder("username", FieldType::String)
            .required()
            .min_length(3)
            .max_length(50)
            .build();

        assert_eq!(field.name, "username");
        assert!(field.required);
        assert_eq!(field.min_length, Some(3));
        assert_eq!(field.max_length, Some(50));

        // Test validation
        assert!(field.validate(Some(&json!("john_doe"))).is_ok());
        assert!(field.validate(Some(&json!("ab"))).is_err()); // Too short
    }
}
