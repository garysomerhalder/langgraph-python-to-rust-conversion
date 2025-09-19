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