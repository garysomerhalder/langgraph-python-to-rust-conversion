# PERSIST-001: PostgreSQL Backend

## 📋 Task Overview
**ID:** PERSIST-001  
**Title:** PostgreSQL backend for persistence  
**Status:** 🔴 TODO  
**Priority:** P0 (Critical)  
**Category:** Enhanced Persistence  
**Estimated Days:** 3  
**Phase:** Phase 2 - Production Features  

## 🎯 Objective
Implement PostgreSQL backend for state persistence, providing production-grade durability and scalability for LangGraph workflows.

## 📝 Description
PostgreSQL backend enables:
- Durable state persistence across restarts
- Multi-instance coordination
- State history with time-travel
- Efficient querying of execution history
- Production-grade reliability

## ✅ Acceptance Criteria
- [ ] Full Checkpointer trait implementation for PostgreSQL
- [ ] Connection pooling with configurable limits
- [ ] Automatic schema creation and migration
- [ ] Efficient state serialization (JSON/JSONB)
- [ ] Transaction support for consistency
- [ ] Batch operations for performance
- [ ] Query interface for state history
- [ ] Automatic cleanup of old checkpoints
- [ ] Comprehensive integration tests
- [ ] Docker-based test environment

## 🔧 Technical Requirements

### Core Components to Implement
```rust
// src/checkpoint/postgres.rs
use sqlx::{PgPool, postgres::PgPoolOptions};
use async_trait::async_trait;

pub struct PostgresCheckpointer {
    pool: PgPool,
    config: PostgresConfig,
}

#[derive(Debug, Clone)]
pub struct PostgresConfig {
    pub database_url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub connect_timeout: Duration,
    pub idle_timeout: Option<Duration>,
    pub max_lifetime: Option<Duration>,
    pub schema_name: String,
}

impl PostgresCheckpointer {
    pub async fn new(config: PostgresConfig) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(config.max_connections)
            .min_connections(config.min_connections)
            .connect_timeout(config.connect_timeout)
            .connect(&config.database_url)
            .await?;
        
        let checkpointer = Self { pool, config };
        checkpointer.ensure_schema().await?;
        Ok(checkpointer)
    }
    
    async fn ensure_schema(&self) -> Result<()> {
        sqlx::query(&format!(
            "CREATE SCHEMA IF NOT EXISTS {}",
            self.config.schema_name
        ))
        .execute(&self.pool)
        .await?;
        
        sqlx::query(&format!(
            "CREATE TABLE IF NOT EXISTS {}.checkpoints (
                thread_id TEXT NOT NULL,
                checkpoint_id TEXT NOT NULL,
                parent_id TEXT,
                state JSONB NOT NULL,
                metadata JSONB,
                created_at TIMESTAMPTZ DEFAULT NOW(),
                updated_at TIMESTAMPTZ DEFAULT NOW(),
                PRIMARY KEY (thread_id, checkpoint_id)
            )",
            self.config.schema_name
        ))
        .execute(&self.pool)
        .await?;
        
        // Create indexes
        sqlx::query(&format!(
            "CREATE INDEX IF NOT EXISTS idx_checkpoint_created 
             ON {}.checkpoints (created_at DESC)",
            self.config.schema_name
        ))
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
}

#[async_trait]
impl Checkpointer for PostgresCheckpointer {
    async fn save(&self, thread_id: &str, checkpoint: Checkpoint) -> Result<()> {
        let state_json = serde_json::to_value(&checkpoint.state)?;
        let metadata_json = serde_json::to_value(&checkpoint.metadata)?;
        
        sqlx::query(&format!(
            "INSERT INTO {}.checkpoints 
             (thread_id, checkpoint_id, parent_id, state, metadata)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (thread_id, checkpoint_id)
             DO UPDATE SET 
                state = EXCLUDED.state,
                metadata = EXCLUDED.metadata,
                updated_at = NOW()",
            self.config.schema_name
        ))
        .bind(thread_id)
        .bind(&checkpoint.id)
        .bind(&checkpoint.parent_id)
        .bind(state_json)
        .bind(metadata_json)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    async fn load(&self, thread_id: &str, checkpoint_id: &str) -> Result<Option<Checkpoint>> {
        let row = sqlx::query_as::<_, CheckpointRow>(&format!(
            "SELECT * FROM {}.checkpoints 
             WHERE thread_id = $1 AND checkpoint_id = $2",
            self.config.schema_name
        ))
        .bind(thread_id)
        .bind(checkpoint_id)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(row.map(|r| r.into()))
    }
    
    async fn list(&self, thread_id: &str, limit: Option<usize>) -> Result<Vec<Checkpoint>> {
        let limit = limit.unwrap_or(100);
        
        let rows = sqlx::query_as::<_, CheckpointRow>(&format!(
            "SELECT * FROM {}.checkpoints 
             WHERE thread_id = $1 
             ORDER BY created_at DESC 
             LIMIT $2",
            self.config.schema_name
        ))
        .bind(thread_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        
        Ok(rows.into_iter().map(Into::into).collect())
    }
    
    async fn delete(&self, thread_id: &str, checkpoint_id: &str) -> Result<()> {
        sqlx::query(&format!(
            "DELETE FROM {}.checkpoints 
             WHERE thread_id = $1 AND checkpoint_id = $2",
            self.config.schema_name
        ))
        .bind(thread_id)
        .bind(checkpoint_id)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
}
```

### Python API Compatibility
```python
# Python LangGraph
from langgraph.checkpoint.postgres import PostgresSaver

checkpointer = PostgresSaver.from_conn_string(
    "postgresql://user:pass@localhost/dbname"
)
graph = workflow.compile(checkpointer=checkpointer)
```

```rust
// Rust equivalent
use langgraph::checkpoint::PostgresCheckpointer;

let checkpointer = PostgresCheckpointer::new(
    PostgresConfig::from_url("postgresql://user:pass@localhost/dbname")
).await?;
let graph = workflow.compile_with_checkpointer(checkpointer)?;
```

## 🚦 Implementation Plan (Traffic-Light)

### 🔴 RED Phase - Tests First (Day 1)
```rust
#[tokio::test]
async fn test_postgres_checkpointer() {
    // Use testcontainers for PostgreSQL
    let postgres = PostgresContainer::new();
    let url = postgres.connection_string();
    
    let checkpointer = PostgresCheckpointer::new(
        PostgresConfig::from_url(&url)
    ).await?;
    
    let checkpoint = create_test_checkpoint();
    checkpointer.save("thread1", checkpoint.clone()).await?;
    
    let loaded = checkpointer.load("thread1", &checkpoint.id).await?;
    assert_eq!(loaded, Some(checkpoint));
    
    let history = checkpointer.list("thread1", Some(10)).await?;
    assert_eq!(history.len(), 1);
}
```

### 🟡 YELLOW Phase - Minimal Implementation (Day 2)
- Basic CRUD operations
- Schema creation
- Simple serialization
- Connection pooling

### 🟢 GREEN Phase - Production Ready (Day 3)
- Transaction support
- Batch operations
- Query optimization
- Connection retry logic
- Monitoring hooks
- Migration system
- Cleanup jobs
- Performance tuning

## 📊 Success Metrics
- < 10ms checkpoint save time
- < 5ms checkpoint load time
- Support for 10K+ checkpoints per thread
- 99.99% reliability
- Automatic failover support

## 🔗 Dependencies
- sqlx for PostgreSQL access
- tokio-postgres as alternative
- testcontainers for testing
- serde_json for serialization

## ⚠️ Risks & Mitigations
| Risk | Mitigation |
|------|------------|
| Connection pool exhaustion | Configurable limits and monitoring |
| Large state serialization | Compression and chunking |
| Schema evolution | Migration system |
| Network failures | Retry logic with backoff |

## 📚 References
- [Python PostgresSaver](https://github.com/langchain-ai/langgraph/blob/main/langgraph/checkpoint/postgres.py)
- [SQLx Documentation](https://github.com/launchbadge/sqlx)
- [PostgreSQL JSON Functions](https://www.postgresql.org/docs/current/functions-json.html)

## 🎯 Definition of Done
- [ ] All CRUD operations working
- [ ] Integration tests with real PostgreSQL
- [ ] Connection pooling verified
- [ ] Performance benchmarked
- [ ] Docker compose example
- [ ] Migration documentation
- [ ] Monitoring metrics exposed