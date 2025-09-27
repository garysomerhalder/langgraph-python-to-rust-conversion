use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Semaphore, mpsc};
use tokio::time::timeout;

use crate::batch::types::*;
use crate::engine::ExecutionEngine;
use crate::state::StateData;
use crate::Result;

/// Batch executor for processing multiple workflows
pub struct BatchExecutor {
    config: BatchConfig,
    engine: Arc<ExecutionEngine>,
    progress_callback: Option<ProgressCallback>,
}

impl BatchExecutor {
    /// Create new batch executor with configuration
    pub fn new(config: BatchConfig, engine: Arc<ExecutionEngine>) -> Self {
        Self {
            config,
            engine,
            progress_callback: None,
        }
    }

    /// Set progress callback for monitoring
    pub fn with_progress_callback(mut self, callback: ProgressCallback) -> Self {
        self.progress_callback = Some(callback);
        self
    }

    /// Execute a batch of jobs
    pub async fn execute_batch(&self, jobs: Vec<BatchJob>) -> Result<Vec<BatchResult>> {
        if jobs.is_empty() {
            return Ok(Vec::new());
        }

        let start_time = Instant::now();
        let semaphore = Arc::new(Semaphore::new(self.config.concurrency_limit));
        let (tx, mut rx) = mpsc::channel(jobs.len());

        let total_jobs = jobs.len();
        let mut results = HashMap::new();
        let mut completed = 0;
        let mut failed = 0;

        // Spawn tasks for each job
        for job in jobs {
            let sem = Arc::clone(&semaphore);
            let engine = Arc::clone(&self.engine);
            let config = self.config.clone();
            let tx = tx.clone();

            tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                let result = Self::execute_job(job.clone(), engine, config).await;
                let _ = tx.send((job.id, result)).await;
            });
        }

        drop(tx); // Close sender

        // Collect results
        while let Some((job_id, result)) = rx.recv().await {
            match &result.status {
                BatchJobStatus::Completed => completed += 1,
                BatchJobStatus::Failed => failed += 1,
                _ => {}
            }

            results.insert(job_id.clone(), result);

            // Call progress callback if set
            if let Some(ref callback) = self.progress_callback {
                let running = total_jobs - completed - failed;
                callback(completed, failed, running);
            }
        }

        // Convert HashMap to Vec maintaining original order by job_id
        let mut final_results: Vec<BatchResult> = results.into_values().collect();
        final_results.sort_by(|a, b| a.job_id.cmp(&b.job_id));

        Ok(final_results)
    }

    /// Execute a single job with retries
    async fn execute_job(
        job: BatchJob,
        engine: Arc<ExecutionEngine>,
        config: BatchConfig,
    ) -> BatchResult {
        let start_time = Instant::now();
        let mut attempts = 0;
        let mut last_error = None;

        while attempts <= config.max_retries {
            attempts += 1;

            let result = timeout(
                config.timeout_duration,
                Self::run_single_job(&job, Arc::clone(&engine))
            ).await;

            match result {
                Ok(Ok(output)) => {
                    return BatchResult {
                        job_id: job.id.clone(),
                        status: BatchJobStatus::Completed,
                        output: Some(output),
                        error: None,
                        duration: start_time.elapsed(),
                        attempts,
                    };
                }
                Ok(Err(err)) => {
                    last_error = Some(err.to_string());
                    if attempts <= config.max_retries {
                        tokio::time::sleep(config.retry_delay).await;
                    }
                }
                Err(_) => {
                    last_error = Some("Job timed out".to_string());
                    if attempts <= config.max_retries {
                        tokio::time::sleep(config.retry_delay).await;
                    }
                }
            }
        }

        BatchResult {
            job_id: job.id.clone(),
            status: BatchJobStatus::Failed,
            output: None,
            error: last_error,
            duration: start_time.elapsed(),
            attempts,
        }
    }

    /// Run a single job through the execution engine
    async fn run_single_job(
        job: &BatchJob,
        engine: Arc<ExecutionEngine>,
    ) -> Result<StateData> {
        // Integration-First: Use real execution engine
        engine.execute(job.graph.clone(), job.input.clone()).await
    }

    /// Get batch execution statistics
    pub fn calculate_stats(&self, results: &[BatchResult]) -> BatchStats {
        let mut stats = BatchStats::default();
        stats.total_jobs = results.len();

        for result in results {
            match result.status {
                BatchJobStatus::Completed => stats.completed_jobs += 1,
                BatchJobStatus::Failed => stats.failed_jobs += 1,
                BatchJobStatus::Running => stats.running_jobs += 1,
                BatchJobStatus::Pending => stats.pending_jobs += 1,
                _ => {}
            }
            stats.total_duration += result.duration;
        }

        stats
    }
}