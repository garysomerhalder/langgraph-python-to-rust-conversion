# LangGraph Rust

A Rust implementation of LangGraph for building stateful, multi-agent applications.

## Overview

LangGraph is a library for building stateful, multi-agent applications with Large Language Models (LLMs). This Rust implementation provides a type-safe, performant alternative to the Python version, leveraging Rust's ownership system and async capabilities.

## Features

- 🚀 **High Performance**: Leverages Rust's zero-cost abstractions
- 🔒 **Type Safety**: Compile-time guarantees with Rust's type system
- ⚡ **Async/Await**: Built on Tokio for efficient async execution
- 📊 **Graph-Based Workflows**: Define complex agent interactions as directed graphs
- 💾 **State Management**: Built-in state persistence and checkpointing
- 🔧 **Extensible**: Easy to add custom nodes, edges, and state types

## Requirements

- Rust 1.75.0 or later
- Cargo (comes with Rust)

## Building

```bash
# Clone the repository
git clone https://github.com/terragon/langgraph-rust.git
cd langgraph-rust

# Build the project
cargo build

# Run tests
cargo test

# Build in release mode with optimizations
cargo build --release
```

## Project Structure

```
├── src/
│   ├── lib.rs         # Main library entry point
│   ├── graph/         # Graph structures and algorithms
│   ├── state/         # State management
│   └── engine/        # Execution engine
├── tests/             # Integration tests
├── benches/           # Performance benchmarks
└── examples/          # Usage examples
```

## Development

This project follows Traffic-Light Development methodology:
- 🔴 **Red**: Write failing tests first
- 🟡 **Yellow**: Implement minimal solution
- 🟢 **Green**: Harden with production readiness

## License

Dual licensed under MIT OR Apache-2.0

## Status

This is an active port of LangGraph from Python to Rust. See the [task tracker](tasks/tracker/tracker.md) for current progress.