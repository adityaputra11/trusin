//! Webhook ingest handlers (`POST /`, `POST /{source}`).

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::Json;
use chrono::Utc;
use uuid::Uuid;

use crate::middleware::headers_to_json;
use crate::model::ForwardRule;
use crate::state::AppState;

/// Redis queue holding event ids awaiting delivery.
const QUEUE_KEY: &str = "terusin:queue";

pub fn public_target_override_enabled() -> bool {
    std::env::var("ALLOW_PUBLIC_TARGET_OVERRIDE")
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false)
}

fn private_targets_enabled() -> bool {
    std::env::var("ALLOW_PRIVATE_TARGETS")
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false)
}

fn is_private_or_reserved(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(ip) => {
            ip.is_loopback()
                || ip.is_private()
                || ip.is_link_local()
                || ip.is_multicast()
                || ip.is_unspecified()
                || ip.octets()[0] == 169 && ip.octets()[1] == 254
                || ip.octets()[0] == 100 && (64..=127).contains(&ip.octets()[1])
        }
        std::net::IpAddr::V6(ip) => {
            ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_multicast()
                || (ip.segments()[0] & 0xfe00) == 0xfc00
                || (ip.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

/// Validate a delivery target before a dashboard send or an explicitly
/// enabled legacy public override. Private and loopback targets require an
/// explicit local-development opt-in.
fn validate_target_url_shape(value: &str) -> Result<reqwest::Url, &'static str> {
    let url = reqwest::Url::parse(value).map_err(|_| "target_url must be a valid URL")?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("target_url must use http or https");
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("target_url must not include credentials");
    }

    url.host_str().ok_or("target_url must include a hostname")?;
    Ok(url)
}

pub async fn validate_target_url(value: &str) -> Result<(), &'static str> {
    let url = validate_target_url_shape(value)?;
    let host = url.host_str().expect("validated target URL host");
    if private_targets_enabled() {
        return Ok(());
    }

    let port = url
        .port_or_known_default()
        .ok_or("target_url must include a supported port")?;
    let addresses = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| "target_url hostname could not be resolved")?;
    if addresses
        .into_iter()
        .any(|address| is_private_or_reserved(address.ip()))
    {
        return Err("private, loopback, link-local, or reserved targets are disabled");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{is_private_or_reserved, validate_target_url_shape};

    #[test]
    fn target_shape_requires_http_or_https_without_credentials() {
        assert!(validate_target_url_shape("https://example.com/hook").is_ok());
        assert_eq!(
            validate_target_url_shape("ftp://example.com/hook").unwrap_err(),
            "target_url must use http or https"
        );
        assert_eq!(
            validate_target_url_shape("https://user:pass@example.com/hook").unwrap_err(),
            "target_url must not include credentials"
        );
    }

    #[test]
    fn private_and_loopback_addresses_are_rejected_by_policy() {
        assert!(is_private_or_reserved("127.0.0.1".parse().unwrap()));
        assert!(is_private_or_reserved("10.0.0.1".parse().unwrap()));
        assert!(is_private_or_reserved("169.254.169.254".parse().unwrap()));
        assert!(!is_private_or_reserved("8.8.8.8".parse().unwrap()));
    }
}

pub async fn handle_root(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let (organization_id, source_path) =
        crate::organizations::resolve_ingest_organization(&state, &headers, "").await?;
    handle_webhook_inner(state, organization_id, source_path, headers, payload).await
}

pub async fn handle_webhook(
    State(state): State<Arc<AppState>>,
    Path(source_path): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let (organization_id, source_path) =
        crate::organizations::resolve_ingest_organization(&state, &headers, &source_path).await?;
    handle_webhook_inner(state, organization_id, source_path, headers, payload).await
}

/// Extract the webhook source from the first non-empty URL segment.
pub fn extract_source(path: &str) -> String {
    let p = path.trim_matches('/');
    if p.is_empty() {
        return "unknown".into();
    }
    p.split('/').next().unwrap_or("unknown").to_string()
}

async fn handle_webhook_inner(
    state: Arc<AppState>,
    organization_id: Uuid,
    source_path: String,
    headers: HeaderMap,
    payload: serde_json::Value,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let source = match headers
        .get("X-Webhook-Source")
        .and_then(|v| v.to_str().ok())
    {
        Some(s) if !s.is_empty() && s != "unknown" => s.to_string(),
        _ => extract_source(&source_path),
    };

    let rule_target: Option<String> = sqlx::query_as::<_, ForwardRule>(
        "SELECT * FROM forward_rules WHERE organization_id = $1 AND source_pattern = $2 AND rule_kind = 'provider' AND active = true LIMIT 1",
    )
    .bind(organization_id)
    .bind(&source)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten()
    .map(|r| r.target_url)
    .filter(|u| !u.is_empty());

    let target_override = headers
        .get("X-Target-Url")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    if target_override.is_some() && !public_target_override_enabled() {
        return Err(StatusCode::FORBIDDEN);
    }

    let target_url = if let Some(target) = target_override {
        validate_target_url(&target).await.map_err(|error| {
            tracing::warn!(%error, "rejected public target override");
            StatusCode::BAD_REQUEST
        })?;
        target
    } else {
        rule_target
            .or(crate::organizations::default_target_for(&state.db, organization_id).await.ok())
            .unwrap_or_default()
    };

    enqueue_event(
        &state,
        organization_id,
        source,
        headers_to_json(&headers),
        payload,
        target_url,
    )
    .await
}

pub async fn enqueue_event(
    state: &Arc<AppState>,
    organization_id: Uuid,
    source: String,
    headers: serde_json::Value,
    payload: serde_json::Value,
    target_url: String,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let request_id = Uuid::new_v4();
    let id = Uuid::new_v4();
    let now = Utc::now().naive_utc();
    let mut transaction = state
        .db
        .begin()
        .await
        .map_err(|error| {
            tracing::error!(%request_id, %error, "begin webhook enqueue transaction");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    crate::organizations::consume_event_quota(&mut transaction, organization_id).await?;

    sqlx::query(
        r#"INSERT INTO webhook_events (id, organization_id, source, headers, body, status, target_url, retry_count, max_retries, created_at)
        VALUES ($1, $2, $3, $4, $5, 'queued', $6, 0, $7, $8)"#,
    )
    .bind(id)
    .bind(organization_id)
    .bind(&source)
    .bind(headers)
    .bind(&payload)
    .bind(&target_url)
    .bind(state.max_retries)
    .bind(now)
    .execute(&mut *transaction)
    .await
    .map_err(|e| {
        tracing::error!(%request_id, "db insert: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    transaction.commit().await.map_err(|error| {
        tracing::error!(%request_id, %error, "commit webhook enqueue transaction");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut conn = state.redis.clone();
    if let Err(error) = redis::cmd("LPUSH")
        .arg(QUEUE_KEY)
        .arg(id.to_string())
        .query_async::<()>(&mut conn)
        .await
    {
        tracing::error!(%request_id, event_id = %id, %error, "event persisted but Redis enqueue failed");
    }

    Ok(Json(serde_json::json!({"id": id, "status": "queued"})))
}
