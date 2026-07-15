//! Audit log helpers and read endpoint.

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::state::AppState;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AuditEntry {
    pub id: Uuid,
    pub actor_user_id: Option<Uuid>,
    pub actor_email: Option<String>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub metadata: Value,
    pub created_at: chrono::NaiveDateTime,
}

#[derive(Debug, Deserialize)]
pub struct AuditQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

pub async fn record(
    state: &AppState,
    actor: Option<&CurrentUser>,
    action: &str,
    resource_type: &str,
    resource_id: Option<String>,
    metadata: Value,
) {
    let actor_user_id = actor.map(|u| u.id);
    let actor_email = match actor_user_id {
        Some(id) => {
            sqlx::query_scalar::<_, Option<String>>("SELECT email FROM users WHERE id = $1")
                .bind(id)
                .fetch_optional(&state.db)
                .await
                .ok()
                .flatten()
                .flatten()
        }
        None => None,
    };

    let _ = sqlx::query(
        r#"INSERT INTO audit_logs
           (actor_user_id, actor_email, action, resource_type, resource_id, metadata)
           VALUES ($1, $2, $3, $4, $5, $6)"#,
    )
    .bind(actor_user_id)
    .bind(actor_email)
    .bind(action)
    .bind(resource_type)
    .bind(resource_id)
    .bind(metadata)
    .execute(&state.db)
    .await
    .map_err(|e| tracing::warn!("audit log insert failed: {e}"));
}

pub async fn list_audit(
    State(state): State<Arc<AppState>>,
    Query(q): Query<AuditQuery>,
) -> Result<Json<Value>, StatusCode> {
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(25).clamp(1, 100);
    let offset = (page - 1) * per_page;

    let (entries, total) = tokio::try_join!(
        sqlx::query_as::<_, AuditEntry>(
            r#"SELECT id, actor_user_id, actor_email, action, resource_type,
                      resource_id, metadata, created_at
               FROM audit_logs
               ORDER BY created_at DESC
               LIMIT $1 OFFSET $2"#,
        )
        .bind(per_page)
        .bind(offset)
        .fetch_all(&state.db),
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM audit_logs").fetch_one(&state.db),
    )
    .map_err(|e| {
        tracing::warn!("list audit: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(json!({
        "entries": entries,
        "total": total,
        "page": page,
        "per_page": per_page,
        "pages": (total as f64 / per_page as f64).ceil() as i64,
    })))
}
