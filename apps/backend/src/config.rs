//! Public/config handlers: default-target get/set, health, OAuth status, endpoint.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use crate::auth;
use crate::middleware::require_admin;
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
    *state.default_target.lock().unwrap() = input.url.clone();
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

pub async fn get_default_target(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let t = state.default_target.lock().unwrap().clone();
    Json(serde_json::json!({"default_target": t}))
}

pub async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok"}))
}

/// Public flag telling the frontend whether the "Continue with Google"
/// button should be shown. Driven by whether GOOGLE_CLIENT_ID/SECRET were
/// set at startup (state.oauth.is_some()).
pub async fn get_oauth_status(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "enabled": state.oauth.is_some() }))
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
        std::env::var("PUBLIC_URL").unwrap_or_else(|_| "https://terusin-dev.my.id".to_string());
    let ngrok = get_ngrok_url().await;
    Json(serde_json::json!({
        "endpoint": public_url,
        "ngrok": ngrok,
    }))
}
