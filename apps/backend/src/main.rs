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
use chrono::Utc;
use hmac::{Hmac, Mac};
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Sha256;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
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
    #[serde(skip_serializing_if = "Option::is_none")]
    #[sqlx(default)]
    signing_secret: Option<String>,
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

async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, Response> {
    // 1) Try the session JWT cookie first (Google OAuth users).
    if let Some(cfg) = &state.oauth {
        if let Some(cookie) = req.headers().get("Cookie").and_then(|v| v.to_str().ok()) {
            if let Some(token) = extract_session_cookie(cookie) {
                if let Some(uid) = auth::verify_jwt(&token, &cfg.jwt_secret) {
                    let exists =
                        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users WHERE id = $1")
                            .bind(uid)
                            .fetch_one(&state.db)
                            .await
                            .map(|n| n > 0)
                            .unwrap_or(false);
                    if exists {
                        return Ok(next.run(req).await);
                    }
                }
            }
        }
    }

    // 2) Fall back to HTTP Basic auth (CLI / MCP / password logins).
    let header = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

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
        let res = req.send().await;

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

                if status < 300 {
                    sqlx::query(
                        "UPDATE webhook_events SET status = 'delivered', response_status = $1, response_headers = $2, response_body = $3 WHERE id = $4 AND status != 'delivered'",
                    )
                    .bind(status).bind(&resp_h).bind(&resp_b).bind(id)
                    .execute(&db).await.ok();
                    info!("delivered {id} -> {} ({})", event.target_url, status);
                    forward_to_rules(&db, &event, &client).await;
                } else {
                    sqlx::query(
                        "UPDATE webhook_events SET status = 'failed', response_status = $1, response_headers = $2, response_body = $3 WHERE id = $4 AND status != 'delivered'",
                    )
                    .bind(status).bind(&resp_h).bind(&resp_b).bind(id)
                    .execute(&db).await.ok();
                    tracing::warn!("failed {id} -> {} ({})", event.target_url, status);
                }
            }
            Err(_e) => {
                if already().await { continue; }
                let retry_count = event.retry_count + 1;
                if retry_count > max_retries {
                    sqlx::query(
                        "UPDATE webhook_events SET status = 'failed', retry_count = $1 WHERE id = $2 AND status != 'delivered'",
                    )
                    .bind(retry_count).bind(id)
                    .execute(&db).await.ok();
                    tracing::warn!("failed {id} after {retry_count} attempts");
                } else {
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
                let res = req.send().await;

                let is_delivered = sqlx::query_scalar::<_, String>("SELECT status FROM webhook_events WHERE id = $1")
                    .bind(id).fetch_optional(&db).await.ok().flatten().unwrap_or_default() == "delivered";
                if is_delivered { continue; }

                match res {
                    Ok(r) if r.status().is_success() => {
                        sqlx::query(
                            "UPDATE webhook_events SET status = 'delivered' WHERE id = $1 AND status != 'delivered'",
                        )
                        .bind(id)
                        .execute(&db)
                        .await
                        .ok();
                        info!("retry delivered {id}");
                        forward_to_rules(&db, &event, &client).await;
                    }
                    _ => {
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
    page: Option<i64>,
    per_page: Option<i64>,
}

async fn list_events(
    State(state): State<Arc<AppState>>,
    Query(q): Query<EventQuery>,
) -> Result<Json<Value>, StatusCode> {
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(50).min(200);
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

async fn retry_event(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> StatusCode {
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
            StatusCode::OK
        }
        _ => StatusCode::NOT_FOUND,
    }
}

async fn ack_event(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> StatusCode {
    sqlx::query("UPDATE webhook_events SET status = 'delivered', response_status = 200 WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .ok();
    let mut conn = state.redis.clone();
    redis::cmd("LREM").arg(QUEUE_KEY).arg(0).arg(id.to_string()).query_async::<()>(&mut conn).await.ok();
    redis::cmd("ZREM").arg(RETRY_KEY).arg(id.to_string()).query_async::<()>(&mut conn).await.ok();
    StatusCode::OK
}

/// Permanently delete an event and remove it from any in-flight Redis queue.
async fn delete_event(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> StatusCode {
    let res = sqlx::query("DELETE FROM webhook_events WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await;
    let removed = matches!(res, Ok(r) if r.rows_affected() > 0);
    if removed {
        let mut conn = state.redis.clone();
        redis::cmd("LREM").arg(QUEUE_KEY).arg(0).arg(id.to_string()).query_async::<()>(&mut conn).await.ok();
        redis::cmd("ZREM").arg(RETRY_KEY).arg(id.to_string()).query_async::<()>(&mut conn).await.ok();
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
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
    Json(input): Json<CreateRule>,
) -> Result<Json<ForwardRule>, StatusCode> {
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

async fn delete_rule(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> StatusCode {
    sqlx::query("DELETE FROM forward_rules WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .map(|_| StatusCode::OK)
        .unwrap_or(StatusCode::NOT_FOUND)
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
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateRule>,
) -> Result<Json<ForwardRule>, StatusCode> {
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
    Json(input): Json<SetTarget>,
) -> Json<serde_json::Value> {
    *state.default_target.lock().unwrap() = input.url.clone();
    Json(serde_json::json!({"default_target": input.url}))
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

    let state = Arc::new(AppState {
        db: db.clone(),
        redis: main_redis,
        max_retries,
        default_target: std::sync::Mutex::new(default_target),
        oauth,
        turnstile,
    });

    let worker_count: usize = std::env::var("WORKER_COUNT")
        .unwrap_or_else(|_| "4".to_string()).parse().unwrap_or(4);
    for _ in 0..worker_count {
        let w_redis = ConnectionManager::new(redis_from_env()).await.unwrap();
        tokio::spawn(worker(db.clone(), w_redis, max_retries));
    }

    let r_redis = ConnectionManager::new(redis_from_env()).await.unwrap();
    tokio::spawn(retry_worker(db, r_redis));

    // Rate limit the auth endpoints that accept credentials. Two buckets:
    //   - /api/auth/login : tight (5/min per IP) — primary brute-force target
    //   - /api/auth/me    : looser (30/min per IP) — called on every page load
    // In-memory (per-process). OK for a single-server deploy. Keyed on the
    // forwarded client IP since traffic flows Cloudflare → Caddy → backend.
    let login_limiter = build_rate_limiter(60, 5);
    let me_limiter = build_rate_limiter(60, 30);

    let auth_router = Router::new()
        // Username/password login with optional Cloudflare Turnstile.
        .route("/api/auth/login", axum::routing::post(auth::login))
        .layer(login_limiter)
        .route("/api/auth/me", get(auth::me))
        .layer(me_limiter);

    let public = Router::new()
        .route("/config/default-target", get(get_default_target).post(set_default_target))
        .route("/config/endpoint", get(get_endpoint))
        // OAuth endpoints (callback must be reachable cross-origin via redirect).
        .route("/api/auth/google", get(auth::google_login))
        .route("/api/auth/callback/google", get(auth::google_callback))
        .route("/api/auth/logout", axum::routing::post(auth::logout))
        .merge(auth_router);

    let protected = Router::new()
        .route("/events", get(list_events))
        .route("/events/{id}", get(get_event).delete(delete_event))
        .route("/events/{id}/retry", post(retry_event))
        .route("/events/{id}/ack", post(ack_event))
        .route("/rules", get(list_rules).post(create_rule))
        .route("/rules/{id}", delete(delete_rule).patch(update_rule))
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
