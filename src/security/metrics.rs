//! Security metrics collection for authentication and authorization
//!
//! Provides Prometheus-compatible metrics for monitoring authentication
//! events, authorization decisions, and security incidents.

use lazy_static::lazy_static;
use prometheus::{
    register_counter_vec, register_histogram_vec, register_int_counter_vec,
    CounterVec, HistogramVec, IntCounterVec, Encoder, TextEncoder,
};

lazy_static! {
    /// Counter for authentication attempts
    pub static ref AUTH_ATTEMPTS: IntCounterVec = register_int_counter_vec!(
        "langgraph_auth_attempts_total",
        "Total number of authentication attempts",
        &["method", "status"]
    ).unwrap();

    /// Counter for authentication failures by reason
    pub static ref AUTH_FAILURES: IntCounterVec = register_int_counter_vec!(
        "langgraph_auth_failures_total",
        "Total number of authentication failures",
        &["method", "reason"]
    ).unwrap();

    /// Histogram for authentication duration
    pub static ref AUTH_DURATION: HistogramVec = register_histogram_vec!(
        "langgraph_auth_duration_seconds",
        "Authentication operation duration in seconds",
        &["method"],
        vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0]
    ).unwrap();

    /// Counter for token operations
    pub static ref TOKEN_OPERATIONS: IntCounterVec = register_int_counter_vec!(
        "langgraph_token_operations_total",
        "Total number of token operations",
        &["operation", "status"]
    ).unwrap();

    /// Counter for token refresh operations
    pub static ref TOKEN_REFRESH: IntCounterVec = register_int_counter_vec!(
        "langgraph_token_refresh_total",
        "Total number of token refresh operations",
        &["status"]
    ).unwrap();

    /// Counter for revoked tokens
    pub static ref TOKENS_REVOKED: IntCounterVec = register_int_counter_vec!(
        "langgraph_tokens_revoked_total",
        "Total number of revoked tokens",
        &["reason"]
    ).unwrap();

    /// Counter for rate limit hits
    pub static ref RATE_LIMIT_HITS: IntCounterVec = register_int_counter_vec!(
        "langgraph_auth_rate_limit_hits_total",
        "Total number of rate limit hits during authentication",
        &["user_id"]
    ).unwrap();

    /// Counter for security events
    pub static ref SECURITY_EVENTS: IntCounterVec = register_int_counter_vec!(
        "langgraph_security_events_total",
        "Total number of security events",
        &["event_type", "severity"]
    ).unwrap();

    /// Counter for authorization decisions
    pub static ref AUTHZ_DECISIONS: IntCounterVec = register_int_counter_vec!(
        "langgraph_authz_decisions_total",
        "Total number of authorization decisions",
        &["action", "decision"]
    ).unwrap();

    /// Counter for permission denials
    pub static ref PERMISSION_DENIALS: IntCounterVec = register_int_counter_vec!(
        "langgraph_permission_denials_total",
        "Total number of permission denials",
        &["permission", "user_role"]
    ).unwrap();

    /// Histogram for Argon2 hashing duration (intentionally slow)
    pub static ref ARGON2_DURATION: HistogramVec = register_histogram_vec!(
        "langgraph_argon2_hash_duration_seconds",
        "Argon2 password hashing duration in seconds",
        &["operation"],
        vec![0.01, 0.05, 0.1, 0.5, 1.0, 2.0, 5.0, 10.0]
    ).unwrap();

    /// Counter for audit log writes
    pub static ref AUDIT_LOGS_WRITTEN: IntCounterVec = register_int_counter_vec!(
        "langgraph_audit_logs_written_total",
        "Total number of audit log entries written",
        &["event_type"]
    ).unwrap();
}

/// Security metrics collector
pub struct SecurityMetrics;

impl SecurityMetrics {
    /// Record an authentication attempt
    pub fn record_auth_attempt(method: &str, success: bool) {
        let status = if success { "success" } else { "failure" };
        AUTH_ATTEMPTS.with_label_values(&[method, status]).inc();
    }

    /// Record an authentication failure
    pub fn record_auth_failure(method: &str, reason: &str) {
        AUTH_FAILURES.with_label_values(&[method, reason]).inc();
    }

    /// Record a token operation
    pub fn record_token_operation(operation: &str, success: bool) {
        let status = if success { "success" } else { "failure" };
        TOKEN_OPERATIONS.with_label_values(&[operation, status]).inc();
    }

    /// Record a token refresh
    pub fn record_token_refresh(success: bool) {
        let status = if success { "success" } else { "failure" };
        TOKEN_REFRESH.with_label_values(&[status]).inc();
    }

    /// Record a token revocation
    pub fn record_token_revocation(reason: &str) {
        TOKENS_REVOKED.with_label_values(&[reason]).inc();
    }

    /// Record a rate limit hit
    pub fn record_rate_limit_hit(user_id: &str) {
        RATE_LIMIT_HITS.with_label_values(&[user_id]).inc();
    }

    /// Record a security event
    pub fn record_security_event(event_type: &str, severity: &str) {
        SECURITY_EVENTS.with_label_values(&[event_type, severity]).inc();
    }

    /// Record an authorization decision
    pub fn record_authz_decision(action: &str, allowed: bool) {
        let decision = if allowed { "allowed" } else { "denied" };
        AUTHZ_DECISIONS.with_label_values(&[action, decision]).inc();
    }

    /// Record a permission denial
    pub fn record_permission_denial(permission: &str, role: &str) {
        PERMISSION_DENIALS.with_label_values(&[permission, role]).inc();
    }

    /// Record an audit log write
    pub fn record_audit_log(event_type: &str) {
        AUDIT_LOGS_WRITTEN.with_label_values(&[event_type]).inc();
    }

    /// Get all security metrics in Prometheus text format
    pub fn export_metrics() -> Result<String, String> {
        let encoder = TextEncoder::new();
        let metric_families = prometheus::gather();
        let mut buffer = Vec::new();
        encoder
            .encode(&metric_families, &mut buffer)
            .map_err(|e| format!("Failed to encode metrics: {}", e))?;
        String::from_utf8(buffer).map_err(|e| format!("Failed to convert metrics to UTF-8: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_auth_attempt() {
        SecurityMetrics::record_auth_attempt("api_key", true);
        SecurityMetrics::record_auth_attempt("jwt", false);
        // Metrics are recorded successfully
    }

    #[test]
    fn test_export_metrics() {
        let metrics = SecurityMetrics::export_metrics();
        assert!(metrics.is_ok());
        let metrics_text = metrics.unwrap();
        assert!(metrics_text.contains("langgraph_auth_attempts_total"));
    }
}
