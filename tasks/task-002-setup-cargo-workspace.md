# Task 002: Setup Cargo Workspace Configuration

## 📋 Task Details
- **Task ID:** 002
- **Title:** Configure Cargo Workspace with Shared Dependencies
- **Phase:** 🔴 RED (Foundation)
- **Priority:** P0 (Critical Path)
- **Estimated Hours:** 6 hours
- **Assigned To:** Gary Somerhalder
- **Status:** ⏸️ Pending

## 🎯 Description
Configure the Cargo workspace with proper dependency management, shared configurations, and optimized build settings. Establish the foundation for efficient multi-crate development with consistent tooling across all components.

## ✅ Acceptance Criteria
- [ ] All crate Cargo.toml files created with proper metadata
- [ ] Workspace-level dependency management configured
- [ ] Shared build configurations and feature flags defined
- [ ] Development tooling (clippy, rustfmt, deny) configured
- [ ] Benchmark and testing infrastructure setup
- [ ] Documentation generation configured
- [ ] Release profile optimization settings applied
- [ ] Feature gates properly defined for optional functionality

## 📦 Dependencies
- **Prerequisites:** Task 001 (Initialize Workspace)
- **Blocks:** Task 003 (Define Channel Traits), Task 004 (Test Framework)
- **Related:** Task 007 (CI/CD Pipeline Setup)

## 🔧 Technical Notes

### Crate Structure and Dependencies

**langgraph-core** - Core traits and types
```toml
[dependencies]
serde = { workspace = true }
uuid = { workspace = true }
tokio = { workspace = true }
async-trait = "0.1"
thiserror = "1.0"
```

**langgraph-channels** - Channel implementations
```toml
[dependencies]
langgraph-core = { path = "../langgraph-core" }
serde = { workspace = true }
tokio = { workspace = true }
parking_lot = "0.12"
dashmap = "5.5"
```

**langgraph-pregel** - Execution engine
```toml
[dependencies]
langgraph-core = { path = "../langgraph-core" }
langgraph-channels = { path = "../langgraph-channels" }
tokio = { workspace = true }
futures = "0.3"
rayon = "1.7"
petgraph = "0.6"
```

**langgraph-checkpoint** - Persistence layer
```toml
[dependencies]
langgraph-core = { path = "../langgraph-core" }
serde = { workspace = true }
serde_json = "1.0"
bincode = "1.3"
sqlx = { version = "0.7", features = ["runtime-tokio-rustls", "sqlite", "postgres"] }
```

**langgraph-runtime** - Runtime and streaming
```toml
[dependencies]
langgraph-core = { path = "../langgraph-core" }
langgraph-pregel = { path = "../langgraph-pregel" }
tokio = { workspace = true }
tokio-stream = "0.1"
pin-project-lite = "0.2"
```

**langgraph-py** - Python bindings
```toml
[dependencies]
langgraph-core = { path = "../crates/langgraph-core" }
langgraph-channels = { path = "../crates/langgraph-channels" }
langgraph-pregel = { path = "../crates/langgraph-pregel" }
langgraph-runtime = { path = "../crates/langgraph-runtime" }
pyo3 = { version = "0.20", features = ["extension-module"] }
pyo3-asyncio = { version = "0.20", features = ["tokio-runtime"] }
```

### Build Configuration

**Release Profile Optimization:**
```toml
[profile.release]
opt-level = 3
lto = true
codegen-units = 1
panic = "abort"
strip = true
```

**Development Profile:**
```toml
[profile.dev]
opt-level = 0
debug = true
split-debuginfo = "unpacked"
```

### Feature Gates Strategy
- **default:** Basic functionality only
- **full:** All features enabled  
- **python:** Python bindings
- **persistence:** Checkpoint features
- **streaming:** Runtime streaming features
- **metrics:** Performance monitoring

## 🧪 Testing Requirements
- [ ] All crates compile individually with `cargo check`
- [ ] Workspace builds completely with `cargo build`
- [ ] Features work correctly (`cargo build --features full`)
- [ ] Documentation builds (`cargo doc --workspace`)
- [ ] Benchmark framework compiles (`cargo bench --no-run`)
- [ ] Cross-crate dependencies resolve properly
- [ ] Clippy passes with workspace lints
- [ ] No unused dependencies reported by `cargo-udeps`

## 📝 Implementation Steps
1. **Create individual crate Cargo.toml files** with proper metadata
2. **Configure workspace dependencies** with version pinning
3. **Set up shared linting configuration** (clippy, rustfmt)
4. **Configure feature gates** for optional functionality  
5. **Add development dependencies** for testing and benchmarking
6. **Set up documentation generation** with proper examples
7. **Configure build profiles** for development and release
8. **Test dependency resolution** and build times
9. **Add cargo-deny configuration** for security and licensing
10. **Verify cross-crate imports** work correctly

## 🔗 Related Tasks
- **Previous:** [Task 001: Initialize Workspace](task-001-initialize-workspace.md)
- **Next:** [Task 003: Define Channel Traits](task-003-define-channel-traits.md)
- **Enables:** [Task 007: CI/CD Pipeline](task-007-setup-cicd-pipeline.md)
- **Tracker:** [Master Tracker](tracker/tracker.md)

## 📊 Success Metrics
- Full workspace builds in <60 seconds (incremental <10 seconds)
- Zero dependency conflicts or circular dependencies
- All feature combinations compile successfully
- Documentation builds without warnings
- Cargo-deny passes security and license checks

## 🚨 Risk Factors
- **Medium Risk:** Complex dependency graph with async requirements
- **Version Conflicts:** Careful management of tokio and async ecosystem
- **Build Time:** Monitor incremental compilation performance
- **Feature Bloat:** Keep feature gates minimal and focused

## 📅 Timeline
- **Start:** Week 1, Day 2
- **Target Completion:** Week 1, Day 3
- **Buffer:** 1 day for dependency conflict resolution

---
*Created: 2025-09-14*  
*Last Updated: 2025-09-14*  
*Status Changed: 2025-09-14*