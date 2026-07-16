use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::auth::{generate_token, hash_token, CurrentUser};
use crate::middleware::{require_admin, require_scope};
use crate::organizations::organization_allows_invites;
use crate::state::AppState;

const INVITE_TTL_DAYS: i64 = 7;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct OrganizationInvite {
    pub id: Uuid,
    pub email: String,
    pub role: String,
    pub expires_at: chrono::DateTime<Utc>,
    pub accepted_at: Option<chrono::DateTime<Utc>>,
    pub revoked_at: Option<chrono::DateTime<Utc>>,
    pub created_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateInvite {
    pub email: String,
    #[serde(default = "default_role")]
    pub role: String,
}

fn default_role() -> String {
    "viewer".to_string()
}

fn normalized_email(email: &str) -> Option<String> {
    let email = email.trim().to_ascii_lowercase();
    (email.len() <= 255 && email.contains('@') && !email.starts_with('@') && !email.ends_with('@'))
        .then_some(email)
}

fn valid_role(role: &str) -> bool {
    matches!(role, "admin" | "viewer")
}

fn app_url() -> Option<String> {
    std::env::var("APP_URL")
        .ok()
        .map(|url| url.trim_end_matches('/').to_string())
        .filter(|url| url.starts_with("http://") || url.starts_with("https://"))
}

async fn send_invite_email(email: &str, token: &str, role: &str) -> Result<(), StatusCode> {
    let api_key = std::env::var("RESEND_API_KEY")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let from = std::env::var("EMAIL_FROM")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let app_url = app_url();
    let (Some(api_key), Some(from), Some(app_url)) = (api_key, from, app_url) else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    let link = format!("{app_url}/login?invite={token}");
    let body = json!({
        "from": from,
        "to": [email],
        "subject": "You have been invited to Terusin",
        "html": format!(
            "<p>You were invited as a <strong>{role}</strong> in Terusin.</p><p><a href=\"{link}\">Accept invitation</a></p><p>This link expires in {INVITE_TTL_DAYS} days.</p>"
        )
    });
    reqwest::Client::new()
        .post("https://api.resend.com/emails")
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?
        .error_for_status()
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    Ok(())
}

pub async fn list_invites(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<CurrentUser>,
) -> Result<Json<Vec<OrganizationInvite>>, StatusCode> {
    require_admin(&cu)?;
    require_scope(&cu, "organization:manage")?;
    let invites = sqlx::query_as::<_, OrganizationInvite>(
        r#"SELECT id, email, role, expires_at, accepted_at, revoked_at, created_at
           FROM organization_invites WHERE organization_id = $1 ORDER BY created_at DESC"#,
    )
    .bind(cu.organization_id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(invites))
}

pub async fn create_invite(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<CurrentUser>,
    Json(input): Json<CreateInvite>,
) -> Result<Json<OrganizationInvite>, StatusCode> {
    require_admin(&cu)?;
    require_scope(&cu, "organization:manage")?;
    if !organization_allows_invites(&state.db, cu.organization_id).await? {
        return Err(StatusCode::PAYMENT_REQUIRED);
    }
    let email = normalized_email(&input.email).ok_or(StatusCode::BAD_REQUEST)?;
    let role = input.role.trim();
    if !valid_role(role) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let already_member: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM users WHERE organization_id = $1 AND lower(email) = $2)",
    )
    .bind(cu.organization_id)
    .bind(&email)
    .fetch_one(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if already_member {
        return Err(StatusCode::CONFLICT);
    }
    sqlx::query(
        "UPDATE organization_invites SET revoked_at = NOW() WHERE organization_id = $1 AND lower(email) = $2 AND accepted_at IS NULL AND revoked_at IS NULL",
    )
    .bind(cu.organization_id)
    .bind(&email)
    .execute(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let token = generate_token();
    let invite = sqlx::query_as::<_, OrganizationInvite>(
        r#"INSERT INTO organization_invites
              (id, organization_id, email, role, token_hash, invited_by, expires_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7)
           RETURNING id, email, role, expires_at, accepted_at, revoked_at, created_at"#,
    )
    .bind(Uuid::new_v4())
    .bind(cu.organization_id)
    .bind(&email)
    .bind(role)
    .bind(hash_token(&token))
    .bind(cu.id)
    .bind(Utc::now() + Duration::days(INVITE_TTL_DAYS))
    .fetch_one(&state.db)
    .await
    .map_err(|error| {
        tracing::warn!("create invite: {error}");
        StatusCode::CONFLICT
    })?;
    if let Err(status) = send_invite_email(&email, &token, role).await {
        let _ = sqlx::query("UPDATE organization_invites SET revoked_at = NOW() WHERE id = $1")
            .bind(invite.id)
            .execute(&state.db)
            .await;
        return Err(status);
    }
    crate::audit::record(
        &state,
        Some(&cu),
        "invite.created",
        "invite",
        Some(invite.id.to_string()),
        json!({ "email": email, "role": role }),
    )
    .await;
    Ok(Json(invite))
}

pub async fn resend_invite(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<CurrentUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<OrganizationInvite>, StatusCode> {
    require_admin(&cu)?;
    require_scope(&cu, "organization:manage")?;
    if !organization_allows_invites(&state.db, cu.organization_id).await? {
        return Err(StatusCode::PAYMENT_REQUIRED);
    }
    let token = generate_token();
    let invite = sqlx::query_as::<_, OrganizationInvite>(
        r#"UPDATE organization_invites SET token_hash = $3, expires_at = $4
           WHERE id = $1 AND organization_id = $2 AND accepted_at IS NULL AND revoked_at IS NULL
           RETURNING id, email, role, expires_at, accepted_at, revoked_at, created_at"#,
    )
    .bind(id)
    .bind(cu.organization_id)
    .bind(hash_token(&token))
    .bind(Utc::now() + Duration::days(INVITE_TTL_DAYS))
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;
    send_invite_email(&invite.email, &token, &invite.role).await?;
    crate::audit::record(
        &state,
        Some(&cu),
        "invite.resent",
        "invite",
        Some(id.to_string()),
        json!({ "email": invite.email }),
    )
    .await;
    Ok(Json(invite))
}

pub async fn revoke_invite(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<CurrentUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    require_admin(&cu)?;
    require_scope(&cu, "organization:manage")?;
    let result = sqlx::query(
        "UPDATE organization_invites SET revoked_at = NOW() WHERE id = $1 AND organization_id = $2 AND accepted_at IS NULL AND revoked_at IS NULL",
    )
    .bind(id)
    .bind(cu.organization_id)
    .execute(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    crate::audit::record(
        &state,
        Some(&cu),
        "invite.revoked",
        "invite",
        Some(id.to_string()),
        json!({}),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn invite_for_token(
    db: &sqlx::PgPool,
    token: &str,
    email: &str,
) -> Result<Option<(Uuid, String, Uuid)>, sqlx::Error> {
    let email = email.trim().to_ascii_lowercase();
    sqlx::query_as::<_, (Uuid, String, Uuid)>(
        r#"SELECT id, role, organization_id FROM organization_invites
           WHERE token_hash = $1 AND lower(email) = $2 AND accepted_at IS NULL
             AND revoked_at IS NULL AND expires_at > NOW()"#,
    )
    .bind(hash_token(token))
    .bind(email)
    .fetch_optional(db)
    .await
}
