//! Security module for LangGraph Rust
//!
//! Provides authentication, authorization, and encryption capabilities
//! for secure graph execution.

pub mod auth;
pub mod api_key;
pub mod jwt;
pub mod audit;

pub use auth::{Authenticator, AuthContext, Credentials, Permission, TokenPair};
pub use api_key::{ApiKeyAuthenticator, ApiKeyMetadata};
pub use jwt::JwtAuthenticator;
pub use audit::{AuditLogger, AuditEvent};

use thiserror::Error;

/// Authentication and authorization errors
#[derive(Error, Debug, Clone)]
pub enum AuthError {
    #[error("Invalid credentials provided")]
    InvalidCredentials,

    #[error("Token has expired")]
    TokenExpired,

    #[error("Authentication required for this operation")]
    AuthenticationRequired,

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("Invalid token format: {0}")]
    InvalidToken(String),

    #[error("Token generation failed: {0}")]
    TokenGenerationFailed(String),

    #[error("Audit logging failed: {0}")]
    AuditError(String),
}

// AuthError is already compatible with anyhow via thiserror
