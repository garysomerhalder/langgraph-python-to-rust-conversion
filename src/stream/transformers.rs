use crate::graph::GraphError;
use async_trait::async_trait;
use futures::stream::{Stream, StreamExt};
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::RwLock;

#[async_trait]
pub trait StreamTransformer: Send + Sync {
    type Input: Send + Sync;
    type Output: Send + Sync;

    async fn transform(&self, input: Self::Input) -> Result<Self::Output, GraphError>;

    async fn transform_stream(
        &self,
        _input_stream: Pin<Box<dyn Stream<Item = Self::Input> + Send>>,
    ) -> Result<Pin<Box<dyn Stream<Item = Self::Output> + Send>>, GraphError>
    where
        Self: Sized + 'static,
    {
        // Default implementation that requires self to be 'static
        // Override this method in implementations that don't need 'static
        Err(GraphError::RuntimeError(
            "Stream transformation not implemented".to_string(),
        ))
    }
}

pub struct MapTransformer<F, I, O> {
    mapper: F,
    _phantom: std::marker::PhantomData<(I, O)>,
}

impl<F, I, O> MapTransformer<F, I, O> {
    pub fn new(mapper: F) -> Self {
        Self {
            mapper,
            _phantom: std::marker::PhantomData,
        }
    }
}

#[async_trait]
impl<F, I, O> StreamTransformer for MapTransformer<F, I, O>
where
    F: Fn(I) -> O + Send + Sync,
    I: Send + Sync,
    O: Send + Sync,
{
    type Input = I;
    type Output = O;

    async fn transform(&self, input: Self::Input) -> Result<Self::Output, GraphError> {
        Ok((self.mapper)(input))
    }
}

pub struct FilterTransformer<F, T> {
    predicate: F,
    _phantom: std::marker::PhantomData<T>,
}

impl<F, T> FilterTransformer<F, T> {
    pub fn new(predicate: F) -> Self {
        Self {
            predicate,
            _phantom: std::marker::PhantomData,
        }
    }
}

#[async_trait]
impl<F, T> StreamTransformer for FilterTransformer<F, T>
where
    F: Fn(&T) -> bool + Send + Sync,
    T: Send + Sync + Clone,
{
    type Input = T;
    type Output = T;

    async fn transform(&self, input: Self::Input) -> Result<Self::Output, GraphError> {
        if (self.predicate)(&input) {
            Ok(input)
        } else {
            Err(GraphError::ValidationError("Item filtered out".to_string()))
        }
    }
}

pub struct BatchTransformer<T> {
    batch_size: usize,
    buffer: Arc<RwLock<Vec<T>>>,
}

impl<T> BatchTransformer<T> {
    pub fn new(batch_size: usize) -> Self {
        Self {
            batch_size,
            buffer: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

#[async_trait]
impl<T> StreamTransformer for BatchTransformer<T>
where
    T: Send + Sync + Clone + 'static,
{
    type Input = T;
    type Output = Vec<T>;

    async fn transform(&self, input: Self::Input) -> Result<Self::Output, GraphError> {
        let mut buffer = self.buffer.write().await;
        buffer.push(input);

        if buffer.len() >= self.batch_size {
            let batch = buffer.clone();
            buffer.clear();
            Ok(batch)
        } else {
            Err(GraphError::ValidationError("Batch not full".to_string()))
        }
    }

    async fn transform_stream(
        &self,
        input_stream: Pin<Box<dyn Stream<Item = Self::Input> + Send>>,
    ) -> Result<Pin<Box<dyn Stream<Item = Self::Output> + Send>>, GraphError> {
        let batch_size = self.batch_size;

        let output_stream = input_stream.chunks(batch_size).map(|chunk| chunk);

        Ok(Box::pin(output_stream))
    }
}

pub struct WindowTransformer<T> {
    window_size: std::time::Duration,
    buffer: Arc<RwLock<Vec<(T, std::time::Instant)>>>,
}

impl<T> WindowTransformer<T> {
    pub fn new(window_size: std::time::Duration) -> Self {
        Self {
            window_size,
            buffer: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

#[async_trait]
impl<T> StreamTransformer for WindowTransformer<T>
where
    T: Send + Sync + Clone + 'static,
{
    type Input = T;
    type Output = Vec<T>;

    async fn transform(&self, input: Self::Input) -> Result<Self::Output, GraphError> {
        let now = std::time::Instant::now();
        let mut buffer = self.buffer.write().await;

        buffer.push((input, now));

        buffer.retain(|(_, timestamp)| now.duration_since(*timestamp) <= self.window_size);

        let window: Vec<T> = buffer.iter().map(|(item, _)| item.clone()).collect();

        Ok(window)
    }
}

pub struct ChainTransformer<T1, T2> {
    first: T1,
    second: T2,
}

impl<T1, T2> ChainTransformer<T1, T2> {
    pub fn new(first: T1, second: T2) -> Self {
        Self { first, second }
    }
}

#[async_trait]
impl<T1, T2> StreamTransformer for ChainTransformer<T1, T2>
where
    T1: StreamTransformer,
    T2: StreamTransformer<Input = T1::Output>,
    T1::Input: Send + Sync,
    T1::Output: Send + Sync,
    T2::Output: Send + Sync,
{
    type Input = T1::Input;
    type Output = T2::Output;

    async fn transform(&self, input: Self::Input) -> Result<Self::Output, GraphError> {
        let intermediate = self.first.transform(input).await?;
        self.second.transform(intermediate).await
    }
}
