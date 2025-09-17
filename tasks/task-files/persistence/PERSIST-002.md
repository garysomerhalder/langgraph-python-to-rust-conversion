# PERSIST-002: Implement Redis Checkpointer

## 📋 Task Details
- **ID**: PERSIST-002
- **Category**: Persistence
- **Priority**: P0 (Critical)
- **Effort**: 2 days
- **Status**: 🔴 TODO

## 📝 Description
Implement a high-performance Redis checkpointer matching Python LangGraph's `RedisSaver` functionality. This provides fast, distributed state storage with automatic expiration support.

## ✅ Acceptance Criteria
- [ ] Create `RedisCheckpointer` struct with connection pooling
- [ ] Implement async operations using `redis-rs` with Tokio
- [ ] Design key structure for checkpoint storage
- [ ] Support thread-based isolation
- [ ] Implement TTL-based expiration
- [ ] Add Redis Cluster support
- [ ] Enable checkpoint compression
- [ ] Support atomic operations with Lua scripts
- [ ] Add connection retry with exponential backoff
- [ ] Integration tests with real Redis instance

## 🔧 Technical Approach
```rust
use redis::{aio::ConnectionManager, Client};

pub struct RedisCheckpointer {
    conn_manager: ConnectionManager,
    key_prefix: String,
    ttl_seconds: Option<u64>,
    compression: bool,
}

impl RedisCheckpointer {
    pub async fn new(redis_url: &str) -> Result<Self>;
    pub async fn with_ttl(mut self, ttl_seconds: u64) -> Self;
}

#[async_trait]
impl Checkpointer for RedisCheckpointer {
    async fn save(&self, thread_id: &str, checkpoint: Checkpoint) -> Result<()>;
    async fn load(&self, thread_id: &str) -> Result<Option<Checkpoint>>;
    async fn list(&self, thread_id: &str) -> Result<Vec<CheckpointMetadata>>;
}

// Key structure:
// checkpoint:{thread_id}:current -> latest checkpoint
// checkpoint:{thread_id}:{checkpoint_id} -> specific checkpoint
// checkpoint:{thread_id}:metadata -> thread metadata
// checkpoint:{thread_id}:history -> checkpoint ID list
```

## 📚 Resources
- Python LangGraph RedisSaver implementation
- `redis-rs` async documentation
- Redis best practices for large values

## 🧪 Test Requirements
- Use testcontainers for real Redis testing
- Concurrent access tests
- TTL expiration tests
- Large state compression tests
- Redis Cluster tests
- Connection failover tests
- Performance benchmarks

## Dependencies
- `redis` = { version = "0.27", features = ["tokio-comp", "connection-manager"] }
- `testcontainers` for testing
- Existing checkpoint trait