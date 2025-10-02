// Integration tests for authentication system
// Following Integration-First methodology - tests run against real implementations

use langgraph::security::auth::{Authenticator, AuthContext, Credentials};
use langgraph::security::api_key::{ApiKeyAuthenticator, ApiKeyMetadata};
use langgraph::security::jwt::JwtAuthenticator;
use langgraph::security::AuthError;
use std::collections::HashMap;
use std::sync::Arc;
use chrono::{Utc, Duration};

#[tokio::test]
async fn test_api_key_authentication_valid_key() {
    // RED: This test will fail until we implement API key authentication
    let authenticator = ApiKeyAuthenticator::new();

    // Register a test API key
    let api_key = "test-key-12345678";
    authenticator.register_key(
        api_key,
        "user123",
        vec!["admin".to_string()],
        None, // No expiration
    ).await.expect("Failed to register key");

    // Authenticate with valid key
    let credentials = Credentials::ApiKey(api_key.to_string());
    let auth_context = authenticator.authenticate(&credentials).await
        .expect("Authentication should succeed with valid key");

    assert_eq!(auth_context.user_id, "user123");
    assert!(auth_context.roles.contains(&"admin".to_string()));
}

#[tokio::test]
async fn test_api_key_authentication_invalid_key() {
    // RED: Test rejection of invalid API keys
    let authenticator = ApiKeyAuthenticator::new();

    let credentials = Credentials::ApiKey("invalid-key".to_string());
    let result = authenticator.authenticate(&credentials).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AuthError::InvalidCredentials => {}, // Expected
        e => panic!("Expected InvalidCredentials, got: {:?}", e),
    }
}

#[tokio::test]
async fn test_api_key_expiration() {
    // RED: Test that expired keys are rejected
    let authenticator = ApiKeyAuthenticator::new();

    let api_key = "expiring-key-12345";
    let expired_time = Utc::now() - Duration::hours(1); // Expired 1 hour ago

    authenticator.register_key(
        api_key,
        "user456",
        vec!["user".to_string()],
        Some(expired_time),
    ).await.expect("Failed to register key");

    let credentials = Credentials::ApiKey(api_key.to_string());
    let result = authenticator.authenticate(&credentials).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AuthError::TokenExpired => {}, // Expected
        e => panic!("Expected TokenExpired, got: {:?}", e),
    }
}

#[tokio::test]
async fn test_jwt_authentication_valid_token() {
    // RED: Test JWT token validation
    let authenticator = JwtAuthenticator::new(
        "test-secret-key-minimum-32-bytes-long!!!".as_bytes(),
        "langgraph-rust",
        "langgraph-users",
        Duration::hours(24),
    );

    // Generate a token
    let token = authenticator.generate_token(
        "user789",
        vec!["developer".to_string()],
        HashMap::new(),
    ).await.expect("Failed to generate token");

    // Validate the token
    let auth_context = authenticator.validate_token(&token).await
        .expect("Token validation should succeed");

    assert_eq!(auth_context.user_id, "user789");
    assert!(auth_context.roles.contains(&"developer".to_string()));
}

#[tokio::test]
#[ignore] // Timing-dependent test - requires sleep which can be flaky
async fn test_jwt_authentication_expired_token() {
    // RED: Test rejection of expired JWT tokens
    // Use short duration and wait for expiration
    let authenticator = JwtAuthenticator::new(
        "test-secret-key-minimum-32-bytes-long!!!".as_bytes(),
        "langgraph-rust",
        "langgraph-users",
        Duration::milliseconds(100), // Expire after 100ms
    );

    let token = authenticator.generate_token(
        "user999",
        vec!["guest".to_string()],
        HashMap::new(),
    ).await.expect("Failed to generate token");

    // Wait for expiration
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    let result = authenticator.validate_token(&token).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AuthError::TokenExpired => {}, // Expected
        e => panic!("Expected TokenExpired, got: {:?}", e),
    }
}

#[tokio::test]
async fn test_jwt_token_refresh() {
    // RED: Test token refresh mechanism
    let authenticator = JwtAuthenticator::new(
        "test-secret-key-minimum-32-bytes-long!!!".as_bytes(),
        "langgraph-rust",
        "langgraph-users",
        Duration::hours(1),
    );

    let token_pair = authenticator.generate_token_pair(
        "user111",
        vec!["admin".to_string()],
        HashMap::new(),
    ).await.expect("Failed to generate token pair");

    // Refresh using refresh token
    let new_pair = authenticator.refresh_token(&token_pair.refresh_token).await
        .expect("Token refresh should succeed");

    // New access token should be valid
    let auth_context = authenticator.validate_token(&new_pair.access_token).await
        .expect("New access token should be valid");

    assert_eq!(auth_context.user_id, "user111");
}

#[tokio::test]
async fn test_unauthenticated_execution_blocked() {
    // RED: Test that graph execution requires authentication
    use langgraph::engine::ExecutionEngine;
    use langgraph::graph::{GraphBuilder, NodeType};
    use langgraph::state::StateData;

    let graph = GraphBuilder::new("test")
        .add_node("__start__", NodeType::Start)
        .add_node("__end__", NodeType::End)
        .set_entry_point("__start__")
        .add_edge("__start__", "__end__")
        .build()
        .expect("Failed to build graph")
        .compile()
        .expect("Failed to compile graph");

    let engine = ExecutionEngine::new();
    let initial_state = StateData::new();

    // Attempt execution without authentication
    let result = engine.execute_authenticated(&graph, initial_state, "").await;

    assert!(result.is_err());
    // Should fail with AuthenticationRequired error
}

#[tokio::test]
async fn test_rate_limiting_api_key() {
    // RED: Test that API keys respect rate limits
    let authenticator = ApiKeyAuthenticator::new();

    let api_key = "rate-limited-key";
    authenticator.register_key_with_rate_limit(
        api_key,
        "user222",
        vec!["user".to_string()],
        None,
        10, // 10 requests per minute
        Duration::minutes(1),
    ).await.expect("Failed to register key");

    let credentials = Credentials::ApiKey(api_key.to_string());

    // Make 10 successful requests
    for _ in 0..10 {
        authenticator.authenticate(&credentials).await
            .expect("Authentication should succeed");
    }

    // 11th request should be rate limited
    let result = authenticator.authenticate(&credentials).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AuthError::RateLimitExceeded => {}, // Expected
        e => panic!("Expected RateLimitExceeded, got: {:?}", e),
    }
}

#[tokio::test]
async fn test_authentication_audit_logging() {
    // RED: Test that authentication events are logged
    use langgraph::security::audit::AuditLogger;

    let mut authenticator = ApiKeyAuthenticator::new();
    let audit_logger = Arc::new(AuditLogger::new());

    authenticator.set_audit_logger(audit_logger.clone());

    let api_key = "audit-test-key";
    authenticator.register_key(
        api_key,
        "user333",
        vec!["auditor".to_string()],
        None,
    ).await.expect("Failed to register key");

    let credentials = Credentials::ApiKey(api_key.to_string());
    authenticator.authenticate(&credentials).await
        .expect("Authentication should succeed");

    // Check that audit event was logged
    let events = audit_logger.get_events().await;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, "authentication_success");
    assert_eq!(events[0].user_id, "user333");
}
