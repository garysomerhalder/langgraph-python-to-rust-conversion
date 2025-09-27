use std::time::Duration;
use serde::{Deserialize, Serialize};
use crate::graph::CompiledGraph;
use crate::state::StateData;

/// Batch job for processing workflows
#[derive(Clone)]
pub struct BatchJob {
    pub id: String,
    pub graph: CompiledGraph,
    pub input: StateData,
    pub priority: u8,
}

impl std::fmt::Debug for BatchJob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BatchJob")
            .field("id", &self.id)
            .field("graph", &"CompiledGraph")
            .field("input", &self.input)
            .field("priority", &self.priority)
            .finish()
    }
}

/// Status of a batch job
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BatchJobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Retrying,
    Cancelled,
}

/// Result of batch job execution
#[derive(Debug, Clone)]
pub struct BatchResult {
    pub job_id: String,
    pub status: BatchJobStatus,
    pub output: Option<StateData>,
    pub error: Option<String>,
    pub duration: Duration,
    pub attempts: u32,
}

/// Batch execution configuration
#[derive(Debug, Clone)]
pub struct BatchConfig {
    pub concurrency_limit: usize,
    pub max_retries: u32,
    pub timeout_duration: Duration,
    pub retry_delay: Duration,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            concurrency_limit: 10,
            max_retries: 3,
            timeout_duration: Duration::from_secs(300), // 5 minutes
            retry_delay: Duration::from_secs(1),
        }
    }
}

/// Progress callback type
pub type ProgressCallback = Box<dyn Fn(usize, usize, usize) + Send + Sync>;

/// Batch execution statistics
#[derive(Debug, Clone, Default)]
pub struct BatchStats {
    pub total_jobs: usize,
    pub completed_jobs: usize,
    pub failed_jobs: usize,
    pub running_jobs: usize,
    pub pending_jobs: usize,
    pub total_duration: Duration,
}