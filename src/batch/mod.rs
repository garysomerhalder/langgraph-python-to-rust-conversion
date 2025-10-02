//! Batch Execution Module
//!
//! Provides batch processing capabilities for executing multiple
//! workflows efficiently with concurrency control and monitoring.

pub mod executor;

pub use executor::{BatchExecutor, BatchJob, BatchResult, BatchJobStatus};
