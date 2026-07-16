//! Database row structs shared across handler modules.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct WebhookEvent {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub source: String,
    pub headers: serde_json::Value,
    pub body: serde_json::Value,
    pub status: String,
    pub target_url: String,
    pub retry_count: i32,
    pub max_retries: i32,
    pub created_at: chrono::NaiveDateTime,
    pub response_status: Option<i32>,
    pub response_headers: Option<serde_json::Value>,
    pub response_body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ForwardRule {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub name: String,
    pub source_pattern: String,
    pub target_url: String,
    pub method: String,
    pub headers: serde_json::Value,
    pub active: bool,
    pub rule_kind: String,
    pub provider_id: Option<Uuid>,
    pub trigger_on: String,
    pub destination_type: String,
    /// Native destination credentials are only used by the delivery worker.
    /// They must never be serialized back to dashboard users.
    #[serde(skip)]
    pub destination_config: serde_json::Value,
    pub ingest_hostname: Option<String>,
    /// Per-rule HMAC secret used to sign outbound deliveries. Never serialized
    /// to API clients (would leak the secret to anyone with read access to
    /// /rules). `sqlx::FromRow` ignores serde attrs and still populates this
    /// from the DB column for internal use in `build_rule_request`.
    #[serde(skip)]
    #[sqlx(default)]
    pub signing_secret: Option<String>,
}

/// One outbound delivery attempt. Used for the per-event retry timeline.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DeliveryAttempt {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub event_id: Uuid,
    pub attempt_number: i32,
    pub status: String,
    pub http_status: Option<i32>,
    pub response_headers: Option<serde_json::Value>,
    pub response_body: Option<String>,
    pub error: Option<String>,
    pub duration_ms: Option<i32>,
    pub created_at: chrono::NaiveDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct HookNotificationDelivery {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub event_id: Uuid,
    pub hook_id: Uuid,
    pub destination_type: String,
    pub status: String,
    pub http_status: Option<i32>,
    pub error: Option<String>,
    pub attempts: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub username: Option<String>,
    pub password_hash: Option<String>,
    pub role: String,
    pub is_platform_operator: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth_subject: Option<String>,
}
