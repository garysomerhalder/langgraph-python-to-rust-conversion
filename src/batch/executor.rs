//! Batch Executor Implementation
//!
//! Provides concurrent execution of multiple graph workflows with
//! resource management, progress tracking, and error handling.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use serde::{Serialize, Deserialize};

use crate::graph::CompiledGraph;
use crate::state::StateData;
use crate::engine::ExecutionEngine;
use crate::Result;

/// Status of a batch job
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BatchJobStatus {
    /// Job is pending execution
    Pending,
    /// Job is currently running
    Running,
    /// Job completed successfully
    Completed,
    /// Job failed with error
    Failed,
    /// Job timed out
    TimedOut,
    /// Job was cancelled
    Cancelled,
}

/// A single job in a batch
#[derive(Clone)]
pub struct BatchJob {
    /// Unique identifier for this job
    pub id: String,
    /// The compiled graph to execute
    pub graph: CompiledGraph,
    /// Initial input state
    pub input: StateData,
    /// Priority (higher number = higher priority)
    pub priority: u8,
    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Result of a batch job execution
#[derive(Debug, Clone)]
pub struct BatchResult {
    /// Job identifier
    pub job_id: String,
    /// Final status
    pub status: BatchJobStatus,
    /// Output state (if successful)
    pub output: Option<StateData>,
    /// Error message (if failed)
    pub error: Option<String>,
    /// Execution duration
    pub duration: Duration,
    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Callback for progress updates
pub type ProgressCallback = Box<dyn Fn(usize, usize) + Send + Sync>;

/// Batch executor for processing multiple workflows
pub struct BatchExecutor {
    /// Maximum concurrent jobs
    concurrency_limit: usize,
    /// Maximum retries for failed jobs
    max_retries: u32,
    /// Timeout for individual jobs
    timeout_duration: Option<Duration>,
    /// Progress callback
    progress_callback: Option<Arc<ProgressCallback>>,
}

impl BatchExecutor {
    /// Create a new batch executor with default settings
    pub fn new() -> Self {
        Self {
            concurrency_limit: num_cpus::get(),
            max_retries: 0,
            timeout_duration: None,
            progress_callback: None,
        }
    }

    /// Set concurrency limit (number of parallel jobs)
    pub fn with_concurrency_limit(mut self, limit: usize) -> Self {
        self.concurrency_limit = limit.max(1);
        self
    }

    /// Set maximum retries for failed jobs
    pub fn with_max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }

    /// Set timeout for individual jobs
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout_duration = Some(timeout);
        self
    }

    /// Set progress callback
    pub fn with_progress_callback<F>(mut self, callback: F) -> Self
    where
        F: Fn(usize, usize) + Send + Sync + 'static,
    {
        self.progress_callback = Some(Arc::new(Box::new(callback)));
        self
    }

    /// Execute a batch of jobs
    pub async fn execute_batch(&self, mut jobs: Vec<BatchJob>) -> Result<Vec<BatchResult>> {
        if jobs.is_empty() {
            return Ok(Vec::new());
        }

        // Sort by priority (highest first)
        jobs.sort_by(|a, b| b.priority.cmp(&a.priority));

        let total_jobs = jobs.len();
        let semaphore = Arc::new(Semaphore::new(self.concurrency_limit));

        // Progress tracking
        let completed_count = Arc::new(tokio::sync::Mutex::new(0usize));
        let progress_callback = self.progress_callback.clone();

        // Spawn tasks for all jobs
        let mut tasks = Vec::new();

        for job in jobs {
            let sem = semaphore.clone();
            let timeout = self.timeout_duration;
            let completed = completed_count.clone();
            let progress = progress_callback.clone();

            let task = tokio::spawn(async move {
                // Acquire semaphore permit (limits concurrency)
                let _permit = sem.acquire().await.expect("Semaphore should not be closed");

                // Execute job
                let start = Instant::now();
                let result = Self::execute_single_job(job.clone(), timeout).await;

                // Update progress
                {
                    let mut count = completed.lock().await;
                    *count += 1;

                    if let Some(ref callback) = progress {
                        callback(*count, total_jobs);
                    }
                }

                // Build result
                let duration = start.elapsed();
                match result {
                    Ok(output) => BatchResult {
                        job_id: job.id,
                        status: BatchJobStatus::Completed,
                        output: Some(output),
                        error: None,
                        duration,
                        metadata: job.metadata,
                    },
                    Err(e) => {
                        // Check if it was a timeout
                        let is_timeout = e.to_string().contains("timeout") ||
                                       e.to_string().contains("timed out");

                        BatchResult {
                            job_id: job.id,
                            status: if is_timeout {
                                BatchJobStatus::TimedOut
                            } else {
                                BatchJobStatus::Failed
                            },
                            output: None,
                            error: Some(e.to_string()),
                            duration,
                            metadata: job.metadata,
                        }
                    }
                }
            });

            tasks.push(task);
        }

        // Collect all results
        let mut results = Vec::new();
        for task in tasks {
            match task.await {
                Ok(result) => results.push(result),
                Err(e) => {
                    tracing::error!("Task join error: {}", e);
                    // Create error result for failed task
                    results.push(BatchResult {
                        job_id: "unknown".to_string(),
                        status: BatchJobStatus::Failed,
                        output: None,
                        error: Some(format!("Task join error: {}", e)),
                        duration: Duration::from_secs(0),
                        metadata: HashMap::new(),
                    });
                }
            }
        }

        Ok(results)
    }

    /// Execute a single job
    async fn execute_single_job(
        job: BatchJob,
        timeout: Option<Duration>,
    ) -> Result<StateData> {
        let execution = async {
            let engine = ExecutionEngine::new();
            engine.execute(job.graph, job.input).await
        };

        // Apply timeout if configured
        if let Some(timeout_duration) = timeout {
            match tokio::time::timeout(timeout_duration, execution).await {
                Ok(result) => result,
                Err(_) => Err(crate::LangGraphError::Execution(
                    format!("Job {} timed out after {:?}", job.id, timeout_duration)
                ).into()),
            }
        } else {
            execution.await
        }
    }
}

impl Default for BatchExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_executor_creation() {
        let executor = BatchExecutor::new();
        assert!(executor.concurrency_limit > 0);
        assert_eq!(executor.max_retries, 0);
        assert!(executor.timeout_duration.is_none());
    }

    #[test]
    fn test_batch_executor_configuration() {
        let executor = BatchExecutor::new()
            .with_concurrency_limit(4)
            .with_max_retries(3)
            .with_timeout(Duration::from_secs(60));

        assert_eq!(executor.concurrency_limit, 4);
        assert_eq!(executor.max_retries, 3);
        assert_eq!(executor.timeout_duration, Some(Duration::from_secs(60)));
    }
}
