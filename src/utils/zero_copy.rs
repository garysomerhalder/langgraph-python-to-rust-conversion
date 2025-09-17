//! Zero-copy optimizations for efficient data handling
//!
//! This module provides zero-copy abstractions to minimize memory allocations
//! and improve performance.

use std::borrow::Cow;
use std::sync::Arc;
use bytes::{Bytes, BytesMut};
use serde::Serialize;
use serde_json::Value;
use dashmap::{DashMap, DashSet};

/// Zero-copy string that can be either borrowed or owned
pub type ZeroCopyString<'a> = Cow<'a, str>;

/// Zero-copy bytes that can be either borrowed or owned
pub type ZeroCopyBytes<'a> = Cow<'a, [u8]>;

/// Efficient string interner for deduplication
pub struct StringInterner {
    strings: Arc<DashSet<Arc<str>>>,
}

impl StringInterner {
    /// Create a new string interner
    pub fn new() -> Self {
        Self {
            strings: Arc::new(DashSet::new()),
        }
    }
    
    /// Intern a string, returning a reference to the shared instance
    pub fn intern(&self, s: &str) -> Arc<str> {
        if let Some(existing) = self.strings.iter().find(|item| item.as_ref() == s) {
            return existing.clone();
        }
        
        let arc_str: Arc<str> = Arc::from(s);
        self.strings.insert(arc_str.clone());
        arc_str
    }
    
    /// Get the number of interned strings
    pub fn len(&self) -> usize {
        self.strings.len()
    }
    
    /// Clear all interned strings
    pub fn clear(&self) {
        self.strings.clear();
    }
}

impl Default for StringInterner {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for StringInterner {
    fn clone(&self) -> Self {
        Self {
            strings: self.strings.clone(),
        }
    }
}

/// Zero-copy JSON value wrapper
#[derive(Debug, Clone)]
pub struct ZeroCopyValue {
    /// The underlying bytes
    bytes: Bytes,
    
    /// Lazily parsed value
    value: Option<Value>,
}

impl ZeroCopyValue {
    /// Create from bytes
    pub fn from_bytes(bytes: Bytes) -> Self {
        Self {
            bytes,
            value: None,
        }
    }
    
    /// Create from a JSON value
    pub fn from_value(value: Value) -> Result<Self, serde_json::Error> {
        let bytes = Bytes::from(serde_json::to_vec(&value)?);
        Ok(Self {
            bytes,
            value: Some(value),
        })
    }
    
    /// Get the underlying bytes
    pub fn as_bytes(&self) -> &Bytes {
        &self.bytes
    }
    
    /// Get the parsed value (lazy parsing)
    pub fn as_value(&mut self) -> Result<&Value, serde_json::Error> {
        if self.value.is_none() {
            self.value = Some(serde_json::from_slice(&self.bytes)?);
        }
        Ok(self.value.as_ref().expect("Value should be set after parsing"))
    }
    
    /// Take ownership of the bytes
    pub fn into_bytes(self) -> Bytes {
        self.bytes
    }
}

/// Zero-copy buffer for efficient data streaming
pub struct ZeroCopyBuffer {
    inner: BytesMut,
    capacity: usize,
}

impl ZeroCopyBuffer {
    /// Create a new buffer with specified capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: BytesMut::with_capacity(capacity),
            capacity,
        }
    }
    
    /// Append data to the buffer
    pub fn append(&mut self, data: &[u8]) {
        self.inner.extend_from_slice(data);
    }
    
    /// Take a slice of the buffer without copying
    pub fn slice(&self, start: usize, end: usize) -> Bytes {
        self.inner.clone().freeze().slice(start..end)
    }
    
    /// Freeze the buffer into immutable bytes
    pub fn freeze(self) -> Bytes {
        self.inner.freeze()
    }
    
    /// Clear the buffer
    pub fn clear(&mut self) {
        self.inner.clear();
    }
    
    /// Get the current length
    pub fn len(&self) -> usize {
        self.inner.len()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

/// Shared immutable data structure using Arc
#[derive(Clone, Debug)]
pub struct SharedData<T> {
    inner: Arc<T>,
}

impl<T> SharedData<T> {
    /// Create new shared data
    pub fn new(data: T) -> Self {
        Self {
            inner: Arc::new(data),
        }
    }
    
    /// Get a reference to the inner data
    pub fn get(&self) -> &T {
        &self.inner
    }
    
    /// Try to unwrap the Arc if there's only one reference
    pub fn try_unwrap(self) -> Result<T, Self> {
        Arc::try_unwrap(self.inner).map_err(|arc| Self { inner: arc })
    }
    
    /// Get the reference count
    pub fn strong_count(&self) -> usize {
        Arc::strong_count(&self.inner)
    }
}

impl<T> std::ops::Deref for SharedData<T> {
    type Target = T;
    
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

/// Zero-copy serialization wrapper
pub struct ZeroCopySerialize<'a, T: Serialize> {
    data: &'a T,
    buffer: Option<Vec<u8>>,
}

impl<'a, T: Serialize> ZeroCopySerialize<'a, T> {
    /// Create a new zero-copy serializer
    pub fn new(data: &'a T) -> Self {
        Self {
            data,
            buffer: None,
        }
    }
    
    /// Serialize to bytes (caches the result)
    pub fn to_bytes(&mut self) -> Result<&[u8], serde_json::Error> {
        if self.buffer.is_none() {
            self.buffer = Some(serde_json::to_vec(self.data)?);
        }
        Ok(self.buffer.as_ref().expect("Buffer should be set after serialization"))
    }
    
    /// Serialize to a writer without intermediate allocation
    pub fn to_writer<W: std::io::Write>(&self, writer: W) -> Result<(), serde_json::Error> {
        serde_json::to_writer(writer, self.data)
    }
}

/// Optimized state data using zero-copy techniques
#[derive(Clone)]
pub struct ZeroCopyStateData {
    /// Interned keys for deduplication
    interner: StringInterner,
    
    /// Shared data values
    data: Arc<DashMap<Arc<str>, SharedData<Value>>>,
}

impl ZeroCopyStateData {
    /// Create new zero-copy state data
    pub fn new() -> Self {
        Self {
            interner: StringInterner::new(),
            data: Arc::new(DashMap::new()),
        }
    }
    
    /// Insert a value with key interning
    pub fn insert(&self, key: &str, value: Value) {
        let interned_key = self.interner.intern(key);
        self.data.insert(interned_key, SharedData::new(value));
    }
    
    /// Get a value by key
    pub fn get(&self, key: &str) -> Option<SharedData<Value>> {
        self.data.get(key).map(|entry| entry.value().clone())
    }
    
    /// Remove a value
    pub fn remove(&self, key: &str) -> Option<SharedData<Value>> {
        self.data.remove(key).map(|(_, v)| v)
    }
    
    /// Get the number of entries
    pub fn len(&self) -> usize {
        self.data.len()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    
    /// Clear all data
    pub fn clear(&self) {
        self.data.clear();
        self.interner.clear();
    }
}

impl Default for ZeroCopyStateData {
    fn default() -> Self {
        Self::new()
    }
}

/// Memory-efficient state diff for version control
#[derive(Clone, Debug)]
pub struct StateDiff {
    /// Added or modified keys
    pub changed: DashMap<Arc<str>, SharedData<Value>>,
    
    /// Removed keys
    pub removed: DashSet<Arc<str>>,
    
    /// Timestamp of this diff
    pub timestamp: u64,
}

impl StateDiff {
    /// Create a new empty diff
    pub fn new() -> Self {
        Self {
            changed: DashMap::new(),
            removed: DashSet::new(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }
    
    /// Add a changed value
    pub fn add_change(&self, key: Arc<str>, value: SharedData<Value>) {
        self.changed.insert(key, value);
    }
    
    /// Add a removed key
    pub fn add_removal(&self, key: Arc<str>) {
        self.removed.insert(key);
    }
    
    /// Apply this diff to state data
    pub fn apply_to(&self, state: &ZeroCopyStateData) {
        // Apply changes
        for entry in self.changed.iter() {
            state.data.insert(entry.key().clone(), entry.value().clone());
        }
        
        // Apply removals
        for key in self.removed.iter() {
            state.data.remove(key.as_ref());
        }
    }
    
    /// Check if this diff is empty
    pub fn is_empty(&self) -> bool {
        self.changed.is_empty() && self.removed.is_empty()
    }
}

/// Copy-on-write state data for efficient branching
#[derive(Clone)]
pub struct CowStateData {
    /// Base state data
    base: Arc<ZeroCopyStateData>,
    
    /// Local modifications (copy-on-write)
    modifications: Option<Arc<DashMap<Arc<str>, SharedData<Value>>>>,
    
    /// Local removals
    removals: Option<Arc<DashSet<Arc<str>>>>,
}

impl CowStateData {
    /// Create new cow state from base
    pub fn new(base: ZeroCopyStateData) -> Self {
        Self {
            base: Arc::new(base),
            modifications: None,
            removals: None,
        }
    }
    
    /// Insert a value (triggers copy-on-write)
    pub fn insert(&mut self, key: &str, value: Value) {
        let interned_key = self.base.interner.intern(key);
        
        if self.modifications.is_none() {
            self.modifications = Some(Arc::new(DashMap::new()));
        }
        
        self.modifications
            .as_ref()
            .unwrap()
            .insert(interned_key, SharedData::new(value));
    }
    
    /// Get a value (checks modifications first, then base)
    pub fn get(&self, key: &str) -> Option<SharedData<Value>> {
        // Check if removed
        if let Some(ref removals) = self.removals {
            if removals.contains(key) {
                return None;
            }
        }
        
        // Check modifications first
        if let Some(ref mods) = self.modifications {
            if let Some(value) = mods.get(key) {
                return Some(value.clone());
            }
        }
        
        // Fall back to base
        self.base.get(key)
    }
    
    /// Remove a value (triggers copy-on-write for removals)
    pub fn remove(&mut self, key: &str) -> Option<SharedData<Value>> {
        let interned_key = self.base.interner.intern(key);
        
        // Add to removals set
        if self.removals.is_none() {
            self.removals = Some(Arc::new(DashSet::new()));
        }
        self.removals.as_ref().unwrap().insert(interned_key.clone());
        
        // Remove from modifications if present
        if let Some(ref mods) = self.modifications {
            mods.remove(&interned_key).map(|(_, v)| v)
        } else {
            self.base.get(key)
        }
    }
    
    /// Materialize into a full state (merge base + modifications)
    pub fn materialize(self) -> ZeroCopyStateData {
        let result = (*self.base).clone();
        
        // Apply modifications
        if let Some(mods) = self.modifications {
            for entry in mods.iter() {
                result.data.insert(entry.key().clone(), entry.value().clone());
            }
        }
        
        // Apply removals
        if let Some(removals) = self.removals {
            for key in removals.iter() {
                result.data.remove(key.as_ref());
            }
        }
        
        result
    }
    
    /// Generate a diff from this COW state
    pub fn create_diff(&self) -> StateDiff {
        let diff = StateDiff::new();
        
        if let Some(ref mods) = self.modifications {
            for entry in mods.iter() {
                diff.add_change(entry.key().clone(), entry.value().clone());
            }
        }
        
        if let Some(ref removals) = self.removals {
            for key in removals.iter() {
                diff.add_removal(key.clone());
            }
        }
        
        diff
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_string_interner() {
        let interner = StringInterner::new();
        
        let s1 = interner.intern("hello");
        let s2 = interner.intern("hello");
        let s3 = interner.intern("world");
        
        // Same strings should share memory
        assert!(Arc::ptr_eq(&s1, &s2));
        assert!(!Arc::ptr_eq(&s1, &s3));
        
        assert_eq!(interner.len(), 2);
    }
    
    #[test]
    fn test_zero_copy_buffer() {
        let mut buffer = ZeroCopyBuffer::with_capacity(1024);
        
        buffer.append(b"hello ");
        buffer.append(b"world");
        
        let slice = buffer.slice(0, 5);
        assert_eq!(&slice[..], b"hello");
        
        let frozen = buffer.freeze();
        assert_eq!(&frozen[..], b"hello world");
    }
    
    #[test]
    fn test_shared_data() {
        let data = SharedData::new(vec![1, 2, 3]);
        let clone = data.clone();
        
        assert_eq!(data.strong_count(), 2);
        assert_eq!(clone.get(), &vec![1, 2, 3]);
        
        drop(clone);
        assert_eq!(data.strong_count(), 1);
        
        let unwrapped = data.try_unwrap().unwrap();
        assert_eq!(unwrapped, vec![1, 2, 3]);
    }
    
    #[test]
    fn test_zero_copy_state_data() {
        let state = ZeroCopyStateData::new();
        
        state.insert("key1", serde_json::json!("value1"));
        state.insert("key2", serde_json::json!("value2"));
        
        let value = state.get("key1").unwrap();
        assert_eq!(*value.get(), serde_json::json!("value1"));
        
        assert_eq!(state.len(), 2);
        
        state.remove("key1");
        assert_eq!(state.len(), 1);
    }
    
    #[test]
    fn test_state_diff() {
        let base_state = ZeroCopyStateData::new();
        base_state.insert("original", serde_json::json!("value"));
        
        let diff = StateDiff::new();
        let interner = StringInterner::new();
        
        // Add changes and removals
        diff.add_change(
            interner.intern("new_key"), 
            SharedData::new(serde_json::json!("new_value"))
        );
        diff.add_removal(interner.intern("original"));
        
        // Apply diff
        diff.apply_to(&base_state);
        
        assert!(base_state.get("original").is_none());
        assert_eq!(
            base_state.get("new_key").unwrap().get(),
            &serde_json::json!("new_value")
        );
    }
    
    #[test]
    fn test_cow_state_data() {
        let mut base_state = ZeroCopyStateData::new();
        base_state.insert("base_key", serde_json::json!("base_value"));
        
        let mut cow_state = CowStateData::new(base_state);
        
        // Should read from base
        assert_eq!(
            cow_state.get("base_key").unwrap().get(),
            &serde_json::json!("base_value")
        );
        
        // Modify (triggers COW)
        cow_state.insert("new_key", serde_json::json!("new_value"));
        cow_state.remove("base_key");
        
        // Should reflect modifications
        assert!(cow_state.get("base_key").is_none());
        assert_eq!(
            cow_state.get("new_key").unwrap().get(),
            &serde_json::json!("new_value")
        );
        
        // Create diff
        let diff = cow_state.create_diff();
        assert!(!diff.is_empty());
        assert!(diff.changed.contains_key("new_key"));
        assert!(diff.removed.contains("base_key"));
    }
}