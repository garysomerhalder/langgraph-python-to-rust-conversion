//! Batch Executor Implementation - GREEN PHASE
//!
//! Production-ready batch processing with:
//! - Concurrent execution with semaphore-based limiting
//! - Priority-based scheduling
//! - Comprehensive error handling and retry logic
//! - Progress tracking and observability
//! - Timeout enforcement per job
//! - Detailed metrics and logging

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use serde::{Serialize, Deserialize};
use tracing::{info, warn, error, debug, instrument};

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

    /// Execute a batch of jobs with comprehensive observability
    #[instrument(skip(self, jobs), fields(job_count = jobs.len()))]
    pub async fn execute_batch(&self, mut jobs: Vec<BatchJob>) -> Result<Vec<BatchResult>> {
        let batch_start = Instant::now();

        if jobs.is_empty() {
            info!("No jobs to execute in batch");
            return Ok(Vec::new());
        }

        let total_jobs = jobs.len();
        info!(
            total_jobs = total_jobs,
            concurrency_limit = self.concurrency_limit,
            max_retries = self.max_retries,
            timeout = ?self.timeout_duration,
            "Starting batch execution"
        );

        // Sort by priority (highest first)
        jobs.sort_by(|a, b| b.priority.cmp(&a.priority));
        debug!("Jobs sorted by priority (highest first)");

        let semaphore = Arc::new(Semaphore::new(self.concurrency_limit));

        // Progress tracking with atomic counters
        let completed_count = Arc::new(AtomicUsize::new(0));
        let failed_count = Arc::new(AtomicUsize::new(0));
        let timeout_count = Arc::new(AtomicUsize::new(0));
        let progress_callback = self.progress_callback.clone();

        // Spawn tasks for all jobs
        let mut tasks = Vec::new();

        for job in jobs {
            let job_id = job.id.clone();
            let sem = semaphore.clone();
            let timeout = self.timeout_duration;
            let max_retries = self.max_retries;
            let completed = completed_count.clone();
            let failed = failed_count.clone();
            let timeouts = timeout_count.clone();
            let progress = progress_callback.clone();

            let task = tokio::spawn(async move {
                debug!(job_id = %job_id, "Job starting");

                // Acquire semaphore permit (limits concurrency)
                let _permit = sem.acquire().await.expect("Semaphore should not be closed");
                debug!(job_id = %job_id, "Acquired execution slot");

                // Execute job with retry logic
                let start = Instant::now();
                let mut last_error = None;
                let mut attempts = 0u32;

                let result = loop {
                    attempts += 1;
                    debug!(job_id = %job_id, attempt = attempts, "Executing job");

                    match Self::execute_single_job(job.clone(), timeout).await {
                        Ok(output) => break Ok(output),
                        Err(e) => {
                            last_error = Some(e);
                            if attempts > max_retries {
                                warn!(
                                    job_id = %job_id,
                                    attempts = attempts,
                                    "Job failed after all retries"
                                );
                                break Err(last_error.unwrap());
                            }
                            warn!(
                                job_id = %job_id,
                                attempt = attempts,
                                max_retries = max_retries,
                                "Job failed, retrying"
                            );
                            // Simple exponential backoff
                            tokio::time::sleep(Duration::from_millis(100 * 2u64.pow(attempts - 1))).await;
                        }
                    }
                };

                let duration = start.elapsed();

                // Update progress and counters
                let current_completed = completed.fetch_add(1, Ordering::SeqCst) + 1;
                if let Some(ref callback) = progress {
                    callback(current_completed, total_jobs);
                }

                // Build result with detailed error classification
                match result {
                    Ok(output) => {
                        info!(
                            job_id = %job_id,
                            duration_ms = duration.as_millis(),
                            attempts = attempts,
                            "Job completed successfully"
                        );
                        BatchResult {
                            job_id: job.id,
                            status: BatchJobStatus::Completed,
                            output: Some(output),
                            error: None,
                            duration,
                            metadata: {
                                let mut meta = job.metadata;
                                meta.insert("attempts".to_string(), serde_json::json!(attempts));
                                meta
                            },
                        }
                    },
                    Err(e) => {
                        // Check if it was a timeout
                        let error_str = e.to_string();
                        let is_timeout = error_str.contains("timeout") ||
                                       error_str.contains("timed out");

                        if is_timeout {
                            timeouts.fetch_add(1, Ordering::SeqCst);
                            error!(
                                job_id = %job_id,
                                duration_ms = duration.as_millis(),
                                "Job timed out"
                            );
                        } else {
                            failed.fetch_add(1, Ordering::SeqCst);
                            error!(
                                job_id = %job_id,
                                duration_ms = duration.as_millis(),
                                error = %error_str,
                                "Job failed"
                            );
                        }

                        BatchResult {
                            job_id: job.id,
                            status: if is_timeout {
                                BatchJobStatus::TimedOut
                            } else {
                                BatchJobStatus::Failed
                            },
                            output: None,
                            error: Some(error_str),
                            duration,
                            metadata: {
                                let mut meta = job.metadata;
                                meta.insert("attempts".to_string(), serde_json::json!(attempts));
                                meta
                            },
                        }
                    }
                }
            });

            tasks.push(task);
        }

        // Collect all results with error handling
        let mut results = Vec::new();
        for task in tasks {
            match task.await {
                Ok(result) => results.push(result),
                Err(e) => {
                    error!("Task join error: {}", e);
                    failed_count.fetch_add(1, Ordering::SeqCst);
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

        // Calculate batch statistics
        let batch_duration = batch_start.elapsed();
        let final_completed = completed_count.load(Ordering::SeqCst);
        let final_failed = failed_count.load(Ordering::SeqCst);
        let final_timeouts = timeout_count.load(Ordering::SeqCst);

        let success_rate = if total_jobs > 0 {
            ((final_completed - final_failed - final_timeouts) as f64 / total_jobs as f64) * 100.0
        } else {
            0.0
        };

        // Log batch summary with comprehensive metrics
        info!(
            total_jobs = total_jobs,
            completed = final_completed,
            failed = final_failed,
            timed_out = final_timeouts,
            success_rate = format!("{:.2}%", success_rate),
            batch_duration_ms = batch_duration.as_millis(),
            avg_job_duration_ms = if total_jobs > 0 {
                results.iter().map(|r| r.duration.as_millis()).sum::<u128>() / total_jobs as u128
            } else {
                0
            },
            "Batch execution completed"
        );

        // Log warning if high failure rate
        if success_rate < 80.0 && total_jobs > 5 {
            warn!(
                success_rate = format!("{:.2}%", success_rate),
                failed = final_failed,
                timed_out = final_timeouts,
                "High failure rate detected in batch execution"
            );
        }

        Ok(results)
    }

    /// Execute a single job with timeout enforcement
    #[instrument(skip(job), fields(job_id = %job.id))]
    async fn execute_single_job(
        job: BatchJob,
        timeout: Option<Duration>,
    ) -> Result<StateData> {
        let execution = async {
            debug!(job_id = %job.id, "Starting graph execution");
            let engine = ExecutionEngine::new();
            let result = engine.execute(job.graph, job.input).await;

            match &result {
                Ok(_) => debug!(job_id = %job.id, "Graph execution completed successfully"),
                Err(e) => error!(job_id = %job.id, error = %e, "Graph execution failed"),
            }

            result
        };

        // Apply timeout if configured
        if let Some(timeout_duration) = timeout {
            match tokio::time::timeout(timeout_duration, execution).await {
                Ok(result) => result,
                Err(_) => {
                    error!(
                        job_id = %job.id,
                        timeout_ms = timeout_duration.as_millis(),
                        "Job timed out"
                    );
                    Err(crate::LangGraphError::Execution(
                        format!("Job {} timed out after {:?}", job.id, timeout_duration)
                    ).into())
                },
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
