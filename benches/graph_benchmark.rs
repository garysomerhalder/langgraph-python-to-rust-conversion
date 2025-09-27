use criterion::{black_box, criterion_group, criterion_main, Criterion};
use langgraph::engine::executor::ExecutionEngine;
use langgraph::graph::{Edge, Node, NodeType, StateGraph};
use langgraph::state::GraphState;
use serde_json::json;
use std::collections::HashMap;
use tokio::runtime::Runtime;

fn create_simple_graph() -> StateGraph {
    let mut graph = StateGraph::new("benchmark_graph");

    // Add required start and end nodes
    graph.add_node(Node {
        id: "__start__".to_string(),
        node_type: NodeType::Start,
        metadata: None,
    });

    graph.add_node(Node {
        id: "__end__".to_string(),
        node_type: NodeType::End,
        metadata: None,
    });

    // Add simple nodes
    graph.add_node(Node {
        id: "process".to_string(),
        node_type: NodeType::Agent("processor".to_string()),
        metadata: None,
    });

    graph.add_node(Node {
        id: "validate".to_string(),
        node_type: NodeType::Agent("validator".to_string()),
        metadata: None,
    });

    // Add edges
    graph
        .add_edge("__start__", "process", Edge::direct())
        .unwrap();
    graph
        .add_edge("process", "validate", Edge::direct())
        .unwrap();
    graph
        .add_edge("validate", "__end__", Edge::direct())
        .unwrap();

    // Set entry point
    graph.set_entry_point("__start__").unwrap();

    graph
}

fn create_complex_graph() -> StateGraph {
    let mut graph = StateGraph::new("complex_benchmark");

    // Add required start and end nodes
    graph.add_node(Node {
        id: "__start__".to_string(),
        node_type: NodeType::Start,
        metadata: None,
    });

    graph.add_node(Node {
        id: "__end__".to_string(),
        node_type: NodeType::End,
        metadata: None,
    });

    // Add multiple nodes
    for i in 0..10 {
        graph.add_node(Node {
            id: format!("node_{}", i),
            node_type: NodeType::Agent(format!("agent_{}", i)),
            metadata: None,
        });
    }

    // Create a complex flow
    graph
        .add_edge("__start__", "node_0", Edge::direct())
        .unwrap();

    for i in 0..9 {
        graph
            .add_edge(
                &format!("node_{}", i),
                &format!("node_{}", i + 1),
                Edge::direct(),
            )
            .unwrap();
    }

    graph.add_edge("node_9", "__end__", Edge::direct()).unwrap();

    // Set entry point
    graph.set_entry_point("__start__").unwrap();

    graph
}

fn benchmark_graph_creation(c: &mut Criterion) {
    c.bench_function("create_simple_graph", |b| b.iter(|| create_simple_graph()));

    c.bench_function("create_complex_graph", |b| {
        b.iter(|| create_complex_graph())
    });
}

fn benchmark_state_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("state_update", |b| {
        b.iter(|| {
            rt.block_on(async {
                let mut state = GraphState::new();

                for i in 0..100 {
                    let mut updates = HashMap::new();
                    updates.insert(format!("counter_{}", i), json!(i));
                    state.update(updates);
                }

                black_box(state)
            })
        })
    });

    c.bench_function("state_get", |b| {
        b.iter(|| {
            rt.block_on(async {
                let mut state = GraphState::new();

                // Pre-populate state
                for i in 0..100 {
                    let mut updates = HashMap::new();
                    updates.insert(format!("key_{}", i), json!(i));
                    state.update(updates);
                }

                // Benchmark reads
                for i in 0..100 {
                    let _value = state.get(&format!("key_{}", i));
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
        b.iter(|| simple_graph.clone().compile())
    });

    c.bench_function("compile_complex_graph", |b| {
        b.iter(|| complex_graph.clone().compile())
    });
}

fn benchmark_graph_execution(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let graph = create_simple_graph();
    let compiled = graph.compile().unwrap();
    let executor = ExecutionEngine::new();

    c.bench_function("execute_simple_graph", |b| {
        b.iter(|| {
            rt.block_on(async {
                let mut input = HashMap::new();
                input.insert("value".to_string(), json!(42));

                executor.execute(compiled.clone(), input).await
            })
        })
    });
}

fn benchmark_parallel_execution(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("parallel_graph_execution", |b| {
        b.iter(|| {
            rt.block_on(async {
                let mut graph = StateGraph::new("parallel_test");

                // Add nodes
                graph.add_node(Node {
                    id: "__start__".to_string(),
                    node_type: NodeType::Start,
                    metadata: None,
                });

                graph.add_node(Node {
                    id: "__end__".to_string(),
                    node_type: NodeType::End,
                    metadata: None,
                });

                // Add parallel branches
                for i in 0..5 {
                    graph.add_node(Node {
                        id: format!("parallel_{}", i),
                        node_type: NodeType::Parallel,
                        metadata: None,
                    });

                    graph
                        .add_edge("__start__", &format!("parallel_{}", i), Edge::direct())
                        .unwrap();
                    graph
                        .add_edge(&format!("parallel_{}", i), "__end__", Edge::direct())
                        .unwrap();
                }

                graph.set_entry_point("__start__").unwrap();
                let compiled = graph.compile().unwrap();

                let executor = ExecutionEngine::new();
                let input = HashMap::new();

                executor.execute(compiled, input).await
            })
        })
    });
}

criterion_group!(
    benches,
    benchmark_graph_creation,
    benchmark_state_operations,
    benchmark_graph_compilation,
    benchmark_graph_execution,
    benchmark_parallel_execution
);

criterion_main!(benches);
