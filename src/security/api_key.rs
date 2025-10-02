//! API Key authentication implementation

use super::auth::{AuthContext, AuthResult, Authenticator, Credentials, TokenPair};
use super::AuthError;
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, warn};

/// API Key authenticator
pub struct ApiKeyAuthenticator {
    /// Storage for API key metadata (key_hash -> metadata)
    keys: Arc<DashMap<String, ApiKeyMetadata>>,

    /// Optional audit logger
    audit_logger: Option<Arc<super::audit::AuditLogger>>,
}

/// Metadata associated with an API key
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyMetadata {
    /// SHA-256 hash of the API key
    pub key_hash: String,

    /// User ID associated with this key
    pub user_id: String,

    /// Roles assigned to this key
    pub roles: Vec<String>,

    /// When the key was created
    pub created_at: DateTime<Utc>,

    /// Optional expiration time
    pub expires_at: Option<DateTime<Utc>>,

    /// Rate limiting configuration
    pub rate_limit: Option<RateLimitConfig>,

    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Maximum requests allowed
    pub max_requests: u32,

    /// Time window for rate limiting
    pub window: Duration,

    /// Current request count
    #[serde(skip)]
    pub request_count: u32,

    /// Window start time
    #[serde(skip)]
    pub window_start: DateTime<Utc>,
}

impl ApiKeyAuthenticator {
    /// Create a new API key authenticator
    pub fn new() -> Self {
        Self {
            keys: Arc::new(DashMap::new()),
            audit_logger: None,
        }
    }

    /// Set the audit logger
    pub fn set_audit_logger(&mut self, logger: Arc<super::audit::AuditLogger>) {
        self.audit_logger = Some(logger);
    }

    /// Register a new API key
    pub async fn register_key(
        &self,
        api_key: &str,
        user_id: impl Into<String>,
        roles: Vec<String>,
        expires_at: Option<DateTime<Utc>>,
    ) -> AuthResult<String> {
        let key_hash = self.hash_key(api_key);
        let user_id = user_id.into();

        let metadata = ApiKeyMetadata {
            key_hash: key_hash.clone(),
            user_id: user_id.clone(),
            roles,
            created_at: Utc::now(),
            expires_at,
            rate_limit: None,
            metadata: HashMap::new(),
        };

        self.keys.insert(key_hash.clone(), metadata);

        debug!("Registered API key for user: {}", user_id);

        Ok(key_hash)
    }

    /// Register a new API key with rate limiting
    pub async fn register_key_with_rate_limit(
        &self,
        api_key: &str,
        user_id: impl Into<String>,
        roles: Vec<String>,
        expires_at: Option<DateTime<Utc>>,
        max_requests: u32,
        window: Duration,
    ) -> AuthResult<String> {
        let key_hash = self.hash_key(api_key);
        let user_id = user_id.into();

        let rate_limit = RateLimitConfig {
            max_requests,
            window,
            request_count: 0,
            window_start: Utc::now(),
        };

        let metadata = ApiKeyMetadata {
            key_hash: key_hash.clone(),
            user_id: user_id.clone(),
            roles,
            created_at: Utc::now(),
            expires_at,
            rate_limit: Some(rate_limit),
            metadata: HashMap::new(),
        };

        self.keys.insert(key_hash.clone(), metadata);

        debug!("Registered rate-limited API key for user: {}", user_id);

        Ok(key_hash)
    }

    /// Hash an API key using SHA-256
    fn hash_key(&self, api_key: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(api_key.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Check rate limit for a key
    fn check_rate_limit(&self, metadata: &mut ApiKeyMetadata) -> AuthResult<()> {
        if let Some(ref mut rate_limit) = metadata.rate_limit {
            let now = Utc::now();

            // Reset window if expired
            if now >= rate_limit.window_start + rate_limit.window {
                rate_limit.request_count = 0;
                rate_limit.window_start = now;
            }

            // Check if rate limit exceeded
            if rate_limit.request_count >= rate_limit.max_requests {
                warn!("Rate limit exceeded for user: {}", metadata.user_id);
                return Err(AuthError::RateLimitExceeded);
            }

            // Increment request count
            rate_limit.request_count += 1;
        }

        Ok(())
    }

    /// Log authentication event
    async fn log_auth_event(&self, user_id: &str, success: bool) {
        if let Some(ref logger) = self.audit_logger {
            let event_type = if success {
                "authentication_success"
            } else {
                "authentication_failure"
            };

            logger.log_event(event_type, user_id, HashMap::new()).await;
        }
    }
}

impl Default for ApiKeyAuthenticator {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Authenticator for ApiKeyAuthenticator {
    async fn authenticate(&self, credentials: &Credentials) -> AuthResult<AuthContext> {
        let api_key = match credentials {
            Credentials::ApiKey(key) => key,
            _ => return Err(AuthError::InvalidCredentials),
        };

        let key_hash = self.hash_key(api_key);

        // Look up key metadata
        let mut metadata = self.keys.get_mut(&key_hash)
            .ok_or_else(|| {
                warn!("Authentication failed: invalid API key");
                AuthError::InvalidCredentials
            })?;

        // Check expiration
        if let Some(expires_at) = metadata.expires_at {
            if Utc::now() > expires_at {
                warn!("Authentication failed: expired API key for user {}", metadata.user_id);
                self.log_auth_event(&metadata.user_id, false).await;
                return Err(AuthError::TokenExpired);
            }
        }

        // Check rate limit
        self.check_rate_limit(&mut metadata)?;

        // Create auth context
        let mut auth_context = AuthContext::new(
            metadata.user_id.clone(),
            metadata.roles.clone(),
        );

        // Set expiration based on key expiration or default 24 hours
        if let Some(expires_at) = metadata.expires_at {
            auth_context.expires_at = expires_at;
        }

        debug!("API key authentication successful for user: {}", metadata.user_id);
        self.log_auth_event(&metadata.user_id, true).await;

        Ok(auth_context)
    }

    async fn validate_token(&self, token: &str) -> AuthResult<AuthContext> {
        // For API keys, validation is the same as authentication
        self.authenticate(&Credentials::ApiKey(token.to_string())).await
    }

    async fn refresh_token(&self, _refresh_token: &str) -> AuthResult<TokenPair> {
        // API keys don't support token refresh
        Err(AuthError::InvalidToken("API keys do not support token refresh".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_api_key_hashing() {
        let authenticator = ApiKeyAuthenticator::new();
        let key = "test-key-123";

        let hash1 = authenticator.hash_key(key);
        let hash2 = authenticator.hash_key(key);

        // Same key should produce same hash
        assert_eq!(hash1, hash2);

        // Hash should be deterministic and hexadecimal
        assert_eq!(hash1.len(), 64); // SHA-256 produces 64 hex characters
    }

    #[tokio::test]
    async fn test_key_registration() {
        let authenticator = ApiKeyAuthenticator::new();

        let result = authenticator.register_key(
            "test-key",
            "user123",
            vec!["admin".to_string()],
            None,
        ).await;

        assert!(result.is_ok());
    }
}
