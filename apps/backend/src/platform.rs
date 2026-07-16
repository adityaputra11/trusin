//! Internal control-plane APIs for platform operators.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use chrono::{Datelike, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::middleware::require_platform_operator;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct BootstrapOperatorRequest {
    pub username: String,
}

/// One-time bootstrap using the deployment secret. Once an operator exists,
/// this endpoint is permanently disabled until database intervention.
pub async fn bootstrap_platform_operator(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(input): Json<BootstrapOperatorRequest>,
) -> Result<Json<Value>, StatusCode> {
    let expected = std::env::var("PLATFORM_ADMIN_TOKEN").map_err(|_| StatusCode::NOT_FOUND)?;
    let supplied = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    if supplied != Some(expected.as_str()) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let existing: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE is_platform_operator = TRUE)")
            .fetch_one(&state.db)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if existing {
        return Err(StatusCode::CONFLICT);
    }
    let user_id: Option<Uuid> = sqlx::query_scalar(
        "UPDATE users SET is_platform_operator = TRUE WHERE username = $1 RETURNING id",
    )
    .bind(input.username.trim())
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    user_id
        .map(|id| Json(json!({ "ok": true, "user_id": id })))
        .ok_or(StatusCode::NOT_FOUND)
}

#[derive(Debug, Deserialize)]
pub struct PlatformOrganizationQuery {
    pub search: Option<String>,
    pub status: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PlatformOrganizationRow {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub subscriber_name: String,
    pub billing_contact_name: String,
    pub billing_contact_email: String,
    pub plan_code: String,
    pub subscription_status: String,
    pub billing_period_start: chrono::DateTime<Utc>,
    pub billing_period_end: chrono::DateTime<Utc>,
    pub created_at: chrono::DateTime<Utc>,
    pub events_accepted: i64,
    pub active_domains: i64,
    pub active_api_keys: i64,
    pub queued_events: i64,
    pub retrying_events: i64,
    pub last_activity_at: Option<chrono::NaiveDateTime>,
}

pub async fn platform_overview(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<CurrentUser>,
) -> Result<Json<Value>, StatusCode> {
    require_platform_operator(&cu)?;
    let period = current_period();
    let totals: (i64, i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        r#"SELECT
             (SELECT COUNT(*) FROM organizations),
             (SELECT COUNT(*) FROM organizations WHERE subscription_status = 'active'),
             (SELECT COALESCE(SUM(events_accepted), 0)::bigint FROM organization_usage WHERE period_start = $1),
             (SELECT COUNT(*) FROM webhook_events WHERE status = 'queued'),
             (SELECT COUNT(*) FROM webhook_events WHERE status = 'retrying'),
             (SELECT COUNT(*) FROM webhook_events WHERE status = 'failed' AND created_at >= NOW() - INTERVAL '24 hours'),
             (SELECT COUNT(*) FROM organization_domains WHERE status = 'active')"#,
    )
    .bind(period)
    .fetch_one(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({
        "period_start": period,
        "organizations": totals.0,
        "active_organizations": totals.1,
        "accepted_events": totals.2,
        "queued_events": totals.3,
        "retrying_events": totals.4,
        "failed_events_24h": totals.5,
        "active_domains": totals.6,
    })))
}

pub async fn list_platform_organizations(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<CurrentUser>,
    Query(query): Query<PlatformOrganizationQuery>,
) -> Result<Json<Value>, StatusCode> {
    require_platform_operator(&cu)?;
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(25).clamp(1, 100);
    let status = query
        .status
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let search = query
        .search
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let period = current_period();
    let filters = r#"($1::text IS NULL OR o.subscription_status = $1)
        AND ($2::text IS NULL OR o.name ILIKE '%' || $2 || '%'
             OR o.slug ILIKE '%' || $2 || '%'
             OR o.subscriber_name ILIKE '%' || $2 || '%'
             OR o.billing_contact_name ILIKE '%' || $2 || '%'
             OR o.billing_contact_email ILIKE '%' || $2 || '%')"#;
    let sql = format!(
        r#"SELECT o.id, o.name, o.slug, o.subscriber_name, o.billing_contact_name,
                   o.billing_contact_email, o.plan_code, o.subscription_status,
                   o.billing_period_start, o.billing_period_end, o.created_at,
                   COALESCE(u.events_accepted, 0)::bigint AS events_accepted,
                   (SELECT COUNT(*) FROM organization_domains d WHERE d.organization_id = o.id AND d.status = 'active') AS active_domains,
                   (SELECT COUNT(*) FROM api_tokens k WHERE k.organization_id = o.id AND k.revoked_at IS NULL) AS active_api_keys,
                   (SELECT COUNT(*) FROM webhook_events e WHERE e.organization_id = o.id AND e.status = 'queued') AS queued_events,
                   (SELECT COUNT(*) FROM webhook_events e WHERE e.organization_id = o.id AND e.status = 'retrying') AS retrying_events,
                   (SELECT MAX(e.created_at) FROM webhook_events e WHERE e.organization_id = o.id) AS last_activity_at
            FROM organizations o
            LEFT JOIN organization_usage u ON u.organization_id = o.id AND u.period_start = $3
            WHERE {filters}
            ORDER BY o.created_at DESC
            LIMIT $4 OFFSET $5"#,
        filters = filters,
    );
    let rows = sqlx::query_as::<_, PlatformOrganizationRow>(&sql)
        .bind(status.as_deref())
        .bind(search.as_deref())
        .bind(period)
        .bind(per_page)
        .bind((page - 1) * per_page)
        .fetch_all(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let count_sql = format!("SELECT COUNT(*) FROM organizations o WHERE {filters}");
    let total: i64 = sqlx::query_scalar(&count_sql)
        .bind(status.as_deref())
        .bind(search.as_deref())
        .fetch_one(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({
        "organizations": rows,
        "total": total,
        "page": page,
        "per_page": per_page,
        "pages": (total as f64 / per_page as f64).ceil() as i64,
    })))
}

pub async fn platform_organization_detail(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<CurrentUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, StatusCode> {
    require_platform_operator(&cu)?;
    let organization: PlatformOrganizationRow = sqlx::query_as(
        r#"SELECT o.id, o.name, o.slug, o.subscriber_name, o.billing_contact_name,
                   o.billing_contact_email, o.plan_code, o.subscription_status,
                   o.billing_period_start, o.billing_period_end, o.created_at,
                   COALESCE(u.events_accepted, 0)::bigint AS events_accepted,
                   (SELECT COUNT(*) FROM organization_domains d WHERE d.organization_id = o.id AND d.status = 'active') AS active_domains,
                   (SELECT COUNT(*) FROM api_tokens k WHERE k.organization_id = o.id AND k.revoked_at IS NULL) AS active_api_keys,
                   (SELECT COUNT(*) FROM webhook_events e WHERE e.organization_id = o.id AND e.status = 'queued') AS queued_events,
                   (SELECT COUNT(*) FROM webhook_events e WHERE e.organization_id = o.id AND e.status = 'retrying') AS retrying_events,
                   (SELECT MAX(e.created_at) FROM webhook_events e WHERE e.organization_id = o.id) AS last_activity_at
            FROM organizations o
            LEFT JOIN organization_usage u ON u.organization_id = o.id AND u.period_start = $2
            WHERE o.id = $1"#,
    )
    .bind(id)
    .bind(current_period())
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;
    let health: (i64, i64, i64, i64) = sqlx::query_as(
        r#"SELECT COUNT(*),
                  COUNT(*) FILTER (WHERE status = 'delivered'),
                  COUNT(*) FILTER (WHERE status = 'failed'),
                  COUNT(*) FILTER (WHERE status IN ('queued', 'retrying'))
           FROM webhook_events
           WHERE organization_id = $1 AND created_at >= NOW() - INTERVAL '24 hours'"#,
    )
    .bind(id)
    .fetch_one(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let users: Vec<Value> = sqlx::query_as::<_, (Uuid, Option<String>, Option<String>, String, chrono::NaiveDateTime)>(
        "SELECT id, username, email, role, created_at FROM users WHERE organization_id = $1 ORDER BY created_at ASC",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .into_iter()
    .map(|(id, username, email, role, created_at)| json!({ "id": id, "username": username, "email": email, "role": role, "created_at": created_at }))
    .collect();
    let domains: Vec<Value> = sqlx::query_as::<_, (Uuid, String, String, Option<chrono::DateTime<Utc>>)>(
        "SELECT id, hostname, status, verified_at FROM organization_domains WHERE organization_id = $1 ORDER BY created_at DESC",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .into_iter()
    .map(|(id, hostname, status, verified_at)| json!({ "id": id, "hostname": hostname, "status": status, "verified_at": verified_at }))
    .collect();
    let api_keys: Vec<Value> = sqlx::query_as::<_, (Uuid, String, Vec<String>, Option<chrono::DateTime<Utc>>, chrono::DateTime<Utc>)>(
        "SELECT id, name, scopes, last_used_at, created_at FROM api_tokens WHERE organization_id = $1 AND revoked_at IS NULL ORDER BY created_at DESC",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .into_iter()
    .map(|(id, name, scopes, last_used_at, created_at)| json!({ "id": id, "name": name, "scopes": scopes, "last_used_at": last_used_at, "created_at": created_at }))
    .collect();
    Ok(Json(json!({
        "organization": organization,
        "health_24h": { "total": health.0, "delivered": health.1, "failed": health.2, "in_flight": health.3 },
        "users": users,
        "domains": domains,
        "api_keys": api_keys,
    })))
}

#[derive(Debug, Deserialize)]
pub struct UpdateSubscription {
    pub subscriber_name: Option<String>,
    pub billing_contact_name: Option<String>,
    pub billing_contact_email: Option<String>,
    pub plan_code: Option<String>,
    pub subscription_status: Option<String>,
}

pub async fn update_platform_subscription(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<CurrentUser>,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateSubscription>,
) -> Result<Json<Value>, StatusCode> {
    require_platform_operator(&cu)?;
    let current: (String, String, String, String, String) = sqlx::query_as(
        "SELECT subscriber_name, billing_contact_name, billing_contact_email, plan_code, subscription_status FROM organizations WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;
    let subscriber_name = input
        .subscriber_name
        .unwrap_or(current.0)
        .trim()
        .to_string();
    let billing_contact_name = input
        .billing_contact_name
        .unwrap_or(current.1)
        .trim()
        .to_string();
    let billing_contact_email = input
        .billing_contact_email
        .unwrap_or(current.2)
        .trim()
        .to_string();
    let plan_code = input.plan_code.unwrap_or(current.3).trim().to_string();
    let subscription_status = input
        .subscription_status
        .unwrap_or(current.4)
        .trim()
        .to_string();
    if subscriber_name.is_empty()
        || subscriber_name.len() > 120
        || billing_contact_name.len() > 120
        || billing_contact_email.len() > 255
        || !matches!(plan_code.as_str(), "free" | "pro")
        || !matches!(
            subscription_status.as_str(),
            "active" | "trialing" | "cancelled"
        )
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    sqlx::query(
        r#"UPDATE organizations
           SET subscriber_name = $2, billing_contact_name = $3, billing_contact_email = $4,
               plan_code = $5, subscription_status = $6
           WHERE id = $1"#,
    )
    .bind(id)
    .bind(subscriber_name)
    .bind(billing_contact_name)
    .bind(billing_contact_email)
    .bind(&plan_code)
    .bind(subscription_status)
    .execute(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if plan_code == "free" {
        let _ = sqlx::query(
            "UPDATE organization_invites SET revoked_at = NOW() WHERE organization_id = $1 AND accepted_at IS NULL AND revoked_at IS NULL",
        )
        .bind(id)
        .execute(&state.db)
        .await;
        let _ = sqlx::query(
            r#"UPDATE users SET role = 'viewer'
               WHERE organization_id = $1 AND id <> (
                   SELECT id FROM users WHERE organization_id = $1 AND role = 'admin'
                   ORDER BY created_at ASC LIMIT 1
               )"#,
        )
        .bind(id)
        .execute(&state.db)
        .await;
    }
    platform_organization_detail(State(state), axum::Extension(cu), Path(id)).await
}

fn current_period() -> chrono::NaiveDate {
    Utc::now()
        .date_naive()
        .with_day(1)
        .expect("first day exists")
}
