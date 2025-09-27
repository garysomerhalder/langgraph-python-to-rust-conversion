use std::collections::HashMap;
use std::fmt::{Debug, Display};
use std::sync::Arc;
use std::time::Duration;
use std::future::Future;
use std::pin::Pin;
use tokio::sync::{mpsc, Mutex};
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use crate::batch::{BatchResult, BatchJob, BatchJobStatus};
use crate::LangGraphError;

/// Comprehensive batch error handling and recovery system
#[derive(Debug)]
pub struct BatchErrorHandler {
    retry_strategy: RetryStrategy,
    dead_letter_queue: Arc<DeadLetterQueue>,
    circuit_breaker: Arc<CircuitBreaker>,
    error_reporter: Arc<ErrorReporter>,
    error_classifiers: Vec<Arc<dyn ErrorClassifier>>,
}

/// Error classification for different error types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ErrorType {
    /// Transient error - retry immediately
    Transient(TransientError),
    /// Recoverable error - retry with backoff
    Recoverable(RecoverableError),
    /// Permanent error - move to dead letter queue
    Permanent(PermanentError),
    /// Fatal error - fail entire batch
    Fatal(FatalError),
}

/// Transient error that should be retried immediately
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransientError {
    pub message: String,
    pub code: String,
    pub context: HashMap<String, String>,
}

/// Recoverable error that should be retried with backoff
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoverableError {
    pub message: String,
    pub code: String,
    pub retry_after: Option<Duration>,
    pub context: HashMap<String, String>,
}

/// Permanent error that should go to dead letter queue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermanentError {
    pub message: String,
    pub code: String,
    pub reason: String,
    pub context: HashMap<String, String>,
}

/// Fatal error that should fail the entire batch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FatalError {
    pub message: String,
    pub code: String,
    pub severity: String,
    pub context: HashMap<String, String>,
}

/// Retry strategy configuration
#[derive(Clone)]
pub struct RetryStrategy {
    pub max_attempts: u32,
    pub backoff_strategy: BackoffStrategy,
    pub jitter: bool,
    pub retry_predicates: Vec<RetryPredicate>,
}

impl std::fmt::Debug for RetryStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RetryStrategy")
            .field("max_attempts", &self.max_attempts)
            .field("backoff_strategy", &self.backoff_strategy)
            .field("jitter", &self.jitter)
            .field("retry_predicates", &format!("{} predicates", self.retry_predicates.len()))
            .finish()
    }
}

/// Backoff strategy for retries
#[derive(Debug, Clone)]
pub enum BackoffStrategy {
    Fixed(Duration),
    Linear { base: Duration, increment: Duration },
    Exponential { base: Duration, multiplier: f64, max: Duration },
}

/// Predicate function for determining if an error should be retried
pub type RetryPredicate = Arc<dyn Fn(&ErrorType) -> bool + Send + Sync>;

/// Dead letter queue for persistently failing jobs
#[derive(Debug)]
pub struct DeadLetterQueue {
    failed_jobs: Arc<Mutex<HashMap<String, FailedJob>>>,
    storage: Arc<dyn DeadLetterStorage>,
    max_size: usize,
}

/// Information about a failed job in the dead letter queue
#[derive(Debug, Clone)]
pub struct FailedJob {
    pub job: BatchJob,
    pub error: ErrorType,
    pub failure_count: u32,
    pub first_failure: DateTime<Utc>,
    pub last_failure: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
}

/// Storage interface for dead letter queue
pub trait DeadLetterStorage: Send + Sync + Debug {
    fn store_failed_job(&self, job_id: &str, failed_job: &FailedJob) -> Pin<Box<dyn Future<Output = Result<(), LangGraphError>> + Send + '_>>;
    fn retrieve_failed_job(&self, job_id: &str) -> Pin<Box<dyn Future<Output = Result<Option<FailedJob>, LangGraphError>> + Send + '_>>;
    fn list_failed_jobs(&self) -> Pin<Box<dyn Future<Output = Result<Vec<String>, LangGraphError>> + Send + '_>>;
    fn remove_failed_job(&self, job_id: &str) -> Pin<Box<dyn Future<Output = Result<(), LangGraphError>> + Send + '_>>;
}

/// Circuit breaker for external dependencies
#[derive(Debug)]
pub struct CircuitBreaker {
    state: Arc<Mutex<CircuitBreakerState>>,
    failure_threshold: u32,
    recovery_timeout: Duration,
    half_open_max_calls: u32,
}

/// Circuit breaker state
#[derive(Debug, Clone)]
pub enum CircuitBreakerState {
    Closed { failure_count: u32 },
    Open { opened_at: DateTime<Utc> },
    HalfOpen { test_calls: u32 },
}

/// Error reporting and aggregation
#[derive(Debug)]
pub struct ErrorReporter {
    error_aggregator: Arc<Mutex<ErrorAggregator>>,
    alerts: Vec<Arc<dyn AlertHandler>>,
}

/// Error aggregation for reporting
#[derive(Debug, Default, Clone)]
pub struct ErrorAggregator {
    pub error_counts: HashMap<String, u64>,
    pub error_trends: Vec<ErrorTrend>,
    pub failure_rates: HashMap<String, f64>,
}

/// Error trend over time
#[derive(Debug, Clone)]
pub struct ErrorTrend {
    pub timestamp: DateTime<Utc>,
    pub error_type: String,
    pub count: u64,
}

/// Alert handler for error notifications
pub trait AlertHandler: Send + Sync + Debug {
    fn handle_alert(&self, alert: &ErrorAlert) -> Pin<Box<dyn Future<Output = Result<(), LangGraphError>> + Send + '_>>;
}

/// Error alert information
#[derive(Debug, Clone)]
pub struct ErrorAlert {
    pub severity: AlertSeverity,
    pub message: String,
    pub error_type: String,
    pub metadata: HashMap<String, String>,
}

/// Alert severity levels
#[derive(Debug, Clone, PartialEq)]
pub enum AlertSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

impl Display for AlertSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlertSeverity::Info => write!(f, "Info"),
            AlertSeverity::Warning => write!(f, "Warning"),
            AlertSeverity::Error => write!(f, "Error"),
            AlertSeverity::Critical => write!(f, "Critical"),
        }
    }
}

/// Error classification interface
pub trait ErrorClassifier: Send + Sync + Debug {
    fn classify_error(&self, error: &LangGraphError, context: &ErrorContext) -> ErrorType;
}

/// Context information for error classification
#[derive(Debug, Clone)]
pub struct ErrorContext {
    pub job_id: String,
    pub attempt_number: u32,
    pub total_attempts: u32,
    pub execution_duration: Duration,
    pub metadata: HashMap<String, String>,
}

/// Batch recovery mechanisms
#[derive(Debug)]
pub struct BatchRecovery {
    recovery_strategies: Vec<Arc<dyn RecoveryStrategy>>,
    recovery_history: Arc<Mutex<Vec<RecoveryAttempt>>>,
}

/// Recovery strategy interface
pub trait RecoveryStrategy: Send + Sync + Debug {
    fn can_recover(&self, error: &ErrorType, context: &ErrorContext) -> Pin<Box<dyn Future<Output = bool> + Send + '_>>;
    fn recover(&self, job: &BatchJob, error: &ErrorType) -> Pin<Box<dyn Future<Output = Result<BatchJob, LangGraphError>> + Send + '_>>;
}

/// Recovery attempt record
#[derive(Debug, Clone)]
pub struct RecoveryAttempt {
    pub job_id: String,
    pub strategy_used: String,
    pub timestamp: DateTime<Utc>,
    pub success: bool,
    pub error: Option<String>,
}

impl BatchErrorHandler {
    /// Create a new batch error handler
    pub fn new(retry_strategy: RetryStrategy) -> Self {
        Self {
            retry_strategy,
            dead_letter_queue: Arc::new(DeadLetterQueue::new()),
            circuit_breaker: Arc::new(CircuitBreaker::new()),
            error_reporter: Arc::new(ErrorReporter::new()),
            error_classifiers: vec![Arc::new(DefaultErrorClassifier)],
        }
    }

    /// Create with custom configuration
    pub fn with_config(
        retry_strategy: RetryStrategy,
        dead_letter_storage: Arc<dyn DeadLetterStorage>,
        alert_handlers: Vec<Arc<dyn AlertHandler>>,
    ) -> Self {
        let mut error_reporter = ErrorReporter::new();
        error_reporter.alerts = alert_handlers;

        Self {
            retry_strategy,
            dead_letter_queue: Arc::new(DeadLetterQueue::with_storage(dead_letter_storage)),
            circuit_breaker: Arc::new(CircuitBreaker::new()),
            error_reporter: Arc::new(error_reporter),
            error_classifiers: vec![Arc::new(DefaultErrorClassifier)],
        }
    }

    /// Add a custom error classifier
    pub fn add_error_classifier(&mut self, classifier: Arc<dyn ErrorClassifier>) {
        self.error_classifiers.push(classifier);
    }

    /// Get current circuit breaker state
    pub async fn get_circuit_state(&self) -> CircuitBreakerState {
        self.circuit_breaker.state.lock().await.clone()
    }

    /// Get error statistics
    pub async fn get_error_stats(&self) -> ErrorAggregator {
        self.error_reporter.get_error_stats().await
    }

    /// Process batch with error handling
    pub async fn process_batch_with_handling<F, Fut>(
        &self,
        jobs: Vec<BatchJob>,
        executor: F,
    ) -> Vec<BatchResult>
    where
        F: Fn(BatchJob) -> Fut + Clone,
        Fut: Future<Output = Result<BatchResult, LangGraphError>> + Send,
    {
        let mut results = Vec::new();

        for job in jobs {
            let mut attempt = 0;

            // Check circuit breaker
            if !self.circuit_breaker.allow_call().await {
                results.push(BatchResult {
                    job_id: job.id.clone(),
                    status: BatchJobStatus::Failed,
                    output: None,
                    error: Some("Circuit breaker open".to_string()),
                    duration: Duration::from_secs(0),
                    attempts: 0,
                });
                continue;
            }

            loop {
                attempt += 1;

                let exec = executor.clone();
                match exec(job.clone()).await {
                    Ok(result) => {
                        self.circuit_breaker.record_success().await;
                        results.push(result);
                        break;
                    }
                    Err(error) => {
                        self.circuit_breaker.record_failure().await;

                        let error_string = error.to_string();
                        let context = ErrorContext {
                            job_id: job.id.clone(),
                            attempt_number: attempt,
                            total_attempts: self.retry_strategy.max_attempts,
                            execution_duration: Duration::from_secs(0),
                            metadata: HashMap::new(),
                        };

                        match self.handle_error(&job, error, context).await {
                            Ok(ErrorHandlingDecision::Retry { delay, .. }) => {
                                if attempt < self.retry_strategy.max_attempts {
                                    tokio::time::sleep(delay).await;
                                    continue;
                                } else {
                                    // Max attempts reached
                                    results.push(BatchResult {
                                        job_id: job.id.clone(),
                                        status: BatchJobStatus::Failed,
                                        output: None,
                                        error: Some(error_string.clone()),
                                        duration: Duration::from_secs(0),
                                        attempts: attempt,
                                    });
                                    break;
                                }
                            }
                            Ok(ErrorHandlingDecision::MoveToDeadLetter) => {
                                results.push(BatchResult {
                                    job_id: job.id.clone(),
                                    status: BatchJobStatus::Failed,
                                    output: None,
                                    error: Some(format!("Moved to DLQ: {}", error_string)),
                                    duration: Duration::from_secs(0),
                                    attempts: attempt,
                                });
                                break;
                            }
                            Ok(ErrorHandlingDecision::FailBatch) => {
                                // Fatal error - fail entire batch
                                results.push(BatchResult {
                                    job_id: job.id.clone(),
                                    status: BatchJobStatus::Failed,
                                    output: None,
                                    error: Some(format!("Fatal error: {}", error_string)),
                                    duration: Duration::from_secs(0),
                                    attempts: attempt,
                                });
                                break;
                            }
                            Ok(ErrorHandlingDecision::Ignore) => {
                                // Ignore the error and continue
                                results.push(BatchResult {
                                    job_id: job.id.clone(),
                                    status: BatchJobStatus::Completed,
                                    output: None,
                                    error: Some(format!("Ignored error: {}", error_string)),
                                    duration: Duration::from_secs(0),
                                    attempts: attempt,
                                });
                                break;
                            }
                            Err(handling_error) => {
                                // Error handling itself failed
                                results.push(BatchResult {
                                    job_id: job.id.clone(),
                                    status: BatchJobStatus::Failed,
                                    output: None,
                                    error: Some(format!("Error handling failed: {}", handling_error)),
                                    duration: Duration::from_secs(0),
                                    attempts: attempt,
                                });
                                break;
                            }
                        }
                    }
                }
            }
        }

        results
    }

    /// Handle a batch job error
    pub async fn handle_error(
        &self,
        job: &BatchJob,
        error: LangGraphError,
        context: ErrorContext,
    ) -> Result<ErrorHandlingDecision, LangGraphError> {
        // Classify error type
        let error_type = self.error_classifiers[0].classify_error(&error, &context);

        // Report the error
        let _ = self.error_reporter.report_error(&error_type, &context).await;

        // Determine handling strategy based on error type
        match error_type {
            ErrorType::Transient(_) => {
                if context.attempt_number < self.retry_strategy.max_attempts {
                    let delay = self.calculate_retry_delay(context.attempt_number);
                    Ok(ErrorHandlingDecision::Retry {
                        delay,
                        attempt: context.attempt_number + 1
                    })
                } else {
                    // Max attempts reached, move to dead letter queue
                    let _ = self.dead_letter_queue.add_failed_job(job.clone(), error_type).await;
                    Ok(ErrorHandlingDecision::MoveToDeadLetter)
                }
            },
            ErrorType::Recoverable(ref recoverable_error) => {
                if context.attempt_number < self.retry_strategy.max_attempts {
                    let delay = recoverable_error.retry_after
                        .unwrap_or_else(|| self.calculate_retry_delay(context.attempt_number));
                    Ok(ErrorHandlingDecision::Retry {
                        delay,
                        attempt: context.attempt_number + 1
                    })
                } else {
                    let _ = self.dead_letter_queue.add_failed_job(job.clone(), error_type).await;
                    Ok(ErrorHandlingDecision::MoveToDeadLetter)
                }
            },
            ErrorType::Permanent(_) => {
                let _ = self.dead_letter_queue.add_failed_job(job.clone(), error_type).await;
                Ok(ErrorHandlingDecision::MoveToDeadLetter)
            },
            ErrorType::Fatal(_) => {
                let _ = self.dead_letter_queue.add_failed_job(job.clone(), error_type).await;
                Ok(ErrorHandlingDecision::FailBatch)
            }
        }
    }

    /// Check if a job should be retried
    pub async fn should_retry(&self, error: &ErrorType, attempt: u32) -> bool {
        // Check if we've exceeded max attempts
        if attempt >= self.retry_strategy.max_attempts {
            return false;
        }

        // Check error type for retry eligibility
        match error {
            ErrorType::Transient(_) => true,
            ErrorType::Recoverable(_) => true,
            ErrorType::Permanent(_) => false,
            ErrorType::Fatal(_) => false,
        }
    }

    /// Calculate the next retry delay
    pub fn calculate_retry_delay(&self, attempt: u32) -> Duration {
        match &self.retry_strategy.backoff_strategy {
            BackoffStrategy::Fixed(duration) => *duration,
            BackoffStrategy::Linear { base, increment } => {
                *base + (*increment * attempt)
            },
            BackoffStrategy::Exponential { base, multiplier, max } => {
                let delay = *base * (*multiplier as u32).pow(attempt - 1);
                let delay_duration = Duration::from_millis(delay.as_millis() as u64);
                delay_duration.min(*max)
            }
        }
    }
}

/// Decision made after handling an error
#[derive(Debug, Clone)]
pub enum ErrorHandlingDecision {
    Retry { delay: Duration, attempt: u32 },
    MoveToDeadLetter,
    FailBatch,
    Ignore,
}

impl DeadLetterQueue {
    /// Create a new dead letter queue with default memory storage
    pub fn new() -> Self {
        Self {
            failed_jobs: Arc::new(Mutex::new(HashMap::new())),
            storage: Arc::new(MemoryDeadLetterStorage::new()),
            max_size: 10000,
        }
    }

    /// Create a new dead letter queue with custom storage
    pub fn with_storage(storage: Arc<dyn DeadLetterStorage>) -> Self {
        Self {
            failed_jobs: Arc::new(Mutex::new(HashMap::new())),
            storage,
            max_size: 10000,
        }
    }

    /// Set maximum queue size
    pub fn with_max_size(mut self, max_size: usize) -> Self {
        self.max_size = max_size;
        self
    }

    /// Add a job to the dead letter queue
    pub async fn add_failed_job(&self, job: BatchJob, error: ErrorType) -> Result<(), LangGraphError> {
        let failed_job = FailedJob {
            job: job.clone(),
            error,
            failure_count: 1,
            first_failure: Utc::now(),
            last_failure: Utc::now(),
            metadata: HashMap::new(),
        };

        // Store in persistent storage
        self.storage.store_failed_job(&job.id, &failed_job).await?;

        // Add to in-memory cache
        let mut cache = self.failed_jobs.lock().await;
        cache.insert(job.id, failed_job);

        Ok(())
    }

    /// Retrieve a job from the dead letter queue
    pub async fn get_failed_job(&self, job_id: &str) -> Result<Option<FailedJob>, LangGraphError> {
        // Check in-memory cache first
        let cache = self.failed_jobs.lock().await;
        if let Some(failed_job) = cache.get(job_id) {
            return Ok(Some(failed_job.clone()));
        }
        drop(cache);

        // Check persistent storage
        self.storage.retrieve_failed_job(job_id).await
    }

    /// Resurrect a job from the dead letter queue for retry
    pub async fn resurrect_job(&self, job_id: &str) -> Result<Option<BatchJob>, LangGraphError> {
        // Get the failed job
        if let Some(failed_job) = self.get_failed_job(job_id).await? {
            // Remove from dead letter queue
            self.storage.remove_failed_job(job_id).await?;
            let mut cache = self.failed_jobs.lock().await;
            cache.remove(job_id);

            Ok(Some(failed_job.job))
        } else {
            Ok(None)
        }
    }
}

impl CircuitBreaker {
    /// Create a new circuit breaker
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(CircuitBreakerState::Closed { failure_count: 0 })),
            failure_threshold: 5,
            recovery_timeout: Duration::from_secs(60),
            half_open_max_calls: 3,
        }
    }

    /// Check if the circuit breaker allows the call
    pub async fn allow_call(&self) -> bool {
        let state = self.state.lock().await;
        match &*state {
            CircuitBreakerState::Closed { .. } => true,
            CircuitBreakerState::Open { opened_at } => {
                // Check if recovery timeout has passed
                Utc::now().signed_duration_since(*opened_at) > chrono::Duration::from_std(self.recovery_timeout).unwrap_or_default()
            },
            CircuitBreakerState::HalfOpen { test_calls } => {
                *test_calls < self.half_open_max_calls
            }
        }
    }

    /// Record a successful call
    pub async fn record_success(&self) {
        let mut state = self.state.lock().await;
        match &*state {
            CircuitBreakerState::Closed { .. } => {
                // Already closed, do nothing
            },
            CircuitBreakerState::HalfOpen { .. } => {
                // Success in half-open state closes the circuit
                *state = CircuitBreakerState::Closed { failure_count: 0 };
            },
            CircuitBreakerState::Open { .. } => {
                // Should not happen if allow_call is respected
                *state = CircuitBreakerState::Closed { failure_count: 0 };
            }
        }
    }

    /// Record a failed call
    pub async fn record_failure(&self) {
        let mut state = self.state.lock().await;
        match &*state {
            CircuitBreakerState::Closed { failure_count } => {
                let new_count = failure_count + 1;
                if new_count >= self.failure_threshold {
                    *state = CircuitBreakerState::Open { opened_at: Utc::now() };
                } else {
                    *state = CircuitBreakerState::Closed { failure_count: new_count };
                }
            },
            CircuitBreakerState::HalfOpen { test_calls } => {
                // Failure in half-open state opens the circuit again
                *state = CircuitBreakerState::Open { opened_at: Utc::now() };
            },
            CircuitBreakerState::Open { .. } => {
                // Already open, do nothing
            }
        }
    }
}

impl ErrorReporter {
    /// Create a new error reporter
    pub fn new() -> Self {
        Self {
            error_aggregator: Arc::new(Mutex::new(ErrorAggregator::default())),
            alerts: vec![],
        }
    }

    /// Report an error
    pub async fn report_error(&self, error: &ErrorType, context: &ErrorContext) -> Result<(), LangGraphError> {
        let mut aggregator = self.error_aggregator.lock().await;

        // Get error type string
        let error_type_str = match error {
            ErrorType::Transient(e) => format!("transient:{}", e.code),
            ErrorType::Recoverable(e) => format!("recoverable:{}", e.code),
            ErrorType::Permanent(e) => format!("permanent:{}", e.code),
            ErrorType::Fatal(e) => format!("fatal:{}", e.code),
        };

        // Update error counts
        *aggregator.error_counts.entry(error_type_str.clone()).or_insert(0) += 1;

        // Add error trend
        aggregator.error_trends.push(ErrorTrend {
            timestamp: Utc::now(),
            error_type: error_type_str,
            count: 1,
        });

        Ok(())
    }

    /// Get error statistics
    pub async fn get_error_stats(&self) -> ErrorAggregator {
        let aggregator = self.error_aggregator.lock().await;
        aggregator.clone()
    }
}

/// Default error classifier implementation
#[derive(Debug)]
pub struct DefaultErrorClassifier;

impl ErrorClassifier for DefaultErrorClassifier {
    fn classify_error(&self, error: &LangGraphError, context: &ErrorContext) -> ErrorType {
        match error {
            LangGraphError::Execution(msg) if msg.contains("timeout") => {
                ErrorType::Transient(TransientError {
                    message: msg.clone(),
                    code: "TIMEOUT".to_string(),
                    context: HashMap::new(),
                })
            },
            LangGraphError::Execution(msg) if msg.contains("Connection") => {
                ErrorType::Transient(TransientError {
                    message: msg.clone(),
                    code: "CONNECTION_ERROR".to_string(),
                    context: HashMap::new(),
                })
            },
            LangGraphError::StateError(msg) if msg.contains("rate limit") => {
                ErrorType::Recoverable(RecoverableError {
                    message: msg.clone(),
                    code: "RATE_LIMIT".to_string(),
                    retry_after: Some(Duration::from_secs(5)),
                    context: HashMap::new(),
                })
            },
            LangGraphError::State(msg) if msg.contains("rate limit") => {
                ErrorType::Recoverable(RecoverableError {
                    message: msg.clone(),
                    code: "RATE_LIMIT".to_string(),
                    retry_after: Some(Duration::from_secs(5)),
                    context: HashMap::new(),
                })
            },
            LangGraphError::GraphValidation(msg) => {
                ErrorType::Permanent(PermanentError {
                    message: msg.clone(),
                    code: "VALIDATION_ERROR".to_string(),
                    reason: "Invalid job configuration".to_string(),
                    context: HashMap::new(),
                })
            },
            LangGraphError::Internal(msg) => {
                ErrorType::Fatal(FatalError {
                    message: msg.clone(),
                    code: "SYSTEM_ERROR".to_string(),
                    severity: "high".to_string(),
                    context: HashMap::new(),
                })
            },
            _ => {
                // Default to recoverable for unknown errors
                ErrorType::Recoverable(RecoverableError {
                    message: error.to_string(),
                    code: "UNKNOWN_ERROR".to_string(),
                    retry_after: Some(Duration::from_secs(1)),
                    context: HashMap::new(),
                })
            }
        }
    }
}

/// Memory-based dead letter storage for testing
#[derive(Debug)]
pub struct MemoryDeadLetterStorage {
    storage: Arc<Mutex<HashMap<String, FailedJob>>>,
}

impl MemoryDeadLetterStorage {
    pub fn new() -> Self {
        Self {
            storage: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl DeadLetterStorage for MemoryDeadLetterStorage {
    fn store_failed_job(&self, job_id: &str, failed_job: &FailedJob) -> Pin<Box<dyn Future<Output = Result<(), LangGraphError>> + Send + '_>> {
        let job_id = job_id.to_string();
        let failed_job = failed_job.clone();
        Box::pin(async move {
            let mut storage = self.storage.lock().await;
            storage.insert(job_id, failed_job);
            Ok(())
        })
    }

    fn retrieve_failed_job(&self, job_id: &str) -> Pin<Box<dyn Future<Output = Result<Option<FailedJob>, LangGraphError>> + Send + '_>> {
        let job_id = job_id.to_string();
        Box::pin(async move {
            let storage = self.storage.lock().await;
            Ok(storage.get(&job_id).cloned())
        })
    }

    fn list_failed_jobs(&self) -> Pin<Box<dyn Future<Output = Result<Vec<String>, LangGraphError>> + Send + '_>> {
        Box::pin(async move {
            let storage = self.storage.lock().await;
            Ok(storage.keys().cloned().collect())
        })
    }

    fn remove_failed_job(&self, job_id: &str) -> Pin<Box<dyn Future<Output = Result<(), LangGraphError>> + Send + '_>> {
        let job_id = job_id.to_string();
        Box::pin(async move {
            let mut storage = self.storage.lock().await;
            storage.remove(&job_id);
            Ok(())
        })
    }
}

/// Recovery strategy implementation for job resurrection
#[derive(Debug)]
pub struct JobRecoveryManager {
    error_handler: Arc<BatchErrorHandler>,
    recovery_strategies: Vec<Arc<dyn RecoveryStrategy>>,
    recovery_history: Arc<Mutex<Vec<RecoveryAttempt>>>,
    max_recovery_attempts: u32,
}

impl JobRecoveryManager {
    pub fn new(error_handler: Arc<BatchErrorHandler>) -> Self {
        Self {
            error_handler,
            recovery_strategies: vec![
                Arc::new(SimpleRetryStrategy),
                Arc::new(ExponentialBackoffStrategy),
                Arc::new(DataCleanupStrategy),
            ],
            recovery_history: Arc::new(Mutex::new(Vec::new())),
            max_recovery_attempts: 5,
        }
    }

    pub async fn attempt_recovery(
        &self,
        job: &BatchJob,
        error: &ErrorType,
    ) -> Result<Option<BatchJob>, LangGraphError> {
        let context = ErrorContext {
            job_id: job.id.clone(),
            attempt_number: 1,
            total_attempts: self.max_recovery_attempts,
            execution_duration: Duration::from_secs(0),
            metadata: HashMap::new(),
        };

        // Try each recovery strategy
        for strategy in &self.recovery_strategies {
            if strategy.can_recover(error, &context).await {
                match strategy.recover(job, error).await {
                    Ok(recovered_job) => {
                        let attempt = RecoveryAttempt {
                            job_id: job.id.clone(),
                            strategy_used: format!("{:?}", strategy),
                            timestamp: Utc::now(),
                            success: true,
                            error: None,
                        };
                        self.recovery_history.lock().await.push(attempt);
                        return Ok(Some(recovered_job));
                    }
                    Err(e) => {
                        let attempt = RecoveryAttempt {
                            job_id: job.id.clone(),
                            strategy_used: format!("{:?}", strategy),
                            timestamp: Utc::now(),
                            success: false,
                            error: Some(e.to_string()),
                        };
                        self.recovery_history.lock().await.push(attempt);
                    }
                }
            }
        }

        Ok(None)
    }

    pub async fn get_recovery_history(&self) -> Vec<RecoveryAttempt> {
        self.recovery_history.lock().await.clone()
    }
}

/// Simple retry recovery strategy
#[derive(Debug)]
pub struct SimpleRetryStrategy;

impl RecoveryStrategy for SimpleRetryStrategy {
    fn can_recover(&self, error: &ErrorType, _context: &ErrorContext) -> Pin<Box<dyn Future<Output = bool> + Send + '_>> {
        let result = matches!(error, ErrorType::Transient(_) | ErrorType::Recoverable(_));
        Box::pin(async move {
            result
        })
    }

    fn recover(&self, job: &BatchJob, _error: &ErrorType) -> Pin<Box<dyn Future<Output = Result<BatchJob, LangGraphError>> + Send + '_>> {
        let job = job.clone();
        Box::pin(async move {
            // Simply return the job for retry
            Ok(job)
        })
    }
}

/// Exponential backoff recovery strategy
#[derive(Debug)]
pub struct ExponentialBackoffStrategy;

impl RecoveryStrategy for ExponentialBackoffStrategy {
    fn can_recover(&self, error: &ErrorType, context: &ErrorContext) -> Pin<Box<dyn Future<Output = bool> + Send + '_>> {
        let result = context.attempt_number < 5 && matches!(error, ErrorType::Recoverable(_));
        Box::pin(async move {
            result
        })
    }

    fn recover(&self, job: &BatchJob, _error: &ErrorType) -> Pin<Box<dyn Future<Output = Result<BatchJob, LangGraphError>> + Send + '_>> {
        let mut recovered_job = job.clone();
        recovered_job.priority = recovered_job.priority.saturating_sub(1);
        Box::pin(async move {
            // Return job with reduced priority for delayed retry
            Ok(recovered_job)
        })
    }
}

/// Data cleanup recovery strategy
#[derive(Debug)]
pub struct DataCleanupStrategy;

impl RecoveryStrategy for DataCleanupStrategy {
    fn can_recover(&self, error: &ErrorType, _context: &ErrorContext) -> Pin<Box<dyn Future<Output = bool> + Send + '_>> {
        let result = matches!(error, ErrorType::Permanent(_));
        Box::pin(async move {
            result
        })
    }

    fn recover(&self, job: &BatchJob, _error: &ErrorType) -> Pin<Box<dyn Future<Output = Result<BatchJob, LangGraphError>> + Send + '_>> {
        let mut recovered_job = job.clone();
        // In a real implementation, this would clean/transform the input data
        recovered_job.input.clear(); // Clear potentially problematic data
        Box::pin(async move {
            // Clean up problematic data and retry
            Ok(recovered_job)
        })
    }
}

/// Email alert handler for critical errors
#[derive(Debug)]
pub struct EmailAlertHandler {
    smtp_config: String,
    recipients: Vec<String>,
}

impl EmailAlertHandler {
    pub fn new(smtp_config: String, recipients: Vec<String>) -> Self {
        Self {
            smtp_config,
            recipients,
        }
    }
}

impl AlertHandler for EmailAlertHandler {
    fn handle_alert(&self, alert: &ErrorAlert) -> Pin<Box<dyn Future<Output = Result<(), LangGraphError>> + Send + '_>> {
        let alert = alert.clone();
        let recipients = self.recipients.clone();
        Box::pin(async move {
            // In production, this would send actual emails
            tracing::error!(
                "EMAIL ALERT: Severity: {}, Message: {}, Recipients: {:?}",
                alert.severity, alert.message, recipients
            );
            Ok(())
        })
    }
}

/// Slack alert handler for team notifications
#[derive(Debug)]
pub struct SlackAlertHandler {
    webhook_url: String,
    channel: String,
}

impl SlackAlertHandler {
    pub fn new(webhook_url: String, channel: String) -> Self {
        Self {
            webhook_url,
            channel,
        }
    }
}

impl AlertHandler for SlackAlertHandler {
    fn handle_alert(&self, alert: &ErrorAlert) -> Pin<Box<dyn Future<Output = Result<(), LangGraphError>> + Send + '_>> {
        let alert = alert.clone();
        let channel = self.channel.clone();
        Box::pin(async move {
            // In production, this would post to Slack
            tracing::warn!(
                "SLACK ALERT to {}: Severity: {}, Message: {}",
                channel, alert.severity, alert.message
            );
            Ok(())
        })
    }
}

/// PagerDuty alert handler for critical incidents
#[derive(Debug)]
pub struct PagerDutyAlertHandler {
    api_key: String,
    service_id: String,
}

impl PagerDutyAlertHandler {
    pub fn new(api_key: String, service_id: String) -> Self {
        Self {
            api_key,
            service_id,
        }
    }
}

impl AlertHandler for PagerDutyAlertHandler {
    fn handle_alert(&self, alert: &ErrorAlert) -> Pin<Box<dyn Future<Output = Result<(), LangGraphError>> + Send + '_>> {
        let alert = alert.clone();
        let service_id = self.service_id.clone();
        Box::pin(async move {
            // In production, this would create PagerDuty incident
            if alert.severity == "critical" || alert.severity == "fatal" {
                tracing::error!(
                    "PAGERDUTY INCIDENT for service {}: {}",
                    service_id, alert.message
                );
            }
            Ok(())
        })
    }
}

impl Default for RetryStrategy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            backoff_strategy: BackoffStrategy::Exponential {
                base: Duration::from_millis(100),
                multiplier: 2.0,
                max: Duration::from_secs(60),
            },
            jitter: true,
            retry_predicates: vec![],
        }
    }
}