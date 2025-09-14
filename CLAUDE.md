# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a Rust port of LangGraph (Python) that maintains 100% API compatibility while delivering 10-100x performance improvements. The project follows Traffic-Light Development methodology and includes PyO3 bindings for drop-in Python replacement.

## Core Architecture

### Component Structure
The port consists of 5 core components that mirror Python LangGraph:

1. **Channel System** - 8 channel types for state management (LastValue, BinaryOperator, Topic, EphemeralValue, AnyValue, UntrackedValue, NamedBarrierValue)
2. **Pregel Engine** - Bulk Synchronous Parallel execution model adapted for LLM workflows
3. **Graph APIs** - StateGraph, MessageGraph, and CompiledGraph for building stateful agent workflows
4. **Checkpoint System** - Persistence and recovery with pluggable backends
5. **Runtime** - Async/sync execution, streaming, and interrupt handling

### Rust Design Patterns
- **Trait-based channels** with associated types for compile-time type safety
- **Arc<RwLock<T>>** for shared state management across async boundaries
- **Tokio** for async runtime and task scheduling
- **Serde** for serialization/deserialization
- **PyO3** for Python bindings with pyo3-asyncio for async support

### Python Compatibility Mapping
The Rust implementation must exactly match these Python APIs:
- `StateGraph` → `PyStateGraph` (via PyO3)
- `Pregel.invoke()` → `Pregel::invoke()` (async in Rust)
- `Pregel.stream()` → Returns async stream
- Channel methods: `update()`, `get()`, `checkpoint()`, `from_checkpoint()`

## Development Commands

### Project Setup (Not Yet Implemented)
```bash
# Initialize Rust workspace structure
mkdir -p langgraph-rust && cd langgraph-rust
cargo init --name langgraph-rust

# Create crate structure
for crate in langgraph-core langgraph-channels langgraph-pregel langgraph-checkpoint langgraph-runtime; do
  cargo init --lib --name $crate crates/$crate
done

# Initialize PyO3 bindings
cargo init --lib --name langgraph-py langgraph-py
```

### Build Commands (Future)
```bash
# Build all crates
cargo build --workspace

# Build release with optimizations
cargo build --release --workspace

# Build Python module
cd langgraph-py && maturin develop
```

### Testing Strategy
```bash
# Run Rust tests
cargo test --workspace

# Run integration tests against Python LangGraph
python tests/test_compatibility.py

# Run benchmarks
cargo bench

# Test Python bindings
pytest langgraph-py/tests/
```

### Task Management
```bash
# View current task status
cat tasks/tracker/tracker.md

# Find next priority task
grep -l "Priority: P0" tasks/task-*.md | grep "Not Started"

# Update task progress
# Edit task file status and update tracker.md
```

## Traffic-Light Development Workflow

### 🔴 RED Phase (Weeks 1-2)
Focus: Define interfaces and write failing tests
- Start with Task 001: Initialize Workspace
- Define channel traits in `crates/langgraph-core/src/channel.rs`
- Write integration tests that call Python LangGraph for comparison
- Tests should fail until implementation

### 🟡 YELLOW Phase (Weeks 3-6)
Focus: Minimal working implementation
- Implement Pregel engine in `crates/langgraph-pregel/`
- Build StateGraph API to match Python signatures
- Get basic graph execution working
- 50% of tests should pass

### 🟢 GREEN Phase (Weeks 7-10)
Focus: Production readiness
- Complete all 8 channel types
- Add comprehensive error handling with custom Result types
- Implement PyO3 bindings in `langgraph-py/`
- Optimize for 10x performance improvement
- 95% test coverage required

## Critical Implementation Details

### Channel Trait Design
```rust
pub trait Channel: Send + Sync {
    type Value: Clone + Send + Sync;
    type Update: Send;
    type Checkpoint: Serialize + for<'de> Deserialize<'de>;
    
    fn update(&mut self, updates: Vec<Self::Update>) -> Result<bool, ChannelError>;
    fn get(&self) -> Result<Self::Value, ChannelError>;
    fn checkpoint(&self) -> Result<Self::Checkpoint, ChannelError>;
    fn from_checkpoint(checkpoint: Self::Checkpoint) -> Result<Self, ChannelError> where Self: Sized;
}
```

### Pregel BSP Execution Model
The Pregel engine executes in supersteps:
1. **Read Phase** - Prepare tasks from channels
2. **Execute Phase** - Run tasks in parallel
3. **Write Phase** - Apply results to channels
4. **Checkpoint Phase** - Save state if configured

### PyO3 Module Structure
Python module exposes Rust types with identical API:
```python
from langgraph_rust import StateGraph, START, END

# Must work exactly like Python LangGraph
graph = StateGraph(dict)
graph.add_node("agent", agent_func)
graph.add_edge(START, "agent")
compiled = graph.compile()
result = compiled.invoke({"input": "data"})
```

## Task System

All development follows the task tracking system in `/tasks/`:
- **Master Tracker**: `/tasks/tracker/tracker.md` - Overall progress dashboard
- **Task Files**: `/tasks/task-XXX-*.md` - Individual task specifications
- **60 Total Tasks**: 19 RED + 20 YELLOW + 21 GREEN phases
- **Critical Path**: Tasks 001→002→003→004→020→042 must complete in order

Task priorities:
- P0: Critical path, blocks other work
- P1: High priority for phase completion
- P2: Important but not blocking
- P3: Nice to have

## Python LangGraph Reference

The Python implementation to port is located in `/langgraph/`:
- Core library: `/langgraph/libs/langgraph/langgraph/`
- Channels: `/langgraph/libs/langgraph/langgraph/channels/`
- Pregel engine: `/langgraph/libs/langgraph/langgraph/pregel/`
- Graph APIs: `/langgraph/libs/langgraph/langgraph/graph/`

Key Python files to study:
- `channels/base.py` - BaseChannel abstract class
- `pregel/main.py` - Core Pregel execution engine
- `graph/state.py` - StateGraph implementation
- `checkpoint/base.py` - Checkpoint system

## Integration Testing

All tests must validate against real Python LangGraph:
```python
# Compare Rust and Python outputs
python_result = python_graph.invoke(input_data)
rust_result = rust_graph.invoke(input_data)
assert rust_result == python_result
```

No mocks or fakes allowed - Integration-First methodology requires testing against real implementations.

## Performance Targets

- Execution speed: 10x faster than Python
- Memory usage: <50% of Python implementation
- Startup time: <100ms for graph compilation
- Streaming latency: <10ms per chunk
- Checkpoint size: Comparable to Python

## Upstream Synchronization

Monitor Python LangGraph for changes:
```bash
# Check for upstream updates
cd langgraph && git fetch upstream
git diff HEAD upstream/main

# Relevant changes should be ported to maintain compatibility
```

GitHub Actions will automate this process once implemented (Task 055).