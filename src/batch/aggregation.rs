use std::fmt::Debug;
use std::sync::Arc;
use std::future::Future;
use std::pin::Pin;
use tokio::sync::{mpsc, Mutex};
use serde::{Serialize, Deserialize};
use crate::batch::BatchResult;
use crate::checkpoint::Checkpointer;
use crate::LangGraphError;

/// Result aggregation framework for batch processing
pub struct ResultAggregator {
    strategy: AggregationStrategy,
    output_format: OutputFormat,
    buffer_size: usize,
    checkpointer: Option<Arc<dyn Checkpointer>>,
}

impl std::fmt::Debug for ResultAggregator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResultAggregator")
            .field("strategy", &self.strategy)
            .field("output_format", &self.output_format)
            .field("buffer_size", &self.buffer_size)
            .field("checkpointer", &self.checkpointer.is_some())
            .finish()
    }
}

/// Aggregation strategies for batch results
#[derive(Debug)]
pub enum AggregationStrategy {
    Collect,                          // Collect all results
    Merge(Arc<dyn MergeFunction>),   // Custom merge function
    Reduce(Arc<dyn ReduceFunction>), // Fold/reduce operation
    Stream(Arc<dyn StreamProcessor>), // Stream processing
}

/// Output formats for aggregated results
#[derive(Debug, Clone)]
pub enum OutputFormat {
    Json,
    Csv,
    Parquet,
    Custom(String),
}

/// Streaming result collector
#[derive(Debug)]
pub struct ResultStream {
    receiver: mpsc::Receiver<BatchResult>,
    aggregator: ResultAggregator,
    consumer: Arc<dyn ResultConsumer>,
}

/// Trait for merging batch results
pub trait MergeFunction: Send + Sync + Debug {
    fn merge(&self, results: Vec<BatchResult>) -> Result<BatchResult, LangGraphError>;
}

/// Trait for reducing batch results
pub trait ReduceFunction: Send + Sync + Debug {
    fn reduce(&self, accumulator: Option<BatchResult>, result: BatchResult) -> Result<BatchResult, LangGraphError>;
}

/// Trait for stream processing
pub trait StreamProcessor: Send + Sync + Debug {
    fn process(&self, result: BatchResult) -> Pin<Box<dyn Future<Output = Result<Option<BatchResult>, LangGraphError>> + Send + '_>>;
}

/// Trait for consuming aggregated results
pub trait ResultConsumer: Send + Sync + Debug {
    fn consume(&self, results: Vec<BatchResult>) -> Pin<Box<dyn Future<Output = Result<(), LangGraphError>> + Send + '_>>;
}

/// Aggregated result collection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedResults {
    pub total_jobs: usize,
    pub successful_jobs: usize,
    pub failed_jobs: usize,
    pub results: Vec<BatchResult>,
    pub metadata: AggregationMetadata,
}

/// Metadata about the aggregation process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregationMetadata {
    pub strategy: String,
    pub output_format: String,
    pub processing_time_ms: u64,
    pub memory_used_bytes: usize,
}

/// Result filter for filtering batch results
pub trait ResultFilter: Send + Sync + Debug {
    fn filter(&self, result: &BatchResult) -> bool;
}

/// Result transformer for transforming batch results
pub trait ResultTransformer: Send + Sync + Debug {
    fn transform(&self, result: BatchResult) -> Result<BatchResult, LangGraphError>;
}

impl ResultAggregator {
    /// Create a new result aggregator
    pub fn new(
        strategy: AggregationStrategy,
        output_format: OutputFormat,
    ) -> Self {
        Self {
            strategy,
            output_format,
            buffer_size: 1000,
            checkpointer: None,
        }
    }

    /// Set buffer size for streaming
    pub fn with_buffer_size(mut self, buffer_size: usize) -> Self {
        self.buffer_size = buffer_size;
        self
    }

    /// Set checkpointer for result persistence
    pub fn with_checkpointer(mut self, checkpointer: Arc<dyn Checkpointer>) -> Self {
        self.checkpointer = Some(checkpointer);
        self
    }

    /// Aggregate a collection of batch results
    pub async fn aggregate(&self, results: Vec<BatchResult>) -> Result<AggregatedResults, LangGraphError> {
        use std::time::Instant;

        let start_time = Instant::now();
        let total_jobs = results.len();

        // Count successful and failed jobs
        let successful_jobs = results.iter()
            .filter(|r| r.status == crate::batch::BatchJobStatus::Completed)
            .count();
        let failed_jobs = results.iter()
            .filter(|r| r.status == crate::batch::BatchJobStatus::Failed)
            .count();

        // Create metadata
        let processing_time_ms = start_time.elapsed().as_millis() as u64;
        let metadata = AggregationMetadata {
            strategy: match &self.strategy {
                AggregationStrategy::Collect => "collect".to_string(),
                AggregationStrategy::Merge(_) => "merge".to_string(),
                AggregationStrategy::Reduce(_) => "reduce".to_string(),
                AggregationStrategy::Stream(_) => "stream".to_string(),
            },
            output_format: match &self.output_format {
                OutputFormat::Json => "json".to_string(),
                OutputFormat::Csv => "csv".to_string(),
                OutputFormat::Parquet => "parquet".to_string(),
                OutputFormat::Custom(name) => name.clone(),
            },
            processing_time_ms,
            memory_used_bytes: std::mem::size_of_val(&results),
        };

        // Apply aggregation strategy
        let aggregated_results = match &self.strategy {
            AggregationStrategy::Collect => results,
            AggregationStrategy::Merge(merge_fn) => {
                if results.is_empty() {
                    results
                } else {
                    vec![merge_fn.merge(results)?]
                }
            }
            AggregationStrategy::Reduce(reduce_fn) => {
                let mut accumulator = None;
                for result in results {
                    accumulator = Some(reduce_fn.reduce(accumulator, result)?);
                }
                accumulator.into_iter().collect()
            }
            AggregationStrategy::Stream(_) => {
                // For stream strategy, just collect all results for now
                results
            }
        };

        // Save to checkpointer if configured
        if let Some(checkpointer) = &self.checkpointer {
            let checkpoint_data = serde_json::to_string(&aggregated_results)
                .map_err(|e| LangGraphError::SerializationError(e.to_string()))?;
            let checkpoint_id = format!("aggregation_{}", uuid::Uuid::new_v4());

            // Create temporary state for checkpointing
            let mut state = crate::state::StateData::new();
            state.insert("aggregated_results".to_string(), serde_json::Value::String(checkpoint_data));

            checkpointer.save_checkpoint("aggregation", &checkpoint_id, &state)
                .await
                .map_err(|e| LangGraphError::CheckpointError(format!("Failed to save aggregation checkpoint: {}", e)))?;
        }

        Ok(AggregatedResults {
            total_jobs,
            successful_jobs,
            failed_jobs,
            results: aggregated_results,
            metadata,
        })
    }

    /// Create a streaming aggregator
    pub fn create_stream(
        &self,
        receiver: mpsc::Receiver<BatchResult>,
        consumer: Arc<dyn ResultConsumer>,
    ) -> ResultStream {
        ResultStream {
            receiver,
            aggregator: self.clone(),
            consumer,
        }
    }

    /// Export results in specified format
    pub async fn export(&self, results: &AggregatedResults) -> Result<String, LangGraphError> {
        match &self.output_format {
            OutputFormat::Json => {
                serde_json::to_string_pretty(results)
                    .map_err(|e| LangGraphError::SerializationError(e.to_string()))
            }
            OutputFormat::Csv => {
                // Create CSV output with basic fields
                let mut csv_output = String::new();
                csv_output.push_str("job_id,status,duration_ms,attempts,error\n");

                for result in &results.results {
                    csv_output.push_str(&format!(
                        "{},{:?},{},{},{}\n",
                        result.job_id,
                        result.status,
                        result.duration.as_millis(),
                        result.attempts,
                        result.error.as_deref().unwrap_or("")
                    ));
                }

                Ok(csv_output)
            }
            OutputFormat::Parquet => {
                // For YELLOW phase, return a placeholder
                // In GREEN phase, we'll implement actual Parquet format
                Ok("Parquet export not yet implemented".to_string())
            }
            OutputFormat::Custom(format_name) => {
                Ok(format!("Custom format '{}' export not yet implemented", format_name))
            }
        }
    }
}

impl Clone for ResultAggregator {
    fn clone(&self) -> Self {
        Self {
            strategy: match &self.strategy {
                AggregationStrategy::Collect => AggregationStrategy::Collect,
                AggregationStrategy::Merge(_) => AggregationStrategy::Collect, // Simplified for cloning
                AggregationStrategy::Reduce(_) => AggregationStrategy::Collect, // Simplified for cloning
                AggregationStrategy::Stream(_) => AggregationStrategy::Collect, // Simplified for cloning
            },
            output_format: self.output_format.clone(),
            buffer_size: self.buffer_size,
            checkpointer: self.checkpointer.clone(),
        }
    }
}

impl ResultStream {
    /// Start processing the result stream
    pub async fn process(mut self) -> Result<(), LangGraphError> {
        let mut collected_results = Vec::new();

        // Process results from the stream
        while let Some(result) = self.receiver.recv().await {
            // Apply any stream processing if configured
            let processed_result = match &self.aggregator.strategy {
                AggregationStrategy::Stream(processor) => {
                    match processor.process(result).await? {
                        Some(processed) => processed,
                        None => continue, // Result was filtered out
                    }
                }
                _ => result, // No stream processing, use result as-is
            };

            collected_results.push(processed_result);

            // Check if buffer is full and process batch
            if collected_results.len() >= self.aggregator.buffer_size {
                self.consumer.consume(collected_results.clone()).await?;
                collected_results.clear();
            }
        }

        // Process any remaining results
        if !collected_results.is_empty() {
            self.consumer.consume(collected_results).await?;
        }

        Ok(())
    }
}

/// Simple collect strategy implementation
#[derive(Debug)]
pub struct CollectStrategy;

/// Simple JSON consumer implementation
#[derive(Debug)]
pub struct JsonConsumer {
    output_path: String,
}

impl JsonConsumer {
    pub fn new(output_path: String) -> Self {
        Self { output_path }
    }
}

impl ResultConsumer for JsonConsumer {
    fn consume(&self, results: Vec<BatchResult>) -> Pin<Box<dyn Future<Output = Result<(), LangGraphError>> + Send + '_>> {
        Box::pin(async move {
            // Serialize results to JSON
            let json_data = serde_json::to_string_pretty(&results)
                .map_err(|e| LangGraphError::SerializationError(e.to_string()))?;

            // For YELLOW phase, just write to string (could write to file in GREEN phase)
            // In a real implementation, we'd write to the output_path file
            tracing::info!("JSON Consumer would write {} bytes to {}", json_data.len(), self.output_path);

            Ok(())
        })
    }
}

/// Status-based result filter
#[derive(Debug)]
pub struct StatusFilter {
    allowed_statuses: Vec<crate::batch::BatchJobStatus>,
}

impl StatusFilter {
    pub fn new(allowed_statuses: Vec<crate::batch::BatchJobStatus>) -> Self {
        Self { allowed_statuses }
    }
}

impl ResultFilter for StatusFilter {
    fn filter(&self, result: &BatchResult) -> bool {
        self.allowed_statuses.contains(&result.status)
    }
}