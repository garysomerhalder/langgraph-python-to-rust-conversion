//! Conditional edge evaluation for dynamic graph routing

use serde_json::Value;
use crate::state::StateData;
use crate::Result;

/// Condition evaluator for routing decisions
pub struct ConditionEvaluator;

impl ConditionEvaluator {
    /// Evaluate a condition string against state
    pub fn evaluate(condition: &str, state: &StateData) -> Result<bool> {
        // Parse different condition types
        if condition.starts_with("eq:") {
            Self::evaluate_equality(condition, state)
        } else if condition.starts_with("gt:") {
            Self::evaluate_greater_than(condition, state)
        } else if condition.starts_with("lt:") {
            Self::evaluate_less_than(condition, state)
        } else if condition.starts_with("contains:") {
            Self::evaluate_contains(condition, state)
        } else if condition.starts_with("exists:") {
            Self::evaluate_exists(condition, state)
        } else if condition.starts_with("fn:") {
            Self::evaluate_function(condition, state)
        } else {
            // Default: check if value is truthy
            Self::evaluate_truthy(condition, state)
        }
    }
    
    /// Evaluate equality condition (eq:field=value)
    fn evaluate_equality(condition: &str, state: &StateData) -> Result<bool> {
        let parts: Vec<&str> = condition[3..].split('=').collect();
        if parts.len() != 2 {
            return Ok(false);
        }
        
        let field = parts[0];
        let expected = parts[1];
        
        if let Some(value) = state.get(field) {
            match value {
                Value::String(s) => Ok(s == expected),
                Value::Number(n) => {
                    if let Ok(expected_num) = expected.parse::<f64>() {
                        Ok(n.as_f64() == Some(expected_num))
                    } else {
                        Ok(false)
                    }
                }
                Value::Bool(b) => {
                    if let Ok(expected_bool) = expected.parse::<bool>() {
                        Ok(*b == expected_bool)
                    } else {
                        Ok(false)
                    }
                }
                _ => Ok(false),
            }
        } else {
            Ok(false)
        }
    }
    
    /// Evaluate greater than condition (gt:field>value)
    fn evaluate_greater_than(condition: &str, state: &StateData) -> Result<bool> {
        let parts: Vec<&str> = condition[3..].split('>').collect();
        if parts.len() != 2 {
            return Ok(false);
        }
        
        let field = parts[0];
        let threshold = parts[1];
        
        if let Some(Value::Number(n)) = state.get(field) {
            if let Ok(threshold_num) = threshold.parse::<f64>() {
                Ok(n.as_f64().unwrap_or(0.0) > threshold_num)
            } else {
                Ok(false)
            }
        } else {
            Ok(false)
        }
    }
    
    /// Evaluate less than condition (lt:field<value)
    fn evaluate_less_than(condition: &str, state: &StateData) -> Result<bool> {
        let parts: Vec<&str> = condition[3..].split('<').collect();
        if parts.len() != 2 {
            return Ok(false);
        }
        
        let field = parts[0];
        let threshold = parts[1];
        
        if let Some(Value::Number(n)) = state.get(field) {
            if let Ok(threshold_num) = threshold.parse::<f64>() {
                Ok(n.as_f64().unwrap_or(0.0) < threshold_num)
            } else {
                Ok(false)
            }
        } else {
            Ok(false)
        }
    }
    
    /// Evaluate contains condition (contains:field:value)
    fn evaluate_contains(condition: &str, state: &StateData) -> Result<bool> {
        let parts: Vec<&str> = condition[9..].split(':').collect();
        if parts.len() != 2 {
            return Ok(false);
        }
        
        let field = parts[0];
        let search = parts[1];
        
        if let Some(value) = state.get(field) {
            match value {
                Value::String(s) => Ok(s.contains(search)),
                Value::Array(arr) => {
                    Ok(arr.iter().any(|v| {
                        if let Value::String(s) = v {
                            s == search
                        } else {
                            false
                        }
                    }))
                }
                _ => Ok(false),
            }
        } else {
            Ok(false)
        }
    }
    
    /// Evaluate exists condition (exists:field)
    fn evaluate_exists(condition: &str, state: &StateData) -> Result<bool> {
        let field = &condition[7..];
        Ok(state.contains_key(field))
    }
    
    /// Evaluate function-based condition (fn:function_name)
    fn evaluate_function(condition: &str, state: &StateData) -> Result<bool> {
        let function_name = &condition[3..];
        
        match function_name {
            "has_errors" => {
                Ok(state.get("errors").and_then(|v| v.as_array()).map(|a| !a.is_empty()).unwrap_or(false))
            }
            "is_complete" => {
                Ok(state.get("status").and_then(|v| v.as_str()).map(|s| s == "complete").unwrap_or(false))
            }
            "needs_review" => {
                Ok(state.get("confidence").and_then(|v| v.as_f64()).map(|c| c < 0.8).unwrap_or(true))
            }
            _ => Ok(false),
        }
    }
    
    /// Evaluate if a field is truthy
    fn evaluate_truthy(field: &str, state: &StateData) -> Result<bool> {
        if let Some(value) = state.get(field) {
            match value {
                Value::Bool(b) => Ok(*b),
                Value::Number(n) => Ok(n.as_f64().unwrap_or(0.0) != 0.0),
                Value::String(s) => Ok(!s.is_empty()),
                Value::Array(a) => Ok(!a.is_empty()),
                Value::Object(o) => Ok(!o.is_empty()),
                Value::Null => Ok(false),
            }
        } else {
            Ok(false)
        }
    }
    
    /// Evaluate multiple conditions with logical operators
    pub fn evaluate_complex(conditions: &[String], operator: &str, state: &StateData) -> Result<bool> {
        match operator {
            "AND" | "&&" => {
                for condition in conditions {
                    if !Self::evaluate(condition, state)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            "OR" | "||" => {
                for condition in conditions {
                    if Self::evaluate(condition, state)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            "XOR" => {
                let mut true_count = 0;
                for condition in conditions {
                    if Self::evaluate(condition, state)? {
                        true_count += 1;
                    }
                }
                Ok(true_count == 1)
            }
            _ => Ok(false),
        }
    }
}

/// Router for selecting next nodes based on conditions
pub struct ConditionalRouter {
    routes: Vec<ConditionalRoute>,
}

#[derive(Debug, Clone)]
pub struct ConditionalRoute {
    pub condition: String,
    pub target: String,
    pub priority: i32,
}

impl ConditionalRouter {
    /// Create a new conditional router
    pub fn new() -> Self {
        Self {
            routes: Vec::new(),
        }
    }
    
    /// Add a conditional route
    pub fn add_route(&mut self, condition: String, target: String, priority: i32) {
        self.routes.push(ConditionalRoute {
            condition,
            target,
            priority,
        });
    }
    
    /// Route based on state
    pub fn route(&self, state: &StateData) -> Result<Option<String>> {
        let mut matching_routes = Vec::new();
        
        for route in &self.routes {
            if ConditionEvaluator::evaluate(&route.condition, state)? {
                matching_routes.push(route);
            }
        }
        
        if matching_routes.is_empty() {
            return Ok(None);
        }
        
        // Sort by priority (higher priority first)
        matching_routes.sort_by(|a, b| b.priority.cmp(&a.priority));
        
        Ok(Some(matching_routes[0].target.clone()))
    }
    
    /// Get all matching routes (for parallel execution)
    pub fn route_all(&self, state: &StateData) -> Result<Vec<String>> {
        let mut targets = Vec::new();
        
        for route in &self.routes {
            if ConditionEvaluator::evaluate(&route.condition, state)? {
                targets.push(route.target.clone());
            }
        }
        
        Ok(targets)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    
    #[test]
    fn test_equality_condition() {
        let mut state = StateData::new();
        state.insert("status".to_string(), json!("active"));
        state.insert("count".to_string(), json!(5));
        
        assert!(ConditionEvaluator::evaluate("eq:status=active", &state).unwrap());
        assert!(!ConditionEvaluator::evaluate("eq:status=inactive", &state).unwrap());
        assert!(ConditionEvaluator::evaluate("eq:count=5", &state).unwrap());
        assert!(!ConditionEvaluator::evaluate("eq:count=10", &state).unwrap());
    }
    
    #[test]
    fn test_comparison_conditions() {
        let mut state = StateData::new();
        state.insert("score".to_string(), json!(75));
        
        assert!(ConditionEvaluator::evaluate("gt:score>50", &state).unwrap());
        assert!(!ConditionEvaluator::evaluate("gt:score>100", &state).unwrap());
        assert!(ConditionEvaluator::evaluate("lt:score<100", &state).unwrap());
        assert!(!ConditionEvaluator::evaluate("lt:score<50", &state).unwrap());
    }
    
    #[test]
    fn test_contains_condition() {
        let mut state = StateData::new();
        state.insert("text".to_string(), json!("hello world"));
        state.insert("tags".to_string(), json!(["rust", "graph", "async"]));
        
        assert!(ConditionEvaluator::evaluate("contains:text:world", &state).unwrap());
        assert!(!ConditionEvaluator::evaluate("contains:text:foo", &state).unwrap());
        assert!(ConditionEvaluator::evaluate("contains:tags:rust", &state).unwrap());
        assert!(!ConditionEvaluator::evaluate("contains:tags:python", &state).unwrap());
    }
    
    #[test]
    fn test_exists_condition() {
        let mut state = StateData::new();
        state.insert("field1".to_string(), json!(null));
        state.insert("field2".to_string(), json!(123));
        
        assert!(ConditionEvaluator::evaluate("exists:field1", &state).unwrap());
        assert!(ConditionEvaluator::evaluate("exists:field2", &state).unwrap());
        assert!(!ConditionEvaluator::evaluate("exists:field3", &state).unwrap());
    }
    
    #[test]
    fn test_truthy_condition() {
        let mut state = StateData::new();
        state.insert("enabled".to_string(), json!(true));
        state.insert("disabled".to_string(), json!(false));
        state.insert("count".to_string(), json!(5));
        state.insert("zero".to_string(), json!(0));
        state.insert("text".to_string(), json!("hello"));
        state.insert("empty".to_string(), json!(""));
        
        assert!(ConditionEvaluator::evaluate("enabled", &state).unwrap());
        assert!(!ConditionEvaluator::evaluate("disabled", &state).unwrap());
        assert!(ConditionEvaluator::evaluate("count", &state).unwrap());
        assert!(!ConditionEvaluator::evaluate("zero", &state).unwrap());
        assert!(ConditionEvaluator::evaluate("text", &state).unwrap());
        assert!(!ConditionEvaluator::evaluate("empty", &state).unwrap());
    }
    
    #[test]
    fn test_conditional_router() {
        let mut router = ConditionalRouter::new();
        router.add_route("gt:score>80".to_string(), "excellent".to_string(), 10);
        router.add_route("gt:score>60".to_string(), "good".to_string(), 5);
        router.add_route("exists:score".to_string(), "default".to_string(), 1);
        
        let mut state = StateData::new();
        state.insert("score".to_string(), json!(85));
        
        let route = router.route(&state).unwrap();
        assert_eq!(route, Some("excellent".to_string()));
        
        state.insert("score".to_string(), json!(70));
        let route = router.route(&state).unwrap();
        assert_eq!(route, Some("good".to_string()));
        
        state.insert("score".to_string(), json!(50));
        let route = router.route(&state).unwrap();
        assert_eq!(route, Some("default".to_string()));
    }
}