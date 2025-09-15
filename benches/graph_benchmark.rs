use criterion::{black_box, criterion_group, criterion_main, Criterion};
use langgraph::{
    graph::{StateGraph, GraphBuilder},
    state::{StateData, StateManager},
    nodes::{NodeType, NodeExecutor},
};
use serde_json::json;
use std::sync::Arc;
use tokio::runtime::Runtime;

fn create_simple_graph() -> StateGraph {
    let mut graph = StateGraph::new("benchmark_graph");
    
    // Add simple nodes
    graph.add_node(
        "process".to_string(),
        NodeType::Agent("processor".to_string()),
        None,
    );
    
    graph.add_node(
        "validate".to_string(), 
        NodeType::Agent("validator".to_string()),
        None,
    );
    
    // Add edges
    graph.add_edge("__start__".to_string(), "process".to_string(), None);
    graph.add_edge("process".to_string(), "validate".to_string(), None);
    graph.add_edge("validate".to_string(), "__end__".to_string(), None);
    
    graph
}

fn create_complex_graph() -> StateGraph {
    let mut graph = StateGraph::new("complex_benchmark");
    
    // Add multiple nodes
    for i in 0..10 {
        graph.add_node(
            format!("node_{}", i),
            NodeType::Agent(format!("agent_{}", i)),
            None,
        );
    }
    
    // Create a complex flow
    graph.add_edge("__start__".to_string(), "node_0".to_string(), None);
    
    for i in 0..9 {
        graph.add_edge(
            format!("node_{}", i),
            format!("node_{}", i + 1),
            None,
        );
    }
    
    graph.add_edge("node_9".to_string(), "__end__".to_string(), None);
    
    graph
}

fn benchmark_graph_creation(c: &mut Criterion) {
    c.bench_function("create_simple_graph", |b| {
        b.iter(|| create_simple_graph())
    });
    
    c.bench_function("create_complex_graph", |b| {
        b.iter(|| create_complex_graph())
    });
}

fn benchmark_state_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    c.bench_function("state_update", |b| {
        b.iter(|| {
            rt.block_on(async {
                let manager = StateManager::new();
                let mut state = StateData::new();
                state.insert("counter".to_string(), json!(0));
                
                for i in 0..100 {
                    manager.update_state(
                        &mut state,
                        "counter",
                        json!(i),
                    ).await.unwrap();
                }
                
                black_box(state)
            })
        })
    });
    
    c.bench_function("state_checkpoint", |b| {
        b.iter(|| {
            rt.block_on(async {
                let manager = StateManager::new();
                let mut state = StateData::new();
                
                for i in 0..10 {
                    state.insert(format!("key_{}", i), json!(i));
                    manager.checkpoint(&state, &format!("checkpoint_{}", i))
                        .await
                        .unwrap();
                }
                
                black_box(state)
            })
        })
    });
}

fn benchmark_graph_compilation(c: &mut Criterion) {
    let simple_graph = create_simple_graph();
    let complex_graph = create_complex_graph();
    
    c.bench_function("compile_simple_graph", |b| {
        b.iter(|| {
            GraphBuilder::new(simple_graph.clone())
                .with_state_manager(Arc::new(StateManager::new()))
                .compile()
        })
    });
    
    c.bench_function("compile_complex_graph", |b| {
        b.iter(|| {
            GraphBuilder::new(complex_graph.clone())
                .with_state_manager(Arc::new(StateManager::new()))
                .compile()
        })
    });
}

fn benchmark_graph_execution(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let graph = create_simple_graph();
    let compiled = GraphBuilder::new(graph)
        .with_state_manager(Arc::new(StateManager::new()))
        .compile()
        .unwrap();
    
    c.bench_function("execute_simple_graph", |b| {
        b.iter(|| {
            rt.block_on(async {
                let mut input = StateData::new();
                input.insert("value".to_string(), json!(42));
                
                compiled.invoke(input).await
            })
        })
    });
}

criterion_group!(
    benches,
    benchmark_graph_creation,
    benchmark_state_operations,
    benchmark_graph_compilation,
    benchmark_graph_execution
);

criterion_main!(benches);