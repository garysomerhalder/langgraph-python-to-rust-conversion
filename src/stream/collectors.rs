use async_trait::async_trait;
use futures::stream::{Stream, StreamExt};
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::graph::GraphError;

#[async_trait]
pub trait StreamCollector: Send + Sync {
    type Item: Send + Sync;
    type Output: Send + Sync;
    
    async fn collect_item(&mut self, item: Self::Item) -> Result<(), GraphError>;
    async fn finalize(self) -> Result<Self::Output, GraphError>;
    
    async fn collect_stream(
        mut self,
        mut stream: Pin<Box<dyn Stream<Item = Self::Item> + Send>>,
    ) -> Result<Self::Output, GraphError>
    where
        Self: Sized,
    {
        while let Some(item) = stream.next().await {
            self.collect_item(item).await?;
        }
        self.finalize().await
    }
}

pub struct VecCollector<T> {
    items: Vec<T>,
    max_size: Option<usize>,
}

impl<T> VecCollector<T> {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            max_size: None,
        }
    }
    
    pub fn with_max_size(max_size: usize) -> Self {
        Self {
            items: Vec::new(),
            max_size: Some(max_size),
        }
    }
}

#[async_trait]
impl<T: Send + Sync> StreamCollector for VecCollector<T> {
    type Item = T;
    type Output = Vec<T>;
    
    async fn collect_item(&mut self, item: Self::Item) -> Result<(), GraphError> {
        if let Some(max) = self.max_size {
            if self.items.len() >= max {
                return Err(GraphError::ValidationError(
                    format!("Collector reached max size of {}", max)
                ));
            }
        }
        
        self.items.push(item);
        Ok(())
    }
    
    async fn finalize(self) -> Result<Self::Output, GraphError> {
        Ok(self.items)
    }
}

pub struct HashMapCollector<K, V> {
    map: HashMap<K, V>,
}

impl<K, V> HashMapCollector<K, V> {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }
}

#[async_trait]
impl<K, V> StreamCollector for HashMapCollector<K, V>
where
    K: Send + Sync + Eq + std::hash::Hash,
    V: Send + Sync,
{
    type Item = (K, V);
    type Output = HashMap<K, V>;
    
    async fn collect_item(&mut self, item: Self::Item) -> Result<(), GraphError> {
        let (key, value) = item;
        self.map.insert(key, value);
        Ok(())
    }
    
    async fn finalize(self) -> Result<Self::Output, GraphError> {
        Ok(self.map)
    }
}

pub struct AggregateCollector<T, A> {
    aggregator: A,
    accumulator: Option<T>,
}

impl<T, A> AggregateCollector<T, A> {
    pub fn new(aggregator: A) -> Self {
        Self {
            aggregator,
            accumulator: None,
        }
    }
}

#[async_trait]
impl<T, A> StreamCollector for AggregateCollector<T, A>
where
    T: Send + Sync + Clone,
    A: Fn(Option<T>, T) -> T + Send + Sync,
{
    type Item = T;
    type Output = Option<T>;
    
    async fn collect_item(&mut self, item: Self::Item) -> Result<(), GraphError> {
        self.accumulator = Some((self.aggregator)(self.accumulator.clone(), item));
        Ok(())
    }
    
    async fn finalize(self) -> Result<Self::Output, GraphError> {
        Ok(self.accumulator)
    }
}

pub struct GroupByCollector<K, V> {
    groups: HashMap<K, Vec<V>>,
}

impl<K, V> GroupByCollector<K, V> {
    pub fn new() -> Self {
        Self {
            groups: HashMap::new(),
        }
    }
}

#[async_trait]
impl<K, V> StreamCollector for GroupByCollector<K, V>
where
    K: Send + Sync + Eq + std::hash::Hash,
    V: Send + Sync,
{
    type Item = (K, V);
    type Output = HashMap<K, Vec<V>>;
    
    async fn collect_item(&mut self, item: Self::Item) -> Result<(), GraphError> {
        let (key, value) = item;
        self.groups.entry(key).or_insert_with(Vec::new).push(value);
        Ok(())
    }
    
    async fn finalize(self) -> Result<Self::Output, GraphError> {
        Ok(self.groups)
    }
}

pub struct StatisticsCollector {
    count: usize,
    sum: f64,
    min: Option<f64>,
    max: Option<f64>,
    values: Vec<f64>,
}

impl StatisticsCollector {
    pub fn new() -> Self {
        Self {
            count: 0,
            sum: 0.0,
            min: None,
            max: None,
            values: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Statistics {
    pub count: usize,
    pub sum: f64,
    pub mean: f64,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub median: Option<f64>,
    pub std_dev: Option<f64>,
}

#[async_trait]
impl StreamCollector for StatisticsCollector {
    type Item = f64;
    type Output = Statistics;
    
    async fn collect_item(&mut self, item: Self::Item) -> Result<(), GraphError> {
        self.count += 1;
        self.sum += item;
        
        self.min = Some(self.min.map_or(item, |m| m.min(item)));
        self.max = Some(self.max.map_or(item, |m| m.max(item)));
        
        self.values.push(item);
        Ok(())
    }
    
    async fn finalize(mut self) -> Result<Self::Output, GraphError> {
        if self.count == 0 {
            return Ok(Statistics {
                count: 0,
                sum: 0.0,
                mean: 0.0,
                min: None,
                max: None,
                median: None,
                std_dev: None,
            });
        }
        
        let mean = self.sum / self.count as f64;
        
        self.values.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = if self.count % 2 == 0 {
            let mid = self.count / 2;
            Some((self.values[mid - 1] + self.values[mid]) / 2.0)
        } else {
            Some(self.values[self.count / 2])
        };
        
        let variance = self.values.iter()
            .map(|v| (v - mean).powi(2))
            .sum::<f64>() / self.count as f64;
        let std_dev = Some(variance.sqrt());
        
        Ok(Statistics {
            count: self.count,
            sum: self.sum,
            mean,
            min: self.min,
            max: self.max,
            median,
            std_dev,
        })
    }
}

pub struct BufferedCollector<T> {
    buffer: Arc<RwLock<Vec<T>>>,
    flush_size: usize,
    flush_callback: Arc<dyn Fn(Vec<T>) + Send + Sync>,
}

impl<T> BufferedCollector<T> {
    pub fn new<F>(flush_size: usize, flush_callback: F) -> Self
    where
        F: Fn(Vec<T>) + Send + Sync + 'static,
    {
        Self {
            buffer: Arc::new(RwLock::new(Vec::new())),
            flush_size,
            flush_callback: Arc::new(flush_callback),
        }
    }
    
    async fn flush(&self) -> Result<(), GraphError> {
        let mut buffer = self.buffer.write().await;
        if !buffer.is_empty() {
            let items = std::mem::take(&mut *buffer);
            (self.flush_callback)(items);
        }
        Ok(())
    }
}

#[async_trait]
impl<T: Send + Sync + Clone> StreamCollector for BufferedCollector<T> {
    type Item = T;
    type Output = ();
    
    async fn collect_item(&mut self, item: Self::Item) -> Result<(), GraphError> {
        let mut buffer = self.buffer.write().await;
        buffer.push(item);
        
        if buffer.len() >= self.flush_size {
            let items = std::mem::take(&mut *buffer);
            drop(buffer);
            (self.flush_callback)(items);
        }
        
        Ok(())
    }
    
    async fn finalize(self) -> Result<Self::Output, GraphError> {
        self.flush().await?;
        Ok(())
    }
}