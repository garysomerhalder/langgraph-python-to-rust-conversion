# PostgreSQL Checkpointer Documentation

## Overview

The PostgreSQL checkpointer provides production-grade persistence for LangGraph workflows, enabling durable state management, multi-instance coordination, and time-travel capabilities.

## Features

### Core Features
- **Durable Persistence**: State survives application restarts and crashes
- **Multi-Instance Support**: Multiple LangGraph instances can coordinate through shared PostgreSQL
- **Time-Travel**: Navigate through checkpoint history
- **Efficient Storage**: JSONB for efficient querying and storage
- **Transactional Safety**: ACID compliance for checkpoint operations

### Production Features
- **Connection Pooling**: Configurable connection limits and lifecycle
- **Retry Logic**: Exponential backoff with jitter for resilience
- **Health Checks**: Monitor database connectivity
- **Metrics**: Prometheus-compatible metrics for monitoring
- **Batch Operations**: Efficient bulk checkpoint operations
- **Compression**: Optional state compression for large checkpoints
- **Auto-Cleanup**: Configurable retention policies

## Installation

### Prerequisites

1. PostgreSQL 12 or higher
2. Rust with `sqlx` dependency

Add to `Cargo.toml`:
```toml
[dependencies]
sqlx = { version = "0.8", features = ["runtime-tokio", "postgres", "json", "chrono", "uuid"] }
```

### Database Setup

1. Create database:
```sql
CREATE DATABASE langgraph;
```

2. Run migrations:
```bash
psql -d langgraph -f src/checkpoint/migrations.sql
```

## Configuration

### Basic Configuration

```rust
use langgraph::checkpoint::{PostgresCheckpointer, PostgresConfig};

let config = PostgresConfig {
    database_url: "postgresql://user:pass@localhost:5432/langgraph".to_string(),
    max_connections: 10,
    min_connections: 2,
    max_lifetime_secs: 3600,
    idle_timeout_secs: 600,
    table_prefix: "langgraph_".to_string(),
    auto_cleanup: true,
    cleanup_interval_secs: 3600,
    retention_days: 30,
};

let checkpointer = PostgresCheckpointer::new(config).await?;
```

### Enhanced Configuration with Retry

```rust
use langgraph::checkpoint::{EnhancedPostgresCheckpointer, RetryConfig};

let retry_config = RetryConfig {
    max_retries: 3,
    initial_delay_ms: 100,
    max_delay_ms: 5000,
    exponential_base: 2.0,
    jitter: true,
};

let checkpointer = EnhancedPostgresCheckpointer::new(config, retry_config).await?;
```

### Environment Variables

```bash
# Required
DATABASE_URL=postgresql://user:pass@localhost:5432/langgraph

# Optional
CHECKPOINT_MAX_CONNECTIONS=10
CHECKPOINT_MIN_CONNECTIONS=2
CHECKPOINT_RETENTION_DAYS=30
CHECKPOINT_AUTO_CLEANUP=true
```

## Usage

### Basic Operations

```rust
// Save checkpoint
let checkpoint_id = checkpointer.save_checkpoint(thread_id, &state).await?;

// Load latest checkpoint
let state = checkpointer.load_checkpoint(thread_id, None).await?;

// Load specific checkpoint
let state = checkpointer.load_checkpoint(thread_id, Some(checkpoint_id)).await?;

// List checkpoints
let checkpoints = checkpointer.list_checkpoints(thread_id, Some(10)).await?;

// Delete checkpoint
checkpointer.delete_checkpoint(thread_id, checkpoint_id).await?;
```

### Advanced Operations

```rust
// Batch save multiple checkpoints
let checkpoints = vec![
    ("thread1".to_string(), state1),
    ("thread2".to_string(), state2),
];
let ids = checkpointer.batch_save_checkpoints(checkpoints).await?;

// Health check
if !checkpointer.health_check().await? {
    // Handle unhealthy connection
}

// Get pool statistics
let stats = checkpointer.pool_stats();
println!("Active connections: {}", stats.connections);

// Manual cleanup
checkpointer.cleanup_old_checkpoints().await?;

// Optimize database
checkpointer.optimize_database().await?;
```

### With Workflow Integration

```rust
use langgraph::graph::GraphBuilder;

let graph = GraphBuilder::new("workflow")
    .with_checkpointer(Arc::new(checkpointer))
    .add_node("process", NodeType::Function(process_fn))
    .build()?;

// Execution automatically checkpoints state
let result = graph.execute(input).await?;

// Resume from checkpoint
let result = graph.resume_from_checkpoint(checkpoint_id).await?;
```

## Performance Optimization

### Indexes

The migration creates optimal indexes for common queries:
- `thread_id` - Primary lookup index
- `created_at` - Time-based queries
- `parent_id` - Lineage tracking
- BRIN index for time-series data

### Connection Pooling

Configure based on workload:
- **Read-heavy**: Higher `max_connections`
- **Write-heavy**: Lower connections with batching
- **Mixed**: Balanced configuration

### Batch Operations

Use batch operations for bulk inserts:
```rust
// More efficient than individual saves
let ids = checkpointer.batch_save_checkpoints(checkpoints).await?;
```

### Compression

For large states, enable compression:
```sql
UPDATE langgraph_checkpoints
SET compressed = TRUE
WHERE calculate_state_size(state) > 10000;
```

## Monitoring

### Prometheus Metrics

Available metrics:
- `postgres_checkpoint_saves_total` - Total save operations
- `postgres_checkpoint_loads_total` - Total load operations
- `postgres_checkpoint_operation_duration_seconds` - Operation latency

### Health Checks

```rust
// Periodic health check
tokio::spawn(async move {
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
        if !checkpointer.health_check().await.unwrap_or(false) {
            // Alert or reconnect
        }
    }
});
```

### Database Monitoring

Monitor PostgreSQL metrics:
- Connection count
- Query performance
- Table size growth
- Index usage

## Troubleshooting

### Common Issues

#### Connection Refused
```
Error: Connection refused (os error 111)
```
**Solution**: Verify PostgreSQL is running and accepting connections

#### Too Many Connections
```
Error: FATAL: remaining connection slots are reserved
```
**Solution**: Increase `max_connections` in `postgresql.conf` or reduce pool size

#### Slow Queries
**Solution**: Run `ANALYZE` and check index usage:
```sql
EXPLAIN ANALYZE SELECT * FROM langgraph_checkpoints WHERE thread_id = 'xxx';
```

### Maintenance

#### Regular Cleanup
```sql
-- Manual cleanup
CALL cleanup_old_checkpoints(30);

-- Check table size
SELECT pg_size_pretty(pg_total_relation_size('langgraph_checkpoints'));

-- Vacuum and analyze
VACUUM ANALYZE langgraph_checkpoints;
```

#### Backup Strategy
```bash
# Backup checkpoints
pg_dump -t langgraph_checkpoints -t langgraph_checkpoint_metadata langgraph > checkpoints_backup.sql

# Restore
psql langgraph < checkpoints_backup.sql
```

## Migration Guide

### From In-Memory to PostgreSQL

```rust
// Export from memory
let memory_checkpointer = MemoryCheckpointer::new();
let checkpoints = memory_checkpointer.export_all().await?;

// Import to PostgreSQL
let pg_checkpointer = PostgresCheckpointer::new(config).await?;
for checkpoint in checkpoints {
    pg_checkpointer.import(checkpoint).await?;
}
```

### Version Upgrades

Check migration status:
```sql
SELECT * FROM langgraph_migrations ORDER BY version;
```

Apply new migrations:
```bash
psql -d langgraph -f migrations/new_migration.sql
```

## Best Practices

1. **Use Connection Pooling**: Always configure appropriate pool sizes
2. **Enable Retry Logic**: Use `EnhancedPostgresCheckpointer` for production
3. **Set Retention Policies**: Configure `retention_days` to manage growth
4. **Monitor Metrics**: Track operation latency and error rates
5. **Regular Maintenance**: Schedule vacuum and analyze operations
6. **Backup Regularly**: Implement automated backup strategy
7. **Test Failover**: Verify behavior during connection loss
8. **Use Transactions**: Batch related operations in transactions
9. **Optimize Queries**: Use prepared statements and appropriate indexes
10. **Security**: Use SSL connections and proper authentication

## Security Considerations

### Connection Security
```rust
let config = PostgresConfig {
    database_url: "postgresql://user:pass@localhost:5432/langgraph?sslmode=require".to_string(),
    // ...
};
```

### Access Control
```sql
-- Create restricted user
CREATE USER langgraph_app WITH PASSWORD 'secure_password';
GRANT CONNECT ON DATABASE langgraph TO langgraph_app;
GRANT USAGE ON SCHEMA public TO langgraph_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO langgraph_app;
```

### Data Encryption
- Use PostgreSQL TDE for at-rest encryption
- Use SSL/TLS for in-transit encryption
- Consider application-level encryption for sensitive state

## License

See LICENSE file in the repository root.