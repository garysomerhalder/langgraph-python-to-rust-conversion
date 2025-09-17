# PERSIST-001: Implement PostgreSQL Checkpointer

## 📋 Task Details
- **ID**: PERSIST-001
- **Category**: Persistence
- **Priority**: P0 (Critical)
- **Effort**: 2 days
- **Status**: 🔴 TODO

## 📝 Description
Implement a production-ready PostgreSQL checkpointer matching Python LangGraph's `PostgresSaver` functionality. This enables persistent state storage across graph executions with full ACID guarantees.

## ✅ Acceptance Criteria
- [ ] Create `PostgresCheckpointer` struct with connection pooling
- [ ] Implement async operations using `sqlx` or `tokio-postgres`
- [ ] Design schema for checkpoint storage (threads, checkpoints, metadata)
- [ ] Support thread-based isolation for conversations
- [ ] Implement checkpoint versioning and rollback
- [ ] Add connection retry and failover logic
- [ ] Support checkpoint compression for large states
- [ ] Enable checkpoint expiration/cleanup
- [ ] Full transaction support with proper isolation levels
- [ ] Integration tests with real PostgreSQL instance

## 🔧 Technical Approach
```rust
use sqlx::{PgPool, postgres::PgPoolOptions};

pub struct PostgresCheckpointer {
    pool: PgPool,
    schema: String,
    compression: bool,
}

impl PostgresCheckpointer {
    pub async fn new(database_url: &str) -> Result<Self>;
    pub async fn init_schema(&self) -> Result<()>;
}

#[async_trait]
impl Checkpointer for PostgresCheckpointer {
    async fn save(&self, thread_id: &str, checkpoint: Checkpoint) -> Result<()>;
    async fn load(&self, thread_id: &str) -> Result<Option<Checkpoint>>;
    async fn list(&self, thread_id: &str) -> Result<Vec<CheckpointMetadata>>;
}

-- Schema
CREATE TABLE IF NOT EXISTS checkpoints (
    id UUID PRIMARY KEY,
    thread_id TEXT NOT NULL,
    parent_id UUID REFERENCES checkpoints(id),
    checkpoint BYTEA NOT NULL,
    metadata JSONB,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    INDEX idx_thread_id (thread_id),
    INDEX idx_created_at (created_at)
);
```

## 📚 Resources
- Python LangGraph PostgresSaver implementation
- `sqlx` documentation for async PostgreSQL
- PostgreSQL best practices for binary data

## 🧪 Test Requirements
- Use testcontainers for real PostgreSQL testing
- Concurrent access tests
- Transaction isolation tests
- Large state compression tests
- Failover and retry tests
- Performance benchmarks

## Dependencies
- `sqlx` = { version = "0.8", features = ["postgres", "runtime-tokio", "json"] }
- `testcontainers` for testing
- Existing checkpoint trait