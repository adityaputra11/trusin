//! Backend entry point: wiring only (modules, re-exports, CORS, main).

mod audit;
mod auth;
mod config;
mod events;
mod invites;
mod middleware;
mod model;
mod organizations;
mod platform;
mod ratelimit;
mod rules;
mod send;
mod state;
mod stats;
mod users;
mod webhook;
mod workers;

// Re-exports so `auth.rs` (and any other module using `crate::` paths) keeps
// resolving after the split. auth.rs imports `AppState`/`User` and calls
// `crate::check_rate_limit` / `crate::client_ip_from`.
pub use model::User;
pub use ratelimit::{check_rate_limit, client_ip_from};
pub use state::AppState;

use std::sync::Arc;

use axum::routing::{delete, get, patch, post};
use axum::Router;
use redis::aio::ConnectionManager;
use sqlx::postgres::PgPoolOptions;
use tracing::info;

use crate::config::{
    get_default_target, get_endpoint, get_oauth_status, health, set_default_target,
};
use crate::events::{
    ack_event, bulk_delete, bulk_retry, delete_event, event_stream, get_event, list_attempts,
    list_events, list_sources, retry_event,
};
use crate::ratelimit::build_rate_limiter;
use crate::rules::{create_rule, delete_rule, list_rules, update_rule};
use crate::send::send_webhook;
use crate::organizations::{
    bootstrap_default_organization, create_domain, current_organization, delete_domain,
    list_domains, provision_organization, retention_worker, verify_domain,
};
use crate::invites::{create_invite, list_invites, resend_invite, revoke_invite};
use crate::platform::{
    bootstrap_platform_operator, list_platform_organizations, platform_organization_detail,
    platform_overview, update_platform_subscription,
};
use crate::state::{redis_from_env, seed_default_user};
use crate::stats::metrics;
use crate::users::{list_users, update_user_role};
use crate::webhook::{handle_root, handle_webhook};
use crate::workers::{retry_worker, worker};

/// Build the CORS layer from WEB_URL env (comma-separated origins).
/// Defaults to the Vite dev server and the embedded prod binary origins.
fn build_cors_layer() -> tower_http::cors::CorsLayer {
    use tower_http::cors::{Any, CorsLayer};

    let raw = std::env::var("WEB_URL").unwrap_or_else(|_| {
        "http://localhost:5173,http://localhost:3012,http://127.0.0.1:5173,http://127.0.0.1:3012"
            .to_string()
    });

    let origins: Vec<_> = raw
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<axum::http::HeaderValue>().ok())
        .collect();

    if origins.is_empty() {
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any)
    } else {
        CorsLayer::new()
            .allow_origin(origins)
            .allow_methods([
                axum::http::Method::GET,
                axum::http::Method::POST,
                axum::http::Method::PATCH,
                axum::http::Method::DELETE,
                axum::http::Method::OPTIONS,
            ])
            .allow_headers([
                axum::http::header::AUTHORIZATION,
                axum::http::header::CONTENT_TYPE,
                axum::http::header::COOKIE,
                axum::http::HeaderName::from_static("x-webhook-source"),
            ])
            .allow_credentials(true)
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://localhost:5432/terusin".to_string());
    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "3001".to_string())
        .parse::<u16>()
        .unwrap_or(3001);
    let max_retries = std::env::var("MAX_RETRIES")
        .unwrap_or_else(|_| "5".to_string())
        .parse::<i32>()
        .unwrap_or(5);
    let default_target = std::env::var("DEFAULT_TARGET_URL").unwrap_or_default();

    let db = PgPoolOptions::new()
        .max_connections(10)
        .connect(&db_url)
        .await
        .expect("can't connect to db");

    let main_redis = ConnectionManager::new(redis_from_env())
        .await
        .expect("can't connect to redis");

    sqlx::migrate!("./migrations")
        .run(&db)
        .await
        .expect("database migrations failed");
    bootstrap_default_organization(&db, &default_target).await;
    seed_default_user(&db).await;

    let oauth = auth::OAuthConfig::from_env().map(Arc::new);
    if let Some(config) = oauth.as_ref() {
        info!(providers = ?config.enabled_providers(), "browser oauth enabled");
    } else {
        info!("google oauth disabled (set GOOGLE_CLIENT_ID / GOOGLE_CLIENT_SECRET to enable)");
    }

    let turnstile = auth::TurnstileConfig::from_env().map(Arc::new);
    if turnstile.is_some() {
        info!("turnstile captcha enabled on /api/auth/login");
    } else {
        info!("turnstile captcha disabled (set TURNSTILE_SECRET_KEY to enable)");
    }

    // Per-IP rate limiters for the auth endpoints. In-memory (per-process).
    let login_limiter = build_rate_limiter(60, 5);
    let me_limiter = build_rate_limiter(60, 30);

    let state = Arc::new(AppState {
        db: db.clone(),
        redis: main_redis,
        max_retries,
        oauth,
        turnstile,
        login_limiter: login_limiter.clone(),
        me_limiter: me_limiter.clone(),
    });

    let worker_count: usize = std::env::var("WORKER_COUNT")
        .unwrap_or_else(|_| "4".to_string())
        .parse()
        .unwrap_or(4);
    for _ in 0..worker_count {
        let w_redis = ConnectionManager::new(redis_from_env()).await.unwrap();
        tokio::spawn(worker(db.clone(), w_redis, max_retries));
    }

    let r_redis = ConnectionManager::new(redis_from_env()).await.unwrap();
    tokio::spawn(retry_worker(db, r_redis));
    tokio::spawn(retention_worker(state.db.clone()));
    tokio::spawn(auth::welcome_email_worker(state.db.clone()));

    let public = Router::new()
        .route("/config/endpoint", get(get_endpoint))
        .route("/config/oauth", get(get_oauth_status))
        // OAuth endpoints (callback must be reachable cross-origin via redirect).
        .route("/api/auth/google", get(auth::google_login))
        .route("/api/auth/callback/google", get(auth::google_callback))
        .route("/api/auth/github", get(auth::github_login))
        .route("/api/auth/callback/github", get(auth::github_callback))
        .route("/api/auth/me", get(auth::me))
        .route("/api/auth/login", post(auth::login))
        .route("/api/auth/logout", post(auth::logout));

    let bootstrap = Router::new().route(
        "/api/platform/bootstrap/operator",
        post(bootstrap_platform_operator),
    );

    let protected = Router::new()
        .route(
            "/config/default-target",
            get(get_default_target).post(set_default_target),
        )
        .route("/events", get(list_events))
        .route("/events/sources", get(list_sources))
        .route("/events/stream", get(event_stream))
        .route("/events/bulk/retry", post(bulk_retry))
        .route("/events/bulk/delete", post(bulk_delete))
        .route("/events/{id}", get(get_event).delete(delete_event))
        .route("/events/{id}/attempts", get(list_attempts))
        .route("/events/{id}/retry", post(retry_event))
        .route("/events/{id}/ack", post(ack_event))
        .route("/rules", get(list_rules).post(create_rule))
        .route("/rules/{id}", delete(delete_rule).patch(update_rule))
        .route("/stats", get(metrics))
        .route("/api/send", post(send_webhook))
        .route("/api/organization", get(current_organization))
        .route("/api/platform/overview", get(platform_overview))
        .route(
            "/api/platform/organizations",
            get(list_platform_organizations).post(provision_organization),
        )
        .route(
            "/api/platform/organizations/{id}",
            get(platform_organization_detail),
        )
        .route(
            "/api/platform/organizations/{id}/subscription",
            patch(update_platform_subscription),
        )
        .route("/api/domains", get(list_domains).post(create_domain))
        .route("/api/domains/{id}/verify", post(verify_domain))
        .route("/api/domains/{id}", delete(delete_domain))
        .route(
            "/api/api-keys",
            get(auth::list_tokens).post(auth::create_token),
        )
        .route("/api/api-keys/{id}", delete(auth::revoke_token))
        .route("/api/audit", get(audit::list_audit))
        .route("/api/users", get(list_users))
        .route("/api/users/{id}/role", patch(update_user_role))
        .route("/api/invites", get(list_invites).post(create_invite))
        .route("/api/invites/{id}/resend", post(resend_invite))
        .route("/api/invites/{id}", delete(revoke_invite))
        // API token management — any authenticated user may mint/scoped keys.
        .route(
            "/api/auth/tokens",
            get(auth::list_tokens).post(auth::create_token),
        )
        .route("/api/auth/tokens/{id}", delete(auth::revoke_token))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::middleware::auth_middleware,
        ));

    // CORS: allow the web frontend origin to call the backend API directly.
    // Default covers the Vite dev server (:5173) and the embedded prod binary
    // (:3012). Override with WEB_URL env (comma-separated for multiple).
    let cors = build_cors_layer();

    let app = Router::new()
        .route("/health", get(health))
        .merge(public)
        .merge(bootstrap)
        .merge(protected)
        .route("/", post(handle_root))
        .route("/{*source}", post(handle_webhook))
        .layer(cors)
        .with_state(state);

    let addr = format!("0.0.0.0:{port}");
    info!("backend listening on {addr}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
