//! Single-workspace user administration.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::auth;
use crate::middleware::{require_admin, require_scope};
use crate::state::AppState;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct WorkspaceUser {
    pub id: Uuid,
    pub username: Option<String>,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub oauth_provider: Option<String>,
    pub role: String,
    pub created_at: chrono::NaiveDateTime,
}

#[derive(Debug, Deserialize)]
pub struct UpdateRole {
    pub role: String,
}

pub async fn list_users(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<auth::CurrentUser>,
) -> Result<Json<Vec<WorkspaceUser>>, StatusCode> {
    require_admin(&cu)?;
    require_scope(&cu, "organization:manage")?;
    let users = sqlx::query_as::<_, WorkspaceUser>(
        r#"SELECT id, username, email, display_name, avatar_url,
                  oauth_provider, role, created_at
           FROM users
           WHERE organization_id = $1
           ORDER BY created_at ASC"#,
    )
    .bind(cu.organization_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::warn!("list users: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(users))
}

pub async fn update_user_role(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<auth::CurrentUser>,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateRole>,
) -> Result<Json<WorkspaceUser>, StatusCode> {
    require_admin(&cu)?;
    require_scope(&cu, "organization:manage")?;
    let role = input.role.trim();
    if role != "admin" && role != "viewer" {
        return Err(StatusCode::BAD_REQUEST);
    }

    let admin_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE organization_id = $1 AND role = 'admin'")
        .bind(cu.organization_id)
        .fetch_one(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if cu.id == id && role != "admin" && admin_count <= 1 {
        return Err(StatusCode::CONFLICT);
    }

    let user = sqlx::query_as::<_, WorkspaceUser>(
        r#"UPDATE users SET role = $2
           WHERE id = $1 AND organization_id = $3
           RETURNING id, username, email, display_name, avatar_url,
                     oauth_provider, role, created_at"#,
    )
    .bind(id)
    .bind(role)
    .bind(cu.organization_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::warn!("update user role: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    crate::audit::record(
        &state,
        Some(&cu),
        "user.role_updated",
        "user",
        Some(id.to_string()),
        json!({ "role": role }),
    )
    .await;

    Ok(Json(user))
}
