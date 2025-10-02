//! Core authentication traits and types

use super::AuthError;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// Result type for authentication operations
pub type AuthResult<T> = Result<T, AuthError>;

/// Core authentication trait implemented by all authenticators
#[async_trait]
pub trait Authenticator: Send + Sync {
    /// Authenticate using provided credentials
    async fn authenticate(&self, credentials: &Credentials) -> AuthResult<AuthContext>;

    /// Validate an existing token and return auth context
    async fn validate_token(&self, token: &str) -> AuthResult<AuthContext>;

    /// Refresh an access token using a refresh token
    async fn refresh_token(&self, refresh_token: &str) -> AuthResult<TokenPair>;
}

/// Authentication context containing validated user information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthContext {
    /// Unique user identifier
    pub user_id: String,

    /// User roles (e.g., "admin", "user", "developer")
    pub roles: Vec<String>,

    /// Granular permissions
    pub permissions: HashSet<Permission>,

    /// Additional metadata
    pub metadata: HashMap<String, Value>,

    /// When this context expires
    pub expires_at: DateTime<Utc>,

    /// When this context was issued
    pub issued_at: DateTime<Utc>,
}

impl AuthContext {
    /// Create a new authentication context
    pub fn new(user_id: impl Into<String>, roles: Vec<String>) -> Self {
        let now = Utc::now();
        Self {
            user_id: user_id.into(),
            roles,
            permissions: HashSet::new(),
            metadata: HashMap::new(),
            expires_at: now + chrono::Duration::hours(24),
            issued_at: now,
        }
    }

    /// Check if context has a specific role
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r == role)
    }

    /// Check if context has a specific permission
    pub fn has_permission(&self, permission: &Permission) -> bool {
        self.permissions.contains(permission)
    }

    /// Check if context is expired
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }
}

/// Credentials for authentication
#[derive(Debug, Clone)]
pub enum Credentials {
    /// API key authentication
    ApiKey(String),

    /// JWT token authentication
    JwtToken(String),

    /// Username and password authentication
    UsernamePassword {
        username: String,
        password: String,
    },
}

/// Granular permission enum
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    /// Execute graphs
    ExecuteGraph,

    /// Create graphs
    CreateGraph,

    /// Modify graphs
    ModifyGraph,

    /// Delete graphs
    DeleteGraph,

    /// Read state
    ReadState,

    /// Write state
    WriteState,

    /// Manage checkpoints
    ManageCheckpoints,

    /// View audit logs
    ViewAuditLogs,

    /// Custom permission
    Custom(String),
}

/// Token pair for authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPair {
    /// Short-lived access token
    pub access_token: String,

    /// Long-lived refresh token
    pub refresh_token: String,

    /// Token type (usually "Bearer")
    pub token_type: String,

    /// Access token expiration in seconds
    pub expires_in: i64,
}

impl TokenPair {
    /// Create a new token pair
    pub fn new(access_token: String, refresh_token: String, expires_in: i64) -> Self {
        Self {
            access_token,
            refresh_token,
            token_type: "Bearer".to_string(),
            expires_in,
        }
    }
}
