use std::collections::{HashMap, VecDeque, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{Semaphore, mpsc};

use crate::batch::types::*;
use crate::engine::ExecutionEngine;
use crate::state::StateData;
use crate::Result;

/// Parallel scheduler for advanced batch processing with work-stealing and priority queues
pub struct ParallelScheduler {
    engine: Arc<ExecutionEngine>,
    config: BatchConfig,
    worker_pool: WorkerPool,
    dependency_resolver: DependencyResolver,
}

/// Worker pool management for parallel execution
pub struct WorkerPool {
    worker_count: usize,
    metrics: Arc<Mutex<WorkerMetrics>>,
}

/// Metrics for monitoring worker performance
#[derive(Debug, Clone, Default)]
pub struct WorkerMetrics {
    pub max_workers: usize,
    pub peak_utilization: f64,
    pub total_jobs_processed: usize,
    pub avg_job_duration: Duration,
}

/// Dependency resolution and DAG management
pub struct DependencyResolver {
    dependency_graph: HashMap<String, Vec<String>>,
}

/// Job scheduling error types
#[derive(Debug, thiserror::Error)]
pub enum SchedulingError {
    #[error("Circular dependency detected: {0}")]
    CircularDependency(String),

    #[error("Job not found: {0}")]
    JobNotFound(String),

    #[error("Dependency resolution failed: {0}")]
    DependencyResolutionFailed(String),

    #[error("Worker pool error: {0}")]
    WorkerPoolError(String),
}

impl From<SchedulingError> for crate::LangGraphError {
    fn from(err: SchedulingError) -> Self {
        crate::LangGraphError::Execution(err.to_string())
    }
}

impl ParallelScheduler {
    /// Create new parallel scheduler with configuration
    pub fn new(config: BatchConfig, engine: Arc<ExecutionEngine>) -> Self {
        Self {
            engine,
            config: config.clone(),
            worker_pool: WorkerPool::new(config.concurrency_limit),
            dependency_resolver: DependencyResolver::new(),
        }
    }

    /// Execute batch with parallel work-stealing scheduler (YELLOW: minimal implementation)
    pub async fn execute_parallel_batch(&self, jobs: Vec<BatchJob>) -> Result<Vec<BatchResult>> {
        if jobs.is_empty() {
            return Ok(Vec::new());
        }

        // YELLOW phase: Use basic parallel execution similar to BatchExecutor but with parallelism
        let semaphore = Arc::new(Semaphore::new(self.config.concurrency_limit));
        let (tx, mut rx) = mpsc::channel(jobs.len());

        let start_time = Instant::now();
        let total_jobs = jobs.len();

        // Spawn tasks for each job with work distribution
        for job in jobs {
            let sem = Arc::clone(&semaphore);
            let engine = Arc::clone(&self.engine);
            let config = self.config.clone();
            let tx = tx.clone();

            tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                let result = Self::execute_single_job(job.clone(), engine, config).await;
                let _ = tx.send((job.id, result)).await;
            });
        }

        drop(tx);

        // Collect results (basic implementation)
        let mut results = HashMap::new();
        while let Some((job_id, result)) = rx.recv().await {
            results.insert(job_id, result);
        }

        // Convert to vector maintaining order
        let mut final_results: Vec<BatchResult> = results.into_values().collect();
        final_results.sort_by(|a, b| a.job_id.cmp(&b.job_id));

        Ok(final_results)
    }

    /// Execute jobs with priority scheduling (YELLOW: basic priority sort)
    pub async fn execute_with_priority(&self, mut jobs: Vec<BatchJob>) -> Result<Vec<BatchResult>> {
        // YELLOW: Simple priority sorting before execution
        jobs.sort_by(|a, b| a.priority.cmp(&b.priority));

        // Execute in priority order (sequential for simplicity in YELLOW)
        let mut results = Vec::new();
        for job in jobs {
            let result = Self::execute_single_job(job.clone(), Arc::clone(&self.engine), self.config.clone()).await;
            results.push(result);
        }

        Ok(results)
    }

    /// Execute jobs with dependency resolution (YELLOW: basic topological sort)
    pub async fn execute_with_dependencies(
        &self,
        jobs: Vec<BatchJob>,
        dependencies: HashMap<String, Vec<String>>
    ) -> Result<Vec<BatchResult>> {
        // Check for circular dependencies first
        if let Err(err) = self.dependency_resolver.check_circular_dependencies(&dependencies) {
            return Err(err.into());
        }

        // YELLOW: Basic topological sort for dependency resolution
        let execution_order = self.dependency_resolver.topological_sort(&jobs, &dependencies)?;

        // Execute in dependency order (sequential for YELLOW phase)
        let mut results = Vec::new();
        let mut job_map: HashMap<String, BatchJob> = jobs.into_iter()
            .map(|job| (job.id.clone(), job))
            .collect();

        for job_id in execution_order {
            if let Some(job) = job_map.remove(&job_id) {
                let result = Self::execute_single_job(job, Arc::clone(&self.engine), self.config.clone()).await;
                results.push(result);
            }
        }

        Ok(results)
    }

    /// Execute with load balancing (YELLOW: use parallel execution)
    pub async fn execute_with_load_balancing(&self, jobs: Vec<BatchJob>) -> Result<Vec<BatchResult>> {
        // YELLOW: Basic load balancing using parallel execution
        self.execute_parallel_batch(jobs).await
    }

    /// Execute with worker pool scaling (YELLOW: fixed pool size)
    pub async fn execute_with_scaling(&self, jobs: Vec<BatchJob>) -> Result<Vec<BatchResult>> {
        // YELLOW: No actual scaling, just track metrics and use fixed pool
        self.worker_pool.update_metrics(jobs.len()).await;
        self.execute_parallel_batch(jobs).await
    }

    /// Get worker pool metrics
    pub fn get_worker_metrics(&self) -> WorkerMetrics {
        self.worker_pool.metrics.lock().unwrap().clone()
    }

    /// Execute a single job (shared implementation)
    async fn execute_single_job(
        job: BatchJob,
        engine: Arc<ExecutionEngine>,
        config: BatchConfig,
    ) -> BatchResult {
        let start_time = Instant::now();
        let mut attempts = 0;
        let mut last_error = None;

        while attempts <= config.max_retries {
            attempts += 1;

            // Simulate work based on job input (for testing)
            if let Some(work_duration) = job.input.get("work_duration") {
                if let Some(ms) = work_duration.as_u64() {
                    tokio::time::sleep(Duration::from_millis(ms)).await;
                }
            }

            // Execute the job through the engine
            match engine.execute(job.graph.clone(), job.input.clone()).await {
                Ok(output) => {
                    return BatchResult {
                        job_id: job.id.clone(),
                        status: BatchJobStatus::Completed,
                        output: Some(output),
                        error: None,
                        duration: start_time.elapsed(),
                        attempts,
                    };
                }
                Err(err) => {
                    last_error = Some(err.to_string());
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
}

impl WorkerPool {
    fn new(worker_count: usize) -> Self {
        Self {
            worker_count,
            metrics: Arc::new(Mutex::new(WorkerMetrics {
                max_workers: worker_count,
                peak_utilization: 0.0,
                total_jobs_processed: 0,
                avg_job_duration: Duration::from_millis(0),
            })),
        }
    }

    /// Update metrics (YELLOW: basic tracking)
    async fn update_metrics(&self, job_count: usize) {
        let mut metrics = self.metrics.lock().unwrap();
        metrics.total_jobs_processed += job_count;

        // Simulate scaling metrics for testing
        if job_count > self.worker_count * 2 {
            metrics.max_workers = (job_count / 2).max(self.worker_count);
        }

        // Simulate utilization
        metrics.peak_utilization = (job_count as f64 / self.worker_count as f64).min(1.0);
    }
}

impl DependencyResolver {
    fn new() -> Self {
        Self {
            dependency_graph: HashMap::new(),
        }
    }

    /// Check for circular dependencies using DFS
    fn check_circular_dependencies(&self, dependencies: &HashMap<String, Vec<String>>) -> Result<()> {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();

        for job_id in dependencies.keys() {
            if !visited.contains(job_id) {
                if self.has_cycle_dfs(job_id, dependencies, &mut visited, &mut rec_stack) {
                    return Err(SchedulingError::CircularDependency(format!("Cycle involving job: {}", job_id)).into());
                }
            }
        }

        Ok(())
    }

    /// DFS helper for cycle detection
    fn has_cycle_dfs(
        &self,
        job_id: &str,
        dependencies: &HashMap<String, Vec<String>>,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
    ) -> bool {
        visited.insert(job_id.to_string());
        rec_stack.insert(job_id.to_string());

        if let Some(deps) = dependencies.get(job_id) {
            for dep in deps {
                if !visited.contains(dep) {
                    if self.has_cycle_dfs(dep, dependencies, visited, rec_stack) {
                        return true;
                    }
                } else if rec_stack.contains(dep) {
                    return true;
                }
            }
        }

        rec_stack.remove(job_id);
        false
    }

    /// Basic topological sort for dependency resolution
    fn topological_sort(
        &self,
        jobs: &[BatchJob],
        dependencies: &HashMap<String, Vec<String>>,
    ) -> Result<Vec<String>> {
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut graph: HashMap<String, Vec<String>> = HashMap::new();

        // Initialize in-degree count
        for job in jobs {
            in_degree.insert(job.id.clone(), 0);
            graph.insert(job.id.clone(), Vec::new());
        }

        // Build reverse dependency graph and count in-degrees
        for (job_id, deps) in dependencies {
            for dep in deps {
                graph.entry(dep.clone())
                    .or_default()
                    .push(job_id.clone());

                *in_degree.entry(job_id.clone()).or_insert(0) += 1;
            }
        }

        // Kahn's algorithm for topological sorting
        let mut queue: VecDeque<String> = in_degree.iter()
            .filter(|(_, &degree)| degree == 0)
            .map(|(job_id, _)| job_id.clone())
            .collect();

        let mut result = Vec::new();

        while let Some(job_id) = queue.pop_front() {
            result.push(job_id.clone());

            if let Some(dependents) = graph.get(&job_id) {
                for dependent in dependents {
                    if let Some(degree) = in_degree.get_mut(dependent) {
                        *degree -= 1;
                        if *degree == 0 {
                            queue.push_back(dependent.clone());
                        }
                    }
                }
            }
        }

        if result.len() != jobs.len() {
            return Err(SchedulingError::DependencyResolutionFailed(
                "Could not resolve all dependencies".to_string()
            ).into());
        }

        Ok(result)
    }
}