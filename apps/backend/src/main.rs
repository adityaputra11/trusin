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
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

const QUEUE_KEY: &str = "terusin:queue";
const RETRY_KEY: &str = "terusin:retry";

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
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
struct User {
    id: Uuid,
    username: String,
    password_hash: String,
    role: String,
}

struct AppState {
    db: sqlx::PgPool,
    redis: ConnectionManager,
    max_retries: i32,
    default_target: std::sync::Mutex<String>,
}

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
                Some(u) if bcrypt::verify(&pass, &u.password_hash).unwrap_or(false) => {
                    Ok(next.run(req).await)
                }
                _ => Err(unauth()),
            }
        }
        None => Err(unauth()),
    }
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

        let res = client
            .post(&event.target_url)
            .json(&event.body)
            .send()
            .await;

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

                let res = client
                    .post(&event.target_url)
                    .json(&event.body)
                    .send()
                    .await;

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
        let res = client.post(&rule.target_url).json(&event.body).send().await;
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
}

async fn create_rule(
    State(state): State<Arc<AppState>>,
    Json(input): Json<CreateRule>,
) -> Result<Json<ForwardRule>, StatusCode> {
    let id = Uuid::new_v4();
    let pattern = input.source_pattern.unwrap_or_else(|| "*".to_string());
    let method = input.method.unwrap_or_else(|| "POST".to_string());

    sqlx::query(
        r#"INSERT INTO forward_rules (id, name, source_pattern, target_url, method, active)
        VALUES ($1, $2, $3, $4, $5, true)"#,
    )
    .bind(id)
    .bind(&input.name)
    .bind(&pattern)
    .bind(&input.target_url)
    .bind(&method)
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

fn redis_from_env() -> redis::Client {
    let url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
    redis::Client::open(url).expect("invalid redis url")
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

    let state = Arc::new(AppState {
        db: db.clone(),
        redis: main_redis,
        max_retries,
        default_target: std::sync::Mutex::new(default_target),
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
        .route("/config/default-target", get(get_default_target).post(set_default_target));

    let protected = Router::new()
        .route("/events", get(list_events))
        .route("/events/{id}/retry", post(retry_event))
        .route("/events/{id}/ack", post(ack_event))
        .route("/rules", get(list_rules).post(create_rule))
        .route("/rules/{id}", delete(delete_rule))
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware));

    let app = Router::new()
        .route("/health", get(health))
        .merge(public)
        .merge(protected)
        .route("/", post(handle_root))
        .route("/{*source}", post(handle_webhook))
        .with_state(state);

    let addr = format!("0.0.0.0:{port}");
    info!("backend listening on {addr}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
