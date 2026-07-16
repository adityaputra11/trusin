use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::auth;
use crate::model::{DeliveryAttempt, WebhookEvent};
use crate::state::AppState;

pub struct AiConfig {
    provider: Arc<dyn AiProvider>,
}

impl AiConfig {
    pub fn from_env() -> Result<Option<Arc<Self>>, String> {
        let enabled = std::env::var("AI_ENABLED")
            .map(|value| value.eq_ignore_ascii_case("true") || value == "1")
            .unwrap_or(false);
        if !enabled {
            return Ok(None);
        }

        let provider = std::env::var("AI_PROVIDER").unwrap_or_else(|_| "nebius".to_string());
        let (api_key, model, base_url) = match provider.to_ascii_lowercase().as_str() {
            "nebius" => (
                std::env::var("NEBIUS_API_KEY")
                    .map_err(|_| "NEBIUS_API_KEY is required when AI_ENABLED=true".to_string())?,
                std::env::var("NEBIUS_MODEL")
                    .map_err(|_| "NEBIUS_MODEL is required when AI_ENABLED=true".to_string())?,
                std::env::var("NEBIUS_BASE_URL")
                    .unwrap_or_else(|_| "https://api.studio.nebius.ai/v1".to_string()),
            ),
            "openai" => (
                std::env::var("OPENAI_API_KEY")
                    .map_err(|_| "OPENAI_API_KEY is required when AI_ENABLED=true".to_string())?,
                std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string()),
                "https://api.openai.com/v1".to_string(),
            ),
            _ => return Err(format!("unsupported AI_PROVIDER: {provider}")),
        };
        let provider = OpenAiCompatibleProvider::new(api_key, model, base_url)?;
        Ok(Some(Arc::new(Self {
            provider: Arc::new(provider),
        })))
    }

    async fn explain(&self, context: Value) -> Result<AiExplanation, AiError> {
        self.provider.explain(context).await
    }
}

trait AiProvider: Send + Sync {
    fn explain<'a>(
        &'a self,
        context: Value,
    ) -> Pin<Box<dyn Future<Output = Result<AiExplanation, AiError>> + Send + 'a>>;
}

struct OpenAiCompatibleProvider {
    http: reqwest::Client,
    api_key: String,
    model: String,
    chat_completions_url: String,
}

impl OpenAiCompatibleProvider {
    fn new(api_key: String, model: String, base_url: String) -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|error| format!("failed to build AI HTTP client: {error}"))?;
        Ok(Self {
            http,
            api_key,
            model,
            chat_completions_url: format!("{}/chat/completions", base_url.trim_end_matches('/')),
        })
    }
}

impl AiProvider for OpenAiCompatibleProvider {
    fn explain<'a>(
        &'a self,
        context: Value,
    ) -> Pin<Box<dyn Future<Output = Result<AiExplanation, AiError>> + Send + 'a>> {
        Box::pin(async move {
            let response = self
                .http
                .post(&self.chat_completions_url)
                .bearer_auth(&self.api_key)
                .json(&json!({
                    "model": self.model,
                    "temperature": 0.1,
                    "response_format": { "type": "json_object" },
                    "messages": [
                        {
                            "role": "system",
                            "content": "You are a webhook operations assistant. Diagnose only from the supplied redacted evidence. Do not invent facts. Keep recommendations actionable and advisory. A retry recommendation of safe means the evidence suggests retrying is unlikely to create duplicate side effects; caution means idempotency is unknown; not_recommended means retrying appears unsafe or irrelevant. Return a JSON object with exactly these keys: summary, likely_cause, evidence, recommended_actions, retry_recommendation. retry_recommendation must be one of safe, caution, not_recommended."
                        },
                        {
                            "role": "user",
                            "content": format!("Explain this webhook delivery event:\n{}", serde_json::to_string(&context).unwrap_or_default())
                        }
                    ]
                }))
                .send()
                .await
                .map_err(|error| AiError::Unavailable(error.to_string()))?;

            if !response.status().is_success() {
                return Err(AiError::Unavailable(format!(
                    "AI provider returned {}",
                    response.status()
                )));
            }

            let payload: Value = response
                .json()
                .await
                .map_err(|error| AiError::Unavailable(error.to_string()))?;
            let content = payload
                .pointer("/choices/0/message/content")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    AiError::InvalidResponse("missing structured response".to_string())
                })?;
            serde_json::from_str(content).map_err(|error| {
                AiError::InvalidResponse(format!("invalid structured response: {error}"))
            })
        })
    }
}

#[derive(Serialize, Deserialize)]
pub struct AiExplanation {
    pub summary: String,
    pub likely_cause: String,
    pub evidence: Vec<String>,
    pub recommended_actions: Vec<String>,
    pub retry_recommendation: RetryRecommendation,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetryRecommendation {
    Safe,
    Caution,
    NotRecommended,
}

enum AiError {
    Unavailable(String),
    InvalidResponse(String),
}

pub async fn status(State(state): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({ "enabled": state.ai.is_some() }))
}

pub async fn explain_event(
    State(state): State<Arc<AppState>>,
    axum::Extension(user): axum::Extension<auth::CurrentUser>,
    Path(id): Path<Uuid>,
) -> Response {
    let Some(ai) = state.ai.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "AI explanations are disabled or unavailable" })),
        )
            .into_response();
    };

    if let Some(response) = crate::check_user_rate_limit(&state.ai_explain_limiter, user.id) {
        return response;
    }

    let event =
        match sqlx::query_as::<_, WebhookEvent>(
            "SELECT * FROM webhook_events WHERE id = $1 AND organization_id = $2",
        )
            .bind(id)
            .bind(user.organization_id)
            .fetch_optional(&state.db)
            .await
        {
            Ok(Some(event)) => event,
            Ok(None) => return StatusCode::NOT_FOUND.into_response(),
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };
    let attempts = match sqlx::query_as::<_, DeliveryAttempt>(
        "SELECT * FROM delivery_attempts WHERE event_id = $1 ORDER BY attempt_number ASC, created_at ASC",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await
    {
        Ok(attempts) => attempts,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    match ai.explain(build_context(&event, &attempts)).await {
        Ok(explanation) => Json(explanation).into_response(),
        Err(AiError::Unavailable(error)) => {
            tracing::warn!("AI explanation unavailable: {error}");
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": "AI provider is temporarily unavailable" })),
            )
                .into_response()
        }
        Err(AiError::InvalidResponse(error)) => {
            tracing::warn!("AI explanation invalid response: {error}");
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": "AI provider returned an invalid explanation" })),
            )
                .into_response()
        }
    }
}

fn build_context(event: &WebhookEvent, attempts: &[DeliveryAttempt]) -> Value {
    json!({
        "privacy_note": "All request and response fields were automatically redacted and truncated before analysis.",
        "event": {
            "source": event.source,
            "status": event.status,
            "target_host": target_host(&event.target_url),
            "retry_count": event.retry_count,
            "max_retries": event.max_retries,
            "request_headers": redact_value(&event.headers, 0),
            "request_body": redact_value(&event.body, 0),
            "response_status": event.response_status,
            "response_headers": event.response_headers.as_ref().map(|value| redact_value(value, 0)),
            "response_body": event.response_body.as_ref().map(|value| redact_text(value)),
        },
        "delivery_attempts": attempts.iter().map(|attempt| json!({
            "number": attempt.attempt_number,
            "status": attempt.status,
            "http_status": attempt.http_status,
            "duration_ms": attempt.duration_ms,
            "error": attempt.error.as_ref().map(|value| redact_text(value)),
            "response_headers": attempt.response_headers.as_ref().map(|value| redact_value(value, 0)),
            "response_body": attempt.response_body.as_ref().map(|value| redact_text(value)),
        })).collect::<Vec<_>>(),
    })
}

fn target_host(target_url: &str) -> String {
    target_url
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(target_url)
        .split('/')
        .next()
        .unwrap_or("")
        .to_string()
}

fn redact_value(value: &Value, depth: usize) -> Value {
    if depth >= 5 {
        return Value::String("[TRUNCATED]".to_string());
    }
    match value {
        Value::Object(values) => {
            let mut redacted = Map::new();
            for (index, (key, item)) in values.iter().enumerate() {
                if index >= 30 {
                    redacted.insert("_truncated".to_string(), Value::Bool(true));
                    break;
                }
                let normalized = key.to_ascii_lowercase();
                let item = if sensitive_key(&normalized) {
                    Value::String("[REDACTED]".to_string())
                } else {
                    redact_value(item, depth + 1)
                };
                redacted.insert(key.clone(), item);
            }
            Value::Object(redacted)
        }
        Value::Array(values) => Value::Array(
            values
                .iter()
                .take(20)
                .map(|item| redact_value(item, depth + 1))
                .collect(),
        ),
        Value::String(value) => Value::String(redact_text(value)),
        value => value.clone(),
    }
}

fn sensitive_key(key: &str) -> bool {
    [
        "authorization",
        "cookie",
        "token",
        "secret",
        "password",
        "api_key",
        "apikey",
        "email",
        "name",
        "phone",
        "address",
        "ip_address",
        "ssn",
        "credit_card",
        "card_number",
    ]
    .iter()
    .any(|needle| key.contains(needle))
}

fn redact_text(value: &str) -> String {
    let mut output = value.to_string();
    for marker in ["bearer ", "basic ", "sk-", "pk_", "xoxb-", "eyJ"] {
        if let Some(index) = output
            .to_ascii_lowercase()
            .find(&marker.to_ascii_lowercase())
        {
            let prefix_end = index + marker.len();
            let suffix = &output[prefix_end..];
            let token_end = suffix
                .find(|character: char| {
                    character.is_whitespace() || character == '"' || character == '&'
                })
                .unwrap_or(suffix.len());
            output.replace_range(prefix_end..prefix_end + token_end, "[REDACTED]");
        }
    }
    let mut chars = output.chars();
    let truncated: String = chars.by_ref().take(2_000).collect();
    if chars.next().is_some() {
        format!("{truncated}[TRUNCATED]")
    } else {
        truncated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_sensitive_keys_and_tokens() {
        let value = json!({
            "authorization": "Bearer secret-value",
            "customer_email": "user@example.com",
            "note": "token sk-live-secret"
        });
        let redacted = redact_value(&value, 0);
        assert_eq!(redacted["authorization"], "[REDACTED]");
        assert_eq!(redacted["customer_email"], "[REDACTED]");
        assert!(redacted["note"].as_str().unwrap().contains("[REDACTED]"));
    }
}
