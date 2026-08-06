//! AuditService — redacted, intentional audit trail over LogService.

use serde_json::{Value, json};
use uuid::Uuid;

use crate::infrastructure::logging::LogService;

/// High-value operations that should always leave an audit trail.
#[derive(Debug, Clone, Copy)]
pub enum AuditAction {
    AdminEventUpdate,
    UnsafeSqlEnabled,
    UnsafeSqlExecuted,
    TokenRotated,
    TeamBanned,
    TeamUnbanned,
    ScoreAdjusted,
    NetworkApplyFailed,
    PrecheckRun,
    GameboxReset,
    ResourceRecovery,
    InternalAuthFailed,
}

impl AuditAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AdminEventUpdate => "ADMIN_EVENT_UPDATE",
            Self::UnsafeSqlEnabled => "UNSAFE_SQL_ENABLED",
            Self::UnsafeSqlExecuted => "UNSAFE_SQL_EXECUTED",
            Self::TokenRotated => "TOKEN_ROTATED",
            Self::TeamBanned => "TEAM_BANNED",
            Self::TeamUnbanned => "TEAM_UNBANNED",
            Self::ScoreAdjusted => "SCORE_ADJUSTED",
            Self::NetworkApplyFailed => "NETWORK_APPLY_FAILED",
            Self::PrecheckRun => "PRECHECK_RUN",
            Self::GameboxReset => "GAMEBOX_RESET",
            Self::ResourceRecovery => "RESOURCE_RECOVERY",
            Self::InternalAuthFailed => "INTERNAL_AUTH_FAILED",
        }
    }

    pub fn category(self) -> &'static str {
        match self {
            Self::UnsafeSqlEnabled | Self::UnsafeSqlExecuted => "DATABASE",
            Self::TokenRotated | Self::InternalAuthFailed => "AWD_AUTH",
            Self::TeamBanned | Self::TeamUnbanned | Self::ScoreAdjusted => "AWD_SCORE",
            Self::NetworkApplyFailed
            | Self::PrecheckRun
            | Self::GameboxReset
            | Self::ResourceRecovery => "AWD_OPS",
            Self::AdminEventUpdate => "ADMIN",
        }
    }
}

/// Thin façade that always writes structured, redacted audit rows.
#[derive(Clone)]
pub struct AuditService {
    log: LogService,
}

impl AuditService {
    pub fn new(log: LogService) -> Self {
        Self { log }
    }

    /// Record an audit event. `details` must already be free of secrets
    /// (callers redact tokens/flags before calling).
    pub async fn record(
        &self,
        action: AuditAction,
        message: &str,
        details: Value,
        actor_user_id: Option<Uuid>,
        actor_superadmin_id: Option<Uuid>,
        request: Option<&actix_web::HttpRequest>,
    ) {
        let safe_details = redact_sensitive_keys(details);
        self.log
            .add_log(
                "INFO",
                action.category(),
                action.as_str(),
                message,
                safe_details,
                actor_user_id,
                actor_superadmin_id,
                request,
            )
            .await;
    }
}

/// Best-effort redaction of common secret field names in JSON objects.
fn redact_sensitive_keys(value: Value) -> Value {
    const SENSITIVE: &[&str] = &[
        "token",
        "flag",
        "password",
        "secret",
        "private_key",
        "presared_key",
        "preshared_key",
        "authorization",
        "jwt",
    ];

    match value {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                let lower = k.to_lowercase();
                if SENSITIVE.iter().any(|s| lower.contains(s)) {
                    out.insert(k, json!("***"));
                } else {
                    out.insert(k, redact_sensitive_keys(v));
                }
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(redact_sensitive_keys).collect()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_nested_token_fields() {
        let v = json!({
            "event_id": "x",
            "token": "super-secret",
            "nested": { "private_key": "pk", "ok": 1 }
        });
        let r = redact_sensitive_keys(v);
        assert_eq!(r["token"], "***");
        assert_eq!(r["nested"]["private_key"], "***");
        assert_eq!(r["nested"]["ok"], 1);
        assert_eq!(r["event_id"], "x");
    }
}
