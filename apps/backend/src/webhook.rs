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

pub async fn handle_root(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    handle_webhook_inner(state, "".to_string(), headers, payload).await
}

pub async fn handle_webhook(
    State(state): State<Arc<AppState>>,
    Path(source_path): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    handle_webhook_inner(state, source_path, headers, payload).await
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
    source_path: String,
    headers: HeaderMap,
    payload: serde_json::Value,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let source = match headers.get("X-Webhook-Source").and_then(|v| v.to_str().ok()) {
        Some(s) if !s.is_empty() && s != "unknown" => s.to_string(),
        _ => extract_source(&source_path),
    };

    let rule_target: Option<String> = sqlx::query_as::<_, ForwardRule>(
        "SELECT * FROM forward_rules WHERE source_pattern = $1 AND active = true LIMIT 1",
    )
    .bind(&source)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten()
    .map(|r| r.target_url)
    .filter(|u| !u.is_empty());

    let target_url = headers
        .get("X-Target-Url")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| rule_target)
        .unwrap_or_else(|| state.default_target.lock().unwrap().clone());

    let id = Uuid::new_v4();
    let now = Utc::now().naive_utc();

    sqlx::query(
        r#"INSERT INTO webhook_events (id, source, headers, body, status, target_url, retry_count, max_retries, created_at)
        VALUES ($1, $2, $3, $4, 'queued', $5, 0, $6, $7)"#,
    )
    .bind(id)
    .bind(&source)
    .bind(headers_to_json(&headers))
    .bind(&payload)
    .bind(&target_url)
    .bind(state.max_retries)
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("db insert: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut conn = state.redis.clone();
    redis::cmd("LPUSH")
        .arg(QUEUE_KEY)
        .arg(id.to_string())
        .query_async::<()>(&mut conn)
        .await
        .ok();

    Ok(Json(serde_json::json!({"id": id, "status": "queued"})))
}
