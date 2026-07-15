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

const METRICS_CACHE_TTL_SECS: usize = 15;

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
    let cache_key = metrics_cache_key(cu.organization_id, hours);
    if let Some(cached) = load_cached_metrics(&state, &cache_key).await {
        return Ok(Json(cached));
    }

    let since = Utc::now().naive_utc() - chrono::Duration::hours(hours);

    let totals_query = sqlx::query_as::<_, (String, i64)>(
        "SELECT status, COUNT(*) FROM webhook_events WHERE organization_id = $1 AND created_at >= $2 GROUP BY status",
    )
    .bind(cu.organization_id)
    .bind(since)
    .fetch_all(&state.db);

    // Hourly throughput. Buckets per hour for 24h, per day for longer ranges.
    let trunc = if hours <= 24 { "hour" } else { "day" };
    let series_sql = format!(
        "SELECT date_trunc('{trunc}', created_at) AS bucket, COUNT(*) AS n \
         FROM webhook_events WHERE organization_id = $1 AND created_at >= $2 GROUP BY bucket ORDER BY bucket ASC"
    );
    let series_query = sqlx::query_as::<_, (chrono::NaiveDateTime, i64)>(&series_sql)
        .bind(cu.organization_id)
        .bind(since)
        .fetch_all(&state.db);

    let top_sources_query = sqlx::query_as::<_, (String, i64)>(
        "SELECT source, COUNT(*) n FROM webhook_events WHERE organization_id = $1 AND created_at >= $2 \
         GROUP BY source ORDER BY n DESC LIMIT 5",
    )
    .bind(cu.organization_id)
    .bind(since)
    .fetch_all(&state.db);
    let top_targets_query = sqlx::query_as::<_, (String, i64)>(
        "SELECT target_url, COUNT(*) n FROM webhook_events WHERE organization_id = $1 AND created_at >= $2 \
         GROUP BY target_url ORDER BY n DESC LIMIT 5",
    )
    .bind(cu.organization_id)
    .bind(since)
    .fetch_all(&state.db);

    // Redis queues are shared by all tenants, so expose tenant-scoped depth
    // derived from persisted event state instead of global Redis cardinality.
    let queue_depth_query = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM webhook_events WHERE organization_id = $1 AND status = 'queued'",
    )
    .bind(cu.organization_id)
    .fetch_one(&state.db);
    let retry_depth_query = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM webhook_events WHERE organization_id = $1 AND status = 'retrying'",
    )
    .bind(cu.organization_id)
    .fetch_one(&state.db);

    let (
        totals_result,
        series_result,
        top_sources_result,
        top_targets_result,
        queue_result,
        retry_result,
    ) = tokio::join!(
        totals_query,
        series_query,
        top_sources_query,
        top_targets_query,
        queue_depth_query,
        retry_depth_query,
    );
    let totals = totals_result.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let series = series_result.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let top_sources = top_sources_result.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let top_targets = top_targets_result.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let queue_depth = queue_result.unwrap_or(0);
    let retry_depth = retry_result.unwrap_or(0);

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

    let series: Vec<Value> = series
        .into_iter()
        .map(|(ts, n)| json!({ "bucket": ts, "count": n }))
        .collect();

    let metrics = json!({
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
    });
    store_cached_metrics(&state, &cache_key, &metrics).await;
    Ok(Json(metrics))
}

fn metrics_cache_key(organization_id: uuid::Uuid, hours: i64) -> String {
    format!("terusin:metrics:{organization_id}:{hours}h")
}

async fn load_cached_metrics(state: &AppState, cache_key: &str) -> Option<Value> {
    let mut redis = state.redis.clone();
    let cached: Option<String> = redis::cmd("GET")
        .arg(cache_key)
        .query_async(&mut redis)
        .await
        .ok()?;
    cached.and_then(|value| serde_json::from_str(&value).ok())
}

async fn store_cached_metrics(state: &AppState, cache_key: &str, metrics: &Value) {
    let Ok(value) = serde_json::to_string(metrics) else {
        return;
    };
    let mut redis = state.redis.clone();
    if let Err(error) = redis::cmd("SETEX")
        .arg(cache_key)
        .arg(METRICS_CACHE_TTL_SECS)
        .arg(value)
        .query_async::<()>(&mut redis)
        .await
    {
        tracing::warn!("metrics cache store failed: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::metrics_cache_key;
    use uuid::Uuid;

    #[test]
    fn cache_key_is_scoped_to_organization_and_range() {
        let organization = Uuid::nil();
        assert_eq!(
            metrics_cache_key(organization, 24),
            "terusin:metrics:00000000-0000-0000-0000-000000000000:24h"
        );
        assert_ne!(
            metrics_cache_key(organization, 24),
            metrics_cache_key(organization, 168)
        );
    }
}
