//! Forward-rule CRUD handlers (admin-gated mutations).

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth;
use crate::middleware::{require_admin, require_scope};
use crate::model::ForwardRule;
use crate::state::AppState;

pub async fn list_rules(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<auth::CurrentUser>,
) -> Result<Json<Vec<ForwardRule>>, StatusCode> {
    require_scope(&cu, "rules:read")?;
    let rules = sqlx::query_as::<_, ForwardRule>(
        "SELECT * FROM forward_rules WHERE organization_id = $1 ORDER BY created_at ASC",
    )
    .bind(cu.organization_id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rules))
}

#[derive(Serialize, sqlx::FromRow)]
pub struct RuleHealth {
    pub rule_id: Uuid,
    pub received_24h: i64,
    pub delivered_24h: i64,
    pub failed_24h: i64,
    pub last_activity_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// A compact 24-hour health view for providers and hooks. This is deliberately
/// derived from delivery records instead of trusting a cached rule flag.
pub async fn rule_health(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<auth::CurrentUser>,
) -> Result<Json<Vec<RuleHealth>>, StatusCode> {
    require_scope(&cu, "rules:read")?;
    let rows = sqlx::query_as::<_, RuleHealth>(
        r#"SELECT provider.id AS rule_id,
                  COUNT(event.id)::BIGINT AS received_24h,
                  COUNT(event.id) FILTER (WHERE event.status = 'delivered')::BIGINT AS delivered_24h,
                  COUNT(event.id) FILTER (WHERE event.status = 'failed')::BIGINT AS failed_24h,
                  MAX(event.created_at) AT TIME ZONE 'UTC' AS last_activity_at
           FROM forward_rules provider
           LEFT JOIN webhook_events event ON event.organization_id = provider.organization_id
               AND event.source = provider.source_pattern
               AND event.created_at >= NOW() - INTERVAL '24 hours'
           WHERE provider.organization_id = $1 AND provider.rule_kind = 'provider'
           GROUP BY provider.id
           UNION ALL
           SELECT hook.id AS rule_id,
                  COUNT(notification.id)::BIGINT AS received_24h,
                  COUNT(notification.id) FILTER (WHERE notification.status = 'delivered')::BIGINT AS delivered_24h,
                  COUNT(notification.id) FILTER (WHERE notification.status IN ('failed', 'skipped'))::BIGINT AS failed_24h,
                  MAX(notification.created_at) AS last_activity_at
           FROM forward_rules hook
           LEFT JOIN hook_notification_deliveries notification ON notification.hook_id = hook.id
               AND notification.organization_id = hook.organization_id
               AND notification.created_at >= NOW() - INTERVAL '24 hours'
           WHERE hook.organization_id = $1 AND hook.rule_kind = 'hook'
           GROUP BY hook.id"#,
    )
    .bind(cu.organization_id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
pub struct CreateRule {
    pub name: String,
    pub source_pattern: Option<String>,
    pub target_url: String,
    pub method: Option<String>,
    /// Custom headers to send on outbound delivery. Defaults to `{}`.
    #[serde(default)]
    pub headers: Option<serde_json::Value>,
    #[serde(default = "default_rule_kind")]
    pub rule_kind: String,
    pub provider_id: Option<Uuid>,
    pub trigger_on: Option<String>,
    pub signing_secret: Option<String>,
    pub destination_type: Option<String>,
    #[serde(default)]
    pub destination_config: Option<serde_json::Value>,
    pub ingest_hostname: Option<String>,
}

fn default_rule_kind() -> String {
    "provider".to_string()
}

fn valid_http_url(value: &str) -> bool {
    reqwest::Url::parse(value)
        .map(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
        .unwrap_or(false)
}

fn valid_email(value: &str) -> bool {
    let value = value.trim();
    value.len() <= 255 && value.contains('@') && !value.starts_with('@') && !value.ends_with('@')
}

fn destination_target_and_config(
    destination_type: &str,
    target_url: String,
    config: serde_json::Value,
) -> Result<(String, serde_json::Value), StatusCode> {
    let config = config.as_object().cloned().ok_or(StatusCode::BAD_REQUEST)?;
    let string = |key: &str| {
        config
            .get(key)
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    match destination_type {
        "webhook" => {
            if !target_url.is_empty() && !valid_http_url(&target_url) {
                return Err(StatusCode::BAD_REQUEST);
            }
            Ok((target_url, serde_json::Value::Object(config)))
        }
        "slack" => {
            if config.is_empty() {
                return Ok(("Slack".to_string(), serde_json::json!({})));
            }
            let webhook_url = string("webhook_url").ok_or(StatusCode::BAD_REQUEST)?;
            if !valid_http_url(webhook_url) {
                return Err(StatusCode::BAD_REQUEST);
            }
            Ok((
                "Slack".to_string(),
                serde_json::json!({ "webhook_url": webhook_url }),
            ))
        }
        "telegram" => {
            if config.is_empty() {
                return Ok(("Telegram".to_string(), serde_json::json!({})));
            }
            let bot_token = string("bot_token").ok_or(StatusCode::BAD_REQUEST)?;
            let chat_id = string("chat_id").ok_or(StatusCode::BAD_REQUEST)?;
            Ok((
                "Telegram".to_string(),
                serde_json::json!({ "bot_token": bot_token, "chat_id": chat_id }),
            ))
        }
        "email" => {
            if config.is_empty() {
                return Ok(("Email".to_string(), serde_json::json!({})));
            }
            let recipient = string("recipient").ok_or(StatusCode::BAD_REQUEST)?;
            if !valid_email(recipient) {
                return Err(StatusCode::BAD_REQUEST);
            }
            let resend_ready = std::env::var("RESEND_API_KEY")
                .ok()
                .is_some_and(|value| !value.trim().is_empty())
                && std::env::var("EMAIL_FROM")
                    .ok()
                    .is_some_and(|value| !value.trim().is_empty());
            if !resend_ready {
                return Err(StatusCode::SERVICE_UNAVAILABLE);
            }
            Ok((
                recipient.to_string(),
                serde_json::json!({ "recipient": recipient }),
            ))
        }
        _ => Err(StatusCode::BAD_REQUEST),
    }
}

async fn validated_ingest_hostname(
    db: &sqlx::PgPool,
    organization_id: Uuid,
    hostname: Option<String>,
) -> Result<Option<String>, StatusCode> {
    let Some(hostname) = hostname else {
        return Ok(None);
    };
    let hostname = hostname.trim().trim_end_matches('.').to_ascii_lowercase();
    if hostname.is_empty() {
        return Ok(None);
    }
    let active: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM organization_domains WHERE organization_id = $1 AND hostname = $2 AND status = 'active')",
    )
    .bind(organization_id)
    .bind(&hostname)
    .fetch_one(db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    active
        .then_some(hostname)
        .map(Some)
        .ok_or(StatusCode::BAD_REQUEST)
}

async fn ensure_native_destination_available(
    db: &sqlx::PgPool,
    organization_id: Uuid,
    destination_type: &str,
) -> Result<(), StatusCode> {
    if destination_type == "webhook" {
        return Ok(());
    }
    let available: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM organization_destinations WHERE organization_id = $1 AND kind = $2 AND enabled = true)",
    )
    .bind(organization_id)
    .bind(destination_type)
    .fetch_one(db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    available.then_some(()).ok_or(StatusCode::CONFLICT)
}

pub async fn create_rule(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<auth::CurrentUser>,
    Json(input): Json<CreateRule>,
) -> Result<Json<ForwardRule>, StatusCode> {
    require_admin(&cu)?;
    require_scope(&cu, "rules:write")?;
    let rule_kind = input.rule_kind.trim();
    if !matches!(rule_kind, "provider" | "hook") {
        return Err(StatusCode::BAD_REQUEST);
    }
    if rule_kind == "provider" {
        crate::organizations::ensure_resource_quota(&state, cu.organization_id, "providers")
            .await?;
    }
    let id = Uuid::new_v4();
    let trigger_on = input.trigger_on.unwrap_or_else(|| "success".to_string());
    if !matches!(trigger_on.as_str(), "success" | "failure") {
        return Err(StatusCode::BAD_REQUEST);
    }
    let provider_id = input.provider_id;
    let pattern = if rule_kind == "hook" {
        let provider_id = provider_id.ok_or(StatusCode::BAD_REQUEST)?;
        sqlx::query_scalar::<_, String>(
            "SELECT source_pattern FROM forward_rules WHERE id = $1 AND organization_id = $2 AND rule_kind = 'provider'",
        )
        .bind(provider_id)
        .bind(cu.organization_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?
    } else {
        input.source_pattern.unwrap_or_else(|| "*".to_string())
    };
    let method = input.method.unwrap_or_else(|| "POST".to_string());
    let destination_type = input
        .destination_type
        .unwrap_or_else(|| "webhook".to_string())
        .to_ascii_lowercase();
    if rule_kind != "hook" && destination_type != "webhook" {
        return Err(StatusCode::BAD_REQUEST);
    }
    if rule_kind == "hook" {
        ensure_native_destination_available(&state.db, cu.organization_id, &destination_type)
            .await?;
    }
    let (target_url, destination_config) = destination_target_and_config(
        &destination_type,
        input.target_url.trim().to_string(),
        if rule_kind == "hook" && destination_type != "webhook" {
            serde_json::json!({})
        } else {
            input
                .destination_config
                .unwrap_or_else(|| serde_json::json!({}))
        },
    )?;
    let ingest_hostname = if rule_kind == "provider" {
        validated_ingest_hostname(&state.db, cu.organization_id, input.ingest_hostname).await?
    } else if input.ingest_hostname.is_some() {
        return Err(StatusCode::BAD_REQUEST);
    } else {
        None
    };
    let headers = input
        .headers
        .filter(|h| h.is_object())
        .unwrap_or_else(|| serde_json::json!({}));

    sqlx::query(
        r#"INSERT INTO forward_rules (id, organization_id, name, source_pattern, target_url, method, headers, active, rule_kind, provider_id, trigger_on, signing_secret, destination_type, destination_config, ingest_hostname)
        VALUES ($1, $2, $3, $4, $5, $6, $7, true, $8, $9, $10, $11, $12, $13, $14)"#,
    )
    .bind(id)
    .bind(cu.organization_id)
    .bind(&input.name)
    .bind(&pattern)
    .bind(&target_url)
    .bind(&method)
    .bind(&headers)
    .bind(rule_kind)
    .bind(provider_id)
    .bind(&trigger_on)
    .bind(input.signing_secret.filter(|secret| !secret.trim().is_empty()))
    .bind(&destination_type)
    .bind(&destination_config)
    .bind(&ingest_hostname)
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("create rule: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let rule = sqlx::query_as::<_, ForwardRule>(
        "SELECT * FROM forward_rules WHERE id = $1 AND organization_id = $2",
    )
    .bind(id)
    .bind(cu.organization_id)
    .fetch_one(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    crate::audit::record(
        &state,
        Some(&cu),
        "rule.created",
        "rule",
        Some(id.to_string()),
        serde_json::json!({ "name": rule.name, "source_pattern": rule.source_pattern, "rule_kind": rule.rule_kind }),
    )
    .await;

    Ok(Json(rule))
}

pub async fn delete_rule(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<auth::CurrentUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    require_admin(&cu)?;
    require_scope(&cu, "rules:write")?;
    let res = sqlx::query("DELETE FROM forward_rules WHERE id = $1 AND organization_id = $2")
        .bind(id)
        .bind(cu.organization_id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if res.rows_affected() > 0 {
        crate::audit::record(
            &state,
            Some(&cu),
            "rule.deleted",
            "rule",
            Some(id.to_string()),
            serde_json::json!({}),
        )
        .await;
        Ok(StatusCode::OK)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

/// Partial update of a forward rule. Any of the fields may be omitted; only
/// provided fields are written. `active` lets the UI toggle a rule on/off.
#[derive(Deserialize)]
pub struct UpdateRule {
    pub name: Option<String>,
    pub source_pattern: Option<String>,
    pub target_url: Option<String>,
    pub method: Option<String>,
    pub headers: Option<serde_json::Value>,
    pub active: Option<bool>,
    pub trigger_on: Option<String>,
    pub provider_id: Option<Uuid>,
    pub signing_secret: Option<String>,
    pub destination_type: Option<String>,
    pub destination_config: Option<serde_json::Value>,
    pub ingest_hostname: Option<String>,
}

pub async fn update_rule(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<auth::CurrentUser>,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateRule>,
) -> Result<Json<ForwardRule>, StatusCode> {
    require_admin(&cu)?;
    require_scope(&cu, "rules:write")?;
    // Coalesce: read current row, apply overrides, write back. Simpler than
    // building a dynamic UPDATE with a variable column list.
    let current = sqlx::query_as::<_, ForwardRule>(
        "SELECT * FROM forward_rules WHERE id = $1 AND organization_id = $2",
    )
    .bind(id)
    .bind(cu.organization_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    let name = input.name.unwrap_or(current.name);
    let (source_pattern, provider_id) = if current.rule_kind == "hook" {
        let provider_id = input
            .provider_id
            .or(current.provider_id)
            .ok_or(StatusCode::BAD_REQUEST)?;
        let source_pattern = if input.provider_id.is_some() {
            sqlx::query_scalar::<_, String>(
                "SELECT source_pattern FROM forward_rules WHERE id = $1 AND organization_id = $2 AND rule_kind = 'provider'",
            )
            .bind(provider_id)
            .bind(cu.organization_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?
        } else {
            current.source_pattern
        };
        (source_pattern, Some(provider_id))
    } else {
        (
            input.source_pattern.unwrap_or(current.source_pattern),
            current.provider_id,
        )
    };
    let destination_type = input
        .destination_type
        .unwrap_or(current.destination_type)
        .to_ascii_lowercase();
    if current.rule_kind != "hook" && destination_type != "webhook" {
        return Err(StatusCode::BAD_REQUEST);
    }
    if current.rule_kind == "hook" {
        ensure_native_destination_available(&state.db, cu.organization_id, &destination_type)
            .await?;
    }
    let config = input
        .destination_config
        .unwrap_or(current.destination_config);
    let candidate_target = input.target_url.unwrap_or(current.target_url);
    let (target_url, destination_config) = destination_target_and_config(
        &destination_type,
        candidate_target,
        if current.rule_kind == "hook" && destination_type != "webhook" {
            serde_json::json!({})
        } else {
            config
        },
    )?;
    let ingest_hostname = if current.rule_kind == "provider" {
        match input.ingest_hostname {
            Some(hostname) => {
                validated_ingest_hostname(&state.db, cu.organization_id, Some(hostname)).await?
            }
            None => current.ingest_hostname,
        }
    } else if input.ingest_hostname.is_some() {
        return Err(StatusCode::BAD_REQUEST);
    } else {
        None
    };
    let method = input.method.unwrap_or(current.method);
    let headers = input
        .headers
        .filter(|h| h.is_object())
        .unwrap_or(current.headers);
    let active = input.active.unwrap_or(current.active);
    let trigger_on = input.trigger_on.unwrap_or(current.trigger_on);
    if !matches!(trigger_on.as_str(), "success" | "failure") {
        return Err(StatusCode::BAD_REQUEST);
    }

    let rule = sqlx::query_as::<_, ForwardRule>(
        r#"UPDATE forward_rules
           SET name = $2, source_pattern = $3, target_url = $4, method = $5, headers = $6, active = $7, trigger_on = $8, provider_id = $9,
               signing_secret = COALESCE($10, signing_secret), destination_type = $11, destination_config = $12, ingest_hostname = $13
           WHERE id = $1 AND organization_id = $14
           RETURNING *"#,
    )
    .bind(id)
    .bind(&name)
    .bind(&source_pattern)
    .bind(&target_url)
    .bind(&method)
    .bind(&headers)
    .bind(active)
    .bind(&trigger_on)
    .bind(provider_id)
    .bind(input.signing_secret.filter(|secret| !secret.trim().is_empty()))
    .bind(&destination_type)
    .bind(&destination_config)
    .bind(&ingest_hostname)
    .bind(cu.organization_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("update rule: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    crate::audit::record(
        &state,
        Some(&cu),
        "rule.updated",
        "rule",
        Some(id.to_string()),
        serde_json::json!({ "name": rule.name, "active": rule.active }),
    )
    .await;

    Ok(Json(rule))
}
