-- PostgreSQL migrations for LangGraph checkpointer
-- Version: 1.0.0

-- Migration 001: Initial schema
CREATE TABLE IF NOT EXISTS langgraph_checkpoints (
    id VARCHAR(36) PRIMARY KEY,
    thread_id VARCHAR(255) NOT NULL,
    parent_id VARCHAR(36),
    state JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for performance
CREATE INDEX IF NOT EXISTS idx_checkpoints_thread_id ON langgraph_checkpoints (thread_id);
CREATE INDEX IF NOT EXISTS idx_checkpoints_created_at ON langgraph_checkpoints (created_at DESC);
CREATE INDEX IF NOT EXISTS idx_checkpoints_parent_id ON langgraph_checkpoints (parent_id) WHERE parent_id IS NOT NULL;

-- Metadata table for checkpoint metadata
CREATE TABLE IF NOT EXISTS langgraph_checkpoint_metadata (
    checkpoint_id VARCHAR(36) PRIMARY KEY REFERENCES langgraph_checkpoints(id) ON DELETE CASCADE,
    metadata JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Migration 002: Add version tracking
CREATE TABLE IF NOT EXISTS langgraph_migrations (
    version INTEGER PRIMARY KEY,
    description TEXT NOT NULL,
    applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Insert initial migration record
INSERT INTO langgraph_migrations (version, description)
VALUES (1, 'Initial schema with checkpoints and metadata tables')
ON CONFLICT (version) DO NOTHING;

-- Migration 003: Add thread metadata table
CREATE TABLE IF NOT EXISTS langgraph_thread_metadata (
    thread_id VARCHAR(255) PRIMARY KEY,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    metadata JSONB,
    is_active BOOLEAN DEFAULT TRUE,
    checkpoint_count INTEGER DEFAULT 0
);

INSERT INTO langgraph_migrations (version, description)
VALUES (2, 'Add thread metadata table')
ON CONFLICT (version) DO NOTHING;

-- Migration 004: Add performance improvements
-- Partial index for active threads
CREATE INDEX IF NOT EXISTS idx_thread_metadata_active
ON langgraph_thread_metadata (thread_id)
WHERE is_active = TRUE;

-- BRIN index for time-series queries
CREATE INDEX IF NOT EXISTS idx_checkpoints_created_at_brin
ON langgraph_checkpoints USING BRIN (created_at);

INSERT INTO langgraph_migrations (version, description)
VALUES (3, 'Add performance indexes')
ON CONFLICT (version) DO NOTHING;

-- Migration 005: Add checkpoint compression
ALTER TABLE langgraph_checkpoints
ADD COLUMN IF NOT EXISTS compressed BOOLEAN DEFAULT FALSE;

ALTER TABLE langgraph_checkpoints
ADD COLUMN IF NOT EXISTS compression_ratio FLOAT;

-- Function to calculate state size
CREATE OR REPLACE FUNCTION calculate_state_size(state JSONB)
RETURNS INTEGER AS $$
BEGIN
    RETURN LENGTH(state::TEXT);
END;
$$ LANGUAGE plpgsql;

INSERT INTO langgraph_migrations (version, description)
VALUES (4, 'Add checkpoint compression support')
ON CONFLICT (version) DO NOTHING;

-- Migration 006: Add checkpoint statistics view
CREATE OR REPLACE VIEW checkpoint_statistics AS
SELECT
    thread_id,
    COUNT(*) as checkpoint_count,
    MIN(created_at) as first_checkpoint,
    MAX(created_at) as last_checkpoint,
    AVG(calculate_state_size(state)) as avg_state_size,
    SUM(calculate_state_size(state)) as total_state_size
FROM langgraph_checkpoints
GROUP BY thread_id;

INSERT INTO langgraph_migrations (version, description)
VALUES (5, 'Add checkpoint statistics view')
ON CONFLICT (version) DO NOTHING;

-- Migration 007: Add cleanup procedures
CREATE OR REPLACE PROCEDURE cleanup_old_checkpoints(retention_days INTEGER)
LANGUAGE plpgsql AS $$
BEGIN
    DELETE FROM langgraph_checkpoints
    WHERE created_at < NOW() - INTERVAL '1 day' * retention_days;

    -- Update thread metadata
    UPDATE langgraph_thread_metadata tm
    SET checkpoint_count = (
        SELECT COUNT(*)
        FROM langgraph_checkpoints c
        WHERE c.thread_id = tm.thread_id
    ),
    updated_at = NOW();

    -- Mark inactive threads
    UPDATE langgraph_thread_metadata
    SET is_active = FALSE
    WHERE checkpoint_count = 0;
END;
$$;

INSERT INTO langgraph_migrations (version, description)
VALUES (6, 'Add cleanup procedures')
ON CONFLICT (version) DO NOTHING;

-- Migration 008: Add checkpoint lineage tracking
CREATE TABLE IF NOT EXISTS langgraph_checkpoint_lineage (
    id SERIAL PRIMARY KEY,
    checkpoint_id VARCHAR(36) REFERENCES langgraph_checkpoints(id) ON DELETE CASCADE,
    ancestor_id VARCHAR(36) REFERENCES langgraph_checkpoints(id) ON DELETE CASCADE,
    generation INTEGER NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(checkpoint_id, ancestor_id)
);

-- Recursive CTE to build lineage
CREATE OR REPLACE FUNCTION build_checkpoint_lineage(checkpoint_id VARCHAR(36))
RETURNS TABLE(ancestor_id VARCHAR(36), generation INTEGER) AS $$
WITH RECURSIVE lineage AS (
    -- Base case: the checkpoint itself
    SELECT
        id as ancestor_id,
        0 as generation
    FROM langgraph_checkpoints
    WHERE id = checkpoint_id

    UNION ALL

    -- Recursive case: follow parent links
    SELECT
        c.parent_id as ancestor_id,
        l.generation + 1
    FROM lineage l
    JOIN langgraph_checkpoints c ON l.ancestor_id = c.id
    WHERE c.parent_id IS NOT NULL
)
SELECT ancestor_id, generation FROM lineage;
$$ LANGUAGE sql;

INSERT INTO langgraph_migrations (version, description)
VALUES (7, 'Add checkpoint lineage tracking')
ON CONFLICT (version) DO NOTHING;

-- Final migration: Optimization hints
COMMENT ON TABLE langgraph_checkpoints IS 'Main checkpoint storage for LangGraph workflow states';
COMMENT ON COLUMN langgraph_checkpoints.state IS 'JSONB state data - consider using compression for large states';
COMMENT ON INDEX idx_checkpoints_thread_id IS 'Primary lookup index for thread-based queries';

INSERT INTO langgraph_migrations (version, description)
VALUES (8, 'Add table and column comments')
ON CONFLICT (version) DO NOTHING;