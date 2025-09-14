//! State reducer implementations for merging state updates

use serde_json::Value;

/// Trait for state reducers
pub trait Reducer: Send + Sync {
    /// Reduce/merge a new value with an existing value
    fn reduce(&self, existing: Option<&Value>, new: Value) -> Value;
    
    /// Get reducer metadata
    fn metadata(&self) -> Option<Value> {
        None
    }
}

/// Function type for reducer functions
pub type ReducerFn = Box<dyn Fn(Option<&Value>, Value) -> Value + Send + Sync>;

/// Default reducer - last write wins
pub struct DefaultReducer;

impl Reducer for DefaultReducer {
    fn reduce(&self, _existing: Option<&Value>, new: Value) -> Value {
        new
    }
}

/// Append reducer - appends to arrays
pub struct AppendReducer;

impl Reducer for AppendReducer {
    fn reduce(&self, existing: Option<&Value>, new: Value) -> Value {
        match existing {
            Some(Value::Array(existing_arr)) => {
                let mut result = existing_arr.clone();
                match new {
                    Value::Array(new_arr) => result.extend(new_arr),
                    other => result.push(other),
                }
                Value::Array(result)
            }
            Some(other) => {
                // If existing is not an array, create array with both values
                let mut result = vec![other.clone()];
                match new {
                    Value::Array(new_arr) => result.extend(new_arr),
                    other => result.push(other),
                }
                Value::Array(result)
            }
            None => {
                // No existing value, wrap new value in array if not already
                match new {
                    Value::Array(arr) => Value::Array(arr),
                    other => Value::Array(vec![other]),
                }
            }
        }
    }
}

/// Merge reducer - deep merges objects
pub struct MergeReducer;

impl Reducer for MergeReducer {
    fn reduce(&self, existing: Option<&Value>, new: Value) -> Value {
        match (existing, &new) {
            (Some(Value::Object(existing_obj)), Value::Object(new_obj)) => {
                let mut result = existing_obj.clone();
                for (key, value) in new_obj {
                    result.insert(key.clone(), value.clone());
                }
                Value::Object(result)
            }
            (Some(_), new_val) => new_val,
            (None, new_val) => new_val,
        }
    }
}

/// Add reducer - adds numeric values
pub struct AddReducer;

impl Reducer for AddReducer {
    fn reduce(&self, existing: Option<&Value>, new: Value) -> Value {
        match (existing, &new) {
            (Some(Value::Number(existing_num)), Value::Number(new_num)) => {
                if let (Some(e), Some(n)) = (existing_num.as_i64(), new_num.as_i64()) {
                    serde_json::json!(e + n)
                } else if let (Some(e), Some(n)) = (existing_num.as_f64(), new_num.as_f64()) {
                    serde_json::json!(e + n)
                } else {
                    new
                }
            }
            (Some(_), new_val) => new_val,
            (None, new_val) => new_val,
        }
    }
}

/// Max reducer - keeps maximum value
pub struct MaxReducer;

impl Reducer for MaxReducer {
    fn reduce(&self, existing: Option<&Value>, new: Value) -> Value {
        match (existing, &new) {
            (Some(Value::Number(existing_num)), Value::Number(new_num)) => {
                if let (Some(e), Some(n)) = (existing_num.as_i64(), new_num.as_i64()) {
                    serde_json::json!(e.max(n))
                } else if let (Some(e), Some(n)) = (existing_num.as_f64(), new_num.as_f64()) {
                    serde_json::json!(e.max(n))
                } else {
                    new
                }
            }
            (Some(existing_val), new_val) => {
                // For non-numeric values, use string comparison
                if existing_val.to_string() > new_val.to_string() {
                    existing_val.clone()
                } else {
                    new_val.clone()
                }
            }
            (None, new_val) => new_val,
        }
    }
}

/// Min reducer - keeps minimum value
pub struct MinReducer;

impl Reducer for MinReducer {
    fn reduce(&self, existing: Option<&Value>, new: Value) -> Value {
        match (existing, &new) {
            (Some(Value::Number(existing_num)), Value::Number(new_num)) => {
                if let (Some(e), Some(n)) = (existing_num.as_i64(), new_num.as_i64()) {
                    serde_json::json!(e.min(n))
                } else if let (Some(e), Some(n)) = (existing_num.as_f64(), new_num.as_f64()) {
                    serde_json::json!(e.min(n))
                } else {
                    new
                }
            }
            (Some(existing_val), new_val) => {
                // For non-numeric values, use string comparison
                if existing_val.to_string() < new_val.to_string() {
                    existing_val.clone()
                } else {
                    new_val.clone()
                }
            }
            (None, new_val) => new_val,
        }
    }
}

/// Custom reducer that uses a provided function
pub struct CustomReducer {
    pub function: ReducerFn,
}

impl Reducer for CustomReducer {
    fn reduce(&self, existing: Option<&Value>, new: Value) -> Value {
        (self.function)(existing, new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    
    #[test]
    fn test_default_reducer() {
        let reducer = DefaultReducer;
        
        // Should always return new value
        assert_eq!(reducer.reduce(None, json!("new")), json!("new"));
        assert_eq!(reducer.reduce(Some(&json!("old")), json!("new")), json!("new"));
        assert_eq!(reducer.reduce(Some(&json!(42)), json!(100)), json!(100));
    }
    
    #[test]
    fn test_append_reducer() {
        let reducer = AppendReducer;
        
        // None -> wrap in array
        assert_eq!(reducer.reduce(None, json!("value")), json!(["value"]));
        assert_eq!(reducer.reduce(None, json!([1, 2])), json!([1, 2]));
        
        // Existing array -> append
        assert_eq!(
            reducer.reduce(Some(&json!([1, 2])), json!(3)),
            json!([1, 2, 3])
        );
        assert_eq!(
            reducer.reduce(Some(&json!([1, 2])), json!([3, 4])),
            json!([1, 2, 3, 4])
        );
        
        // Non-array existing -> create array with both
        assert_eq!(
            reducer.reduce(Some(&json!("first")), json!("second")),
            json!(["first", "second"])
        );
    }
    
    #[test]
    fn test_merge_reducer() {
        let reducer = MergeReducer;
        
        // Merge objects
        let existing = json!({"a": 1, "b": 2});
        let new = json!({"b": 3, "c": 4});
        let expected = json!({"a": 1, "b": 3, "c": 4});
        assert_eq!(reducer.reduce(Some(&existing), new), expected);
        
        // Non-object -> replace
        assert_eq!(reducer.reduce(Some(&json!("old")), json!("new")), json!("new"));
        
        // None -> use new
        assert_eq!(reducer.reduce(None, json!({"a": 1})), json!({"a": 1}));
    }
    
    #[test]
    fn test_add_reducer() {
        let reducer = AddReducer;
        
        // Add integers
        assert_eq!(reducer.reduce(Some(&json!(5)), json!(3)), json!(8));
        
        // Add floats
        assert_eq!(reducer.reduce(Some(&json!(5.5)), json!(2.5)), json!(8.0));
        
        // Non-numeric -> replace
        assert_eq!(reducer.reduce(Some(&json!("old")), json!("new")), json!("new"));
        
        // None -> use new
        assert_eq!(reducer.reduce(None, json!(42)), json!(42));
    }
    
    #[test]
    fn test_max_reducer() {
        let reducer = MaxReducer;
        
        // Max of integers
        assert_eq!(reducer.reduce(Some(&json!(5)), json!(10)), json!(10));
        assert_eq!(reducer.reduce(Some(&json!(10)), json!(5)), json!(10));
        
        // Max of floats
        assert_eq!(reducer.reduce(Some(&json!(5.5)), json!(2.5)), json!(5.5));
        
        // None -> use new
        assert_eq!(reducer.reduce(None, json!(42)), json!(42));
    }
    
    #[test]
    fn test_min_reducer() {
        let reducer = MinReducer;
        
        // Min of integers
        assert_eq!(reducer.reduce(Some(&json!(5)), json!(10)), json!(5));
        assert_eq!(reducer.reduce(Some(&json!(10)), json!(5)), json!(5));
        
        // Min of floats
        assert_eq!(reducer.reduce(Some(&json!(5.5)), json!(2.5)), json!(2.5));
        
        // None -> use new
        assert_eq!(reducer.reduce(None, json!(42)), json!(42));
    }
    
    #[test]
    fn test_custom_reducer() {
        // Custom reducer that concatenates strings
        let reducer = CustomReducer {
            function: Box::new(|existing, new| {
                match (existing, &new) {
                    (Some(Value::String(existing_str)), Value::String(new_str)) => {
                        json!(format!("{} {}", existing_str, new_str))
                    }
                    _ => new,
                }
            }),
        };
        
        assert_eq!(
            reducer.reduce(Some(&json!("hello")), json!("world")),
            json!("hello world")
        );
        assert_eq!(reducer.reduce(None, json!("test")), json!("test"));
    }
}