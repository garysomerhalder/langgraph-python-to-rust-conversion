# SCHEMA-001: Schema Definition Framework

## 📋 Task Overview
**ID:** SCHEMA-001  
**Title:** Schema definition framework  
**Status:** 🔴 TODO  
**Priority:** P0 (Critical)  
**Category:** State Schemas  
**Estimated Days:** 3  
**Phase:** Phase 1 - Critical Features  

## 🎯 Objective
Implement a comprehensive schema definition framework for type-safe state management, providing Rust equivalent of Python LangGraph's Pydantic-based state schemas.

## 📝 Description
State schemas ensure type safety and validation at runtime, preventing invalid state transitions and providing clear contracts for state shape. This is critical for production reliability.

## ✅ Acceptance Criteria
- [ ] Schema definition using Rust types
- [ ] Runtime validation of state against schema
- [ ] Compile-time type checking where possible
- [ ] Support for nested schemas
- [ ] Optional and required fields
- [ ] Default values
- [ ] Custom validators
- [ ] Schema evolution/migration
- [ ] Integration with StateData
- [ ] Clear error messages on validation failure

## 🔧 Technical Requirements

### Core Components to Implement
```rust
// src/state/schema.rs
use serde::{Serialize, Deserialize};
use validator::{Validate, ValidationError};

#[derive(Debug, Clone)]
pub struct Schema {
    pub name: String,
    pub fields: HashMap<String, FieldDefinition>,
    pub validators: Vec<Box<dyn SchemaValidator>>,
}

#[derive(Debug, Clone)]
pub struct FieldDefinition {
    pub name: String,
    pub field_type: FieldType,
    pub required: bool,
    pub default: Option<Value>,
    pub validators: Vec<Box<dyn FieldValidator>>,
}

#[derive(Debug, Clone)]
pub enum FieldType {
    String,
    Integer,
    Float,
    Boolean,
    Array(Box<FieldType>),
    Object(Schema),
    Enum(Vec<String>),
    Any,
}

pub trait SchemaValidator: Send + Sync {
    fn validate(&self, data: &StateData) -> Result<(), ValidationError>;
}

pub trait FieldValidator: Send + Sync {
    fn validate(&self, value: &Value) -> Result<(), ValidationError>;
}

// Derive macro for automatic schema generation
#[derive(Schema, Validate, Serialize, Deserialize)]
pub struct UserStateSchema {
    #[validate(length(min = 1, max = 100))]
    pub username: String,
    
    #[validate(range(min = 0, max = 150))]
    pub age: Option<i32>,
    
    #[validate(email)]
    pub email: String,
    
    #[serde(default)]
    pub preferences: HashMap<String, String>,
}

impl StateSchema for UserStateSchema {
    fn validate_state(&self, state: &StateData) -> Result<()> {
        // Automatic validation based on attributes
    }
}
```

### Python Compatibility
```python
# Python LangGraph with Pydantic
from pydantic import BaseModel, Field

class UserState(BaseModel):
    username: str = Field(min_length=1, max_length=100)
    age: Optional[int] = Field(ge=0, le=150)
    email: EmailStr
    preferences: dict = Field(default_factory=dict)
```

```rust
// Rust equivalent
#[derive(Schema, Validate)]
struct UserState {
    #[validate(length(min = 1, max = 100))]
    username: String,
    #[validate(range(min = 0, max = 150))]
    age: Option<i32>,
    #[validate(email)]
    email: String,
    #[serde(default)]
    preferences: HashMap<String, String>,
}
```

## 🚦 Implementation Plan (Traffic-Light)

### 🔴 RED Phase - Tests First (Day 1)
```rust
#[test]
fn test_schema_validation() {
    let schema = UserStateSchema::new();
    
    let valid_state = StateData::from_json(json!({
        "username": "john",
        "age": 25,
        "email": "john@example.com"
    }));
    
    assert!(schema.validate_state(&valid_state).is_ok());
    
    let invalid_state = StateData::from_json(json!({
        "username": "",  // Too short
        "age": 200,      // Out of range
        "email": "invalid"  // Not email
    }));
    
    let errors = schema.validate_state(&invalid_state).unwrap_err();
    assert_eq!(errors.len(), 3);
}
```

### 🟡 YELLOW Phase - Minimal Implementation (Day 2)
- Basic schema structure
- Field definitions
- Simple validation
- Integration with StateData

### 🟢 GREEN Phase - Production Ready (Day 3)
- Derive macros for easy schema creation
- Complex validators (regex, custom)
- Schema composition
- Migration support
- Performance optimization
- Detailed error messages
- Schema serialization

## 📊 Success Metrics
- < 1ms validation time for typical schemas
- Zero false positives/negatives
- Clear, actionable error messages
- 100% Python parity for common patterns

## 🔗 Dependencies
- validator crate for validation rules
- serde for serialization
- proc-macro for derive support

## ⚠️ Risks & Mitigations
| Risk | Mitigation |
|------|------------|
| Performance overhead | Cache compiled validators |
| Complex nested validation | Recursive validation strategy |
| Schema evolution | Version tracking and migrations |

## 📚 References
- [Pydantic Documentation](https://pydantic-docs.helpmanual.io/)
- [Rust Validator Crate](https://github.com/Keats/validator)
- [Serde Schema](https://github.com/GREsau/schemars)

## 🎯 Definition of Done
- [ ] All acceptance criteria met
- [ ] Derive macro working
- [ ] Validation benchmarks acceptable
- [ ] Documentation with examples
- [ ] Migration from schemaless tested
- [ ] Error messages user-friendly