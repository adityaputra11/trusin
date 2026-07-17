//! Shared `AppState`, default-user seed, Redis client construction.

use std::sync::Arc;

use redis::aio::ConnectionManager;
use tracing::info;
use uuid::Uuid;

use crate::ai::AiConfig;
use crate::auth::{OAuthConfig, PasskeyConfig, TurnstileConfig};
use crate::model::User;
use crate::ratelimit::{KeyedLimiter, UserKeyedLimiter};

/// Shared state cloned into every handler via `Arc<AppState>`. All fields are
/// `pub` because sibling modules (handlers, workers, `auth`) read them
/// directly.
pub struct AppState {
    pub db: sqlx::PgPool,
    pub redis: ConnectionManager,
    pub max_retries: i32,
    /// Present when Google OAuth is configured (GOOGLE_CLIENT_ID/SECRET set).
    pub oauth: Option<Arc<OAuthConfig>>,
    pub passkey: Option<Arc<PasskeyConfig>>,
    /// Present when Cloudflare Turnstile is configured (TURNSTILE_SECRET_KEY set).
    pub turnstile: Option<Arc<TurnstileConfig>>,
    pub ai: Option<Arc<AiConfig>>,
    /// Per-IP limiter shared by password and OAuth sign-in starts (5/10 min).
    /// Handlers call `check_key` directly without a separate middleware layer.
    pub login_limiter: Arc<KeyedLimiter>,
    /// Per-IP rate limiter for /api/auth/me (30/min — called on every page load).
    pub me_limiter: Arc<KeyedLimiter>,
    pub ai_explain_limiter: Arc<UserKeyedLimiter>,
}

/// Seed the legacy admin/password user from `AUTH_USERNAME` / `AUTH_PASSWORD`
/// if set and not already present. Idempotent.
pub async fn seed_default_user(db: &sqlx::PgPool) {
    let user = std::env::var("AUTH_USERNAME");
    let pass = std::env::var("AUTH_PASSWORD");
    if let (Ok(username), Ok(password)) = (user, pass) {
        let exists = sqlx::query_as::<_, User>("SELECT * FROM users WHERE username = $1")
            .bind(&username)
            .fetch_optional(db)
            .await
            .ok()
            .flatten()
            .is_some();

        if !exists {
            let organization_id: Option<Uuid> =
                sqlx::query_scalar("SELECT id FROM organizations WHERE slug = 'default'")
                    .fetch_optional(db)
                    .await
                    .ok()
                    .flatten();
            let Some(organization_id) = organization_id else {
                tracing::warn!("default organization missing; skipping default user seed");
                return;
            };
            let hash = tokio::task::spawn_blocking(move || bcrypt::hash(&password, 10))
                .await
                .expect("join error")
                .expect("bcrypt hash");
            sqlx::query(
                "INSERT INTO users (id, organization_id, username, password_hash, role) VALUES ($1, $2, $3, $4, 'admin')",
            )
            .bind(Uuid::new_v4())
            .bind(organization_id)
            .bind(&username)
            .bind(&hash)
            .execute(db)
            .await
            .ok();
            info!("seeded default user: {username}");
        }
    }
}

/// Build a Redis client from `REDIS_URL` (default `redis://127.0.0.1:6379`).
pub fn redis_from_env() -> redis::Client {
    let url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
    redis::Client::open(url).expect("invalid redis url")
}
