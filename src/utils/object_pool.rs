//! Object pooling for frequently allocated types
//!
//! This module provides object pools to reduce allocation overhead
//! for frequently created and destroyed objects.

use std::collections::{VecDeque, HashMap};
use std::sync::Arc;
use parking_lot::Mutex;
use std::ops::{Deref, DerefMut};
use serde_json::Value;

/// A pool of reusable objects
pub struct ObjectPool<T> {
    /// Available objects
    pool: Arc<Mutex<VecDeque<T>>>,
    
    /// Factory function to create new objects
    factory: Arc<dyn Fn() -> T + Send + Sync>,
    
    /// Reset function to clean objects before reuse
    reset: Arc<dyn Fn(&mut T) + Send + Sync>,
    
    /// Maximum pool size
    max_size: usize,
    
    /// Current number of objects created
    created: Arc<Mutex<usize>>,
}

impl<T> ObjectPool<T> {
    /// Create a new object pool
    pub fn new<F, R>(factory: F, reset: R, max_size: usize) -> Self
    where
        F: Fn() -> T + Send + Sync + 'static,
        R: Fn(&mut T) + Send + Sync + 'static,
    {
        Self {
            pool: Arc::new(Mutex::new(VecDeque::with_capacity(max_size))),
            factory: Arc::new(factory),
            reset: Arc::new(reset),
            max_size,
            created: Arc::new(Mutex::new(0)),
        }
    }
    
    /// Get an object from the pool
    pub fn get(&self) -> PooledObject<T> {
        let mut pool = self.pool.lock();
        
        let obj = if let Some(mut obj) = pool.pop_front() {
            // Reset and reuse existing object
            (self.reset)(&mut obj);
            obj
        } else {
            // Create new object
            let mut created = self.created.lock();
            *created += 1;
            (self.factory)()
        };
        
        PooledObject {
            value: Some(obj),
            pool: self.pool.clone(),
            reset: self.reset.clone(),
            max_size: self.max_size,
        }
    }
    
    /// Get current pool size
    pub fn size(&self) -> usize {
        self.pool.lock().len()
    }
    
    /// Get total objects created
    pub fn total_created(&self) -> usize {
        *self.created.lock()
    }
    
    /// Clear the pool
    pub fn clear(&self) {
        self.pool.lock().clear();
    }
}

impl<T> Clone for ObjectPool<T> {
    fn clone(&self) -> Self {
        Self {
            pool: self.pool.clone(),
            factory: self.factory.clone(),
            reset: self.reset.clone(),
            max_size: self.max_size,
            created: self.created.clone(),
        }
    }
}

/// A pooled object that returns to the pool when dropped
pub struct PooledObject<T> {
    value: Option<T>,
    pool: Arc<Mutex<VecDeque<T>>>,
    reset: Arc<dyn Fn(&mut T) + Send + Sync>,
    max_size: usize,
}

impl<T> Drop for PooledObject<T> {
    fn drop(&mut self) {
        if let Some(mut obj) = self.value.take() {
            let mut pool = self.pool.lock();
            if pool.len() < self.max_size {
                // Reset and return to pool
                (self.reset)(&mut obj);
                pool.push_back(obj);
            }
            // Otherwise, let it be dropped
        }
    }
}

impl<T> Deref for PooledObject<T> {
    type Target = T;
    
    fn deref(&self) -> &Self::Target {
        self.value.as_ref().expect("PooledObject already dropped")
    }
}

impl<T> DerefMut for PooledObject<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.value.as_mut().expect("PooledObject already dropped")
    }
}

/// Pre-configured pools for common types
pub mod pools {
    use super::*;
    
    lazy_static::lazy_static! {
        /// Pool for HashMaps used in state data
        pub static ref STATE_MAP_POOL: ObjectPool<HashMap<String, Value>> = ObjectPool::new(
            || HashMap::with_capacity(32),
            |map| map.clear(),
            100
        );
        
        /// Pool for Vecs used in history tracking
        pub static ref HISTORY_VEC_POOL: ObjectPool<Vec<String>> = ObjectPool::new(
            || Vec::with_capacity(100),
            |vec| vec.clear(),
            50
        );
        
        /// Pool for String buffers
        pub static ref STRING_POOL: ObjectPool<String> = ObjectPool::new(
            || String::with_capacity(256),
            |s| s.clear(),
            200
        );
        
        /// Pool for byte buffers
        pub static ref BUFFER_POOL: ObjectPool<Vec<u8>> = ObjectPool::new(
            || Vec::with_capacity(4096),
            |buf| buf.clear(),
            50
        );
    }
}

/// Specialized pool for graph nodes
pub struct NodePool {
    pool: ObjectPool<NodeBuffer>,
}

/// Buffer for node data
pub struct NodeBuffer {
    pub id: String,
    pub data: HashMap<String, Value>,
    pub metadata: HashMap<String, String>,
}

impl NodePool {
    /// Create a new node pool
    pub fn new(max_size: usize) -> Self {
        let pool = ObjectPool::new(
            || NodeBuffer {
                id: String::with_capacity(64),
                data: HashMap::with_capacity(16),
                metadata: HashMap::with_capacity(8),
            },
            |buf| {
                buf.id.clear();
                buf.data.clear();
                buf.metadata.clear();
            },
            max_size,
        );
        
        Self { pool }
    }
    
    /// Get a node buffer from the pool
    pub fn get(&self) -> PooledObject<NodeBuffer> {
        self.pool.get()
    }
}

/// Thread-local pools for performance-critical allocations
thread_local! {
    /// Thread-local small vector pool
    pub static SMALL_VEC_POOL: std::cell::RefCell<Vec<Vec<u8>>> = 
        std::cell::RefCell::new(Vec::with_capacity(10));
}

/// Get a small vector from thread-local pool
pub fn get_small_vec() -> Vec<u8> {
    SMALL_VEC_POOL.with(|pool| {
        pool.borrow_mut().pop().unwrap_or_else(|| Vec::with_capacity(64))
    })
}

/// Return a small vector to thread-local pool
pub fn return_small_vec(mut vec: Vec<u8>) {
    vec.clear();
    SMALL_VEC_POOL.with(|pool| {
        let mut pool = pool.borrow_mut();
        if pool.len() < 10 {
            pool.push(vec);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_object_pool() {
        let pool: ObjectPool<Vec<i32>> = ObjectPool::new(
            || Vec::with_capacity(10),
            |v| v.clear(),
            5,
        );
        
        // Get object from pool
        let mut obj1 = pool.get();
        obj1.push(1);
        obj1.push(2);
        
        // Drop returns to pool
        drop(obj1);
        assert_eq!(pool.size(), 1);
        
        // Reuse object
        let obj2 = pool.get();
        assert!(obj2.is_empty()); // Should be cleared
    }
    
    #[test]
    fn test_pool_max_size() {
        let pool: ObjectPool<String> = ObjectPool::new(
            || String::with_capacity(10),
            |s| s.clear(),
            2,
        );
        
        // Create multiple objects
        let obj1 = pool.get();
        let obj2 = pool.get();
        let obj3 = pool.get();
        
        // Drop all
        drop(obj1);
        drop(obj2);
        drop(obj3);
        
        // Pool should only keep up to max_size
        assert!(pool.size() <= 2);
    }
    
    #[test]
    fn test_state_map_pool() {
        let mut map = pools::STATE_MAP_POOL.get();
        map.insert("key".to_string(), serde_json::json!("value"));
        
        drop(map);
        
        let map2 = pools::STATE_MAP_POOL.get();
        assert!(map2.is_empty());
    }
}