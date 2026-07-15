//! Aggregated observability metrics for the dashboard.

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth;
use crate::middleware::require_scope;
use crate::state::AppState;

#[derive(Deserialize, Default)]
pub struct MetricsQuery {
    /// Time window: 24h | 7d | 30d. Defaults to 24h.
    pub range: Option<String>,
}

pub async fn metrics(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<auth::CurrentUser>,
    Query(q): Query<MetricsQuery>,
) -> Result<Json<Value>, StatusCode> {
    require_scope(&cu, "events:read")?;
    let hours = match q.range.as_deref() {
        Some("7d") => 24 * 7,
        Some("30d") => 24 * 30,
        _ => 24,
    };
    let since = Utc::now().naive_utc() - chrono::Duration::hours(hours);

    // Status totals within the window.
    let totals: Vec<(String, i64)> = sqlx::query_as(
        "SELECT status, COUNT(*) FROM webhook_events WHERE organization_id = $1 AND created_at >= $2 GROUP BY status",
    )
    .bind(cu.organization_id)
    .bind(since)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let total_count: i64 = totals.iter().map(|(_, c)| c).sum();
    let delivered: i64 = totals
        .iter()
        .filter(|(s, _)| s == "delivered")
        .map(|(_, c)| c)
        .sum();
    let failed: i64 = totals
        .iter()
        .filter(|(s, _)| s == "failed")
        .map(|(_, c)| c)
        .sum();
    let success_rate = if total_count > 0 {
        (delivered as f64 / total_count as f64) * 100.0
    } else {
        0.0
    };

    // Hourly throughput. Buckets per hour for 24h, per day for longer ranges.
    let trunc = if hours <= 24 { "hour" } else { "day" };
    let series_sql = format!(
        "SELECT date_trunc('{trunc}', created_at) AS bucket, COUNT(*) AS n \
         FROM webhook_events WHERE organization_id = $1 AND created_at >= $2 GROUP BY bucket ORDER BY bucket ASC"
    );
    let series: Vec<(chrono::NaiveDateTime, i64)> = sqlx::query_as(&series_sql)
        .bind(cu.organization_id)
        .bind(since)
        .fetch_all(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let series: Vec<Value> = series
        .into_iter()
        .map(|(ts, n)| json!({ "bucket": ts, "count": n }))
        .collect();

    // Top 5 sources and targets by count in the window.
    let top_sources: Vec<(String, i64)> = sqlx::query_as(
        "SELECT source, COUNT(*) n FROM webhook_events WHERE organization_id = $1 AND created_at >= $2 \
         GROUP BY source ORDER BY n DESC LIMIT 5",
    )
    .bind(cu.organization_id)
    .bind(since)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let top_targets: Vec<(String, i64)> = sqlx::query_as(
        "SELECT target_url, COUNT(*) n FROM webhook_events WHERE organization_id = $1 AND created_at >= $2 \
         GROUP BY target_url ORDER BY n DESC LIMIT 5",
    )
    .bind(cu.organization_id)
    .bind(since)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Redis queues are shared by all tenants, so expose tenant-scoped depth
    // derived from persisted event state instead of global Redis cardinality.
    let queue_depth: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM webhook_events WHERE organization_id = $1 AND status = 'queued'",
    )
    .bind(cu.organization_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);
    let retry_depth: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM webhook_events WHERE organization_id = $1 AND status = 'retrying'",
    )
    .bind(cu.organization_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    Ok(Json(json!({
        "range_hours": hours,
        "total": total_count,
        "delivered": delivered,
        "failed": failed,
        "success_rate": (success_rate * 100.0).round() / 100.0,
        "queue_depth": queue_depth,
        "retry_depth": retry_depth,
        "series": series,
        "top_sources": top_sources.into_iter().map(|(s, c)| json!({"source": s, "count": c})).collect::<Vec<_>>(),
        "top_targets": top_targets.into_iter().map(|(t, c)| json!({"target": t, "count": c})).collect::<Vec<_>>(),
    })))
}
