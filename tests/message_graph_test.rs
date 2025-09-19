//! Integration tests for MessageGraph
//! RED Phase: Writing failing tests for MSG-001

use langgraph::{
    message::{MessageGraph, Message, MessageType, MessageRole, MessageHandler},
    graph::GraphBuilder,
    state::StateData,
    Result,
};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Test basic MessageGraph creation and structure
#[tokio::test]
async fn test_message_graph_creation() -> Result<()> {
    // Create a message graph
    let graph = MessageGraph::new("chat_graph")
        .add_message_node("greeting", |msg: Message| async move {
            if msg.content.contains("hello") {
                Message::new(MessageRole::Assistant, "Hello! How can I help you?")
            } else {
                Message::new(MessageRole::Assistant, "I don't understand")
            }
        })
        .add_message_node("farewell", |msg: Message| async move {
            Message::new(MessageRole::Assistant, "Goodbye!")
        })
        .add_router("main_router", |msg: &Message| {
            if msg.content.to_lowercase().contains("bye") {
                "farewell"
            } else {
                "greeting"
            }
        })
        .set_entry_point("main_router")
        .build()?;

    // Test message processing
    let input_msg = Message::new(MessageRole::User, "hello there");
    let response = graph.process_message(input_msg).await?;

    assert_eq!(response.role, MessageRole::Assistant);
    assert!(response.content.contains("Hello"));

    Ok(())
}

/// Test message routing based on content
#[tokio::test]
async fn test_message_routing() -> Result<()> {
    let graph = MessageGraph::new("routing_test")
        .add_message_node("support", |msg: Message| async move {
            Message::new(MessageRole::Assistant, "Let me help you with that issue")
        })
        .add_message_node("sales", |msg: Message| async move {
            Message::new(MessageRole::Assistant, "I can help you with pricing information")
        })
        .add_message_node("general", |msg: Message| async move {
            Message::new(MessageRole::Assistant, "How can I assist you today?")
        })
        .add_content_router("department_router", vec![
            ("support", vec!["help", "issue", "problem", "broken"]),
            ("sales", vec!["price", "cost", "buy", "purchase"]),
        ])
        .set_default_route("general")
        .build()?;

    // Test routing to support
    let support_msg = Message::new(MessageRole::User, "I have a problem with my account");
    let response = graph.process_message(support_msg).await?;
    assert!(response.content.contains("help you with that issue"));

    // Test routing to sales
    let sales_msg = Message::new(MessageRole::User, "What's the price of the premium plan?");
    let response = graph.process_message(sales_msg).await?;
    assert!(response.content.contains("pricing information"));

    // Test default routing
    let general_msg = Message::new(MessageRole::User, "Hi there");
    let response = graph.process_message(general_msg).await?;
    assert!(response.content.contains("assist you today"));

    Ok(())
}

/// Test conversation history tracking
#[tokio::test]
async fn test_conversation_history() -> Result<()> {
    let graph = MessageGraph::new("history_test")
        .with_history_enabled(10) // Keep last 10 messages
        .add_message_node("responder", |msg: Message| async move {
            Message::new(
                MessageRole::Assistant,
                format!("You said: {}", msg.content),
            )
        })
        .build()?;

    // Process multiple messages
    let messages = vec![
        "First message",
        "Second message",
        "Third message",
    ];

    for content in messages {
        let msg = Message::new(MessageRole::User, content);
        graph.process_message(msg).await?;
    }

    // Get conversation history
    let history = graph.get_conversation_history().await;
    assert_eq!(history.len(), 6); // 3 user + 3 assistant messages

    // Verify message order
    assert_eq!(history[0].content, "First message");
    assert_eq!(history[1].content, "You said: First message");

    Ok(())
}

/// Test message metadata and context
#[tokio::test]
async fn test_message_metadata() -> Result<()> {
    let graph = MessageGraph::new("metadata_test")
        .add_message_node("analyzer", |msg: Message| async move {
            let sentiment = if msg.content.contains("happy") {
                "positive"
            } else if msg.content.contains("sad") {
                "negative"
            } else {
                "neutral"
            };

            let mut response = Message::new(
                MessageRole::Assistant,
                format!("Detected sentiment: {}", sentiment),
            );
            response.add_metadata("sentiment", json!(sentiment));
            response.add_metadata("analyzed_at", json!(chrono::Utc::now().to_rfc3339()));
            response
        })
        .build()?;

    // Process message with sentiment
    let msg = Message::new(MessageRole::User, "I'm so happy today!");
    let response = graph.process_message(msg).await?;

    // Check metadata
    assert_eq!(
        response.get_metadata("sentiment"),
        Some(&json!("positive"))
    );
    assert!(response.get_metadata("analyzed_at").is_some());

    Ok(())
}

/// Test message state management
#[tokio::test]
async fn test_message_state() -> Result<()> {
    let graph = MessageGraph::new("state_test")
        .with_state_management()
        .add_message_node("counter", |msg: Message| async move {
            // Get current count from state
            let count = msg.get_state("message_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            let new_count = count + 1;

            let mut response = Message::new(
                MessageRole::Assistant,
                format!("This is message number {}", new_count),
            );
            response.set_state("message_count", json!(new_count));
            response
        })
        .build()?;

    // Process multiple messages
    for i in 1..=5 {
        let msg = Message::new(MessageRole::User, format!("Message {}", i));
        let response = graph.process_message(msg).await?;
        assert!(response.content.contains(&format!("message number {}", i)));
    }

    // Verify state persistence
    let state = graph.get_current_state().await;
    assert_eq!(state.get("message_count"), Some(&json!(5)));

    Ok(())
}

/// Test message transformation pipeline
#[tokio::test]
async fn test_message_pipeline() -> Result<()> {
    let graph = MessageGraph::new("pipeline_test")
        .add_transformer("lowercase", |mut msg: Message| {
            msg.content = msg.content.to_lowercase();
            msg
        })
        .add_transformer("trim", |mut msg: Message| {
            msg.content = msg.content.trim().to_string();
            msg
        })
        .add_transformer("sanitize", |mut msg: Message| {
            msg.content = msg.content.replace("bad", "***");
            msg
        })
        .add_message_node("echo", |msg: Message| async move {
            Message::new(MessageRole::Assistant, msg.content.clone())
        })
        .build()?;

    // Test transformation pipeline
    let input = Message::new(MessageRole::User, "  This is a BAD Test  ");
    let response = graph.process_message(input).await?;

    assert_eq!(response.content, "this is a *** test");

    Ok(())
}

/// Test parallel message processing
#[tokio::test]
async fn test_parallel_processing() -> Result<()> {
    let graph = MessageGraph::new("parallel_test")
        .add_parallel_nodes(vec![
            ("translator", Box::new(|msg: Message| {
                Box::pin(async move {
                    let mut result = msg.clone();
                    result.add_metadata("translated", json!(true));
                    result
                })
            })),
            ("sentiment", Box::new(|msg: Message| {
                Box::pin(async move {
                    let mut result = msg.clone();
                    result.add_metadata("sentiment", json!("positive"));
                    result
                })
            })),
            ("keywords", Box::new(|msg: Message| {
                Box::pin(async move {
                    let mut result = msg.clone();
                    result.add_metadata("keywords", json!(["test", "parallel"]));
                    result
                })
            })),
        ])
        .add_aggregator("combine_results", |messages: Vec<Message>| {
            let mut combined = Message::new(
                MessageRole::Assistant,
                "Processing complete",
            );

            for msg in messages {
                for (key, value) in msg.metadata.iter() {
                    combined.add_metadata(key, value.clone());
                }
            }

            combined
        })
        .build()?;

    let input = Message::new(MessageRole::User, "Test parallel processing");
    let response = graph.process_message(input).await?;

    // Verify all parallel processes ran
    assert!(response.get_metadata("translated").is_some());
    assert!(response.get_metadata("sentiment").is_some());
    assert!(response.get_metadata("keywords").is_some());

    Ok(())
}

/// Test message error handling
#[tokio::test]
async fn test_message_error_handling() -> Result<()> {
    let graph = MessageGraph::new("error_test")
        .add_message_node("validator", |msg: Message| async move {
            if msg.content.is_empty() {
                return Err(MessageError::ValidationFailed("Empty message".to_string()));
            }
            Ok(Message::new(MessageRole::Assistant, "Valid message"))
        })
        .add_error_handler(|error, msg| async move {
            Message::new(
                MessageRole::System,
                format!("Error processing message: {}", error),
            )
        })
        .build()?;

    // Test with empty message
    let empty_msg = Message::new(MessageRole::User, "");
    let response = graph.process_message(empty_msg).await?;

    assert_eq!(response.role, MessageRole::System);
    assert!(response.content.contains("Error processing message"));

    Ok(())
}

/// Test message streaming
#[tokio::test]
async fn test_message_streaming() -> Result<()> {
    let graph = MessageGraph::new("streaming_test")
        .with_streaming_enabled()
        .add_streaming_node("streamer", |msg: Message| {
            Box::pin(async_stream::stream! {
                let words = msg.content.split_whitespace();
                for word in words {
                    yield MessageChunk::new(word.to_string());
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
            })
        })
        .build()?;

    let input = Message::new(MessageRole::User, "Stream this message please");
    let mut stream = graph.stream_message(input).await?;

    let mut chunks = Vec::new();
    while let Some(chunk) = stream.next().await {
        chunks.push(chunk?);
    }

    assert_eq!(chunks.len(), 4); // "Stream", "this", "message", "please"

    Ok(())
}

/// Test message graph composition
#[tokio::test]
async fn test_graph_composition() -> Result<()> {
    // Create sub-graph for authentication
    let auth_graph = MessageGraph::new("auth_subgraph")
        .add_message_node("authenticate", |msg: Message| async move {
            if msg.get_metadata("token").is_some() {
                let mut response = msg.clone();
                response.add_metadata("authenticated", json!(true));
                response
            } else {
                Message::new(MessageRole::System, "Authentication required")
            }
        })
        .build()?;

    // Create main graph that uses auth sub-graph
    let main_graph = MessageGraph::new("main_graph")
        .add_subgraph("auth", auth_graph)
        .add_message_node("process", |msg: Message| async move {
            if msg.get_metadata("authenticated") == Some(&json!(true)) {
                Message::new(MessageRole::Assistant, "Processing authorized request")
            } else {
                Message::new(MessageRole::System, "Unauthorized")
            }
        })
        .add_edge("auth", "process")
        .set_entry_point("auth")
        .build()?;

    // Test with authentication
    let mut auth_msg = Message::new(MessageRole::User, "Process this");
    auth_msg.add_metadata("token", json!("valid_token"));

    let response = main_graph.process_message(auth_msg).await?;
    assert!(response.content.contains("authorized request"));

    // Test without authentication
    let unauth_msg = Message::new(MessageRole::User, "Process this");
    let response = main_graph.process_message(unauth_msg).await?;
    assert!(response.content.contains("Authentication required"));

    Ok(())
}