// State diff types for state inspection
// HIL-003: State inspection during execution

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Represents a diff between two states
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateDiff {
    pub added: HashMap<String, Value>,
    pub removed: HashMap<String, Value>,
    pub modified: HashMap<String, (Value, Value)>,
}

impl StateDiff {
    /// Create a new empty diff
    pub fn new() -> Self {
        Self {
            added: HashMap::new(),
            removed: HashMap::new(),
            modified: HashMap::new(),
        }
    }

    /// Check if the diff is empty
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.modified.is_empty()
    }

    /// Get the total number of changes
    pub fn change_count(&self) -> usize {
        self.added.len() + self.removed.len() + self.modified.len()
    }
}

impl Default for StateDiff {
    fn default() -> Self {
        Self::new()
    }
}

/// Export format for state data
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExportFormat {
    Json,
    Yaml,
    Csv,
}

/// Filter for state snapshots
#[derive(Debug, Clone, Default)]
pub struct StateFilter {
    pub include_fields: Vec<String>,
    pub exclude_fields: Vec<String>,
    pub node_pattern: Option<String>,
}

impl StateFilter {
    /// Create a new filter
    pub fn new() -> Self {
        Self::default()
    }

    /// Add field to include
    pub fn include_field(mut self, field: impl Into<String>) -> Self {
        self.include_fields.push(field.into());
        self
    }

    /// Add field to exclude
    pub fn exclude_field(mut self, field: impl Into<String>) -> Self {
        self.exclude_fields.push(field.into());
        self
    }

    /// Set node pattern
    pub fn with_node_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.node_pattern = Some(pattern.into());
        self
    }

    /// Apply filter to a state
    pub fn apply(&self, state: &HashMap<String, Value>) -> HashMap<String, Value> {
        let mut filtered = HashMap::new();

        for (key, value) in state {
            // Check if excluded
            if self.exclude_fields.contains(key) {
                continue;
            }

            // Check if included (or include all if empty)
            if self.include_fields.is_empty() || self.include_fields.contains(key) {
                filtered.insert(key.clone(), value.clone());
            }
        }

        filtered
    }
}
