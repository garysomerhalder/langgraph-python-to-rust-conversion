//! JWT (JSON Web Token) authentication implementation

use super::auth::{AuthContext, AuthResult, Authenticator, Credentials, Permission, TokenPair};
use super::metrics::SecurityMetrics;
use super::AuthError;
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use dashmap::{DashMap, DashSet};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{debug, warn};
use uuid::Uuid;

/// JWT authentication implementation
pub struct JwtAuthenticator {
    /// Encoding key for signing tokens
    encoding_key: Arc<RwLock<EncodingKey>>,

    /// Decoding key for verifying tokens
    decoding_key: Arc<RwLock<DecodingKey>>,

    /// Token issuer
    issuer: String,

    /// Token audience
    audience: String,

    /// Access token expiration duration
    access_token_expiration: Duration,

    /// Refresh token expiration duration
    refresh_token_expiration: Duration,

    /// Validation rules
    validation: Validation,

    /// Revoked token IDs (jti) - for token rotation security
    revoked_tokens: Arc<DashSet<String>>,

    /// Token rotation tracking: maps old refresh token JTI to expiration time
    rotation_tracker: Arc<DashMap<String, DateTime<Utc>>>,
}

/// JWT claims structure
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Claims {
    /// Subject (user ID)
    sub: String,

    /// Issuer
    iss: String,

    /// Audience
    aud: String,

    /// Expiration time (Unix timestamp)
    exp: i64,

    /// Issued at (Unix timestamp)
    iat: i64,

    /// JWT ID (unique identifier)
    jti: String,

    /// User roles
    roles: Vec<String>,

    /// Custom metadata
    metadata: HashMap<String, Value>,

    /// Token type (access or refresh)
    #[serde(default)]
    token_type: String,
}

impl JwtAuthenticator {
    /// Create a new JWT authenticator
    pub fn new(
        secret: &[u8],
        issuer: impl Into<String>,
        audience: impl Into<String>,
        access_token_expiration: Duration,
    ) -> Self {
        let mut validation = Validation::new(Algorithm::HS256);
        let issuer_str = issuer.into();
        let audience_str = audience.into();

        validation.set_issuer(&[&issuer_str]);
        validation.set_audience(&[&audience_str]);

        Self {
            encoding_key: Arc::new(RwLock::new(EncodingKey::from_secret(secret))),
            decoding_key: Arc::new(RwLock::new(DecodingKey::from_secret(secret))),
            issuer: issuer_str,
            audience: audience_str,
            access_token_expiration,
            refresh_token_expiration: Duration::days(30), // Default 30 days for refresh tokens
            validation,
            revoked_tokens: Arc::new(DashSet::new()),
            rotation_tracker: Arc::new(DashMap::new()),
        }
    }

    /// Generate a JWT access token
    pub async fn generate_token(
        &self,
        user_id: impl Into<String>,
        roles: Vec<String>,
        metadata: HashMap<String, Value>,
    ) -> AuthResult<String> {
        let now = Utc::now();
        let user_id = user_id.into();

        let claims = Claims {
            sub: user_id.clone(),
            iss: self.issuer.clone(),
            aud: self.audience.clone(),
            exp: (now + self.access_token_expiration).timestamp(),
            iat: now.timestamp(),
            jti: Uuid::new_v4().to_string(),
            roles,
            metadata,
            token_type: "access".to_string(),
        };

        let encoding_key = self.encoding_key.read().await;
        let token = encode(&Header::default(), &claims, &*encoding_key)
            .map_err(|e| {
                warn!("Failed to generate JWT token: {}", e);
                AuthError::TokenGenerationFailed(e.to_string())
            })?;

        debug!("Generated JWT access token for user: {}", user_id);

        Ok(token)
    }

    /// Generate a JWT refresh token
    async fn generate_refresh_token(
        &self,
        user_id: impl Into<String>,
        roles: Vec<String>,
        metadata: HashMap<String, Value>,
    ) -> AuthResult<String> {
        let now = Utc::now();
        let user_id = user_id.into();

        let claims = Claims {
            sub: user_id.clone(),
            iss: self.issuer.clone(),
            aud: self.audience.clone(),
            exp: (now + self.refresh_token_expiration).timestamp(),
            iat: now.timestamp(),
            jti: Uuid::new_v4().to_string(),
            roles,
            metadata,
            token_type: "refresh".to_string(),
        };

        let encoding_key = self.encoding_key.read().await;
        let token = encode(&Header::default(), &claims, &*encoding_key)
            .map_err(|e| {
                warn!("Failed to generate refresh token: {}", e);
                AuthError::TokenGenerationFailed(e.to_string())
            })?;

        debug!("Generated JWT refresh token for user: {}", user_id);

        Ok(token)
    }

    /// Generate both access and refresh tokens
    pub async fn generate_token_pair(
        &self,
        user_id: impl Into<String>,
        roles: Vec<String>,
        metadata: HashMap<String, Value>,
    ) -> AuthResult<TokenPair> {
        let user_id_str = user_id.into();

        let access_token = self.generate_token(
            user_id_str.clone(),
            roles.clone(),
            metadata.clone(),
        ).await?;

        let refresh_token = self.generate_refresh_token(
            user_id_str,
            roles,
            metadata,
        ).await?;

        Ok(TokenPair::new(
            access_token,
            refresh_token,
            self.access_token_expiration.num_seconds(),
        ))
    }

    /// Decode and validate a JWT token
    async fn decode_token(&self, token: &str) -> AuthResult<Claims> {
        let decoding_key = self.decoding_key.read().await;

        let token_data = decode::<Claims>(token, &*decoding_key, &self.validation)
            .map_err(|e| {
                match e.kind() {
                    jsonwebtoken::errors::ErrorKind::ExpiredSignature => {
                        warn!("Token validation failed: expired signature");
                        AuthError::TokenExpired
                    }
                    _ => {
                        warn!("Token validation failed: {}", e);
                        AuthError::InvalidToken(e.to_string())
                    }
                }
            })?;

        Ok(token_data.claims)
    }

    /// Convert claims to AuthContext
    fn claims_to_auth_context(&self, claims: Claims) -> AuthContext {
        let mut auth_context = AuthContext::new(claims.sub, claims.roles);

        auth_context.expires_at = chrono::DateTime::from_timestamp(claims.exp, 0)
            .unwrap_or_else(|| Utc::now() + self.access_token_expiration);
        auth_context.issued_at = chrono::DateTime::from_timestamp(claims.iat, 0)
            .unwrap_or_else(|| Utc::now());
        auth_context.metadata = claims.metadata;

        // Add default permissions based on roles
        if auth_context.has_role("admin") {
            auth_context.permissions.insert(Permission::ExecuteGraph);
            auth_context.permissions.insert(Permission::CreateGraph);
            auth_context.permissions.insert(Permission::ModifyGraph);
            auth_context.permissions.insert(Permission::DeleteGraph);
            auth_context.permissions.insert(Permission::ReadState);
            auth_context.permissions.insert(Permission::WriteState);
            auth_context.permissions.insert(Permission::ManageCheckpoints);
            auth_context.permissions.insert(Permission::ViewAuditLogs);
        } else if auth_context.has_role("developer") || auth_context.has_role("user") {
            auth_context.permissions.insert(Permission::ExecuteGraph);
            auth_context.permissions.insert(Permission::ReadState);
        }

        auth_context
    }

    /// Clean up expired tokens from revocation list (prevents unbounded growth)
    fn cleanup_expired_tokens(&self) {
        let now = Utc::now();
        let mut to_remove = Vec::new();

        // Find expired tokens
        for entry in self.rotation_tracker.iter() {
            if *entry.value() < now {
                to_remove.push(entry.key().clone());
            }
        }

        // Remove expired tokens
        for jti in to_remove {
            self.rotation_tracker.remove(&jti);
            self.revoked_tokens.remove(&jti);
        }
    }

    /// Manually revoke a token by its JTI (for logout or security events)
    pub fn revoke_token(&self, jti: impl Into<String>) {
        let jti_str = jti.into();
        self.revoked_tokens.insert(jti_str.clone());
        debug!("Manually revoked token: {}", jti_str);
    }

    /// Check if a token is revoked
    pub fn is_token_revoked(&self, jti: &str) -> bool {
        self.revoked_tokens.contains(jti)
    }
}

#[async_trait]
impl Authenticator for JwtAuthenticator {
    async fn authenticate(&self, credentials: &Credentials) -> AuthResult<AuthContext> {
        let token = match credentials {
            Credentials::JwtToken(token) => token,
            _ => return Err(AuthError::InvalidCredentials),
        };

        let claims = self.decode_token(token).await?;

        // Ensure this is an access token
        if claims.token_type != "access" {
            return Err(AuthError::InvalidToken("Expected access token".to_string()));
        }

        Ok(self.claims_to_auth_context(claims))
    }

    async fn validate_token(&self, token: &str) -> AuthResult<AuthContext> {
        let claims = self.decode_token(token).await?;

        // Accept both access and refresh tokens for validation
        Ok(self.claims_to_auth_context(claims))
    }

    async fn refresh_token(&self, refresh_token: &str) -> AuthResult<TokenPair> {
        let start = Instant::now();

        let claims = self.decode_token(refresh_token).await.map_err(|e| {
            SecurityMetrics::record_token_refresh(false);
            e
        })?;

        // Ensure this is a refresh token
        if claims.token_type != "refresh" {
            SecurityMetrics::record_token_refresh(false);
            return Err(AuthError::InvalidToken("Expected refresh token".to_string()));
        }

        // Check if this refresh token has already been used (rotation attack detection)
        if self.revoked_tokens.contains(&claims.jti) {
            warn!("Attempted reuse of revoked refresh token: {} for user: {}", claims.jti, claims.sub);
            SecurityMetrics::record_security_event("token_reuse_attack", "high");
            SecurityMetrics::record_token_refresh(false);
            return Err(AuthError::InvalidToken("Refresh token has been revoked".to_string()));
        }

        // Revoke the old refresh token (single-use pattern for rotation)
        self.revoked_tokens.insert(claims.jti.clone());
        SecurityMetrics::record_token_revocation("rotation");

        // Track rotation for audit purposes
        self.rotation_tracker.insert(
            claims.jti.clone(),
            Utc::now() + self.refresh_token_expiration,
        );

        // Clean up expired tokens from revocation list periodically
        self.cleanup_expired_tokens();

        // Generate new token pair using the same user info
        let new_token_pair = self.generate_token_pair(
            claims.sub.clone(),
            claims.roles.clone(),
            claims.metadata.clone(),
        ).await?;

        let duration = start.elapsed().as_secs_f64();
        SecurityMetrics::record_token_refresh(true);
        SecurityMetrics::record_token_operation("refresh", true);

        debug!("Rotated refresh token for user: {}, old JTI: {} ({}s)", claims.sub, claims.jti, duration);

        Ok(new_token_pair)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_token_generation() {
        let authenticator = JwtAuthenticator::new(
            b"test-secret-key-minimum-32-bytes",
            "test-issuer",
            "test-audience",
            Duration::minutes(15),
        );

        let token = authenticator.generate_token(
            "user123",
            vec!["admin".to_string()],
            HashMap::new(),
        ).await;

        assert!(token.is_ok());
    }

    #[tokio::test]
    async fn test_token_validation() {
        let authenticator = JwtAuthenticator::new(
            b"test-secret-key-minimum-32-bytes",
            "test-issuer",
            "test-audience",
            Duration::hours(1),
        );

        let token = authenticator.generate_token(
            "user456",
            vec!["developer".to_string()],
            HashMap::new(),
        ).await.unwrap();

        let result = authenticator.validate_token(&token).await;
        assert!(result.is_ok());

        let auth_context = result.unwrap();
        assert_eq!(auth_context.user_id, "user456");
        assert!(auth_context.has_role("developer"));
    }
}
