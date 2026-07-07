//! Forward-rule CRUD handlers. All mutating handlers are admin-gated.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use uuid::Uuid;

use crate::auth;
use crate::middleware::require_admin;
use crate::model::ForwardRule;
use crate::state::AppState;

pub async fn list_rules(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<ForwardRule>>, StatusCode> {
    let rules = sqlx::query_as::<_, ForwardRule>(
        "SELECT * FROM forward_rules ORDER BY created_at ASC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rules))
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
}

pub async fn create_rule(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<auth::CurrentUser>,
    Json(input): Json<CreateRule>,
) -> Result<Json<ForwardRule>, StatusCode> {
    require_admin(&cu)?;
    let id = Uuid::new_v4();
    let pattern = input.source_pattern.unwrap_or_else(|| "*".to_string());
    let method = input.method.unwrap_or_else(|| "POST".to_string());
    let headers = input
        .headers
        .filter(|h| h.is_object())
        .unwrap_or_else(|| serde_json::json!({}));

    sqlx::query(
        r#"INSERT INTO forward_rules (id, name, source_pattern, target_url, method, headers, active)
        VALUES ($1, $2, $3, $4, $5, $6, true)"#,
    )
    .bind(id)
    .bind(&input.name)
    .bind(&pattern)
    .bind(&input.target_url)
    .bind(&method)
    .bind(&headers)
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("create rule: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let rule = sqlx::query_as::<_, ForwardRule>("SELECT * FROM forward_rules WHERE id = $1")
        .bind(id)
        .fetch_one(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(rule))
}

pub async fn delete_rule(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<auth::CurrentUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    require_admin(&cu)?;
    let res = sqlx::query("DELETE FROM forward_rules WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if res.rows_affected() > 0 {
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
}

pub async fn update_rule(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<auth::CurrentUser>,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateRule>,
) -> Result<Json<ForwardRule>, StatusCode> {
    require_admin(&cu)?;
    // Coalesce: read current row, apply overrides, write back. Simpler than
    // building a dynamic UPDATE with a variable column list.
    let current =
        sqlx::query_as::<_, ForwardRule>("SELECT * FROM forward_rules WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;

    let name = input.name.unwrap_or(current.name);
    let source_pattern = input.source_pattern.unwrap_or(current.source_pattern);
    let target_url = input.target_url.unwrap_or(current.target_url);
    let method = input.method.unwrap_or(current.method);
    let headers = input
        .headers
        .filter(|h| h.is_object())
        .unwrap_or(current.headers);
    let active = input.active.unwrap_or(current.active);

    let rule = sqlx::query_as::<_, ForwardRule>(
        r#"UPDATE forward_rules
           SET name = $2, source_pattern = $3, target_url = $4, method = $5, headers = $6, active = $7
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(id)
    .bind(&name)
    .bind(&source_pattern)
    .bind(&target_url)
    .bind(&method)
    .bind(&headers)
    .bind(active)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("update rule: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(rule))
}
