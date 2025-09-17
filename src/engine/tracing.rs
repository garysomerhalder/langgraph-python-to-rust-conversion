//! Distributed tracing support for graph execution
//! Provides comprehensive tracing with OpenTelemetry compatibility

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use uuid::Uuid;
use tracing::{debug, error, info, warn, instrument, span, Level};
use serde::{Deserialize, Serialize};

/// Trace context for distributed tracing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceContext {
    /// Unique trace ID
    pub trace_id: String,
    /// Parent span ID
    pub parent_span_id: Option<String>,
    /// Current span ID
    pub span_id: String,
    /// Trace flags
    pub flags: u8,
    /// Baggage items
    pub baggage: HashMap<String, String>,
}

impl TraceContext {
    /// Create a new trace context
    pub fn new() -> Self {
        Self {
            trace_id: Uuid::new_v4().to_string(),
            parent_span_id: None,
            span_id: Uuid::new_v4().to_string(),
            flags: 1, // Sampled
            baggage: HashMap::new(),
        }
    }
    
    /// Create a child span context
    pub fn child(&self) -> Self {
        Self {
            trace_id: self.trace_id.clone(),
            parent_span_id: Some(self.span_id.clone()),
            span_id: Uuid::new_v4().to_string(),
            flags: self.flags,
            baggage: self.baggage.clone(),
        }
    }
    
    /// Add baggage item
    pub fn add_baggage(&mut self, key: String, value: String) {
        self.baggage.insert(key, value);
    }
    
    /// Get baggage item
    pub fn get_baggage(&self, key: &str) -> Option<&String> {
        self.baggage.get(key)
    }
}

/// Span represents a unit of work in a trace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    /// Span ID
    pub span_id: String,
    /// Trace ID
    pub trace_id: String,
    /// Parent span ID
    pub parent_span_id: Option<String>,
    /// Operation name
    pub operation_name: String,
    /// Start time
    pub start_time: SystemTime,
    /// End time
    pub end_time: Option<SystemTime>,
    /// Duration
    pub duration: Option<Duration>,
    /// Tags
    pub tags: HashMap<String, String>,
    /// Events
    pub events: Vec<SpanEvent>,
    /// Status
    pub status: SpanStatus,
}

impl Span {
    /// Create a new span
    pub fn new(context: &TraceContext, operation_name: String) -> Self {
        Self {
            span_id: context.span_id.clone(),
            trace_id: context.trace_id.clone(),
            parent_span_id: context.parent_span_id.clone(),
            operation_name,
            start_time: SystemTime::now(),
            end_time: None,
            duration: None,
            tags: HashMap::new(),
            events: Vec::new(),
            status: SpanStatus::Unset,
        }
    }
    
    /// Add a tag to the span
    pub fn add_tag(&mut self, key: String, value: String) {
        self.tags.insert(key, value);
    }
    
    /// Add an event to the span
    pub fn add_event(&mut self, event: SpanEvent) {
        self.events.push(event);
    }
    
    /// Set the span status
    pub fn set_status(&mut self, status: SpanStatus) {
        self.status = status;
    }
    
    /// End the span
    pub fn end(&mut self) {
        let end_time = SystemTime::now();
        self.end_time = Some(end_time);
        self.duration = Some(end_time.duration_since(self.start_time).unwrap_or_default());
    }
}

/// Span event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanEvent {
    /// Event name
    pub name: String,
    /// Timestamp
    pub timestamp: SystemTime,
    /// Attributes
    pub attributes: HashMap<String, String>,
}

impl SpanEvent {
    /// Create a new span event
    pub fn new(name: String) -> Self {
        Self {
            name,
            timestamp: SystemTime::now(),
            attributes: HashMap::new(),
        }
    }
    
    /// Add an attribute
    pub fn add_attribute(&mut self, key: String, value: String) {
        self.attributes.insert(key, value);
    }
}

/// Span status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SpanStatus {
    /// Unset status
    Unset,
    /// OK status
    Ok,
    /// Error status
    Error(String),
}

/// Tracer for creating and managing spans
pub struct Tracer {
    /// Active spans
    active_spans: Arc<RwLock<HashMap<String, Span>>>,
    /// Completed spans
    completed_spans: Arc<RwLock<Vec<Span>>>,
    /// Exporters
    exporters: Arc<RwLock<Vec<Box<dyn SpanExporter>>>>,
}

impl Tracer {
    /// Create a new tracer
    pub fn new(_name: &str) -> Self {
        Self {
            active_spans: Arc::new(RwLock::new(HashMap::new())),
            completed_spans: Arc::new(RwLock::new(Vec::new())),
            exporters: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    /// Start a new span with a new trace context
    pub fn start_span(&self, operation: &str) -> SpanHandle {
        let context = TraceContext::new();
        let span = Span::new(&context, operation.to_string());
        let span_id = span.span_id.clone();
        
        // Use tokio::spawn to handle the async operation
        let active_spans = self.active_spans.clone();
        tokio::spawn(async move {
            active_spans.write().await.insert(span_id.clone(), span);
        });
        
        SpanHandle {
            span_id: context.span_id,
            tracer: self.clone(),
        }
    }
    
    /// Start a new span with provided context
    #[instrument(skip(self))]
    pub async fn start_span_with_context(&self, context: &TraceContext, operation: String) -> SpanHandle {
        let span = Span::new(context, operation);
        let span_id = span.span_id.clone();
        
        self.active_spans.write().await.insert(span_id.clone(), span);
        
        SpanHandle {
            span_id,
            tracer: self.clone(),
        }
    }
    
    /// End a span
    async fn end_span(&self, span_id: String) {
        let mut active = self.active_spans.write().await;
        if let Some(mut span) = active.remove(&span_id) {
            span.end();
            
            // Export to all exporters
            let exporters = self.exporters.read().await;
            for exporter in exporters.iter() {
                if let Err(e) = exporter.export(&span).await {
                    error!("Failed to export span: {}", e);
                }
            }
            
            // Store completed span
            self.completed_spans.write().await.push(span);
        }
    }
    
    /// Add a span exporter
    pub async fn add_exporter(&self, exporter: Box<dyn SpanExporter>) {
        self.exporters.write().await.push(exporter);
    }
    
    /// Get metrics
    pub async fn get_metrics(&self) -> TracingMetrics {
        let active_count = self.active_spans.read().await.len();
        let completed_count = self.completed_spans.read().await.len();
        
        let completed = self.completed_spans.read().await;
        let total_duration: Duration = completed
            .iter()
            .filter_map(|s| s.duration)
            .sum();
        
        let error_count = completed
            .iter()
            .filter(|s| matches!(s.status, SpanStatus::Error(_)))
            .count();
        
        TracingMetrics {
            active_spans: active_count,
            completed_spans: completed_count,
            total_duration,
            error_count,
            average_duration: if completed_count > 0 {
                Some(total_duration / completed_count as u32)
            } else {
                None
            },
        }
    }
}

impl Clone for Tracer {
    fn clone(&self) -> Self {
        Self {
            active_spans: self.active_spans.clone(),
            completed_spans: self.completed_spans.clone(),
            exporters: self.exporters.clone(),
        }
    }
}

/// Handle to an active span
pub struct SpanHandle {
    span_id: String,
    tracer: Tracer,
}

impl SpanHandle {
    /// Add a tag to the span
    pub async fn add_tag(&self, key: String, value: String) {
        if let Some(span) = self.tracer.active_spans.write().await.get_mut(&self.span_id) {
            span.add_tag(key, value);
        }
    }
    
    /// Add an event to the span
    pub async fn add_event(&self, event: SpanEvent) {
        if let Some(span) = self.tracer.active_spans.write().await.get_mut(&self.span_id) {
            span.add_event(event);
        }
    }
    
    /// Set the span status
    pub async fn set_status(&self, status: SpanStatus) {
        if let Some(span) = self.tracer.active_spans.write().await.get_mut(&self.span_id) {
            span.set_status(status);
        }
    }
    
    /// End the span
    pub fn end(self) {
        let tracer = self.tracer.clone();
        let span_id = self.span_id.clone();
        tokio::spawn(async move {
            tracer.end_span(span_id).await;
        });
    }
}

/// Trait for span exporters
#[async_trait::async_trait]
pub trait SpanExporter: Send + Sync {
    /// Export a span
    async fn export(&self, span: &Span) -> Result<(), Box<dyn std::error::Error>>;
}

/// Console span exporter for debugging
pub struct ConsoleSpanExporter;

#[async_trait::async_trait]
impl SpanExporter for ConsoleSpanExporter {
    async fn export(&self, span: &Span) -> Result<(), Box<dyn std::error::Error>> {
        let duration = span.duration
            .map(|d| format!("{}ms", d.as_millis()))
            .unwrap_or_else(|| "unknown".to_string());
        
        let status = match &span.status {
            SpanStatus::Ok => "OK",
            SpanStatus::Error(e) => e,
            SpanStatus::Unset => "UNSET",
        };
        
        info!(
            "[TRACE] {} - {} ({}): {} - Tags: {:?}",
            span.trace_id,
            span.operation_name,
            duration,
            status,
            span.tags
        );
        
        for event in &span.events {
            debug!("  Event: {} - {:?}", event.name, event.attributes);
        }
        
        Ok(())
    }
}

/// JSON span exporter for file output
pub struct JsonSpanExporter {
    file_path: String,
}

impl JsonSpanExporter {
    /// Create a new JSON exporter
    pub fn new(file_path: String) -> Self {
        Self { file_path }
    }
}

#[async_trait::async_trait]
impl SpanExporter for JsonSpanExporter {
    async fn export(&self, span: &Span) -> Result<(), Box<dyn std::error::Error>> {
        let json = serde_json::to_string_pretty(span)?;
        tokio::fs::write(&self.file_path, json).await?;
        Ok(())
    }
}

/// Tracing metrics
#[derive(Debug, Clone)]
pub struct TracingMetrics {
    /// Number of active spans
    pub active_spans: usize,
    /// Number of completed spans
    pub completed_spans: usize,
    /// Total duration of completed spans
    pub total_duration: Duration,
    /// Number of error spans
    pub error_count: usize,
    /// Average span duration
    pub average_duration: Option<Duration>,
}

/// Instrumented executor wrapper
pub struct InstrumentedExecutor<E> {
    inner: E,
    tracer: Tracer,
}

impl<E> InstrumentedExecutor<E> {
    /// Create a new instrumented executor
    pub fn new(inner: E, tracer: Tracer) -> Self {
        Self { inner, tracer }
    }
    
    /// Execute with tracing
    pub async fn execute_with_tracing<F, Fut, T>(
        &self,
        context: TraceContext,
        operation: String,
        f: F,
    ) -> Result<T, Box<dyn std::error::Error>>
    where
        F: FnOnce(&E) -> Fut,
        Fut: std::future::Future<Output = Result<T, Box<dyn std::error::Error>>>,
    {
        let span = self.tracer.start_span_with_context(&context, operation).await;
        
        let result = f(&self.inner).await;
        
        match &result {
            Ok(_) => span.set_status(SpanStatus::Ok).await,
            Err(e) => span.set_status(SpanStatus::Error(e.to_string())).await,
        }
        
        span.end();
        
        result
    }
}

/// Context propagation for distributed tracing
pub struct ContextPropagator;

impl ContextPropagator {
    /// Extract trace context from headers
    pub fn extract(headers: &HashMap<String, String>) -> Option<TraceContext> {
        let trace_id = headers.get("X-Trace-Id")?;
        let span_id = headers.get("X-Span-Id")?;
        let parent_span_id = headers.get("X-Parent-Span-Id");
        
        let mut context = TraceContext {
            trace_id: trace_id.clone(),
            span_id: span_id.clone(),
            parent_span_id: parent_span_id.cloned(),
            flags: headers.get("X-Trace-Flags")
                .and_then(|f| f.parse().ok())
                .unwrap_or(1),
            baggage: HashMap::new(),
        };
        
        // Extract baggage
        for (key, value) in headers {
            if key.starts_with("X-Baggage-") {
                let baggage_key = key.strip_prefix("X-Baggage-").unwrap();
                context.baggage.insert(baggage_key.to_string(), value.clone());
            }
        }
        
        Some(context)
    }
    
    /// Inject trace context into headers
    pub fn inject(context: &TraceContext, headers: &mut HashMap<String, String>) {
        headers.insert("X-Trace-Id".to_string(), context.trace_id.clone());
        headers.insert("X-Span-Id".to_string(), context.span_id.clone());
        
        if let Some(parent) = &context.parent_span_id {
            headers.insert("X-Parent-Span-Id".to_string(), parent.clone());
        }
        
        headers.insert("X-Trace-Flags".to_string(), context.flags.to_string());
        
        // Inject baggage
        for (key, value) in &context.baggage {
            headers.insert(format!("X-Baggage-{}", key), value.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_trace_context() {
        let context = TraceContext::new();
        assert!(!context.trace_id.is_empty());
        assert!(!context.span_id.is_empty());
        assert!(context.parent_span_id.is_none());
        
        let child = context.child();
        assert_eq!(child.trace_id, context.trace_id);
        assert_eq!(child.parent_span_id, Some(context.span_id));
    }
    
    #[tokio::test]
    async fn test_span_lifecycle() {
        let tracer = Tracer::new();
        let context = TraceContext::new();
        
        let span = tracer.start_span(&context, "test_operation".to_string()).await;
        
        span.add_tag("key".to_string(), "value".to_string()).await;
        
        let mut event = SpanEvent::new("test_event".to_string());
        event.add_attribute("attr".to_string(), "value".to_string());
        span.add_event(event).await;
        
        span.set_status(SpanStatus::Ok).await;
        span.end().await;
        
        let metrics = tracer.get_metrics().await;
        assert_eq!(metrics.completed_spans, 1);
        assert_eq!(metrics.error_count, 0);
    }
    
    #[tokio::test]
    async fn test_context_propagation() {
        let mut context = TraceContext::new();
        context.baggage.insert("user_id".to_string(), "123".to_string());
        
        let mut headers = HashMap::new();
        ContextPropagator::inject(&context, &mut headers);
        
        assert!(headers.contains_key("X-Trace-Id"));
        assert!(headers.contains_key("X-Span-Id"));
        assert!(headers.contains_key("X-Baggage-user_id"));
        
        let extracted = ContextPropagator::extract(&headers).unwrap();
        assert_eq!(extracted.trace_id, context.trace_id);
        assert_eq!(extracted.span_id, context.span_id);
        assert_eq!(extracted.baggage.get("user_id"), Some(&"123".to_string()));
    }
}