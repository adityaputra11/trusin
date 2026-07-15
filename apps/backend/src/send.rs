//! Authenticated dashboard webhook composer.

use std::sync::Arc;

use axum::extract::{Extension, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use crate::auth;
use crate::middleware::{require_admin, require_scope};
use crate::model::ForwardRule;
use crate::state::AppState;
use crate::webhook::{enqueue_event, validate_target_url};

#[derive(Debug, Deserialize)]
pub struct SendWebhookRequest {
    pub provider_id: Option<Uuid>,
    pub source: Option<String>,
    pub target_url: Option<String>,
    pub body: Value,
}

type SendResult = Result<Json<Value>, (StatusCode, Json<Value>)>;

fn error(status: StatusCode, message: &str) -> (StatusCode, Json<Value>) {
    (status, Json(serde_json::json!({ "error": message })))
}

fn enqueue_error(status: StatusCode) -> (StatusCode, Json<Value>) {
    if status == StatusCode::TOO_MANY_REQUESTS {
        return (
            status,
            Json(serde_json::json!({
                "error": "event_quota_exceeded",
                "message": "Your Free plan has reached its monthly event limit.",
                "limit": crate::organizations::FREE_EVENT_LIMIT,
                "reset_at": crate::organizations::next_event_quota_reset(),
            })),
        );
    }
    error(status, "Could not queue the webhook. Please try again.")
}

pub async fn send_webhook(
    State(state): State<Arc<AppState>>,
    Extension(cu): Extension<auth::CurrentUser>,
    Json(input): Json<SendWebhookRequest>,
) -> SendResult {
    require_admin(&cu).map_err(|status| error(status, "admin access required"))?;
    require_scope(&cu, "webhooks:send")
        .map_err(|status| error(status, "API key lacks webhooks:send scope"))?;

    let (source, target_url) = if let Some(provider_id) = input.provider_id {
        if input
            .source
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
            || input
                .target_url
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
        {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "provider_id cannot be combined with manual source or target_url",
            ));
        }

        let rule = sqlx::query_as::<_, ForwardRule>(
            "SELECT * FROM forward_rules WHERE id = $1 AND organization_id = $2 AND active = true AND name <> 'Default'",
        )
        .bind(provider_id)
        .bind(cu.organization_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "failed to load provider"))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "active provider not found"))?;

        let source = if rule.source_pattern == "*" {
            rule.name
        } else {
            rule.source_pattern
        };
        let target_url = rule.target_url.trim().to_string();
        if target_url.is_empty() {
            return Err(error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "selected provider has no target URL",
            ));
        }
        (source, target_url)
    } else {
        let source = input.source.unwrap_or_default().trim().to_string();
        if source.len() > 255 {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "source must be 255 characters or fewer",
            ));
        }
        let source = if source.is_empty() {
            "unknown".to_string()
        } else {
            source
        };
        let target_url = input
            .target_url
            .filter(|value| !value.trim().is_empty())
            .map(|value| value.trim().to_string())
            .or(crate::organizations::default_target_for(&state.db, cu.organization_id).await.ok())
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                error(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "target_url is required when no default target is configured",
                )
            })?;
        (source, target_url)
    };

    validate_target_url(&target_url)
        .await
        .map_err(|message| error(StatusCode::BAD_REQUEST, message))?;

    let headers = serde_json::json!({
        "content-type": "application/json",
        "x-webhook-source": source.clone(),
    });
    enqueue_event(
        &state,
        cu.organization_id,
        source,
        headers,
        input.body,
        target_url,
    )
    .await
    .map_err(enqueue_error)
}
