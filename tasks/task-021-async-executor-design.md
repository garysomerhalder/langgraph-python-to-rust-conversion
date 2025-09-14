# Task 021: Async Task Executor and Scheduler Design

## 📋 Task Details
- **Task ID:** 021
- **Title:** Design Advanced Async Task Executor with Intelligent Scheduling
- **Phase:** 🟡 YELLOW (Implementation)
- **Priority:** P0 (Critical Path)
- **Estimated Hours:** 14 hours
- **Assigned To:** Gary Somerhalder
- **Status:** ⏸️ Pending

## 🎯 Description
Implement a sophisticated async task executor and scheduler that optimizes graph execution through intelligent task scheduling, resource management, and adaptive load balancing. Builds upon the Pregel core to provide advanced execution strategies.

## ✅ Acceptance Criteria
- [ ] Multi-threaded task executor with work-stealing queues
- [ ] Intelligent task scheduling based on dependency analysis
- [ ] Adaptive concurrency control with backpressure handling
- [ ] Resource-aware scheduling with CPU and memory monitoring
- [ ] Priority-based execution with deadline awareness
- [ ] Task cancellation and timeout handling
- [ ] Performance monitoring and adaptive optimization
- [ ] Integration with Pregel execution engine
- [ ] Comprehensive benchmarking and performance validation
- [ ] Thread-safe coordination with channel system

## 📦 Dependencies
- **Prerequisites:** Task 020 (Pregel Core)
- **Blocks:** Task 022 (Task Scheduler), Task 026 (Runtime Integration)
- **Related:** Task 023 (StateGraph Builder)

## 🔧 Technical Notes

### Advanced Task Executor Architecture

```rust
use std::collections::{BinaryHeap, HashMap, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::{Semaphore, RwLock, Notify};
use tokio::time::{Duration, Instant};

/// Advanced async task executor with intelligent scheduling
pub struct AsyncTaskExecutor {
    /// Work-stealing task queues for each worker
    work_queues: Vec<Arc<WorkStealingQueue>>,
    /// Global task scheduler for dependency management
    scheduler: Arc<TaskScheduler>,
    /// Resource monitor for adaptive scheduling
    resource_monitor: Arc<ResourceMonitor>,
    /// Execution metrics and monitoring
    metrics: Arc<ExecutionMetrics>,
    /// Configuration and tuning parameters
    config: ExecutorConfig,
    /// Worker thread pool
    worker_pool: Vec<tokio::task::JoinHandle<()>>,
}

/// Configuration for task executor
#[derive(Debug, Clone)]
pub struct ExecutorConfig {
    /// Number of worker threads
    pub worker_count: usize,
    /// Maximum tasks per worker queue
    pub max_queue_size: usize,
    /// Task timeout duration
    pub default_timeout: Duration,
    /// Enable work stealing between queues
    pub enable_work_stealing: bool,
    /// CPU utilization target (0.0 to 1.0)
    pub cpu_target: f64,
    /// Memory pressure threshold (bytes)
    pub memory_threshold: usize,
    /// Enable adaptive scheduling
    pub adaptive_scheduling: bool,
}

/// Executable task with metadata
#[derive(Debug)]
pub struct ExecutableTask {
    /// Unique task identifier
    pub id: TaskId,
    /// Task priority (higher = more important)
    pub priority: u32,
    /// Task deadline (optional)
    pub deadline: Option<Instant>,
    /// Task dependencies
    pub dependencies: Vec<TaskId>,
    /// Estimated execution duration
    pub estimated_duration: Duration,
    /// Resource requirements
    pub resource_requirements: ResourceRequirements,
    /// The actual async task
    pub task: Box<dyn AsyncTask>,
    /// Execution context
    pub context: TaskContext,
}

/// Resource requirements for task execution
#[derive(Debug, Clone)]
pub struct ResourceRequirements {
    /// CPU cores needed (fractional allowed)
    pub cpu_cores: f64,
    /// Memory required in bytes
    pub memory_bytes: usize,
    /// I/O intensity (0.0 to 1.0)
    pub io_intensity: f64,
}
```

### Work-Stealing Queue Implementation

```rust
/// Lock-free work-stealing deque for task distribution
pub struct WorkStealingQueue {
    /// Local task queue (LIFO for owner, FIFO for stealers)
    tasks: Arc<RwLock<VecDeque<ExecutableTask>>>,
    /// Queue statistics
    stats: Arc<QueueStats>,
    /// Notification for new work
    notify: Arc<Notify>,
}

impl WorkStealingQueue {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(RwLock::new(VecDeque::new())),
            stats: Arc::new(QueueStats::new()),
            notify: Arc::new(Notify::new()),
        }
    }
    
    /// Push task to local queue (owner thread)
    pub async fn push_local(&self, task: ExecutableTask) -> Result<(), ExecutorError> {
        let mut queue = self.tasks.write().await;
        
        if queue.len() >= self.max_capacity() {
            return Err(ExecutorError::QueueFull);
        }
        
        queue.push_back(task);
        self.stats.increment_enqueued();
        self.notify.notify_one();
        Ok(())
    }
    
    /// Pop task from local queue (owner thread, LIFO)
    pub async fn pop_local(&self) -> Option<ExecutableTask> {
        let mut queue = self.tasks.write().await;
        let task = queue.pop_back();
        
        if task.is_some() {
            self.stats.increment_dequeued();
        }
        
        task
    }
    
    /// Steal task from remote queue (stealer thread, FIFO)
    pub async fn steal(&self) -> Option<ExecutableTask> {
        let mut queue = self.tasks.write().await;
        
        // Only steal if queue has multiple tasks (leave some for owner)
        if queue.len() > 1 {
            let task = queue.pop_front();
            if task.is_some() {
                self.stats.increment_stolen();
            }
            task
        } else {
            None
        }
    }
    
    /// Wait for new work notification
    pub async fn wait_for_work(&self) {
        self.notify.notified().await;
    }
    
    fn max_capacity(&self) -> usize {
        1000 // Configurable
    }
}

/// Queue performance statistics
#[derive(Debug, Default)]
pub struct QueueStats {
    enqueued: AtomicUsize,
    dequeued: AtomicUsize,
    stolen: AtomicUsize,
    work_stealing_attempts: AtomicUsize,
}
```

### Intelligent Task Scheduler

```rust
/// Advanced task scheduler with dependency resolution
pub struct TaskScheduler {
    /// Pending tasks awaiting dependencies
    pending_tasks: Arc<RwLock<HashMap<TaskId, ExecutableTask>>>,
    /// Ready queue sorted by priority and deadline
    ready_queue: Arc<RwLock<BinaryHeap<PriorityTask>>>,
    /// Dependency graph
    dependencies: Arc<RwLock<DependencyGraph>>,
    /// Running tasks
    running_tasks: Arc<RwLock<HashMap<TaskId, TaskHandle>>>,
    /// Completed tasks
    completed_tasks: Arc<RwLock<HashSet<TaskId>>>,
    /// Failed tasks
    failed_tasks: Arc<RwLock<HashSet<TaskId>>>,
    /// Scheduler configuration
    config: SchedulerConfig,
}

impl TaskScheduler {
    /// Submit task for scheduling
    pub async fn submit_task(&self, task: ExecutableTask) -> Result<TaskId, SchedulerError> {
        let task_id = task.id;
        
        // Check if dependencies are satisfied
        if self.dependencies_satisfied(&task).await {
            // Add to ready queue
            let priority_task = PriorityTask::new(task);
            self.ready_queue.write().await.push(priority_task);
        } else {
            // Add to pending tasks
            self.pending_tasks.write().await.insert(task_id, task);
        }
        
        // Update dependency graph
        self.update_dependencies(&task_id, &task.dependencies).await;
        
        Ok(task_id)
    }
    
    /// Get next ready task for execution
    pub async fn get_ready_task(&self) -> Option<ExecutableTask> {
        let mut ready = self.ready_queue.write().await;
        ready.pop().map(|pt| pt.task)
    }
    
    /// Mark task as completed and resolve dependencies
    pub async fn complete_task(&self, task_id: TaskId) -> Result<(), SchedulerError> {
        // Remove from running tasks
        self.running_tasks.write().await.remove(&task_id);
        
        // Add to completed tasks
        self.completed_tasks.write().await.insert(task_id);
        
        // Check if any pending tasks can now run
        let newly_ready = self.resolve_dependencies(task_id).await;
        
        // Move newly ready tasks to ready queue
        for ready_task in newly_ready {
            let priority_task = PriorityTask::new(ready_task);
            self.ready_queue.write().await.push(priority_task);
        }
        
        Ok(())
    }
    
    async fn dependencies_satisfied(&self, task: &ExecutableTask) -> bool {
        let completed = self.completed_tasks.read().await;
        task.dependencies.iter().all(|dep| completed.contains(dep))
    }
    
    async fn resolve_dependencies(&self, completed_task: TaskId) -> Vec<ExecutableTask> {
        let mut newly_ready = Vec::new();
        let mut pending = self.pending_tasks.write().await;
        
        // Find tasks that were waiting on this completed task
        let mut to_remove = Vec::new();
        for (task_id, task) in pending.iter() {
            if task.dependencies.contains(&completed_task) {
                if self.dependencies_satisfied(task).await {
                    newly_ready.push(task.clone());
                    to_remove.push(*task_id);
                }
            }
        }
        
        // Remove newly ready tasks from pending
        for task_id in to_remove {
            pending.remove(&task_id);
        }
        
        newly_ready
    }
}

/// Task wrapper with priority and deadline information
#[derive(Debug)]
struct PriorityTask {
    task: ExecutableTask,
    effective_priority: u64,
}

impl PriorityTask {
    fn new(task: ExecutableTask) -> Self {
        let effective_priority = Self::calculate_priority(&task);
        Self { task, effective_priority }
    }
    
    fn calculate_priority(task: &ExecutableTask) -> u64 {
        let mut priority = task.priority as u64;
        
        // Boost priority for tasks near deadline
        if let Some(deadline) = task.deadline {
            let now = Instant::now();
            if now < deadline {
                let time_left = deadline - now;
                let urgency_boost = if time_left < Duration::from_secs(10) {
                    1000
                } else if time_left < Duration::from_secs(60) {
                    500
                } else {
                    0
                };
                priority += urgency_boost;
            } else {
                // Overdue tasks get maximum priority
                priority += 10000;
            }
        }
        
        priority
    }
}

impl Ord for PriorityTask {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.effective_priority.cmp(&other.effective_priority)
    }
}

impl PartialOrd for PriorityTask {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for PriorityTask {
    fn eq(&self, other: &Self) -> bool {
        self.effective_priority == other.effective_priority
    }
}

impl Eq for PriorityTask {}
```

### Resource Monitor and Adaptive Scheduling

```rust
/// System resource monitoring for adaptive scheduling
pub struct ResourceMonitor {
    /// Current CPU utilization (0.0 to 1.0)
    cpu_utilization: Arc<RwLock<f64>>,
    /// Current memory usage in bytes
    memory_usage: Arc<RwLock<usize>>,
    /// I/O wait percentage
    io_wait: Arc<RwLock<f64>>,
    /// Monitoring task handle
    monitor_task: Option<tokio::task::JoinHandle<()>>,
    /// Configuration
    config: ResourceMonitorConfig,
}

impl ResourceMonitor {
    pub fn new(config: ResourceMonitorConfig) -> Self {
        Self {
            cpu_utilization: Arc::new(RwLock::new(0.0)),
            memory_usage: Arc::new(RwLock::new(0)),
            io_wait: Arc::new(RwLock::new(0.0)),
            monitor_task: None,
            config,
        }
    }
    
    /// Start resource monitoring
    pub async fn start_monitoring(&mut self) {
        let cpu = self.cpu_utilization.clone();
        let memory = self.memory_usage.clone();
        let io = self.io_wait.clone();
        let interval = self.config.monitoring_interval;
        
        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(interval);
            
            loop {
                interval.tick().await;
                
                // Update CPU utilization
                if let Ok(cpu_percent) = Self::get_cpu_utilization().await {
                    *cpu.write().await = cpu_percent;
                }
                
                // Update memory usage
                if let Ok(memory_bytes) = Self::get_memory_usage().await {
                    *memory.write().await = memory_bytes;
                }
                
                // Update I/O wait
                if let Ok(io_percent) = Self::get_io_wait().await {
                    *io.write().await = io_percent;
                }
            }
        });
        
        self.monitor_task = Some(handle);
    }
    
    /// Check if system can handle additional CPU-intensive tasks
    pub async fn can_schedule_cpu_task(&self) -> bool {
        let cpu_util = *self.cpu_utilization.read().await;
        cpu_util < self.config.cpu_threshold
    }
    
    /// Check if system can handle additional memory-intensive tasks  
    pub async fn can_schedule_memory_task(&self, required_memory: usize) -> bool {
        let current_usage = *self.memory_usage.read().await;
        current_usage + required_memory < self.config.memory_threshold
    }
    
    /// Get recommended concurrency level based on current resources
    pub async fn recommended_concurrency(&self) -> usize {
        let cpu_util = *self.cpu_utilization.read().await;
        let io_wait = *self.io_wait.read().await;
        
        // Adaptive concurrency based on system state
        if io_wait > 0.3 {
            // High I/O wait, increase concurrency
            (num_cpus::get() as f64 * 2.0) as usize
        } else if cpu_util > 0.8 {
            // High CPU usage, reduce concurrency
            (num_cpus::get() as f64 * 0.5) as usize
        } else {
            // Normal conditions
            num_cpus::get()
        }
    }
    
    async fn get_cpu_utilization() -> Result<f64, std::io::Error> {
        // Implementation depends on platform
        // Using procfs on Linux, APIs on other platforms
        todo!("Implement platform-specific CPU monitoring")
    }
    
    async fn get_memory_usage() -> Result<usize, std::io::Error> {
        // Implementation depends on platform
        todo!("Implement platform-specific memory monitoring")
    }
    
    async fn get_io_wait() -> Result<f64, std::io::Error> {
        // Implementation depends on platform  
        todo!("Implement platform-specific I/O monitoring")
    }
}
```

## 🧪 Testing Requirements
- [ ] Work-stealing queues show balanced load distribution
- [ ] Task scheduling respects dependencies and priorities
- [ ] Adaptive concurrency responds to system load changes
- [ ] Resource monitoring accurately reflects system state
- [ ] Task cancellation and timeout handling works correctly
- [ ] Performance benchmarks show improved execution times
- [ ] Thread safety verified under high concurrency
- [ ] Integration with Pregel engine maintains correctness

## 📝 Implementation Steps
1. **Implement work-stealing task queues** with lock-free operations
2. **Build intelligent task scheduler** with dependency resolution
3. **Create resource monitoring system** for adaptive scheduling  
4. **Implement adaptive concurrency control** based on system resources
5. **Add task cancellation and timeout handling** with proper cleanup
6. **Build comprehensive metrics collection** and monitoring
7. **Integrate with Pregel execution engine** for graph execution
8. **Create extensive benchmarking suite** for performance validation
9. **Optimize for common execution patterns** and edge cases
10. **Add comprehensive error handling** and recovery mechanisms

## 🔗 Related Tasks
- **Foundation:** [Task 020: Pregel Core](task-020-pregel-core-architecture.md)
- **Next:** [Task 022: Task Scheduler](task-022-task-scheduler.md)
- **Integration:** [Task 026: Runtime Integration](task-026-runtime-integration.md)
- **Architecture:** [Task 023: StateGraph Builder](task-023-stategraph-builder.md)
- **Tracker:** [Master Tracker](tracker/tracker.md)

## 📊 Success Metrics
- Work-stealing achieves <5% load imbalance across workers
- Task scheduling overhead <1ms per task for graphs with 100+ nodes
- Adaptive concurrency improves throughput by 20%+ under varying loads
- Resource monitoring accuracy within 5% of system tools
- Task execution times show 15%+ improvement over naive scheduling
- Zero deadlocks or race conditions under stress testing

## 🚨 Risk Factors
- **High Risk:** Complex lock-free data structures and synchronization
- **Performance Overhead:** Scheduling intelligence vs execution speed tradeoffs
- **Platform Dependencies:** Resource monitoring across different operating systems
- **Memory Management:** Preventing memory leaks in long-running executors

## 💡 Design Decisions
- **Work-Stealing:** Maximize CPU utilization and load balancing
- **Priority Scheduling:** Support deadline-aware and priority-based execution
- **Adaptive Control:** Respond to system conditions dynamically
- **Zero-Copy:** Minimize allocations in hot execution paths

## 📅 Timeline
- **Start:** Week 3, Day 6
- **Target Completion:** Week 4, Day 3
- **Buffer:** 1 day for lock-free algorithm complexity

---
*Created: 2025-09-14*  
*Last Updated: 2025-09-14*  
*Status Changed: 2025-09-14*