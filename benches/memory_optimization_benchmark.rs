use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use langgraph::utils::object_pool::{ObjectPool, pools, ExecutionContextPool, ExecutionContextBuffer};
use langgraph::utils::zero_copy::{
    ZeroCopyStateData, CowStateData, StateDiff, StringInterner, ZeroCopyBuffer
};
use langgraph::state::StateData;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

/// Benchmark the new object pools vs standard allocation
fn benchmark_enhanced_object_pools(c: &mut Criterion) {
    let mut group = c.benchmark_group("enhanced_object_pools");
    
    // Test memory entry pool
    group.bench_function("memory_entry_pool", |b| {
        b.iter(|| {
            let vec = pools::MEMORY_ENTRY_POOL.get();
            black_box(&vec);
            drop(vec);
        })
    });
    
    // Test tool params pool
    group.bench_function("tool_params_pool", |b| {
        b.iter(|| {
            let mut map = pools::TOOL_PARAMS_POOL.get();
            map.insert("param1".to_string(), json!("value1"));
            map.insert("param2".to_string(), json!(42));
            black_box(&map);
            drop(map);
        })
    });
    
    // Test execution metadata pool
    group.bench_function("exec_metadata_pool", |b| {
        b.iter(|| {
            let mut map = pools::EXEC_METADATA_POOL.get();
            map.insert("execution_id".to_string(), "test-id".to_string());
            map.insert("timestamp".to_string(), "1234567890".to_string());
            black_box(&map);
            drop(map);
        })
    });
    
    // Test node vec pool
    group.bench_function("node_vec_pool", |b| {
        b.iter(|| {
            let mut vec = pools::NODE_VEC_POOL.get();
            vec.push("node_1".to_string());
            vec.push("node_2".to_string());
            vec.push("node_3".to_string());
            black_box(&vec);
            drop(vec);
        })
    });
    
    // Test stream message pool
    group.bench_function("stream_message_pool", |b| {
        b.iter(|| {
            let mut vec = pools::STREAM_MESSAGE_POOL.get();
            vec.push(json!({"type": "message", "data": "test"}));
            vec.push(json!({"type": "status", "value": 200}));
            black_box(&vec);
            drop(vec);
        })
    });
    
    // Compare with regular allocation
    group.bench_function("regular_hashmap_allocation", |b| {
        b.iter(|| {
            let mut map: HashMap<String, Value> = HashMap::with_capacity(16);
            map.insert("param1".to_string(), json!("value1"));
            map.insert("param2".to_string(), json!(42));
            black_box(map);
        })
    });
    
    group.finish();
}

/// Benchmark execution context pool
fn benchmark_execution_context_pool(c: &mut Criterion) {
    let mut group = c.benchmark_group("execution_context_pool");
    let pool = ExecutionContextPool::new(50);
    
    group.bench_function("get_context_from_pool", |b| {
        b.iter(|| {
            let mut ctx = pool.get();
            ctx.execution_id = "test-execution-id".to_string();
            ctx.metadata.insert("key1".to_string(), "value1".to_string());
            ctx.start_time = 1234567890;
            ctx.node_stack.push("node_1".to_string());
            black_box(&ctx);
            drop(ctx);
        })
    });
    
    group.bench_function("create_context_regularly", |b| {
        b.iter(|| {
            let mut ctx = ExecutionContextBuffer {
                execution_id: "test-execution-id".to_string(),
                metadata: HashMap::with_capacity(8),
                start_time: 1234567890,
                node_stack: Vec::with_capacity(10),
            };
            ctx.metadata.insert("key1".to_string(), "value1".to_string());
            ctx.node_stack.push("node_1".to_string());
            black_box(ctx);
        })
    });
    
    group.finish();
}

/// Benchmark zero-copy state data
fn benchmark_zero_copy_state(c: &mut Criterion) {
    let mut group = c.benchmark_group("zero_copy_state");
    
    // Regular state data operations
    group.bench_function("regular_state_operations", |b| {
        b.iter(|| {
            let mut state: StateData = HashMap::new();
            for i in 0..100 {
                state.insert(format!("key_{}", i), json!(i));
            }
            
            // Read operations
            for i in 0..100 {
                let _ = state.get(&format!("key_{}", i));
            }
            
            black_box(state);
        })
    });
    
    // Zero-copy state data operations
    group.bench_function("zero_copy_state_operations", |b| {
        b.iter(|| {
            let state = ZeroCopyStateData::new();
            for i in 0..100 {
                state.insert(&format!("key_{}", i), json!(i));
            }
            
            // Read operations
            for i in 0..100 {
                let _ = state.get(&format!("key_{}", i));
            }
            
            black_box(state);
        })
    });
    
    group.finish();
}

/// Benchmark copy-on-write state operations
fn benchmark_cow_state(c: &mut Criterion) {
    let mut group = c.benchmark_group("cow_state");
    
    // Setup base state
    let mut base_state = ZeroCopyStateData::new();
    for i in 0..100 {
        base_state.insert(&format!("base_key_{}", i), json!(i));
    }
    
    group.bench_function("cow_state_read_only", |b| {
        let cow_state = CowStateData::new(base_state.clone());
        
        b.iter(|| {
            // Read from base (no COW triggered)
            for i in 0..100 {
                let _ = cow_state.get(&format!("base_key_{}", i));
            }
            black_box(&cow_state);
        })
    });
    
    group.bench_function("cow_state_with_modifications", |b| {
        b.iter(|| {
            let mut cow_state = CowStateData::new(base_state.clone());
            
            // Trigger COW with modifications
            for i in 0..10 {
                cow_state.insert(&format!("new_key_{}", i), json!(i * 10));
            }
            
            // Read mixed data
            for i in 0..100 {
                let _ = cow_state.get(&format!("base_key_{}", i));
            }
            for i in 0..10 {
                let _ = cow_state.get(&format!("new_key_{}", i));
            }
            
            black_box(cow_state);
        })
    });
    
    group.bench_function("cow_state_materialize", |b| {
        b.iter(|| {
            let mut cow_state = CowStateData::new(base_state.clone());
            
            // Add some modifications
            for i in 0..20 {
                cow_state.insert(&format!("mod_key_{}", i), json!(i));
            }
            
            // Materialize the state
            let materialized = cow_state.materialize();
            black_box(materialized);
        })
    });
    
    group.finish();
}

/// Benchmark string interning
fn benchmark_string_interning(c: &mut Criterion) {
    let mut group = c.benchmark_group("string_interning");
    
    let interner = StringInterner::new();
    let test_strings = (0..100).map(|i| format!("test_string_{}", i)).collect::<Vec<_>>();
    
    group.bench_function("string_interning", |b| {
        b.iter(|| {
            for s in &test_strings {
                let interned = interner.intern(s);
                black_box(interned);
            }
        })
    });
    
    group.bench_function("string_reinterning", |b| {
        // Pre-intern the strings
        for s in &test_strings {
            interner.intern(s);
        }
        
        b.iter(|| {
            // Re-intern (should find existing)
            for s in &test_strings {
                let interned = interner.intern(s);
                black_box(interned);
            }
        })
    });
    
    group.bench_function("regular_string_cloning", |b| {
        b.iter(|| {
            for s in &test_strings {
                let cloned: Arc<str> = Arc::from(s.as_str());
                black_box(cloned);
            }
        })
    });
    
    group.finish();
}

/// Benchmark state diff operations
fn benchmark_state_diff(c: &mut Criterion) {
    let mut group = c.benchmark_group("state_diff");
    
    let base_state = ZeroCopyStateData::new();
    for i in 0..100 {
        base_state.insert(&format!("key_{}", i), json!(i));
    }
    
    group.bench_function("create_diff", |b| {
        b.iter(|| {
            let diff = StateDiff::new();
            let interner = StringInterner::new();
            
            // Add changes
            for i in 0..20 {
                diff.add_change(
                    interner.intern(&format!("new_key_{}", i)),
                    langgraph::utils::zero_copy::SharedData::new(json!(i * 2))
                );
            }
            
            // Add removals
            for i in 0..10 {
                diff.add_removal(interner.intern(&format!("key_{}", i)));
            }
            
            black_box(diff);
        })
    });
    
    group.bench_function("apply_diff", |b| {
        // Pre-create diff
        let diff = StateDiff::new();
        let interner = StringInterner::new();
        
        for i in 0..20 {
            diff.add_change(
                interner.intern(&format!("new_key_{}", i)),
                langgraph::utils::zero_copy::SharedData::new(json!(i * 2))
            );
        }
        
        b.iter(|| {
            let state = base_state.clone();
            diff.apply_to(&state);
            black_box(state);
        })
    });
    
    group.finish();
}

/// Benchmark zero-copy buffer operations
fn benchmark_zero_copy_buffer(c: &mut Criterion) {
    let mut group = c.benchmark_group("zero_copy_buffer");
    
    group.bench_function("buffer_append_and_slice", |b| {
        b.iter(|| {
            let mut buffer = ZeroCopyBuffer::with_capacity(4096);
            
            // Append data
            for i in 0..100 {
                buffer.append(format!("data_{} ", i).as_bytes());
            }
            
            // Create slices
            let slice1 = buffer.slice(0, 50);
            let slice2 = buffer.slice(50, 100);
            
            black_box((slice1, slice2));
        })
    });
    
    group.bench_function("regular_vec_operations", |b| {
        b.iter(|| {
            let mut buffer = Vec::with_capacity(4096);
            
            // Append data
            for i in 0..100 {
                buffer.extend_from_slice(format!("data_{} ", i).as_bytes());
            }
            
            // Clone slices (not zero-copy)
            let slice1 = buffer[0..50].to_vec();
            let slice2 = buffer[50..100].to_vec();
            
            black_box((slice1, slice2));
        })
    });
    
    group.finish();
}

criterion_group!(
    benches,
    benchmark_enhanced_object_pools,
    benchmark_execution_context_pool,
    benchmark_zero_copy_state,
    benchmark_cow_state,
    benchmark_string_interning,
    benchmark_state_diff,
    benchmark_zero_copy_buffer
);

criterion_main!(benches);