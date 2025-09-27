use crate::graph::GraphError;
use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Semaphore};
use tokio::time::sleep;

#[async_trait]
pub trait FlowController: Send + Sync {
    async fn acquire(&self) -> Result<FlowPermit, GraphError>;
    async fn release(&self, permit: FlowPermit);
    fn is_healthy(&self) -> bool;
}

pub struct FlowPermit {
    id: usize,
    timestamp: Instant,
}

pub struct BackpressureController {
    max_in_flight: usize,
    semaphore: Arc<Semaphore>,
    in_flight: Arc<AtomicUsize>,
}

impl BackpressureController {
    pub fn new(max_in_flight: usize) -> Self {
        Self {
            max_in_flight,
            semaphore: Arc::new(Semaphore::new(max_in_flight)),
            in_flight: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn current_load(&self) -> usize {
        self.in_flight.load(Ordering::Relaxed)
    }

    pub fn load_percentage(&self) -> f64 {
        (self.current_load() as f64 / self.max_in_flight as f64) * 100.0
    }
}

#[async_trait]
impl FlowController for BackpressureController {
    async fn acquire(&self) -> Result<FlowPermit, GraphError> {
        self.semaphore
            .acquire()
            .await
            .map_err(|_| GraphError::RuntimeError("Failed to acquire permit".to_string()))?;

        let count = self.in_flight.fetch_add(1, Ordering::Relaxed);

        Ok(FlowPermit {
            id: count,
            timestamp: Instant::now(),
        })
    }

    async fn release(&self, _permit: FlowPermit) {
        self.in_flight.fetch_sub(1, Ordering::Relaxed);
    }

    fn is_healthy(&self) -> bool {
        self.load_percentage() < 90.0
    }
}

pub struct RateLimiter {
    max_per_second: usize,
    window: Arc<RwLock<Vec<Instant>>>,
}

impl RateLimiter {
    pub fn new(max_per_second: usize) -> Self {
        Self {
            max_per_second,
            window: Arc::new(RwLock::new(Vec::new())),
        }
    }

    async fn clean_window(&self) {
        let mut window = self.window.write().await;
        let now = Instant::now();
        window.retain(|&timestamp| now.duration_since(timestamp) < Duration::from_secs(1));
    }
}

#[async_trait]
impl FlowController for RateLimiter {
    async fn acquire(&self) -> Result<FlowPermit, GraphError> {
        loop {
            self.clean_window().await;

            let window = self.window.read().await;
            if window.len() < self.max_per_second {
                drop(window);

                let now = Instant::now();
                self.window.write().await.push(now);

                return Ok(FlowPermit {
                    id: 0,
                    timestamp: now,
                });
            }

            drop(window);
            sleep(Duration::from_millis(10)).await;
        }
    }

    async fn release(&self, _permit: FlowPermit) {}

    fn is_healthy(&self) -> bool {
        true
    }
}

pub struct CircuitBreaker {
    failure_threshold: usize,
    success_threshold: usize,
    timeout: Duration,

    failures: Arc<AtomicUsize>,
    successes: Arc<AtomicUsize>,
    state: Arc<RwLock<CircuitState>>,
    last_failure: Arc<RwLock<Option<Instant>>>,
}

#[derive(Debug, Clone, PartialEq)]
enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: usize, success_threshold: usize, timeout: Duration) -> Self {
        Self {
            failure_threshold,
            success_threshold,
            timeout,
            failures: Arc::new(AtomicUsize::new(0)),
            successes: Arc::new(AtomicUsize::new(0)),
            state: Arc::new(RwLock::new(CircuitState::Closed)),
            last_failure: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn record_success(&self) {
        self.successes.fetch_add(1, Ordering::Relaxed);

        let mut state = self.state.write().await;
        if *state == CircuitState::HalfOpen {
            let successes = self.successes.load(Ordering::Relaxed);
            if successes >= self.success_threshold {
                *state = CircuitState::Closed;
                self.failures.store(0, Ordering::Relaxed);
                self.successes.store(0, Ordering::Relaxed);
            }
        }
    }

    pub async fn record_failure(&self) {
        self.failures.fetch_add(1, Ordering::Relaxed);
        *self.last_failure.write().await = Some(Instant::now());

        let mut state = self.state.write().await;
        let failures = self.failures.load(Ordering::Relaxed);

        if failures >= self.failure_threshold && *state == CircuitState::Closed {
            *state = CircuitState::Open;
        }
    }

    async fn check_timeout(&self) {
        let last_failure = *self.last_failure.read().await;

        if let Some(last) = last_failure {
            if Instant::now().duration_since(last) >= self.timeout {
                let mut state = self.state.write().await;
                if *state == CircuitState::Open {
                    *state = CircuitState::HalfOpen;
                    self.successes.store(0, Ordering::Relaxed);
                }
            }
        }
    }
}

#[async_trait]
impl FlowController for CircuitBreaker {
    async fn acquire(&self) -> Result<FlowPermit, GraphError> {
        self.check_timeout().await;

        let state = self.state.read().await;
        match *state {
            CircuitState::Open => Err(GraphError::RuntimeError(
                "Circuit breaker is open".to_string(),
            )),
            CircuitState::Closed | CircuitState::HalfOpen => Ok(FlowPermit {
                id: 0,
                timestamp: Instant::now(),
            }),
        }
    }

    async fn release(&self, _permit: FlowPermit) {}

    fn is_healthy(&self) -> bool {
        true
    }
}

pub struct AdaptiveFlowController {
    base_controller: Arc<dyn FlowController>,
    metrics: Arc<RwLock<FlowMetrics>>,
}

struct FlowMetrics {
    latencies: Vec<Duration>,
    throughput: Vec<usize>,
    errors: Vec<Instant>,
}

impl AdaptiveFlowController {
    pub fn new(base_controller: Arc<dyn FlowController>) -> Self {
        Self {
            base_controller,
            metrics: Arc::new(RwLock::new(FlowMetrics {
                latencies: Vec::new(),
                throughput: Vec::new(),
                errors: Vec::new(),
            })),
        }
    }

    pub async fn record_latency(&self, latency: Duration) {
        let mut metrics = self.metrics.write().await;
        metrics.latencies.push(latency);

        if metrics.latencies.len() > 1000 {
            metrics.latencies.remove(0);
        }
    }

    pub async fn get_p99_latency(&self) -> Option<Duration> {
        let metrics = self.metrics.read().await;
        if metrics.latencies.is_empty() {
            return None;
        }

        let mut sorted = metrics.latencies.clone();
        sorted.sort();

        let index = (sorted.len() as f64 * 0.99) as usize;
        sorted.get(index).copied()
    }
}

#[async_trait]
impl FlowController for AdaptiveFlowController {
    async fn acquire(&self) -> Result<FlowPermit, GraphError> {
        self.base_controller.acquire().await
    }

    async fn release(&self, permit: FlowPermit) {
        let latency = permit.timestamp.elapsed();
        self.record_latency(latency).await;
        self.base_controller.release(permit).await;
    }

    fn is_healthy(&self) -> bool {
        self.base_controller.is_healthy()
    }
}
