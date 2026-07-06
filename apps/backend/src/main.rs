use axum::{
    body::Body,
    extract::{Path, Query, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use base64::Engine;
use bytes::Bytes;
use chrono::Utc;
use hmac::{Hmac, Mac};
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Sha256;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use tokio_stream::StreamExt;
use tracing::info;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

const QUEUE_KEY: &str = "terusin:queue";
const RETRY_KEY: &str = "terusin:retry";

/// Compute `sha256=<hex>` HMAC-SHA256 signature of the request body bytes.
/// Receivers verify by recomputing over the raw body using the shared secret.
fn sign_body(secret: &str, body: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("hmac key any length");
    mac.update(body);
    format!("sha256={}", hex_encode(mac.finalize().into_bytes().as_slice()))
}

/// Lowercase hex encoding (no external dep).
fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// Persist a single delivery attempt for the per-event timeline. Best-effort:
/// logging must never break delivery. `attempt_number` is 1-based and reflects
/// the event's retry_count *before* this attempt (so the first try is #1).
#[allow(clippy::too_many_arguments)]
async fn record_attempt(
    db: &sqlx::PgPool,
    event_id: Uuid,
    attempt_number: i32,
    status: &str,
    http_status: Option<i32>,
    response_headers: Option<&serde_json::Value>,
    response_body: Option<&str>,
    error: Option<&str>,
    duration_ms: Option<i32>,
) {
    let _ = sqlx::query(
        r#"INSERT INTO delivery_attempts
           (event_id, attempt_number, status, http_status, response_headers, response_body, error, duration_ms)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
    )
    .bind(event_id)
    .bind(attempt_number)
    .bind(status)
    .bind(http_status)
    .bind(response_headers)
    .bind(response_body)
    .bind(error)
    .bind(duration_ms)
    .execute(db)
    .await
    .map(|_| ());
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
struct WebhookEvent {
    id: Uuid,
    source: String,
    headers: serde_json::Value,
    body: serde_json::Value,
    status: String,
    target_url: String,
    retry_count: i32,
    max_retries: i32,
    created_at: chrono::NaiveDateTime,
    response_status: Option<i32>,
    response_headers: Option<serde_json::Value>,
    response_body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
struct ForwardRule {
    id: Uuid,
    name: String,
    source_pattern: String,
    target_url: String,
    method: String,
    headers: serde_json::Value,
    active: bool,
    /// Per-rule HMAC secret used to sign outbound deliveries. Never serialized
    /// to API clients (would leak the secret to anyone with read access to
    /// /rules). `sqlx::FromRow` ignores serde attrs and still populates this
    /// from the DB column for internal use in `build_rule_request`.
    #[serde(skip)]
    #[sqlx(default)]
    signing_secret: Option<String>,
}

/// One outbound delivery attempt. Used for the per-event retry timeline.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
struct DeliveryAttempt {
    id: Uuid,
    event_id: Uuid,
    attempt_number: i32,
    status: String,
    http_status: Option<i32>,
    response_headers: Option<serde_json::Value>,
    response_body: Option<String>,
    error: Option<String>,
    duration_ms: Option<i32>,
    created_at: chrono::NaiveDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
struct User {
    id: Uuid,
    username: Option<String>,
    password_hash: Option<String>,
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    avatar_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    oauth_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    oauth_subject: Option<String>,
}

struct AppState {
    db: sqlx::PgPool,
    redis: ConnectionManager,
    max_retries: i32,
    default_target: std::sync::Mutex<String>,
    /// Present when Google OAuth is configured (GOOGLE_CLIENT_ID/SECRET set).
    oauth: Option<Arc<auth::OAuthConfig>>,
    /// Present when Cloudflare Turnstile is configured (TURNSTILE_SECRET_KEY set).
    turnstile: Option<Arc<auth::TurnstileConfig>>,
    /// Per-IP rate limiter for the login endpoint (5/min). Shared so handlers
    /// can call `check_key` directly without a separate middleware layer.
    login_limiter: Arc<KeyedLimiter>,
    /// Per-IP rate limiter for /api/auth/me (30/min — called on every page load).
    me_limiter: Arc<KeyedLimiter>,
}

mod auth;

fn headers_to_json(headers: &HeaderMap) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for (k, v) in headers.iter() {
        if let Ok(s) = v.to_str() {
            map.insert(k.to_string(), serde_json::Value::String(s.to_string()));
        }
    }
    serde_json::Value::Object(map)
}

fn unauth() -> Response {
    let mut res = (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    res.headers_mut().insert(
        "WWW-Authenticate",
        "Basic realm=\"Terusin\"".parse().unwrap(),
    );
    res
}

/// Returns Ok if `cu` is an admin, else a 403. Handlers that mutate state
/// call this on the extracted `Extension<auth::CurrentUser>`. Works for both
/// `StatusCode` and `Response` return types via `IntoResponse`.
fn require_admin(cu: &auth::CurrentUser) -> Result<(), StatusCode> {
    if cu.role == "admin" {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, Response> {
    // 1) Try the session JWT cookie first (Google OAuth users).
    if let Some(cfg) = &state.oauth {
        if let Some(cookie) = req.headers().get("Cookie").and_then(|v| v.to_str().ok()) {
            if let Some(token) = extract_session_cookie(cookie) {
                if let Some(uid) = auth::verify_jwt(&token, &cfg.jwt_secret) {
                    let user =
                        sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
                            .bind(uid)
                            .fetch_optional(&state.db)
                            .await
                            .map_err(|_| unauth())?;
                    if let Some(u) = user {
                        req.extensions_mut().insert(auth::CurrentUser {
                            id: u.id,
                            role: u.role.clone(),
                        });
                        return Ok(next.run(req).await);
                    }
                }
            }
        }
    }

    // 2) Try a Bearer API token (CLI / MCP pairing).
    let header = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if let Some(bearer) = header.strip_prefix("Bearer ").map(str::trim) {
        if bearer.starts_with("ts_") {
            if let Some(cu) = auth::authenticate_bearer(&state, bearer).await {
                req.extensions_mut().insert(cu);
                return Ok(next.run(req).await);
            }
        }
    }

    // 3) Fall back to HTTP Basic auth (CLI / MCP / password logins).
    let creds = header.strip_prefix("Basic ").and_then(|encoded| {
        base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .and_then(|s| {
                let mut parts = s.splitn(2, ':');
                Some((parts.next()?.to_string(), parts.next()?.to_string()))
            })
    });

    match creds {
        Some((user, pass)) => {
            let db_user = sqlx::query_as::<_, User>(
                "SELECT * FROM users WHERE username = $1",
            )
            .bind(&user)
            .fetch_optional(&state.db)
            .await
            .map_err(|_| unauth())?;

            match db_user {
                Some(u)
                    if u.password_hash
                        .as_ref()
                        .map(|h| bcrypt::verify(&pass, h).unwrap_or(false))
                        .unwrap_or(false) =>
                {
                    req.extensions_mut().insert(auth::CurrentUser {
                        id: u.id,
                        role: u.role.clone(),
                    });
                    Ok(next.run(req).await)
                }
                _ => Err(unauth()),
            }
        }
        None => Err(unauth()),
    }
}

/// Pull the value of the `terusin_session` cookie out of a Cookie header.
fn extract_session_cookie(cookie_header: &str) -> Option<String> {
    for kv in cookie_header.split(';') {
        let kv = kv.trim();
        if let Some(rest) = kv.strip_prefix(&format!("{}=", auth::COOKIE_NAME)) {
            return Some(rest.to_string());
        }
    }
    None
}

async fn seed_default_user(db: &sqlx::PgPool) {
    let user = std::env::var("AUTH_USERNAME");
    let pass = std::env::var("AUTH_PASSWORD");
    if let (Ok(username), Ok(password)) = (user, pass) {
        let username = username;
        let exists = sqlx::query_as::<_, User>("SELECT * FROM users WHERE username = $1")
            .bind(&username)
            .fetch_optional(db)
            .await
            .ok()
            .flatten()
            .is_some();

        if !exists {
            let hash = tokio::task::spawn_blocking(move || bcrypt::hash(&password, 10))
                .await
                .expect("join error")
                .expect("bcrypt hash");
            sqlx::query(
                "INSERT INTO users (id, username, password_hash, role) VALUES ($1, $2, $3, 'admin')",
            )
            .bind(Uuid::new_v4())
            .bind(&username)
            .bind(&hash)
            .execute(db)
            .await
            .ok();
            info!("seeded default user: {username}");
        }
    }
}

async fn handle_root(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    handle_webhook_inner(state, "".to_string(), headers, payload).await
}

async fn handle_webhook(
    State(state): State<Arc<AppState>>,
    Path(source_path): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    handle_webhook_inner(state, source_path, headers, payload).await
}

fn extract_source(path: &str) -> String {
    let p = path.trim_matches('/');
    if p.is_empty() {
        return "unknown".into();
    }
    p.split('/').next().unwrap_or("unknown").to_string()
}

async fn handle_webhook_inner(
    state: Arc<AppState>,
    source_path: String,
    headers: HeaderMap,
    payload: serde_json::Value,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let source = match headers.get("X-Webhook-Source").and_then(|v| v.to_str().ok()) {
        Some(s) if !s.is_empty() && s != "unknown" => s.to_string(),
        _ => extract_source(&source_path),
    };

    let rule_target: Option<String> = sqlx::query_as::<_, ForwardRule>(
        "SELECT * FROM forward_rules WHERE source_pattern = $1 AND active = true LIMIT 1",
    )
    .bind(&source)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten()
    .map(|r| r.target_url)
    .filter(|u| !u.is_empty());

    let target_url = headers
        .get("X-Target-Url")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| rule_target)
        .unwrap_or_else(|| state.default_target.lock().unwrap().clone());

    let id = Uuid::new_v4();
    let now = Utc::now().naive_utc();

    sqlx::query(
        r#"INSERT INTO webhook_events (id, source, headers, body, status, target_url, retry_count, max_retries, created_at)
        VALUES ($1, $2, $3, $4, 'queued', $5, 0, $6, $7)"#,
    )
    .bind(id)
    .bind(&source)
    .bind(headers_to_json(&headers))
    .bind(&payload)
    .bind(&target_url)
    .bind(state.max_retries)
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("db insert: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut conn = state.redis.clone();
    redis::cmd("LPUSH")
        .arg(QUEUE_KEY)
        .arg(id.to_string())
        .query_async::<()>(&mut conn)
        .await
        .ok();

    Ok(Json(serde_json::json!({"id": id, "status": "queued"})))
}

async fn worker(db: sqlx::PgPool, mut redis: ConnectionManager, max_retries: i32) {
    let client = reqwest::Client::new();
    // Optional global signing secret applied to every main-target delivery.
    let default_signing_secret = std::env::var("DEFAULT_SIGNING_SECRET").ok();
    loop {
        let result: Option<(String, String)> = redis::cmd("BRPOP")
            .arg(QUEUE_KEY)
            .arg(5)
            .query_async(&mut redis)
            .await
            .ok()
            .flatten();

        let Some((_, id_str)) = result else { continue };
        let id: Uuid = match id_str.parse() {
            Ok(id) => id,
            Err(_) => continue,
        };

        let event = sqlx::query_as::<_, WebhookEvent>("SELECT * FROM webhook_events WHERE id = $1")
            .bind(id)
            .fetch_optional(&db)
            .await;

        let Some(event) = event.unwrap_or(None) else { continue };

        if event.target_url.is_empty() {
            tracing::warn!("skip {id}: no target URL");
            sqlx::query("UPDATE webhook_events SET status = 'failed' WHERE id = $1")
                .bind(id).execute(&db).await.ok();
            continue;
        }

        let body_bytes = serde_json::to_vec(&event.body).unwrap_or_default();
        let mut req = client
            .post(&event.target_url)
            .header("content-type", "application/json")
            .body(body_bytes.clone());
        if let Some(secret) = &default_signing_secret {
            if !secret.is_empty() {
                req = req.header("X-Terusin-Signature", sign_body(secret, &body_bytes));
            }
        }
        let started = tokio::time::Instant::now();
        let res = req.send().await;
        let duration_ms = Some(started.elapsed().as_millis() as i32);
        // attempt_number is 1-based: first try is #1, matches retry_count before increment.
        let attempt_number = event.retry_count + 1;

        let already = || async {
            sqlx::query_scalar::<_, String>("SELECT status FROM webhook_events WHERE id = $1")
                .bind(id).fetch_optional(&db).await.ok().flatten().unwrap_or_default() == "delivered"
        };

        match res {
            Ok(r) => {
                let status = r.status().as_u16() as i32;
                let mut resp_h = serde_json::Map::new();
                for (k, v) in r.headers() {
                    resp_h.insert(k.to_string(), serde_json::Value::String(v.to_str().unwrap_or("").to_string()));
                }
                let resp_h = serde_json::Value::Object(resp_h);
                let resp_b = r.text().await.ok();
                let resp_b_ref = resp_b.as_deref();

                if status < 300 {
                    record_attempt(
                        &db, id, attempt_number, "delivered", Some(status),
                        Some(&resp_h), resp_b_ref, None, duration_ms,
                    ).await;
                    sqlx::query(
                        "UPDATE webhook_events SET status = 'delivered', response_status = $1, response_headers = $2, response_body = $3 WHERE id = $4 AND status != 'delivered'",
                    )
                    .bind(status).bind(&resp_h).bind(&resp_b).bind(id)
                    .execute(&db).await.ok();
                    info!("delivered {id} -> {} ({})", event.target_url, status);
                    forward_to_rules(&db, &event, &client).await;
                } else {
                    record_attempt(
                        &db, id, attempt_number, "failed", Some(status),
                        Some(&resp_h), resp_b_ref, None, duration_ms,
                    ).await;
                    sqlx::query(
                        "UPDATE webhook_events SET status = 'failed', response_status = $1, response_headers = $2, response_body = $3 WHERE id = $4 AND status != 'delivered'",
                    )
                    .bind(status).bind(&resp_h).bind(&resp_b).bind(id)
                    .execute(&db).await.ok();
                    tracing::warn!("failed {id} -> {} ({})", event.target_url, status);
                }
            }
            Err(e) => {
                if already().await { continue; }
                let err_msg = e.to_string();
                let retry_count = event.retry_count + 1;
                if retry_count > max_retries {
                    record_attempt(
                        &db, id, attempt_number, "failed", None, None, None,
                        Some(&err_msg), duration_ms,
                    ).await;
                    sqlx::query(
                        "UPDATE webhook_events SET status = 'failed', retry_count = $1 WHERE id = $2 AND status != 'delivered'",
                    )
                    .bind(retry_count).bind(id)
                    .execute(&db).await.ok();
                    tracing::warn!("failed {id} after {retry_count} attempts");
                } else {
                    record_attempt(
                        &db, id, attempt_number, "retrying", None, None, None,
                        Some(&err_msg), duration_ms,
                    ).await;
                    sqlx::query(
                        "UPDATE webhook_events SET status = 'retrying', retry_count = $1 WHERE id = $2 AND status != 'delivered'",
                    )
                    .bind(retry_count).bind(id)
                    .execute(&db).await.ok();
                    let delay = 10 * 2u64.pow(retry_count as u32);
                    let retry_at = Utc::now().timestamp() as i64 + delay as i64;
                    redis::cmd("ZADD")
                        .arg(RETRY_KEY).arg(retry_at).arg(id.to_string())
                        .query_async::<()>(&mut redis).await.ok();
                    info!("queued {id} for retry #{retry_count} in {delay}s");
                }
            }
        }
    }
}

async fn retry_worker(db: sqlx::PgPool, mut redis: ConnectionManager) {
    let client = reqwest::Client::new();
    let default_signing_secret = std::env::var("DEFAULT_SIGNING_SECRET").ok();
    loop {
        let result: Option<(String, String)> = redis::cmd("ZPOPMIN")
            .arg(RETRY_KEY)
            .arg(1)
            .query_async(&mut redis)
            .await
            .ok()
            .flatten();

        match result {
            Some((id_str, score)) => {
                let now = Utc::now().timestamp() as f64;
                if score.parse::<f64>().unwrap_or(0.0) > now {
                    redis::cmd("ZADD")
                        .arg(RETRY_KEY)
                        .arg(score)
                        .arg(&id_str)
                        .query_async::<()>(&mut redis)
                        .await
                        .ok();
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    continue;
                }

                let id: Uuid = match id_str.parse() {
                    Ok(id) => id,
                    Err(_) => continue,
                };

                let event =
                    sqlx::query_as::<_, WebhookEvent>("SELECT * FROM webhook_events WHERE id = $1")
                        .bind(id)
                        .fetch_optional(&db)
                        .await;

                let Some(event) = event.unwrap_or(None) else { continue };

                let body_bytes = serde_json::to_vec(&event.body).unwrap_or_default();
                let mut req = client
                    .post(&event.target_url)
                    .header("content-type", "application/json")
                    .body(body_bytes.clone());
                if let Some(secret) = &default_signing_secret {
                    if !secret.is_empty() {
                        req = req.header("X-Terusin-Signature", sign_body(secret, &body_bytes));
                    }
                }
                let started = tokio::time::Instant::now();
                let res = req.send().await;
                let duration_ms = Some(started.elapsed().as_millis() as i32);
                let attempt_number = event.retry_count + 1;

                let is_delivered = sqlx::query_scalar::<_, String>("SELECT status FROM webhook_events WHERE id = $1")
                    .bind(id).fetch_optional(&db).await.ok().flatten().unwrap_or_default() == "delivered";
                if is_delivered { continue; }

                match res {
                    Ok(r) => {
                        let status = r.status().as_u16() as i32;
                        let ok = status < 300;
                        let mut resp_h = serde_json::Map::new();
                        for (k, v) in r.headers() {
                            resp_h.insert(k.to_string(), serde_json::Value::String(v.to_str().unwrap_or("").to_string()));
                        }
                        let resp_h = serde_json::Value::Object(resp_h);
                        let resp_b = r.text().await.ok();
                        let resp_b_ref = resp_b.as_deref();

                        if ok {
                            record_attempt(
                                &db, id, attempt_number, "delivered", Some(status),
                                Some(&resp_h), resp_b_ref, None, duration_ms,
                            ).await;
                            sqlx::query(
                                "UPDATE webhook_events SET status = 'delivered', response_status = $1, response_headers = $2, response_body = $3 WHERE id = $4 AND status != 'delivered'",
                            )
                            .bind(status).bind(&resp_h).bind(&resp_b).bind(id)
                            .execute(&db).await.ok();
                            info!("retry delivered {id}");
                            forward_to_rules(&db, &event, &client).await;
                        } else {
                            record_attempt(
                                &db, id, attempt_number, "failed", Some(status),
                                Some(&resp_h), resp_b_ref, None, duration_ms,
                            ).await;
                            let retry_count = event.retry_count + 1;
                            if retry_count > event.max_retries {
                                sqlx::query(
                                    "UPDATE webhook_events SET status = 'failed', retry_count = $1 WHERE id = $2 AND status != 'delivered'",
                                )
                                .bind(retry_count)
                                .bind(id)
                                .execute(&db)
                                .await
                                .ok();
                                tracing::warn!("retry failed {id} after {retry_count} attempts");
                            } else {
                                let delay = 10 * 2u64.pow(retry_count as u32);
                                let retry_at = Utc::now().timestamp() as i64 + delay as i64;
                                redis::cmd("ZADD")
                                    .arg(RETRY_KEY)
                                    .arg(retry_at)
                                    .arg(id.to_string())
                                    .query_async::<()>(&mut redis)
                                    .await
                                    .ok();
                            }
                        }
                    }
                    Err(e) => {
                        let err_msg = e.to_string();
                        let retry_count = event.retry_count + 1;
                        if retry_count > event.max_retries {
                            record_attempt(
                                &db, id, attempt_number, "failed", None, None, None,
                                Some(&err_msg), duration_ms,
                            ).await;
                            sqlx::query(
                                "UPDATE webhook_events SET status = 'failed', retry_count = $1 WHERE id = $2 AND status != 'delivered'",
                            )
                            .bind(retry_count)
                            .bind(id)
                            .execute(&db)
                            .await
                            .ok();
                            tracing::warn!("retry failed {id} after {retry_count} attempts");
                        } else {
                            record_attempt(
                                &db, id, attempt_number, "retrying", None, None, None,
                                Some(&err_msg), duration_ms,
                            ).await;
                            let delay = 10 * 2u64.pow(retry_count as u32);
                            let retry_at = Utc::now().timestamp() as i64 + delay as i64;
                            redis::cmd("ZADD")
                                .arg(RETRY_KEY)
                                .arg(retry_at)
                                .arg(id.to_string())
                                .query_async::<()>(&mut redis)
                                .await
                                .ok();
                        }
                    }
                }
            }
            None => {
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            }
        }
    }
}

async fn rule_matches(source: &str, event: &WebhookEvent) -> bool {
    source == "*" || source == &event.source
}

/// Build an outbound request honoring the rule's `method`, custom `headers`,
/// and optional HMAC `signing_secret`. When a secret is present, an
/// `X-Terusin-Signature: sha256=<hex>` header is added over the body bytes.
fn build_rule_request(
    client: &reqwest::Client,
    rule: &ForwardRule,
    body: &serde_json::Value,
) -> reqwest::RequestBuilder {
    let method = match rule.method.to_uppercase().as_str() {
        "GET" => reqwest::Method::GET,
        "PUT" => reqwest::Method::PUT,
        "PATCH" => reqwest::Method::PATCH,
        "DELETE" => reqwest::Method::DELETE,
        _ => reqwest::Method::POST,
    };
    // Serialize once so the signature covers the exact bytes we send.
    let body_bytes = serde_json::to_vec(body).unwrap_or_default();
    let mut req = client
        .request(method, &rule.target_url)
        .header("content-type", "application/json")
        .body(body_bytes.clone());

    if let Some(obj) = rule.headers.as_object() {
        for (k, v) in obj {
            if let Some(s) = v.as_str() {
                req = req.header(k, s);
            }
        }
    }
    if let Some(secret) = &rule.signing_secret {
        if !secret.is_empty() {
            req = req.header("X-Terusin-Signature", sign_body(secret, &body_bytes));
        }
    }
    req
}

async fn forward_to_rules(
    db: &sqlx::PgPool,
    event: &WebhookEvent,
    client: &reqwest::Client,
) {
    let rules = sqlx::query_as::<_, ForwardRule>(
        "SELECT * FROM forward_rules WHERE active = true",
    )
    .fetch_all(db)
    .await
    .unwrap_or_default();

    for rule in rules {
        if !rule_matches(&rule.source_pattern, event).await {
            continue;
        }
        let req = build_rule_request(client, &rule, &event.body);
        let res = req.send().await;
        if let Ok(r) = res {
            tracing::info!("hook {} -> {}: {}", rule.name, rule.target_url, r.status());
        }
    }
}

#[derive(Deserialize, Default)]
struct EventQuery {
    search: Option<String>,
    status: Option<String>,
    source: Option<String>,
    /// ISO timestamp lower bound (inclusive) on created_at.
    from: Option<String>,
    /// ISO timestamp upper bound (exclusive) on created_at.
    to: Option<String>,
    page: Option<i64>,
    per_page: Option<i64>,
}

async fn list_events(
    State(state): State<Arc<AppState>>,
    Query(q): Query<EventQuery>,
) -> Result<Json<Value>, StatusCode> {
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(10).min(200);
    let offset = (page - 1) * per_page;

    let mut sql = "SELECT * FROM webhook_events WHERE 1=1".to_string();
    let mut count_sql = "SELECT COUNT(*) FROM webhook_events WHERE 1=1".to_string();
    let mut params: Vec<String> = vec![];

    if let Some(ref s) = q.search { if !s.is_empty() {
        let like = format!("%{}%", s);
        let idx = params.len() + 1;
        sql += &format!(" AND (source ILIKE ${idx} OR target_url ILIKE ${idx} OR body::text ILIKE ${idx})");
        count_sql += &format!(" AND (source ILIKE ${idx} OR target_url ILIKE ${idx} OR body::text ILIKE ${idx})");
        params.push(like);
    }}
    if let Some(ref s) = q.status { if !s.is_empty() && s != "all" {
        let idx = params.len() + 1;
        sql += &format!(" AND status = ${idx}");
        count_sql += &format!(" AND status = ${idx}");
        params.push(s.clone());
    }}
    if let Some(ref s) = q.source { if !s.is_empty() {
        let idx = params.len() + 1;
        sql += &format!(" AND source = ${idx}");
        count_sql += &format!(" AND source = ${idx}");
        params.push(s.clone());
    }}
    if let Some(ref ts) = q.from { if !ts.is_empty() {
        let idx = params.len() + 1;
        sql += &format!(" AND created_at >= ${idx}::timestamp");
        count_sql += &format!(" AND created_at >= ${idx}::timestamp");
        params.push(ts.clone());
    }}
    if let Some(ref ts) = q.to { if !ts.is_empty() {
        let idx = params.len() + 1;
        sql += &format!(" AND created_at < ${idx}::timestamp");
        count_sql += &format!(" AND created_at < ${idx}::timestamp");
        params.push(ts.clone());
    }}

    sql += &format!(" ORDER BY created_at DESC LIMIT {per_page} OFFSET {offset}");

    let mut query = sqlx::query_as::<_, WebhookEvent>(&sql);
    let mut count_q = sqlx::query_scalar::<_, i64>(&count_sql);
    for p in &params {
        query = query.bind(p);
        count_q = count_q.bind(p);
    }

    let (events, total) = tokio::try_join!(
        query.fetch_all(&state.db),
        count_q.fetch_one(&state.db),
    ).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(json!({
        "events": events,
        "total": total,
        "page": page,
        "per_page": per_page,
        "pages": (total as f64 / per_page as f64).ceil() as i64,
    })))
}

async fn get_event(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<WebhookEvent>, StatusCode> {
    let event =
        sqlx::query_as::<_, WebhookEvent>("SELECT * FROM webhook_events WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    event.map(Json).ok_or(StatusCode::NOT_FOUND)
}

/// Delivery attempts for the per-event retry timeline (newest last).
async fn list_attempts(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<DeliveryAttempt>>, StatusCode> {
    let rows = sqlx::query_as::<_, DeliveryAttempt>(
        "SELECT * FROM delivery_attempts WHERE event_id = $1 ORDER BY attempt_number ASC, created_at ASC",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rows))
}

/// Distinct sources (for the dashboard source filter dropdown).
async fn list_sources(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<String>>, StatusCode> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT source FROM webhook_events ORDER BY source ASC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rows.into_iter().map(|(s,)| s).collect()))
}

/// Server-Sent Events stream of newly-created events. Polls the DB every 2s
/// for events newer than the last seen created_at and emits each as an SSE
/// `data:` line (JSON). Manual impl because axum-extra 0.10 has no SSE helper.
async fn event_stream(State(state): State<Arc<AppState>>) -> Response {
    let db = state.db.clone();
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<String, std::io::Error>>(16);

    tokio::spawn(async move {
        let mut last_seen = chrono::Utc::now().naive_utc();
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(2));
        // Send an initial comment so the client knows the stream is alive.
        let _ = tx.send(Ok(": connected\n\n".to_string())).await;
        loop {
            ticker.tick().await;
            let rows: Vec<WebhookEvent> = match sqlx::query_as::<_, WebhookEvent>(
                "SELECT * FROM webhook_events WHERE created_at > $1 ORDER BY created_at ASC LIMIT 100",
            )
            .bind(last_seen)
            .fetch_all(&db)
            .await
            {
                Ok(r) => r,
                Err(_) => continue,
            };
            if let Some(last) = rows.last() {
                last_seen = last.created_at;
            }
            for ev in rows {
                let payload = match serde_json::to_string(&ev) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let chunk = format!("event: event\ndata: {payload}\n\n");
                if tx.send(Ok(chunk)).await.is_err() {
                    return; // client disconnected
                }
            }
        }
    });

    let body = Body::from_stream(
        tokio_stream::wrappers::ReceiverStream::new(rx)
            .map(|r| r.map(|s| Bytes::from(s))),
    );
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-cache")
        .header("x-accel-buffering", "no")
        .body(body)
        .unwrap()
}

async fn retry_event(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<auth::CurrentUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    require_admin(&cu)?;
    let event =
        sqlx::query_as::<_, WebhookEvent>("SELECT * FROM webhook_events WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await;

    match event {
        Ok(Some(_)) => {
            let mut conn = state.redis.clone();
            redis::cmd("LPUSH")
                .arg(QUEUE_KEY)
                .arg(id.to_string())
                .query_async::<()>(&mut conn)
                .await
                .ok();
            Ok(StatusCode::OK)
        }
        _ => Err(StatusCode::NOT_FOUND),
    }
}

async fn ack_event(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<auth::CurrentUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    require_admin(&cu)?;
    sqlx::query("UPDATE webhook_events SET status = 'delivered', response_status = 200 WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .ok();
    let mut conn = state.redis.clone();
    redis::cmd("LREM").arg(QUEUE_KEY).arg(0).arg(id.to_string()).query_async::<()>(&mut conn).await.ok();
    redis::cmd("ZREM").arg(RETRY_KEY).arg(id.to_string()).query_async::<()>(&mut conn).await.ok();
    Ok(StatusCode::OK)
}

/// Permanently delete an event and remove it from any in-flight Redis queue.
async fn delete_event(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<auth::CurrentUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    require_admin(&cu)?;
    let res = sqlx::query("DELETE FROM webhook_events WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await;
    let removed = matches!(res, Ok(r) if r.rows_affected() > 0);
    if removed {
        let mut conn = state.redis.clone();
        redis::cmd("LREM").arg(QUEUE_KEY).arg(0).arg(id.to_string()).query_async::<()>(&mut conn).await.ok();
        redis::cmd("ZREM").arg(RETRY_KEY).arg(id.to_string()).query_async::<()>(&mut conn).await.ok();
        Ok(StatusCode::OK)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

#[derive(Deserialize)]
struct BulkIds {
    ids: Vec<Uuid>,
}

/// Re-enqueue many events at once (LPUSH each id to the queue).
async fn bulk_retry(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<auth::CurrentUser>,
    Json(input): Json<BulkIds>,
) -> Result<Json<Value>, StatusCode> {
    require_admin(&cu)?;
    let mut enqueued = 0;
    let mut conn = state.redis.clone();
    for id in &input.ids {
        let exists: bool = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM webhook_events WHERE id = $1)",
        )
        .bind(id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(false);
        if !exists {
            continue;
        }
        redis::cmd("LPUSH")
            .arg(QUEUE_KEY)
            .arg(id.to_string())
            .query_async::<()>(&mut conn)
            .await
            .ok();
        enqueued += 1;
    }
    Ok(Json(json!({ "enqueued": enqueued, "requested": input.ids.len() })))
}

/// Delete many events at once and scrub them from Redis.
async fn bulk_delete(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<auth::CurrentUser>,
    Json(input): Json<BulkIds>,
) -> Result<Json<Value>, StatusCode> {
    require_admin(&cu)?;
    let mut conn = state.redis.clone();
    let mut deleted = 0;
    for id in &input.ids {
        let res = sqlx::query("DELETE FROM webhook_events WHERE id = $1")
            .bind(id)
            .execute(&state.db)
            .await;
        if matches!(res, Ok(r) if r.rows_affected() > 0) {
            redis::cmd("LREM").arg(QUEUE_KEY).arg(0).arg(id.to_string()).query_async::<()>(&mut conn).await.ok();
            redis::cmd("ZREM").arg(RETRY_KEY).arg(id.to_string()).query_async::<()>(&mut conn).await.ok();
            deleted += 1;
        }
    }
    Ok(Json(json!({ "deleted": deleted, "requested": input.ids.len() })))
}

#[derive(Deserialize, Default)]
struct MetricsQuery {
    /// Time window: 24h | 7d | 30d. Defaults to 24h.
    range: Option<String>,
}

/// Aggregated observability metrics for the metrics dashboard: status totals,
/// success rate, hourly throughput timeseries, top sources/targets, and the
/// current Redis queue depth.
async fn metrics(
    State(state): State<Arc<AppState>>,
    Query(q): Query<MetricsQuery>,
) -> Result<Json<Value>, StatusCode> {
    let hours = match q.range.as_deref() {
        Some("7d") => 24 * 7,
        Some("30d") => 24 * 30,
        _ => 24,
    };
    let since = Utc::now().naive_utc() - chrono::Duration::hours(hours);

    // Status totals within the window.
    let totals: Vec<(String, i64)> = sqlx::query_as(
        "SELECT status, COUNT(*) FROM webhook_events WHERE created_at >= $1 GROUP BY status",
    )
    .bind(since)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let total_count: i64 = totals.iter().map(|(_, c)| c).sum();
    let delivered: i64 = totals
        .iter()
        .filter(|(s, _)| s == "delivered")
        .map(|(_, c)| c)
        .sum();
    let failed: i64 = totals
        .iter()
        .filter(|(s, _)| s == "failed")
        .map(|(_, c)| c)
        .sum();
    let success_rate = if total_count > 0 {
        (delivered as f64 / total_count as f64) * 100.0
    } else {
        0.0
    };

    // Hourly throughput. Buckets per hour for 24h, per day for longer ranges.
    let trunc = if hours <= 24 { "hour" } else { "day" };
    let series_sql = format!(
        "SELECT date_trunc('{trunc}', created_at) AS bucket, COUNT(*) AS n \
         FROM webhook_events WHERE created_at >= $1 GROUP BY bucket ORDER BY bucket ASC"
    );
    let series: Vec<(chrono::NaiveDateTime, i64)> =
        sqlx::query_as(&series_sql)
            .bind(since)
            .fetch_all(&state.db)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let series: Vec<Value> = series
        .into_iter()
        .map(|(ts, n)| json!({ "bucket": ts, "count": n }))
        .collect();

    // Top 5 sources and targets by count in the window.
    let top_sources: Vec<(String, i64)> = sqlx::query_as(
        "SELECT source, COUNT(*) n FROM webhook_events WHERE created_at >= $1 \
         GROUP BY source ORDER BY n DESC LIMIT 5",
    )
    .bind(since)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let top_targets: Vec<(String, i64)> = sqlx::query_as(
        "SELECT target_url, COUNT(*) n FROM webhook_events WHERE created_at >= $1 \
         GROUP BY target_url ORDER BY n DESC LIMIT 5",
    )
    .bind(since)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Queue depth (best-effort; ignore Redis errors).
    let mut conn = state.redis.clone();
    let queue_depth: i64 = redis::cmd("LLEN")
        .arg(QUEUE_KEY)
        .query_async(&mut conn)
        .await
        .unwrap_or(0);
    let retry_depth: i64 = redis::cmd("ZCARD")
        .arg(RETRY_KEY)
        .query_async(&mut conn)
        .await
        .unwrap_or(0);

    Ok(Json(json!({
        "range_hours": hours,
        "total": total_count,
        "delivered": delivered,
        "failed": failed,
        "success_rate": (success_rate * 100.0).round() / 100.0,
        "queue_depth": queue_depth,
        "retry_depth": retry_depth,
        "series": series,
        "top_sources": top_sources.into_iter().map(|(s, c)| json!({"source": s, "count": c})).collect::<Vec<_>>(),
        "top_targets": top_targets.into_iter().map(|(t, c)| json!({"target": t, "count": c})).collect::<Vec<_>>(),
    })))
}

async fn list_rules(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<ForwardRule>>, StatusCode> {
    let rules = sqlx::query_as::<_, ForwardRule>(
        "SELECT * FROM forward_rules ORDER BY created_at ASC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rules))
}

#[derive(Deserialize)]
struct CreateRule {
    name: String,
    source_pattern: Option<String>,
    target_url: String,
    method: Option<String>,
    /// Custom headers to send on outbound delivery. Defaults to `{}`.
    #[serde(default)]
    headers: Option<serde_json::Value>,
}

async fn create_rule(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<auth::CurrentUser>,
    Json(input): Json<CreateRule>,
) -> Result<Json<ForwardRule>, StatusCode> {
    require_admin(&cu)?;
    let id = Uuid::new_v4();
    let pattern = input.source_pattern.unwrap_or_else(|| "*".to_string());
    let method = input.method.unwrap_or_else(|| "POST".to_string());
    let headers = input
        .headers
        .filter(|h| h.is_object())
        .unwrap_or_else(|| serde_json::json!({}));

    sqlx::query(
        r#"INSERT INTO forward_rules (id, name, source_pattern, target_url, method, headers, active)
        VALUES ($1, $2, $3, $4, $5, $6, true)"#,
    )
    .bind(id)
    .bind(&input.name)
    .bind(&pattern)
    .bind(&input.target_url)
    .bind(&method)
    .bind(&headers)
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("create rule: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let rule = sqlx::query_as::<_, ForwardRule>("SELECT * FROM forward_rules WHERE id = $1")
        .bind(id)
        .fetch_one(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(rule))
}

async fn delete_rule(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<auth::CurrentUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    require_admin(&cu)?;
    let res = sqlx::query("DELETE FROM forward_rules WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if res.rows_affected() > 0 {
        Ok(StatusCode::OK)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

/// Partial update of a forward rule. Any of the fields may be omitted; only
/// provided fields are written. `active` lets the UI toggle a rule on/off.
#[derive(Deserialize)]
struct UpdateRule {
    name: Option<String>,
    source_pattern: Option<String>,
    target_url: Option<String>,
    method: Option<String>,
    headers: Option<serde_json::Value>,
    active: Option<bool>,
}

async fn update_rule(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<auth::CurrentUser>,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateRule>,
) -> Result<Json<ForwardRule>, StatusCode> {
    require_admin(&cu)?;
    // Coalesce: read current row, apply overrides, write back. Simpler than
    // building a dynamic UPDATE with a variable column list.
    let current =
        sqlx::query_as::<_, ForwardRule>("SELECT * FROM forward_rules WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;

    let name = input.name.unwrap_or(current.name);
    let source_pattern = input.source_pattern.unwrap_or(current.source_pattern);
    let target_url = input.target_url.unwrap_or(current.target_url);
    let method = input.method.unwrap_or(current.method);
    let headers = input
        .headers
        .filter(|h| h.is_object())
        .unwrap_or(current.headers);
    let active = input.active.unwrap_or(current.active);

    let rule = sqlx::query_as::<_, ForwardRule>(
        r#"UPDATE forward_rules
           SET name = $2, source_pattern = $3, target_url = $4, method = $5, headers = $6, active = $7
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(id)
    .bind(&name)
    .bind(&source_pattern)
    .bind(&target_url)
    .bind(&method)
    .bind(&headers)
    .bind(active)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("update rule: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(rule))
}

#[derive(Deserialize)]
struct SetTarget {
    url: String,
}

async fn set_default_target(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<auth::CurrentUser>,
    Json(input): Json<SetTarget>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Mutates global forwarding state — admin only. Previously this lived on
    // the *public* router, which let anyone redirect all webhooks.
    require_admin(&cu)?;
    *state.default_target.lock().unwrap() = input.url.clone();
    Ok(Json(serde_json::json!({"default_target": input.url})))
}

async fn get_default_target(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let t = state.default_target.lock().unwrap().clone();
    Json(serde_json::json!({"default_target": t}))
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok"}))
}

/// Public flag telling the frontend whether the "Continue with Google"
/// button should be shown. Driven by whether GOOGLE_CLIENT_ID/SECRET were
/// set at startup (state.oauth.is_some()).
async fn get_oauth_status(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
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
async fn get_endpoint() -> Json<serde_json::Value> {
    let public_url = std::env::var("PUBLIC_URL")
        .unwrap_or_else(|_| "https://terusin-dev.my.id".to_string());
    let ngrok = get_ngrok_url().await;
    Json(serde_json::json!({
        "endpoint": public_url,
        "ngrok": ngrok,
    }))
}

fn redis_from_env() -> redis::Client {
    let url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
    redis::Client::open(url).expect("invalid redis url")
}

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
        CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any)
    } else {
        CorsLayer::new()
            .allow_origin(origins)
            .allow_methods([
                axum::http::Method::GET,
                axum::http::Method::POST,
                axum::http::Method::DELETE,
                axum::http::Method::OPTIONS,
            ])
            .allow_headers([
                axum::http::header::AUTHORIZATION,
                axum::http::header::CONTENT_TYPE,
                axum::http::header::COOKIE,
                axum::http::HeaderName::from_static("x-webhook-source"),
                axum::http::HeaderName::from_static("x-target-url"),
            ])
            .allow_credentials(true)
    }
}

/// Keyed rate limiter type used for per-IP auth throttling.
pub type KeyedLimiter = governor::RateLimiter<
    std::net::IpAddr,
    governor::state::keyed::DefaultKeyedStateStore<std::net::IpAddr>,
    governor::clock::DefaultClock,
>;

/// Build a keyed rate limiter (GCRA via `governor`).
///
/// `period_secs` / `burst` define the quota. Requests are keyed on the
/// forwarded client IP (Cloudflare → Caddy → backend sets XFF/X-Real-IP);
/// when no forwarded header is present 0.0.0.0 is used as a fallback so the
/// limiter always has a key. Wrap with
/// `middleware::from_fn_with_state(limiter, rate_limit_middleware)` to attach
/// to a router — over-quota requests get HTTP 429 + `Retry-After`.
fn build_rate_limiter(period_secs: u64, burst: u32) -> std::sync::Arc<KeyedLimiter> {
    use std::num::NonZeroU32;
    let quota = governor::Quota::with_period(std::time::Duration::from_secs(period_secs))
        .expect("non-zero period")
        .allow_burst(NonZeroU32::new(burst).expect("non-zero burst"));
    std::sync::Arc::new(governor::RateLimiter::keyed(quota))
}

/// Check a per-IP rate limiter; on quota exceeded, returns a 429 Response
/// with `Retry-After` set. Otherwise returns None (caller continues).
pub fn check_rate_limit(
    limiter: &KeyedLimiter,
    ip: std::net::IpAddr,
) -> Option<Response> {
    use governor::clock::Clock;
    match limiter.check_key(&ip) {
        Ok(_) => None,
        Err(negative) => {
            let clock = governor::clock::DefaultClock::default();
            let wait = negative.wait_time_from(clock.now());
            let secs = wait.as_secs().max(1);
            let mut res = (
                StatusCode::TOO_MANY_REQUESTS,
                Json(json!({ "error": "rate_limited", "retry_after": secs })),
            )
                .into_response();
            res.headers_mut().insert(
                axum::http::header::RETRY_AFTER,
                axum::http::HeaderValue::from_str(&secs.to_string())
                    .unwrap_or_else(|_| axum::http::HeaderValue::from_static("60")),
            );
            Some(res)
        }
    }
}

pub fn client_ip_from(headers: &HeaderMap) -> Option<std::net::IpAddr> {
    headers
        .get("CF-Connecting-IP")
        .or_else(|| headers.get("X-Real-IP"))
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse().ok())
        .or_else(|| {
            headers
                .get(axum::http::header::FORWARDED)
                .or_else(|| headers.get("X-Forwarded-For"))
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.split(',').next())
                .and_then(|s| s.trim().parse().ok())
        })
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

    let main_redis = ConnectionManager::new(redis_from_env()).await.expect("can't connect to redis");

    sqlx::migrate!("./migrations").run(&db).await.ok();
    seed_default_user(&db).await;

    let oauth = auth::OAuthConfig::from_env().map(Arc::new);
    if oauth.is_some() {
        info!("google oauth enabled (redirect_uri will be set per env)");
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
        default_target: std::sync::Mutex::new(default_target),
        oauth,
        turnstile,
        login_limiter: login_limiter.clone(),
        me_limiter: me_limiter.clone(),
    });

    let worker_count: usize = std::env::var("WORKER_COUNT")
        .unwrap_or_else(|_| "4".to_string()).parse().unwrap_or(4);
    for _ in 0..worker_count {
        let w_redis = ConnectionManager::new(redis_from_env()).await.unwrap();
        tokio::spawn(worker(db.clone(), w_redis, max_retries));
    }

    let r_redis = ConnectionManager::new(redis_from_env()).await.unwrap();
    tokio::spawn(retry_worker(db, r_redis));

    let public = Router::new()
        .route("/config/default-target", get(get_default_target))
        .route("/config/endpoint", get(get_endpoint))
        .route("/config/oauth", get(get_oauth_status))
        // OAuth endpoints (callback must be reachable cross-origin via redirect).
        .route("/api/auth/google", get(auth::google_login))
        .route("/api/auth/callback/google", get(auth::google_callback))
        .route("/api/auth/me", get(auth::me))
        .route("/api/auth/login", axum::routing::post(auth::login))
        .route("/api/auth/logout", axum::routing::post(auth::logout))
        // Pairing: the code IS the credential, so pair-complete is public.
        .route("/api/auth/pair", axum::routing::post(auth::pair_complete));

    let protected = Router::new()
        .route("/config/default-target", post(set_default_target))
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
        // API token pairing + management (require the calling user to be authed).
        .route("/api/auth/pair/init", axum::routing::post(auth::pair_init))
        .route("/api/auth/tokens", get(auth::list_tokens))
        .route("/api/auth/tokens/{id}", axum::routing::delete(auth::revoke_token))
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware));

    // CORS: allow the web frontend origin to call the backend API directly.
    // Default covers the Vite dev server (:5173) and the embedded prod binary
    // (:3012). Override with WEB_URL env (comma-separated for multiple).
    let cors = build_cors_layer();

    let app = Router::new()
        .route("/health", get(health))
        .merge(public)
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
