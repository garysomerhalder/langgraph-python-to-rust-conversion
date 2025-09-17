//! State validation and sanitization
//!
//! This module provides validation rules and sanitization for state updates
//! to ensure data integrity and prevent invalid state transitions.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use thiserror::Error;
use smallvec::SmallVec;

/// Validation errors
#[derive(Error, Debug)]
pub enum ValidationError {
    #[error("Value type mismatch: expected {expected}, got {actual}")]
    TypeMismatch { expected: String, actual: String },
    
    #[error("Value out of range: {value} not in [{min}, {max}]")]
    OutOfRange { value: String, min: String, max: String },
    
    #[error("Pattern validation failed: {value} does not match {pattern}")]
    PatternMismatch { value: String, pattern: String },
    
    #[error("Required field missing: {field}")]
    RequiredFieldMissing { field: String },
    
    #[error("Field not allowed: {field}")]
    UnauthorizedField { field: String },
    
    #[error("Value exceeds maximum length: {length} > {max}")]
    LengthExceeded { length: usize, max: usize },
    
    #[error("Invalid enum value: {value} not in allowed values")]
    InvalidEnumValue { value: String },
    
    #[error("State transition not allowed: from {from} to {to}")]
    InvalidTransition { from: String, to: String },
    
    #[error("Validation rule failed: {rule}")]
    RuleFailed { rule: String },
}

/// Type of validation to perform
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValidationType {
    /// Ensure value is of specific type
    Type(ValueType),
    
    /// Ensure string matches regex pattern
    Pattern(String),
    
    /// Ensure numeric value is within range
    Range { min: Option<f64>, max: Option<f64> },
    
    /// Ensure string/array length is within bounds
    Length { min: Option<usize>, max: Option<usize> },
    
    /// Ensure value is one of allowed values
    Enum(SmallVec<[Value; 4]>),
    
    /// Field is required
    Required,
    
    /// Custom validation function
    Custom(String),
}

/// Value types for type validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValueType {
    String,
    Number,
    Boolean,
    Array,
    Object,
    Null,
}

/// Validation rule for a field
#[derive(Debug, Clone)]
pub struct ValidationRule {
    /// Field path (e.g., "user.email")
    pub field: String,
    
    /// Validations to apply (using SmallVec for typical small sets)
    pub validations: SmallVec<[ValidationType; 4]>,
    
    /// Whether to sanitize the value
    pub sanitize: bool,
    
    /// Whether this field is allowed
    pub allow_field: bool,
}

/// State validator
pub struct StateValidator {
    /// Validation rules by field path
    rules: HashMap<String, ValidationRule>,
    
    /// Global validation rules
    global_rules: Vec<Box<dyn Fn(&HashMap<String, Value>) -> Result<(), ValidationError> + Send + Sync>>,
    
    /// Allowed state transitions
    allowed_transitions: HashMap<String, Vec<String>>,
    
    /// Whether to allow unknown fields
    allow_unknown_fields: bool,
    
    /// Whether to sanitize values
    enable_sanitization: bool,
}

impl StateValidator {
    /// Create a new state validator
    pub fn new() -> Self {
        Self {
            rules: HashMap::new(),
            global_rules: Vec::new(),
            allowed_transitions: HashMap::new(),
            allow_unknown_fields: true,
            enable_sanitization: true,
        }
    }
    
    /// Add a validation rule
    pub fn add_rule(&mut self, rule: ValidationRule) {
        self.rules.insert(rule.field.clone(), rule);
    }
    
    /// Set whether unknown fields are allowed
    pub fn set_allow_unknown_fields(&mut self, allow: bool) {
        self.allow_unknown_fields = allow;
    }
    
    /// Add allowed state transition
    pub fn add_transition(&mut self, from: String, to: Vec<String>) {
        self.allowed_transitions.insert(from, to);
    }
    
    /// Validate a state update
    pub fn validate(&self, current: &HashMap<String, Value>, updates: &HashMap<String, Value>) -> Result<(), ValidationError> {
        // Check for unauthorized fields
        if !self.allow_unknown_fields {
            for key in updates.keys() {
                if !self.rules.contains_key(key) {
                    return Err(ValidationError::UnauthorizedField { 
                        field: key.clone() 
                    });
                }
            }
        }
        
        // Validate each field
        for (field, rule) in &self.rules {
            if let Some(value) = updates.get(field) {
                self.validate_field(field, value, rule)?;
            } else if rule.validations.contains(&ValidationType::Required) {
                return Err(ValidationError::RequiredFieldMissing { 
                    field: field.clone() 
                });
            }
        }
        
        // Run global validation rules
        for rule in &self.global_rules {
            rule(updates)?;
        }
        
        Ok(())
    }
    
    /// Validate a single field
    fn validate_field(&self, field: &str, value: &Value, rule: &ValidationRule) -> Result<(), ValidationError> {
        for validation in &rule.validations {
            match validation {
                ValidationType::Type(expected_type) => {
                    self.validate_type(value, expected_type)?;
                }
                ValidationType::Pattern(pattern) => {
                    if let Value::String(s) = value {
                        let re = regex::Regex::new(pattern).map_err(|_| {
                            ValidationError::RuleFailed { 
                                rule: format!("Invalid regex pattern: {}", pattern) 
                            }
                        })?;
                        if !re.is_match(s) {
                            return Err(ValidationError::PatternMismatch {
                                value: s.clone(),
                                pattern: pattern.clone(),
                            });
                        }
                    }
                }
                ValidationType::Range { min, max } => {
                    if let Value::Number(n) = value {
                        if let Some(num) = n.as_f64() {
                            if let Some(min_val) = min {
                                if num < *min_val {
                                    return Err(ValidationError::OutOfRange {
                                        value: num.to_string(),
                                        min: min_val.to_string(),
                                        max: max.map(|m| m.to_string()).unwrap_or_else(|| "inf".to_string()),
                                    });
                                }
                            }
                            if let Some(max_val) = max {
                                if num > *max_val {
                                    return Err(ValidationError::OutOfRange {
                                        value: num.to_string(),
                                        min: min.map(|m| m.to_string()).unwrap_or_else(|| "-inf".to_string()),
                                        max: max_val.to_string(),
                                    });
                                }
                            }
                        }
                    }
                }
                ValidationType::Length { min, max } => {
                    let len = match value {
                        Value::String(s) => s.len(),
                        Value::Array(a) => a.len(),
                        _ => 0,
                    };
                    
                    if let Some(min_len) = min {
                        if len < *min_len {
                            return Err(ValidationError::LengthExceeded {
                                length: len,
                                max: *min_len,
                            });
                        }
                    }
                    if let Some(max_len) = max {
                        if len > *max_len {
                            return Err(ValidationError::LengthExceeded {
                                length: len,
                                max: *max_len,
                            });
                        }
                    }
                }
                ValidationType::Enum(allowed) => {
                    if !allowed.contains(value) {
                        return Err(ValidationError::InvalidEnumValue {
                            value: value.to_string(),
                        });
                    }
                }
                ValidationType::Required => {
                    // Already handled in validate()
                }
                ValidationType::Custom(name) => {
                    // Custom validation would be implemented via global rules
                }
            }
        }
        
        Ok(())
    }
    
    /// Validate value type
    fn validate_type(&self, value: &Value, expected: &ValueType) -> Result<(), ValidationError> {
        let actual = match value {
            Value::String(_) => ValueType::String,
            Value::Number(_) => ValueType::Number,
            Value::Bool(_) => ValueType::Boolean,
            Value::Array(_) => ValueType::Array,
            Value::Object(_) => ValueType::Object,
            Value::Null => ValueType::Null,
        };
        
        let matches = match (expected, &actual) {
            (ValueType::String, ValueType::String) |
            (ValueType::Number, ValueType::Number) |
            (ValueType::Boolean, ValueType::Boolean) |
            (ValueType::Array, ValueType::Array) |
            (ValueType::Object, ValueType::Object) |
            (ValueType::Null, ValueType::Null) => true,
            _ => false,
        };
        
        if !matches {
            return Err(ValidationError::TypeMismatch {
                expected: format!("{:?}", expected),
                actual: format!("{:?}", actual),
            });
        }
        
        Ok(())
    }
    
    /// Sanitize a value
    pub fn sanitize(&self, value: &mut Value) {
        if !self.enable_sanitization {
            return;
        }
        
        match value {
            Value::String(s) => {
                // Trim whitespace
                *s = s.trim().to_string();
                
                // Remove null bytes
                *s = s.replace('\0', "");
                
                // Limit length to prevent DoS
                if s.len() > 1_000_000 {
                    *s = s.chars().take(1_000_000).collect();
                }
            }
            Value::Array(arr) => {
                // Limit array size
                if arr.len() > 10_000 {
                    arr.truncate(10_000);
                }
                
                // Recursively sanitize array elements
                for item in arr.iter_mut() {
                    self.sanitize(item);
                }
            }
            Value::Object(obj) => {
                // Limit object keys
                if obj.len() > 1_000 {
                    let keys: Vec<_> = obj.keys().take(1_000).cloned().collect();
                    obj.retain(|k, _| keys.contains(k));
                }
                
                // Recursively sanitize object values
                for (_, v) in obj.iter_mut() {
                    self.sanitize(v);
                }
            }
            _ => {}
        }
    }
    
    /// Validate state transition
    pub fn validate_transition(&self, from: &str, to: &str) -> Result<(), ValidationError> {
        if let Some(allowed) = self.allowed_transitions.get(from) {
            if !allowed.iter().any(|s| s == to) {
                return Err(ValidationError::InvalidTransition {
                    from: from.to_string(),
                    to: to.to_string(),
                });
            }
        }
        Ok(())
    }
}

/// Builder for creating validation rules
pub struct ValidationRuleBuilder {
    field: String,
    validations: SmallVec<[ValidationType; 4]>,
    sanitize: bool,
    allow_field: bool,
}

impl ValidationRuleBuilder {
    /// Create a new validation rule builder
    pub fn new(field: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            validations: SmallVec::new(),
            sanitize: true,
            allow_field: true,
        }
    }
    
    /// Add type validation
    pub fn with_type(mut self, value_type: ValueType) -> Self {
        self.validations.push(ValidationType::Type(value_type));
        self
    }
    
    /// Add pattern validation
    pub fn with_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.validations.push(ValidationType::Pattern(pattern.into()));
        self
    }
    
    /// Add range validation
    pub fn with_range(mut self, min: Option<f64>, max: Option<f64>) -> Self {
        self.validations.push(ValidationType::Range { min, max });
        self
    }
    
    /// Add length validation
    pub fn with_length(mut self, min: Option<usize>, max: Option<usize>) -> Self {
        self.validations.push(ValidationType::Length { min, max });
        self
    }
    
    /// Add enum validation
    pub fn with_enum(mut self, values: SmallVec<[Value; 4]>) -> Self {
        self.validations.push(ValidationType::Enum(values));
        self
    }
    
    /// Mark field as required
    pub fn required(mut self) -> Self {
        self.validations.push(ValidationType::Required);
        self
    }
    
    /// Disable sanitization
    pub fn no_sanitize(mut self) -> Self {
        self.sanitize = false;
        self
    }
    
    /// Build the validation rule
    pub fn build(self) -> ValidationRule {
        ValidationRule {
            field: self.field,
            validations: self.validations,
            sanitize: self.sanitize,
            allow_field: self.allow_field,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    
    #[test]
    fn test_type_validation() {
        let mut validator = StateValidator::new();
        validator.add_rule(
            ValidationRuleBuilder::new("name")
                .with_type(ValueType::String)
                .build()
        );
        
        let current = HashMap::new();
        let mut updates = HashMap::new();
        updates.insert("name".to_string(), json!("John"));
        
        assert!(validator.validate(&current, &updates).is_ok());
        
        updates.insert("name".to_string(), json!(123));
        assert!(validator.validate(&current, &updates).is_err());
    }
    
    #[test]
    fn test_range_validation() {
        let mut validator = StateValidator::new();
        validator.add_rule(
            ValidationRuleBuilder::new("age")
                .with_type(ValueType::Number)
                .with_range(Some(0.0), Some(150.0))
                .build()
        );
        
        let current = HashMap::new();
        let mut updates = HashMap::new();
        
        updates.insert("age".to_string(), json!(25));
        assert!(validator.validate(&current, &updates).is_ok());
        
        updates.insert("age".to_string(), json!(200));
        assert!(validator.validate(&current, &updates).is_err());
    }
    
    #[test]
    fn test_sanitization() {
        let validator = StateValidator::new();
        
        let mut value = json!("  hello world  ");
        validator.sanitize(&mut value);
        assert_eq!(value, json!("hello world"));
        
        let mut value = json!("hello\0world");
        validator.sanitize(&mut value);
        assert_eq!(value, json!("helloworld"));
    }
}