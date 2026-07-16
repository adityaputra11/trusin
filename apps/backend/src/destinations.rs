use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::auth::CurrentUser;
use crate::middleware::{require_admin, require_scope};
use crate::state::AppState;

#[derive(Serialize, sqlx::FromRow)]
pub struct Destination {
    pub kind: String,
    pub enabled: bool,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub hooks: i64,
}

#[derive(Deserialize)]
pub struct SaveDestination {
    pub kind: String,
    pub enabled: bool,
    pub config: serde_json::Value,
}

fn config_value<'a>(
    config: &'a serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<&'a str> {
    config
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn valid_http_url(value: &str) -> bool {
    reqwest::Url::parse(value)
        .map(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
        .unwrap_or(false)
}

fn normalize_config(
    kind: &str,
    config: serde_json::Value,
) -> Result<serde_json::Value, StatusCode> {
    let config = config.as_object().ok_or(StatusCode::BAD_REQUEST)?;
    match kind {
        "slack" => {
            let webhook_url = config_value(config, "webhook_url").ok_or(StatusCode::BAD_REQUEST)?;
            valid_http_url(webhook_url)
                .then_some(serde_json::json!({ "webhook_url": webhook_url }))
                .ok_or(StatusCode::BAD_REQUEST)
        }
        "telegram" => {
            let bot_token = config_value(config, "bot_token").ok_or(StatusCode::BAD_REQUEST)?;
            let chat_id = config_value(config, "chat_id").ok_or(StatusCode::BAD_REQUEST)?;
            Ok(serde_json::json!({ "bot_token": bot_token, "chat_id": chat_id }))
        }
        _ => Err(StatusCode::BAD_REQUEST),
    }
}

pub async fn list_destinations(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<CurrentUser>,
) -> Result<Json<Vec<Destination>>, StatusCode> {
    require_scope(&cu, "rules:read")?;
    let rows = sqlx::query_as::<_, Destination>(
        r#"SELECT d.kind, d.enabled, d.updated_at,
           (SELECT COUNT(*) FROM forward_rules r WHERE r.organization_id = d.organization_id AND r.rule_kind = 'hook' AND r.destination_type = d.kind) AS hooks
           FROM organization_destinations d
           WHERE d.organization_id = $1 AND d.kind IN ('slack', 'telegram')
           ORDER BY d.kind"#,
    ).bind(cu.organization_id).fetch_all(&state.db).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rows))
}

pub async fn save_destination(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<CurrentUser>,
    Json(input): Json<SaveDestination>,
) -> Result<Json<Destination>, StatusCode> {
    require_admin(&cu)?;
    require_scope(&cu, "rules:write")?;
    let kind = input.kind.trim().to_ascii_lowercase();
    if !matches!(kind.as_str(), "slack" | "telegram") || !input.config.is_object() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let retain_existing_config = input
        .config
        .as_object()
        .is_some_and(|config| config.is_empty());
    if retain_existing_config {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM organization_destinations WHERE organization_id = $1 AND kind = $2)")
            .bind(cu.organization_id)
            .bind(&kind)
            .fetch_one(&state.db)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if !exists {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    let config = if retain_existing_config {
        serde_json::json!({})
    } else {
        normalize_config(&kind, input.config)?
    };
    let row = sqlx::query_as::<_, Destination>(
        r#"INSERT INTO organization_destinations (organization_id, kind, config, enabled) VALUES ($1,$2,$3,$4)
           ON CONFLICT (organization_id, kind) DO UPDATE SET config = CASE WHEN $5 THEN organization_destinations.config ELSE EXCLUDED.config END, enabled = EXCLUDED.enabled, updated_at = NOW()
           RETURNING kind, enabled, updated_at, 0::BIGINT AS hooks"#,
    ).bind(cu.organization_id).bind(kind).bind(config).bind(input.enabled).bind(retain_existing_config).fetch_one(&state.db).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(row))
}

pub async fn test_destination(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<CurrentUser>,
    Path(kind): Path<String>,
) -> Result<StatusCode, StatusCode> {
    require_admin(&cu)?;
    require_scope(&cu, "rules:write")?;
    let kind = kind.trim().to_ascii_lowercase();
    if !matches!(kind.as_str(), "slack" | "telegram") {
        return Err(StatusCode::BAD_REQUEST);
    }
    let setting = sqlx::query_as::<_, (serde_json::Value, bool)>(
        "SELECT config, enabled FROM organization_destinations WHERE organization_id = $1 AND kind = $2",
    )
    .bind(cu.organization_id)
    .bind(&kind)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .filter(|(_, enabled)| *enabled)
    .ok_or(StatusCode::CONFLICT)?;
    let config = normalize_config(&kind, setting.0)?;
    let config = config
        .as_object()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let text = "Terusin destination test: this workspace can send hook notifications.";
    let client = reqwest::Client::new();
    let request = match kind.as_str() {
        "slack" => client
            .post(config_value(config, "webhook_url").ok_or(StatusCode::INTERNAL_SERVER_ERROR)?)
            .json(&serde_json::json!({ "text": text })),
        "telegram" => {
            let token =
                config_value(config, "bot_token").ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
            let chat_id =
                config_value(config, "chat_id").ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
            client
                .post(format!("https://api.telegram.org/bot{token}/sendMessage"))
                .json(&serde_json::json!({ "chat_id": chat_id, "text": text }))
        }
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    let response = request.send().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
    response
        .status()
        .is_success()
        .then_some(StatusCode::NO_CONTENT)
        .ok_or(StatusCode::BAD_GATEWAY)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_and_normalizes_slack_webhook_urls() {
        let config = normalize_config(
            "slack",
            serde_json::json!({ "webhook_url": "https://hooks.slack.com/services/a/b/c" }),
        )
        .unwrap();
        assert_eq!(
            config,
            serde_json::json!({ "webhook_url": "https://hooks.slack.com/services/a/b/c" })
        );
        assert_eq!(
            normalize_config(
                "slack",
                serde_json::json!({ "webhook_url": "ftp://invalid" })
            ),
            Err(StatusCode::BAD_REQUEST)
        );
    }

    #[test]
    fn telegram_requires_both_private_fields() {
        assert_eq!(
            normalize_config("telegram", serde_json::json!({ "bot_token": "secret" })),
            Err(StatusCode::BAD_REQUEST)
        );
        assert!(normalize_config(
            "telegram",
            serde_json::json!({ "bot_token": "secret", "chat_id": "-100123" })
        )
        .is_ok());
    }

    #[test]
    fn email_destinations_are_not_supported() {
        assert_eq!(
            normalize_config("email", serde_json::json!({ "recipient": "alerts@example.com" })),
            Err(StatusCode::BAD_REQUEST)
        );
    }
}
