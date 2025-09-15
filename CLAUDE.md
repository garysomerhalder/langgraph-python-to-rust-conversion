# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

LangGraph Rust is a high-performance implementation of LangGraph for building stateful, multi-agent applications. The codebase follows Traffic-Light Development methodology (RED-YELLOW-GREEN) and emphasizes production-ready code with comprehensive testing.

## Build Commands

```bash
# Build the project
cargo build
cargo build --release  # With optimizations

# Run all tests
cargo test

# Run specific test suites
cargo test --test integration_test
cargo test --test streaming_test
cargo test --test advanced_features_test
cargo test --test h_cycle_integration_test

# Run a single test by name
cargo test test_name_here -- --exact

# Run tests with output
cargo test -- --nocapture

# Run tests in release mode (faster)
cargo test --release

# Check code without building
cargo check

# Format code
cargo fmt

# Lint code
cargo clippy -- -D warnings

# Generate documentation
cargo doc --open

# Run examples
cargo run --example simple_workflow
cargo run --example conditional_routing
cargo run --example multi_agent
cargo run --example state_management

# Run benchmarks
cargo bench
```

## Architecture Overview

The codebase implements a graph-based workflow engine with these core architectural patterns:

### Module Organization

- **`src/graph/`** - Core graph structures using petgraph, builder pattern for construction
  - `StateGraph` manages nodes and edges with Arc-based thread safety
  - `ConditionalRouter` for dynamic routing based on state evaluation
  - `SubgraphExecutor` for nested graph composition
  
- **`src/state/`** - State management with versioning and persistence
  - `GraphState` with concurrent access via DashMap
  - Channel-based state updates with reducers
  - Delta compression for efficient versioning
  
- **`src/engine/`** - Execution strategies and runtime
  - `ExecutionEngine` orchestrates graph traversal
  - `ParallelExecutor` with semaphore-based concurrency control
  - Deadlock detection and prevention mechanisms
  
- **`src/stream/`** - Async streaming with backpressure
  - Broadcast channels for multi-consumer scenarios
  - Flow control and buffering strategies
  
- **`src/tools/`** - External tool integration framework
  - Trait-based tool abstraction
  - HTTP and function tool implementations
  
- **`src/agents/`** - Agent reasoning capabilities
  - Chain of Thought, ReAct, Tree of Thoughts strategies
  - Memory management (short-term, long-term, working)
  
- **`src/checkpoint/`** - Persistence and recovery
  - Memory and SQLite checkpointer implementations
  - Thread-based checkpoint isolation

### Key Design Patterns

1. **Builder Pattern**: GraphBuilder provides fluent API for graph construction
2. **Strategy Pattern**: ExecutionStrategy trait for different execution modes
3. **Observer Pattern**: Streaming updates via broadcast channels
4. **Command Pattern**: Tool and Agent abstractions for extensibility
5. **Memento Pattern**: Checkpoint system for state persistence

### Concurrency Model

- **DashMap** for lock-free concurrent state access
- **Arc<RwLock>** for versioned state management  
- **Tokio** async runtime with work-stealing scheduler
- **Semaphore-based** concurrency limiting in parallel executor
- **Channel-based** communication between components

### Error Handling Strategy

All errors use `thiserror` for structured error types:
- `GraphError` for graph construction issues
- `ExecutionError` for runtime failures
- `StateError` for state management problems
- Retry logic with exponential backoff built into execution engine

## Development Patterns

### Testing Approach

The codebase uses Integration-First testing:
- All tests run against real implementations (no mocks)
- Comprehensive integration tests in `tests/` directory
- Unit tests colocated with implementation in `src/`
- Tests follow Traffic-Light Development phases

### State Management

State flows through typed channels with reducers:
```rust
// States are updated via channels
state.update_channel("messages", MessageOp::Append, value);

// Versioning tracks all changes
let version = state.create_version();
state.rollback_to_version(version_id);
```

### Graph Construction

Graphs use a fluent builder API:
```rust
GraphBuilder::new("workflow")
    .add_node("start", NodeType::Start)
    .add_conditional_edge("router", vec![
        Branch::new(condition_fn, "path_a"),
        Branch::new(else_fn, "path_b"),
    ])
    .compile()
```

### Parallel Execution

The parallel executor manages concurrency:
- Dependency analysis determines execution order
- Semaphore limits concurrent tasks
- Deadlock detection prevents circular dependencies
- Metrics track execution performance

## Code Standards

- **Rust Edition**: 2021
- **Async Runtime**: Tokio with full features
- **Error Handling**: Result types with anyhow/thiserror
- **Logging**: tracing with structured logging
- **Serialization**: serde with serde_json
- **Testing**: Integration-first with real implementations

## Common Tasks

### Adding a New Node Type

1. Extend `NodeType` enum in `src/graph/mod.rs`
2. Implement execution logic in `src/engine/executor.rs`
3. Add builder method in `GraphBuilder`
4. Write integration test in `tests/`

### Creating a Custom Tool

1. Implement `Tool` trait in `src/tools/`
2. Define `ToolSpec` with parameters
3. Implement `execute()` method
4. Register in `ToolRegistry`

### Adding an Agent Strategy

1. Implement `Agent` trait in `src/agents/`
2. Define reasoning strategy (observe, reason, act, reflect)
3. Add memory management if needed
4. Write tests for agent behavior

### Implementing a Checkpointer

1. Implement `Checkpointer` trait in `src/checkpoint/`
2. Define storage backend (file, database, cloud)
3. Handle serialization/deserialization
4. Add tests for persistence/recovery

## Performance Considerations

- Graph compilation pre-computes traversal order
- State diffing transmits only changes
- Parallel execution with configurable concurrency
- Channel buffering prevents memory bloat
- String interning reduces memory usage

## Current Status

The project is 89% complete with all core features implemented:
- ✅ Graph construction and traversal
- ✅ State management with versioning
- ✅ Parallel and streaming execution
- ✅ Conditional routing and subgraphs
- ✅ Tool and agent integration
- ✅ Checkpoint system
- ✅ Comprehensive test coverage (94 tests passing)
- 🚧 Documentation improvements ongoing
- 🚧 Performance benchmarks to be added