use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::middleware::{require_admin, require_scope};
use crate::state::AppState;

#[derive(Serialize, sqlx::FromRow)]
pub struct WeeklyDigestSettings {
    pub enabled: bool,
    pub last_sent_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Deserialize)]
pub struct UpdateWeeklyDigest {
    pub enabled: bool,
}

pub async fn get_weekly_digest(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<CurrentUser>,
) -> Result<Json<WeeklyDigestSettings>, StatusCode> {
    require_scope(&cu, "rules:read")?;
    let settings = sqlx::query_as::<_, WeeklyDigestSettings>(
        "SELECT enabled, last_sent_at FROM organization_weekly_digests WHERE organization_id = $1",
    )
    .bind(cu.organization_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .unwrap_or(WeeklyDigestSettings {
        enabled: false,
        last_sent_at: None,
    });
    Ok(Json(settings))
}

pub async fn update_weekly_digest(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<CurrentUser>,
    Json(input): Json<UpdateWeeklyDigest>,
) -> Result<Json<WeeklyDigestSettings>, StatusCode> {
    require_admin(&cu)?;
    require_scope(&cu, "rules:write")?;
    let settings = sqlx::query_as::<_, WeeklyDigestSettings>(
        r#"INSERT INTO organization_weekly_digests (organization_id, enabled, last_sent_at)
           VALUES ($1, $2, CASE WHEN $2 THEN NOW() ELSE NULL END)
           ON CONFLICT (organization_id) DO UPDATE
           SET enabled = EXCLUDED.enabled,
               last_sent_at = CASE WHEN EXCLUDED.enabled AND NOT organization_weekly_digests.enabled THEN NOW() WHEN NOT EXCLUDED.enabled THEN NULL ELSE organization_weekly_digests.last_sent_at END,
               updated_at = NOW()
           RETURNING enabled, last_sent_at"#,
    )
    .bind(cu.organization_id)
    .bind(input.enabled)
    .fetch_one(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(settings))
}

#[derive(sqlx::FromRow)]
struct DigestRecipient {
    organization_id: Uuid,
    organization_name: String,
    recipient: String,
}

#[derive(sqlx::FromRow)]
struct DigestStats {
    received: i64,
    delivered: i64,
    failed: i64,
}

fn digest_text(name: &str, stats: &DigestStats) -> String {
    let rate = if stats.received == 0 {
        100.0
    } else {
        stats.delivered as f64 / stats.received as f64 * 100.0
    };
    format!("{name} weekly reliability digest\n\nEvents received: {}\nDelivered: {}\nFailed: {}\nDelivery rate: {rate:.1}%\n\nOpen trusin to inspect delivery details and retry failed events.", stats.received, stats.delivered, stats.failed)
}

pub async fn weekly_digest_worker(db: sqlx::PgPool) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(60 * 60));
    loop {
        interval.tick().await;
        let Ok(api_key) = std::env::var("RESEND_API_KEY") else {
            continue;
        };
        let Ok(from) = std::env::var("EMAIL_FROM") else {
            continue;
        };
        if api_key.trim().is_empty() || from.trim().is_empty() {
            continue;
        }
        let recipients = sqlx::query_as::<_, DigestRecipient>(
            r#"SELECT digest.organization_id, organization.name AS organization_name, destination.config->>'recipient' AS recipient
               FROM organization_weekly_digests digest
               JOIN organizations organization ON organization.id = digest.organization_id
               JOIN organization_destinations destination ON destination.organization_id = digest.organization_id AND destination.kind = 'email' AND destination.enabled = true
               WHERE digest.enabled = true AND digest.last_sent_at <= NOW() - INTERVAL '7 days'"#,
        )
        .fetch_all(&db)
        .await
        .unwrap_or_default();
        let client = reqwest::Client::new();
        for recipient in recipients {
            let stats = sqlx::query_as::<_, DigestStats>(
                r#"SELECT COUNT(*)::BIGINT AS received,
                          COUNT(*) FILTER (WHERE status = 'delivered')::BIGINT AS delivered,
                          COUNT(*) FILTER (WHERE status = 'failed')::BIGINT AS failed
                   FROM webhook_events WHERE organization_id = $1 AND created_at >= NOW() - INTERVAL '7 days'"#,
            )
            .bind(recipient.organization_id)
            .fetch_one(&db)
            .await;
            let Ok(stats) = stats else { continue };
            let response = client
                .post("https://api.resend.com/emails")
                .bearer_auth(&api_key)
                .json(&serde_json::json!({ "from": from, "to": [recipient.recipient], "subject": format!("{} weekly reliability digest", recipient.organization_name), "text": digest_text(&recipient.organization_name, &stats) }))
                .send()
                .await;
            if response.is_ok_and(|response| response.status().is_success()) {
                let _ = sqlx::query("UPDATE organization_weekly_digests SET last_sent_at = NOW(), updated_at = NOW() WHERE organization_id = $1")
                    .bind(recipient.organization_id)
                    .execute(&db)
                    .await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_includes_delivery_counts() {
        let text = digest_text(
            "Acme",
            &DigestStats {
                received: 10,
                delivered: 9,
                failed: 1,
            },
        );
        assert!(text.contains("Delivery rate: 90.0%"));
        assert!(text.contains("Failed: 1"));
    }
}
