//! Public/config handlers: default-target get/set, health, OAuth status, endpoint.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use crate::auth;
use crate::middleware::{require_admin, require_scope};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct SetTarget {
    pub url: String,
}

/// Mutates global forwarding state — admin only. Previously this lived on the
/// *public* router, which let anyone redirect all webhooks.
pub async fn set_default_target(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<auth::CurrentUser>,
    Json(input): Json<SetTarget>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_admin(&cu)?;
    require_scope(&cu, "organization:manage")?;
    sqlx::query("UPDATE organizations SET default_target_url = $1 WHERE id = $2")
        .bind(&input.url)
        .bind(cu.organization_id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    crate::audit::record(
        &state,
        Some(&cu),
        "config.default_target_updated",
        "config",
        Some("default-target".to_string()),
        serde_json::json!({ "url": input.url }),
    )
    .await;
    Ok(Json(serde_json::json!({"default_target": input.url})))
}

pub async fn get_default_target(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<auth::CurrentUser>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let target = crate::organizations::default_target_for(&state.db, cu.organization_id).await?;
    Ok(Json(serde_json::json!({"default_target": target})))
}

pub async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok"}))
}

/// Public browser OAuth configuration used to show only enabled providers.
pub async fn get_oauth_status(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let providers = state
        .oauth
        .as_ref()
        .map(|config| config.enabled_providers())
        .unwrap_or_default();
    Json(serde_json::json!({ "enabled": !providers.is_empty(), "providers": providers }))
}

/// Probe ngrok's local API (port 4040) for an active public tunnel.
/// Only reachable when ngrok runs on the same host as the backend.
async fn get_ngrok_url() -> Option<String> {
    let d: serde_json::Value = reqwest::get("http://127.0.0.1:4040/api/tunnels")
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    for t in d["tunnels"].as_array()? {
        if t["proto"].as_str() == Some("https") {
            return t["public_url"].as_str().map(|s| s.to_string());
        }
    }
    d["tunnels"][0]
        .get("public_url")?
        .as_str()
        .map(|s| s.to_string())
}

/// Public endpoint info for the dashboard: the configured PUBLIC_URL plus the
/// live ngrok tunnel if one is running. Replaces the SSR web app's server-side
/// ngrok probe, which a browser cannot reach directly.
pub async fn get_endpoint() -> Json<serde_json::Value> {
    let public_url =
        std::env::var("PUBLIC_URL").unwrap_or_else(|_| "https://ingest.trusin.my.id".to_string());
    let ngrok = get_ngrok_url().await;
    Json(serde_json::json!({
        "endpoint": public_url,
        "ngrok": ngrok,
    }))
}
