//! Schema Validation Tests - RED Phase
//!
//! Tests for compile-time and runtime type-safe state schemas

use langgraph::state::StateData;
use langgraph::state::schema::{Schema, FieldDefinition, FieldType};
use serde_json::json;

#[test]
fn test_basic_schema_validation() {
    // RED: Define a simple schema for user state
    let mut schema = Schema::new("UserState");

    schema.add_field(FieldDefinition {
        name: "username".to_string(),
        field_type: FieldType::String,
        required: true,
        default: None,
        min_length: Some(1),
        max_length: Some(100),
        min_value: None,
        max_value: None,
        validators: vec![],
    });

    schema.add_field(FieldDefinition {
        name: "age".to_string(),
        field_type: FieldType::Integer,
        required: false,
        default: None,
        min_length: None,
        max_length: None,
        min_value: Some(0),
        max_value: Some(150),
        validators: vec![],
    });

    // Valid state should pass
    let mut valid_state = StateData::new();
    valid_state.insert("username".to_string(), json!("john_doe"));
    valid_state.insert("age".to_string(), json!(25));

    let result = schema.validate(&valid_state);
    assert!(result.is_ok(), "Valid state should pass validation");
}

#[test]
fn test_schema_validation_failures() {
    // RED: Test validation errors
    let mut schema = Schema::new("UserState");

    schema.add_field(FieldDefinition {
        name: "username".to_string(),
        field_type: FieldType::String,
        required: true,
        default: None,
        min_length: Some(1),
        max_length: Some(100),
        min_value: None,
        max_value: None,
        validators: vec![],
    });

    // Missing required field
    let empty_state = StateData::new();
    let result = schema.validate(&empty_state);
    assert!(result.is_err(), "Missing required field should fail");

    let err = result.unwrap_err();
    assert!(err.to_string().contains("username"), "Error should mention missing field");

    // Field too short
    let mut invalid_state = StateData::new();
    invalid_state.insert("username".to_string(), json!(""));

    let result = schema.validate(&invalid_state);
    assert!(result.is_err(), "Empty username should fail");

    // Field too long
    let mut too_long = StateData::new();
    too_long.insert("username".to_string(), json!("a".repeat(101)));

    let result = schema.validate(&too_long);
    assert!(result.is_err(), "Username over 100 chars should fail");
}

#[test]
fn test_schema_type_validation() {
    // RED: Test type checking
    let mut schema = Schema::new("TypedState");

    schema.add_field(FieldDefinition {
        name: "count".to_string(),
        field_type: FieldType::Integer,
        required: true,
        default: None,
        min_length: None,
        max_length: None,
        min_value: None,
        max_value: None,
        validators: vec![],
    });

    schema.add_field(FieldDefinition {
        name: "active".to_string(),
        field_type: FieldType::Boolean,
        required: true,
        default: None,
        min_length: None,
        max_length: None,
        min_value: None,
        max_value: None,
        validators: vec![],
    });

    // Valid types
    let mut valid = StateData::new();
    valid.insert("count".to_string(), json!(42));
    valid.insert("active".to_string(), json!(true));
    assert!(schema.validate(&valid).is_ok());

    // Invalid type - string instead of integer
    let mut invalid = StateData::new();
    invalid.insert("count".to_string(), json!("not a number"));
    invalid.insert("active".to_string(), json!(true));

    let result = schema.validate(&invalid);
    assert!(result.is_err(), "Wrong type should fail");
    assert!(result.unwrap_err().to_string().contains("count"), "Error should mention wrong field");
}

#[test]
fn test_schema_optional_fields() {
    // RED: Test optional fields with defaults
    let mut schema = Schema::new("OptionalState");

    schema.add_field(FieldDefinition {
        name: "required_field".to_string(),
        field_type: FieldType::String,
        required: true,
        default: None,
        min_length: None,
        max_length: None,
        min_value: None,
        max_value: None,
        validators: vec![],
    });

    schema.add_field(FieldDefinition {
        name: "optional_field".to_string(),
        field_type: FieldType::String,
        required: false,
        default: Some(json!("default_value")),
        min_length: None,
        max_length: None,
        min_value: None,
        max_value: None,
        validators: vec![],
    });

    // Only required field provided
    let mut minimal = StateData::new();
    minimal.insert("required_field".to_string(), json!("present"));

    assert!(schema.validate(&minimal).is_ok(), "Optional fields should be allowed to be missing");

    // Apply defaults should fill in missing optional fields
    let filled = schema.apply_defaults(&minimal).unwrap();
    assert_eq!(
        filled.get("optional_field").unwrap().as_str().unwrap(),
        "default_value",
        "Default value should be applied"
    );
}

#[test]
fn test_schema_nested_objects() {
    // RED: Test nested schema validation
    let mut address_schema = Schema::new("Address");
    address_schema.add_field(FieldDefinition {
        name: "street".to_string(),
        field_type: FieldType::String,
        required: true,
        default: None,
        min_length: None,
        max_length: None,
        min_value: None,
        max_value: None,
        validators: vec![],
    });
    address_schema.add_field(FieldDefinition {
        name: "city".to_string(),
        field_type: FieldType::String,
        required: true,
        default: None,
        min_length: None,
        max_length: None,
        min_value: None,
        max_value: None,
        validators: vec![],
    });

    let mut user_schema = Schema::new("UserWithAddress");
    user_schema.add_field(FieldDefinition {
        name: "name".to_string(),
        field_type: FieldType::String,
        required: true,
        default: None,
        min_length: None,
        max_length: None,
        min_value: None,
        max_value: None,
        validators: vec![],
    });
    user_schema.add_field(FieldDefinition {
        name: "address".to_string(),
        field_type: FieldType::Object(Box::new(address_schema)),
        required: true,
        default: None,
        min_length: None,
        max_length: None,
        min_value: None,
        max_value: None,
        validators: vec![],
    });

    // Valid nested object
    let mut valid = StateData::new();
    valid.insert("name".to_string(), json!("John"));
    valid.insert("address".to_string(), json!({
        "street": "123 Main St",
        "city": "Boston"
    }));

    assert!(user_schema.validate(&valid).is_ok(), "Valid nested object should pass");

    // Invalid nested object (missing city)
    let mut invalid = StateData::new();
    invalid.insert("name".to_string(), json!("John"));
    invalid.insert("address".to_string(), json!({
        "street": "123 Main St"
    }));

    let result = user_schema.validate(&invalid);
    assert!(result.is_err(), "Missing nested field should fail");
    assert!(result.unwrap_err().to_string().contains("city"), "Error should mention missing nested field");
}

#[test]
fn test_schema_array_validation() {
    // RED: Test array type validation
    let mut schema = Schema::new("ArrayState");

    schema.add_field(FieldDefinition {
        name: "tags".to_string(),
        field_type: FieldType::Array(Box::new(FieldType::String)),
        required: true,
        default: None,
        min_length: Some(1),
        max_length: Some(10),
        min_value: None,
        max_value: None,
        validators: vec![],
    });

    // Valid array
    let mut valid = StateData::new();
    valid.insert("tags".to_string(), json!(["rust", "programming"]));
    assert!(schema.validate(&valid).is_ok());

    // Empty array (below min_length)
    let mut empty = StateData::new();
    empty.insert("tags".to_string(), json!([]));
    assert!(schema.validate(&empty).is_err(), "Empty array below min_length should fail");

    // Array too long
    let mut too_long = StateData::new();
    too_long.insert("tags".to_string(), json!(vec!["tag"; 11]));
    assert!(schema.validate(&too_long).is_err(), "Array over max_length should fail");

    // Wrong element type
    let mut wrong_type = StateData::new();
    wrong_type.insert("tags".to_string(), json!([1, 2, 3]));
    assert!(schema.validate(&wrong_type).is_err(), "Array with wrong element type should fail");
}

#[test]
fn test_schema_enum_validation() {
    // RED: Test enum value validation
    let mut schema = Schema::new("EnumState");

    schema.add_field(FieldDefinition {
        name: "status".to_string(),
        field_type: FieldType::Enum(vec![
            "pending".to_string(),
            "active".to_string(),
            "completed".to_string(),
        ]),
        required: true,
        default: None,
        min_length: None,
        max_length: None,
        min_value: None,
        max_value: None,
        validators: vec![],
    });

    // Valid enum value
    let mut valid = StateData::new();
    valid.insert("status".to_string(), json!("active"));
    assert!(schema.validate(&valid).is_ok());

    // Invalid enum value
    let mut invalid = StateData::new();
    invalid.insert("status".to_string(), json!("unknown"));

    let result = schema.validate(&invalid);
    assert!(result.is_err(), "Invalid enum value should fail");
    assert!(result.unwrap_err().to_string().contains("pending"), "Error should list valid options");
}
