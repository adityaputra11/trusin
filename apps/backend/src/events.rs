//! Webhook event handlers: list/get/stream, per-event attempts, admin actions.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use bytes::Bytes;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio_stream::StreamExt;
use uuid::Uuid;

use crate::auth;
use crate::middleware::{require_admin, require_scope};
use crate::model::{DeliveryAttempt, HookNotificationDelivery, WebhookEvent};
use crate::state::AppState;
use crate::workers::{QUEUE_KEY, RETRY_KEY};

#[derive(Deserialize, Default)]
pub struct EventQuery {
    pub search: Option<String>,
    pub status: Option<String>,
    pub source: Option<String>,
    /// ISO timestamp lower bound (inclusive) on created_at.
    pub from: Option<String>,
    /// ISO timestamp upper bound (exclusive) on created_at.
    pub to: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Deserialize, Default)]
pub struct EventStreamQuery {
    pub source: Option<String>,
}

pub async fn list_events(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<auth::CurrentUser>,
    Query(q): Query<EventQuery>,
) -> Result<Json<Value>, StatusCode> {
    require_scope(&cu, "events:read")?;
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(10).min(200);
    let offset = (page - 1) * per_page;

    let mut sql = "SELECT * FROM webhook_events WHERE organization_id = $1".to_string();
    let mut count_sql =
        "SELECT COUNT(*) FROM webhook_events WHERE organization_id = $1".to_string();
    let mut params: Vec<String> = vec![];

    if let Some(ref s) = q.search {
        if !s.is_empty() {
            let like = format!("%{}%", s);
            let idx = params.len() + 2;
            sql += &format!(
                " AND (source ILIKE ${idx} OR target_url ILIKE ${idx} OR body::text ILIKE ${idx})"
            );
            count_sql += &format!(
                " AND (source ILIKE ${idx} OR target_url ILIKE ${idx} OR body::text ILIKE ${idx})"
            );
            params.push(like);
        }
    }
    if let Some(ref s) = q.status {
        if !s.is_empty() && s != "all" {
            let idx = params.len() + 2;
            sql += &format!(" AND status = ${idx}");
            count_sql += &format!(" AND status = ${idx}");
            params.push(s.clone());
        }
    }
    if let Some(ref s) = q.source {
        if !s.is_empty() {
            let idx = params.len() + 2;
            sql += &format!(" AND source = ${idx}");
            count_sql += &format!(" AND source = ${idx}");
            params.push(s.clone());
        }
    }
    if let Some(ref ts) = q.from {
        if !ts.is_empty() {
            let idx = params.len() + 2;
            sql += &format!(" AND created_at >= ${idx}::timestamp");
            count_sql += &format!(" AND created_at >= ${idx}::timestamp");
            params.push(ts.clone());
        }
    }
    if let Some(ref ts) = q.to {
        if !ts.is_empty() {
            let idx = params.len() + 2;
            sql += &format!(" AND created_at < ${idx}::timestamp");
            count_sql += &format!(" AND created_at < ${idx}::timestamp");
            params.push(ts.clone());
        }
    }

    sql += &format!(" ORDER BY created_at DESC LIMIT {per_page} OFFSET {offset}");

    let mut query = sqlx::query_as::<_, WebhookEvent>(&sql);
    let mut count_q = sqlx::query_scalar::<_, i64>(&count_sql);
    query = query.bind(cu.organization_id);
    count_q = count_q.bind(cu.organization_id);
    for p in &params {
        query = query.bind(p);
        count_q = count_q.bind(p);
    }

    let (events, total) =
        tokio::try_join!(query.fetch_all(&state.db), count_q.fetch_one(&state.db),)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(json!({
        "events": events,
        "total": total,
        "page": page,
        "per_page": per_page,
        "pages": (total as f64 / per_page as f64).ceil() as i64,
    })))
}

pub async fn get_event(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<auth::CurrentUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<WebhookEvent>, StatusCode> {
    require_scope(&cu, "events:read")?;
    let event = sqlx::query_as::<_, WebhookEvent>(
        "SELECT * FROM webhook_events WHERE id = $1 AND organization_id = $2",
    )
    .bind(id)
    .bind(cu.organization_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    event.map(Json).ok_or(StatusCode::NOT_FOUND)
}

/// Delivery attempts for the per-event retry timeline (newest last).
pub async fn list_attempts(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<auth::CurrentUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<DeliveryAttempt>>, StatusCode> {
    require_scope(&cu, "events:read")?;
    let rows = sqlx::query_as::<_, DeliveryAttempt>(
        "SELECT * FROM delivery_attempts WHERE event_id = $1 AND organization_id = $2 ORDER BY created_at ASC, id ASC",
    )
    .bind(id)
    .bind(cu.organization_id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rows))
}

/// Final Hook notification outcomes are intentionally separate from the main
/// provider forwarding timeline so operators can diagnose each path clearly.
pub async fn list_hook_notifications(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<auth::CurrentUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<HookNotificationDelivery>>, StatusCode> {
    require_scope(&cu, "events:read")?;
    let rows = sqlx::query_as::<_, HookNotificationDelivery>(
        "SELECT * FROM hook_notification_deliveries WHERE event_id = $1 AND organization_id = $2 ORDER BY created_at ASC, id ASC",
    )
    .bind(id)
    .bind(cu.organization_id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rows))
}

/// Distinct sources (for the dashboard source filter dropdown).
pub async fn list_sources(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<auth::CurrentUser>,
) -> Result<Json<Vec<String>>, StatusCode> {
    require_scope(&cu, "events:read")?;
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT source FROM webhook_events WHERE organization_id = $1 ORDER BY source ASC",
    )
    .bind(cu.organization_id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rows.into_iter().map(|(s,)| s).collect()))
}

/// Server-Sent Events stream of newly-created events. Polls the DB every 2s
/// for events newer than the last seen created_at and emits each as an SSE
/// `data:` line (JSON). Manual impl because axum-extra 0.10 has no SSE helper.
pub async fn event_stream(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<auth::CurrentUser>,
    Query(query): Query<EventStreamQuery>,
) -> Response {
    if require_scope(&cu, "events:read").is_err() {
        return StatusCode::FORBIDDEN.into_response();
    }
    let db = state.db.clone();
    let organization_id = cu.organization_id;
    let source = query.source.filter(|source| !source.trim().is_empty());
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<String, std::io::Error>>(16);

    tokio::spawn(async move {
        let mut last_seen = chrono::Utc::now().naive_utc();
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(2));
        // Send an initial comment so the client knows the stream is alive.
        let _ = tx.send(Ok(": connected\n\n".to_string())).await;
        loop {
            ticker.tick().await;
            let rows: Vec<WebhookEvent> = match if let Some(source) = &source {
                sqlx::query_as::<_, WebhookEvent>(
                    "SELECT * FROM webhook_events WHERE organization_id = $1 AND created_at > $2 AND source = $3 ORDER BY created_at ASC LIMIT 100",
                )
                .bind(organization_id)
                .bind(last_seen)
                .bind(source)
                .fetch_all(&db)
                .await
            } else {
                sqlx::query_as::<_, WebhookEvent>(
                    "SELECT * FROM webhook_events WHERE organization_id = $1 AND created_at > $2 ORDER BY created_at ASC LIMIT 100",
                )
                .bind(organization_id)
                .bind(last_seen)
                .fetch_all(&db)
                .await
            } {
                Ok(r) => r,
                Err(_) => continue,
            };
            if let Some(last) = rows.last() {
                last_seen = last.created_at;
            }
            for ev in rows {
                let payload = match serde_json::to_string(&ev) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let chunk = format!("event: event\ndata: {payload}\n\n");
                if tx.send(Ok(chunk)).await.is_err() {
                    return; // client disconnected
                }
            }
        }
    });

    let body = Body::from_stream(
        tokio_stream::wrappers::ReceiverStream::new(rx).map(|r| r.map(|s| Bytes::from(s))),
    );
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-cache")
        .header("x-accel-buffering", "no")
        .body(body)
        .unwrap()
}

pub async fn retry_event(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<auth::CurrentUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    require_admin(&cu)?;
    require_scope(&cu, "organization:manage")?;
    let event = sqlx::query_as::<_, WebhookEvent>(
        "SELECT * FROM webhook_events WHERE id = $1 AND organization_id = $2",
    )
    .bind(id)
    .bind(cu.organization_id)
    .fetch_optional(&state.db)
    .await;

    match event {
        Ok(Some(_)) => {
            sqlx::query(
                "UPDATE webhook_events SET status = 'queued', retry_count = 0 WHERE id = $1 AND organization_id = $2",
            )
            .bind(id)
            .bind(cu.organization_id)
            .execute(&state.db)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let mut conn = state.redis.clone();
            redis::cmd("ZREM")
                .arg(RETRY_KEY)
                .arg(id.to_string())
                .query_async::<()>(&mut conn)
                .await
                .ok();
            redis::cmd("LPUSH")
                .arg(QUEUE_KEY)
                .arg(id.to_string())
                .query_async::<()>(&mut conn)
                .await
                .ok();
            crate::audit::record(
                &state,
                Some(&cu),
                "event.retried",
                "event",
                Some(id.to_string()),
                json!({}),
            )
            .await;
            Ok(StatusCode::OK)
        }
        _ => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn ack_event(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<auth::CurrentUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    require_admin(&cu)?;
    require_scope(&cu, "organization:manage")?;
    sqlx::query(
        "UPDATE webhook_events SET status = 'delivered', response_status = 200 WHERE id = $1 AND organization_id = $2",
    )
    .bind(id)
    .bind(cu.organization_id)
    .execute(&state.db)
    .await
    .ok();
    let mut conn = state.redis.clone();
    redis::cmd("LREM")
        .arg(QUEUE_KEY)
        .arg(0)
        .arg(id.to_string())
        .query_async::<()>(&mut conn)
        .await
        .ok();
    redis::cmd("ZREM")
        .arg(RETRY_KEY)
        .arg(id.to_string())
        .query_async::<()>(&mut conn)
        .await
        .ok();
    crate::audit::record(
        &state,
        Some(&cu),
        "event.acked",
        "event",
        Some(id.to_string()),
        json!({ "status": "delivered" }),
    )
    .await;
    Ok(StatusCode::OK)
}

/// Permanently delete an event and remove it from any in-flight Redis queue.
pub async fn delete_event(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<auth::CurrentUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    require_admin(&cu)?;
    require_scope(&cu, "organization:manage")?;
    let res = sqlx::query("DELETE FROM webhook_events WHERE id = $1 AND organization_id = $2")
        .bind(id)
        .bind(cu.organization_id)
        .execute(&state.db)
        .await;
    let removed = matches!(res, Ok(r) if r.rows_affected() > 0);
    if removed {
        let mut conn = state.redis.clone();
        redis::cmd("LREM")
            .arg(QUEUE_KEY)
            .arg(0)
            .arg(id.to_string())
            .query_async::<()>(&mut conn)
            .await
            .ok();
        redis::cmd("ZREM")
            .arg(RETRY_KEY)
            .arg(id.to_string())
            .query_async::<()>(&mut conn)
            .await
            .ok();
        crate::audit::record(
            &state,
            Some(&cu),
            "event.deleted",
            "event",
            Some(id.to_string()),
            json!({}),
        )
        .await;
        Ok(StatusCode::OK)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

#[derive(Deserialize)]
pub struct BulkIds {
    pub ids: Vec<Uuid>,
}

/// Re-enqueue many events at once (LPUSH each id to the queue).
pub async fn bulk_retry(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<auth::CurrentUser>,
    Json(input): Json<BulkIds>,
) -> Result<Json<Value>, StatusCode> {
    require_admin(&cu)?;
    require_scope(&cu, "organization:manage")?;
    let mut enqueued = 0;
    let mut conn = state.redis.clone();
    for id in &input.ids {
        let reset = sqlx::query(
            "UPDATE webhook_events SET status = 'queued', retry_count = 0 WHERE id = $1 AND organization_id = $2",
        )
        .bind(id)
        .bind(cu.organization_id)
        .execute(&state.db)
        .await
        .ok();
        if !matches!(reset, Some(result) if result.rows_affected() > 0) {
            continue;
        }
        redis::cmd("ZREM")
            .arg(RETRY_KEY)
            .arg(id.to_string())
            .query_async::<()>(&mut conn)
            .await
            .ok();
        redis::cmd("LPUSH")
            .arg(QUEUE_KEY)
            .arg(id.to_string())
            .query_async::<()>(&mut conn)
            .await
            .ok();
        enqueued += 1;
    }
    crate::audit::record(
        &state,
        Some(&cu),
        "event.bulk_retried",
        "event",
        None,
        json!({ "requested": input.ids.len(), "enqueued": enqueued }),
    )
    .await;
    Ok(Json(
        json!({ "enqueued": enqueued, "requested": input.ids.len() }),
    ))
}

/// Delete many events at once and scrub them from Redis.
pub async fn bulk_delete(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<auth::CurrentUser>,
    Json(input): Json<BulkIds>,
) -> Result<Json<Value>, StatusCode> {
    require_admin(&cu)?;
    require_scope(&cu, "organization:manage")?;
    let mut conn = state.redis.clone();
    let mut deleted = 0;
    for id in &input.ids {
        let res = sqlx::query("DELETE FROM webhook_events WHERE id = $1 AND organization_id = $2")
            .bind(id)
            .bind(cu.organization_id)
            .execute(&state.db)
            .await;
        if matches!(res, Ok(r) if r.rows_affected() > 0) {
            redis::cmd("LREM")
                .arg(QUEUE_KEY)
                .arg(0)
                .arg(id.to_string())
                .query_async::<()>(&mut conn)
                .await
                .ok();
            redis::cmd("ZREM")
                .arg(RETRY_KEY)
                .arg(id.to_string())
                .query_async::<()>(&mut conn)
                .await
                .ok();
            deleted += 1;
        }
    }
    crate::audit::record(
        &state,
        Some(&cu),
        "event.bulk_deleted",
        "event",
        None,
        json!({ "requested": input.ids.len(), "deleted": deleted }),
    )
    .await;
    Ok(Json(
        json!({ "deleted": deleted, "requested": input.ids.len() }),
    ))
}
