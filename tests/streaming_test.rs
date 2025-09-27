use futures::stream::StreamExt;
use langgraph::graph::{Graph, GraphError};
use langgraph::state::{GraphState, State};
use langgraph::stream::*;
use std::sync::Arc;
use tokio;

#[tokio::test]
async fn test_streaming_engine() {
    let engine = DefaultStreamingEngine::new(100);

    // Create a simple graph for testing
    let graph_builder = langgraph::graph::GraphBuilder::new("test_graph")
        .add_node("__start__", langgraph::graph::NodeType::Start)
        .add_node("__end__", langgraph::graph::NodeType::End)
        .add_edge("__start__", "__end__")
        .set_entry_point("__start__");

    let graph = graph_builder.build().unwrap().compile().unwrap();
    let graph_arc = Arc::new(graph);

    let initial_state = GraphState::new();

    let mut stream = engine
        .stream_execution(graph_arc, initial_state)
        .await
        .unwrap();

    let mut count = 0;
    while let Some(output) = stream.next().await {
        assert!(output.sequence >= 0);
        assert!(output.value.is_object());
        count += 1;
    }

    assert!(count > 0);
}

#[tokio::test]
async fn test_channel_registry() {
    let registry = ChannelRegistry::new();

    // Create broadcast channel
    registry
        .create_channel("test_broadcast".to_string(), ChannelType::Broadcast(10))
        .await
        .unwrap();

    // Create MPSC channel
    registry
        .create_channel("test_mpsc".to_string(), ChannelType::Mpsc(10))
        .await
        .unwrap();

    // Test MPSC channel (more reliable for testing)
    let sender: ChannelSender<String> = registry.get_sender("test_mpsc").await.unwrap();
    let mut receiver: ChannelReceiver<String> = registry.get_receiver("test_mpsc").await.unwrap();

    // Test sending and receiving
    sender.send("Hello".to_string()).await.unwrap();
    let received = receiver.recv().await.unwrap();
    assert_eq!(received, "Hello");

    // Test broadcast channel with proper receiver setup
    let mut broadcast_receiver: ChannelReceiver<String> =
        registry.get_receiver("test_broadcast").await.unwrap();
    let broadcast_sender: ChannelSender<String> =
        registry.get_sender("test_broadcast").await.unwrap();

    // Spawn a task to handle the receiver to keep it alive
    let handle = tokio::spawn(async move { broadcast_receiver.recv().await });

    // Small delay to ensure receiver is ready
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Now send
    broadcast_sender
        .send("Broadcast Hello".to_string())
        .await
        .unwrap();

    // Get the result
    let broadcast_result = handle.await.unwrap().unwrap();
    assert_eq!(broadcast_result, "Broadcast Hello");
}

#[tokio::test]
async fn test_stream_transformers() {
    // Test map transformer
    let mapper = MapTransformer::new(|x: i32| x * 2);
    let result = mapper.transform(5).await.unwrap();
    assert_eq!(result, 10);

    // Test filter transformer
    let filter = FilterTransformer::new(|x: &i32| *x > 5);
    assert!(filter.transform(10).await.is_ok());
    assert!(filter.transform(3).await.is_err());

    // Test batch transformer
    let batch = BatchTransformer::new(3);
    assert!(batch.transform(1).await.is_err()); // Not full
    assert!(batch.transform(2).await.is_err()); // Not full
    let batched = batch.transform(3).await.unwrap();
    assert_eq!(batched.len(), 3);
}

#[tokio::test]
async fn test_flow_control() {
    // Test backpressure controller
    let controller = BackpressureController::new(2);

    let permit1 = controller.acquire().await.unwrap();
    let permit2 = controller.acquire().await.unwrap();

    assert_eq!(controller.current_load(), 2);
    assert_eq!(controller.load_percentage(), 100.0);

    controller.release(permit1).await;
    assert_eq!(controller.current_load(), 1);

    controller.release(permit2).await;
    assert_eq!(controller.current_load(), 0);

    // Test rate limiter
    let rate_limiter = RateLimiter::new(5);

    for _ in 0..5 {
        let _permit = rate_limiter.acquire().await.unwrap();
    }

    // Test circuit breaker
    let circuit_breaker = CircuitBreaker::new(
        3, // failure threshold
        3, // success threshold
        std::time::Duration::from_secs(1),
    );

    // Should be able to acquire initially
    let _permit = circuit_breaker.acquire().await.unwrap();
}

#[tokio::test]
async fn test_stream_collectors() {
    // Test VecCollector
    let mut vec_collector = VecCollector::new();
    vec_collector.collect_item(1).await.unwrap();
    vec_collector.collect_item(2).await.unwrap();
    vec_collector.collect_item(3).await.unwrap();

    let result = vec_collector.finalize().await.unwrap();
    assert_eq!(result, vec![1, 2, 3]);

    // Test HashMapCollector
    let mut map_collector = HashMapCollector::new();
    map_collector
        .collect_item(("key1".to_string(), "value1".to_string()))
        .await
        .unwrap();
    map_collector
        .collect_item(("key2".to_string(), "value2".to_string()))
        .await
        .unwrap();

    let map = map_collector.finalize().await.unwrap();
    assert_eq!(map.get("key1"), Some(&"value1".to_string()));
    assert_eq!(map.get("key2"), Some(&"value2".to_string()));

    // Test StatisticsCollector
    let mut stats_collector = StatisticsCollector::new();
    stats_collector.collect_item(1.0).await.unwrap();
    stats_collector.collect_item(2.0).await.unwrap();
    stats_collector.collect_item(3.0).await.unwrap();

    let stats = stats_collector.finalize().await.unwrap();
    assert_eq!(stats.count, 3);
    assert_eq!(stats.sum, 6.0);
    assert_eq!(stats.mean, 2.0);
    assert_eq!(stats.median, Some(2.0));
}
