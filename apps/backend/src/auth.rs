// Google OAuth + JWT session auth.
//
// Flow:
//   browser → GET /api/auth/google          → 302 to Google consent
//   Google  → GET /api/auth/callback/google → exchange code → upsert user
//                                              → set http-only JWT cookie
//                                              → 302 back to the web app
//   browser → /api/auth/me                  → JSON user info (cookie auth)
//   browser → /api/auth/logout              → clear cookie
//
// The cookie JWT is also accepted by `auth_middleware` in main.rs, so the
// protected API endpoints (/events, /rules) work for Google-authenticated
// users without them needing Basic credentials.

use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    Json,
};
use base64::Engine;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

use crate::{AppState, User};

/// The authenticated principal, made available to handlers via the axum
/// `Extension<CurrentUser>` extractor. Inserted by `auth_middleware` after any
/// of cookie/Basic/Bearer auth succeeds. Handlers extract it to do per-user
/// and role-based decisions.
#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub id: Uuid,
    pub role: String,
}

/// Cloudflare Turnstile verification.
///
/// `verify` calls the siteverify API; returns false on any network/parsing
/// error so a misconfigured Turnstile can't lock everyone out — instead, set
/// `TURNSTILE_SECRET_KEY` to empty/unset to disable verification entirely.
pub struct TurnstileConfig {
    pub secret: String,
    pub http: reqwest::Client,
}

impl TurnstileConfig {
    /// Build from env; None when Turnstile is disabled (no secret set).
    pub fn from_env() -> Option<Self> {
        let secret = std::env::var("TURNSTILE_SECRET_KEY").ok()?;
        if secret.trim().is_empty() {
            return None;
        }
        Some(Self {
            secret,
            http: reqwest::Client::builder().build().expect("reqwest client"),
        })
    }

    /// Verify a Turnstile token. Always-fail-silent: false on any error.
    pub async fn verify(&self, token: &str, remoteip: Option<&str>) -> bool {
        let mut form = vec![("secret", self.secret.as_str()), ("response", token)];
        if let Some(ip) = remoteip {
            form.push(("remoteip", ip));
        }
        match self
            .http
            .post("https://challenges.cloudflare.com/turnstile/v0/siteverify")
            .form(&form)
            .send()
            .await
        {
            Ok(r) => r.json::<serde_json::Value>().await.map_or(false, |v| {
                v.get("success").and_then(|s| s.as_bool()).unwrap_or(false)
            }),
            Err(_) => false,
        }
    }
}

/// OAuth + JWT configuration, built once from env vars at startup.
#[derive(Clone)]
pub struct OAuthConfig {
    pub client_id: String,
    pub client_secret: String,
    /// Where Google should send the user back. Same-origin with the browser.
    pub redirect_uri: String,
    /// Where to send the user after a successful login (the SPA root).
    pub frontend_url: String,
    /// HS256 secret for signing session JWTs.
    pub jwt_secret: String,
    pub http_client: reqwest::Client,
}

impl OAuthConfig {
    /// Returns Some(config) only if Google OAuth is enabled (client id+secret
    /// present). When None, the auth routes respond 501.
    pub fn from_env() -> Option<Self> {
        let client_id = std::env::var("GOOGLE_CLIENT_ID").ok()?;
        let client_secret = std::env::var("GOOGLE_CLIENT_SECRET").ok()?;
        if client_id.is_empty() || client_secret.is_empty() {
            return None;
        }
        let redirect_uri = std::env::var("OAUTH_REDIRECT_URI")
            .unwrap_or_else(|_| "http://localhost:5173/api/auth/callback/google".to_string());
        let frontend_url =
            std::env::var("FRONTEND_URL").unwrap_or_else(|_| "http://localhost:5173".to_string());
        let jwt_secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| {
            tracing::warn!("JWT_SECRET not set — using insecure default. Set it in production!");
            "dev-insecure-secret-change-me".to_string()
        });
        Some(Self {
            client_id,
            client_secret,
            redirect_uri,
            frontend_url,
            jwt_secret,
            http_client: reqwest::Client::builder().build().expect("reqwest client"),
        })
    }
}

/// Claims embedded in the session JWT.
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionClaims {
    pub sub: String, // user id (uuid string)
    pub exp: usize,  // unix seconds
}

/// Lifetime of the session cookie / JWT: 7 days.
const SESSION_TTL_SECS: i64 = 7 * 24 * 60 * 60;
pub const COOKIE_NAME: &str = "terusin_session";

pub fn issue_jwt(user_id: &Uuid, secret: &str) -> anyhow::Result<String> {
    let exp = chrono::Utc::now().timestamp() as usize + SESSION_TTL_SECS as usize;
    let claims = SessionClaims {
        sub: user_id.to_string(),
        exp,
    };
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )?;
    Ok(token)
}

/// Verify a JWT and return the user id it contains.
pub fn verify_jwt(token: &str, secret: &str) -> Option<Uuid> {
    let data = decode::<SessionClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .ok()?;
    Uuid::parse_str(&data.claims.sub).ok()
}

// ── API tokens (CLI / MCP pairing) ────────────────────────────────────────
//
// Opaque, 256-bit random tokens (`ts_<base64url>`), stored as sha256 hex in
// the `api_tokens` table. The cleartext is shown ONCE (on pair/creation) and
// never persisted; lookups are by hash, so the table is index-able.

/// Prefix so tokens are easy to grep/redact. `ts_` + 32 random bytes
/// base64url-encoded (≈43 chars) ≈ 46 chars total, 256 bits of entropy.
const TOKEN_PREFIX: &str = "ts_";

/// Generate a fresh token (`ts_<43 chars>`). Returned to the CLI exactly once.
pub fn generate_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    format!("{TOKEN_PREFIX}{b64}")
}

/// sha256(token) → hex. This is what we store and index in `api_tokens`.
/// Fast hash is safe here: tokens are 256-bit random, so brute force is
/// infeasible, and a fast hash enables `WHERE token_hash = $1` lookups.
pub fn hash_token(token: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex_encode(&hasher.finalize())
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// A token row as exposed to the user (no `token_hash`, no cleartext).
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ApiTokenInfo {
    pub id: Uuid,
    pub name: String,
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Throttle `last_used_at` writes to once per minute per token (Redis NX lock)
/// so chatty MCP/CLI clients don't write-hot the table.
const TOKEN_TOUCH_TTL_SECS: usize = 60;
const TOKEN_TOUCH_KEY_PREFIX: &str = "terusin:tokentouch:";

/// Update `last_used_at` at most once per minute per token. Best-effort:
/// errors (Redis down, etc.) are silently ignored so a touch failure never
/// fails an authenticated request.
pub async fn touch_token_last_used(
    redis: &mut redis::aio::ConnectionManager,
    db: &sqlx::PgPool,
    id: &Uuid,
) {
    let key = format!("{TOKEN_TOUCH_KEY_PREFIX}{id}");
    let won: Option<()> = redis::cmd("SET")
        .arg(&key)
        .arg("1")
        .arg("NX")
        .arg("EX")
        .arg(TOKEN_TOUCH_TTL_SECS)
        .query_async(redis)
        .await
        .ok();
    if won.is_some() {
        let _ = sqlx::query("UPDATE api_tokens SET last_used_at = NOW() WHERE id = $1")
            .bind(id)
            .execute(db)
            .await;
    }
}

// ── Google API types ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CodeExchangeResponse {
    pub access_token: String,
    // refresh_token / id_token / scope also present but unused here.
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GoogleUserInfo {
    pub sub: String,
    pub email: String,
    #[serde(default)]
    pub email_verified: bool,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub picture: Option<String>,
}

// ── Endpoint: GET /api/auth/google ────────────────────────────────────────

pub async fn google_login(State(state): State<Arc<AppState>>) -> Response {
    let Some(cfg) = state.oauth.as_ref() else {
        return not_configured();
    };
    let oauth_state = generate_oauth_state();
    let mut redis = state.redis.clone();
    let stored = redis::cmd("SETEX")
        .arg(format!("terusin:oauth:state:{oauth_state}"))
        .arg(600)
        .arg("1")
        .query_async::<()>(&mut redis)
        .await;
    if let Err(e) = stored {
        tracing::warn!("oauth state store failed: {e}");
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "Could not start Google OAuth login.",
        )
            .into_response();
    }
    let params = [
        ("client_id", cfg.client_id.as_str()),
        ("redirect_uri", cfg.redirect_uri.as_str()),
        ("response_type", "code"),
        ("scope", "openid email profile"),
        ("access_type", "online"),
        ("prompt", "consent"),
        ("state", oauth_state.as_str()),
    ];
    let qs = serde_urlencoded::to_string(&params).unwrap_or_default();
    Redirect::temporary(&format!(
        "https://accounts.google.com/o/oauth2/v2/auth?{qs}"
    ))
    .into_response()
}

fn not_configured() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        "Google OAuth not configured (set GOOGLE_CLIENT_ID / GOOGLE_CLIENT_SECRET).",
    )
        .into_response()
}

fn generate_oauth_state() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

// ── Endpoint: GET /api/auth/callback/google ───────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CallbackParams {
    pub code: String,
    pub state: String,
    // Google may also send `scope`; we ignore it.
}

pub async fn google_callback(
    State(state): State<Arc<AppState>>,
    Query(params): Query<CallbackParams>,
) -> Response {
    let Some(cfg) = state.oauth.as_ref() else {
        return not_configured();
    };
    let mut redis = state.redis.clone();
    let key = format!("terusin:oauth:state:{}", params.state);
    let valid_state: Option<String> = redis::cmd("GET")
        .arg(&key)
        .query_async(&mut redis)
        .await
        .ok();
    if valid_state.is_none() {
        return oauth_error(&cfg, "Google login expired. Please try again.");
    }
    let _ = redis::cmd("DEL")
        .arg(&key)
        .query_async::<()>(&mut redis)
        .await;

    // 1. Exchange code → access_token (server-side, secret stays here).
    let token_res = cfg
        .http_client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("code", params.code.as_str()),
            ("client_id", cfg.client_id.as_str()),
            ("client_secret", cfg.client_secret.as_str()),
            ("redirect_uri", cfg.redirect_uri.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await;

    let tokens: CodeExchangeResponse = match token_res {
        Ok(r) if r.status().is_success() => match r.json().await {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!("google token parse: {e}");
                return oauth_error(&cfg, "Could not read Google response.");
            }
        },
        Ok(r) => {
            let status = r.status();
            let body = r.text().await.unwrap_or_default();
            tracing::warn!("google token exchange {status}: {body}");
            return oauth_error(&cfg, "Google rejected the login code.");
        }
        Err(e) => {
            tracing::warn!("google token network: {e}");
            return oauth_error(&cfg, "Could not reach Google.");
        }
    };

    // 2. Fetch the user profile from Google.
    let user_info: GoogleUserInfo = match cfg
        .http_client
        .get("https://www.googleapis.com/oauth2/v3/userinfo")
        .bearer_auth(&tokens.access_token)
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => match r.json().await {
            Ok(u) => u,
            Err(e) => {
                tracing::warn!("google userinfo parse: {e}");
                return oauth_error(&cfg, "Could not read your Google profile.");
            }
        },
        Ok(r) => {
            let status = r.status();
            let body = r.text().await.unwrap_or_default();
            tracing::warn!("google userinfo {status}: {body}");
            return oauth_error(&cfg, "Google did not return your profile.");
        }
        Err(e) => {
            tracing::warn!("google userinfo network: {e}");
            return oauth_error(&cfg, "Could not reach Google.");
        }
    };

    if !user_info.email_verified {
        return oauth_error(&cfg, "Your Google email is not verified.");
    }

    // 3. Upsert user row.
    let user = match upsert_oauth_user(&state.db, &user_info).await {
        Ok(u) => u,
        Err(e) => {
            tracing::error!("upsert oauth user: {e}");
            return oauth_error(&cfg, "Could not save your account.");
        }
    };

    // 4. Issue JWT + set cookie + redirect to SPA.
    let jwt = match issue_jwt(&user.id, &cfg.jwt_secret) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("jwt issue: {e}");
            return oauth_error(&cfg, "Could not start a session.");
        }
    };

    let (name, value) = build_cookie(&jwt, cfg);
    let mut res = Redirect::temporary(&cfg.frontend_url).into_response();
    res.headers_mut().insert(name, value);
    crate::audit::record(
        &state,
        Some(&CurrentUser {
            id: user.id,
            role: user.role.clone(),
        }),
        "auth.google_login",
        "user",
        Some(user.id.to_string()),
        serde_json::json!({ "email": user.email }),
    )
    .await;
    res
}

/// Insert-or-update the user row for a Google subject. Returns the user.
async fn upsert_oauth_user(db: &PgPool, info: &GoogleUserInfo) -> Result<User, sqlx::Error> {
    // Try to find an existing OAuth user with this provider+subject.
    if let Some(existing) = sqlx::query_as::<_, User>(
        "SELECT * FROM users WHERE oauth_provider = 'google' AND oauth_subject = $1",
    )
    .bind(&info.sub)
    .fetch_optional(db)
    .await?
    {
        // Refresh display info (name/picture/email can change on Google side).
        sqlx::query(
            r#"UPDATE users SET email = $1, display_name = $2, avatar_url = $3
               WHERE id = $4"#,
        )
        .bind(&info.email)
        .bind(info.name.as_deref().unwrap_or(&info.email))
        .bind(info.picture.as_deref())
        .bind(existing.id)
        .execute(db)
        .await?;
        return sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
            .bind(existing.id)
            .fetch_one(db)
            .await;
    }

    // Also avoid duplicate email: link an existing email-matched account.
    if let Some(existing) = sqlx::query_as::<_, User>("SELECT * FROM users WHERE email = $1")
        .bind(&info.email)
        .fetch_optional(db)
        .await?
    {
        sqlx::query(
            r#"UPDATE users SET oauth_provider = 'google', oauth_subject = $1,
               display_name = COALESCE(display_name, $2),
               avatar_url = COALESCE(avatar_url, $3)
               WHERE id = $4"#,
        )
        .bind(&info.sub)
        .bind(info.name.as_deref().unwrap_or(&info.email))
        .bind(info.picture.as_deref())
        .bind(existing.id)
        .execute(db)
        .await?;
        return sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
            .bind(existing.id)
            .fetch_one(db)
            .await;
    }

    // Otherwise create a new account with role 'viewer'.
    let username = derive_username(db, &info.email).await?;
    sqlx::query(
        r#"INSERT INTO users
             (id, username, role, email, display_name, avatar_url, oauth_provider, oauth_subject)
           VALUES ($1, $2, 'viewer', $3, $4, $5, 'google', $6)"#,
    )
    .bind(Uuid::new_v4())
    .bind(&username)
    .bind(&info.email)
    .bind(info.name.as_deref().unwrap_or(&info.email))
    .bind(info.picture.as_deref())
    .bind(&info.sub)
    .execute(db)
    .await?;

    sqlx::query_as::<_, User>(
        "SELECT * FROM users WHERE oauth_provider = 'google' AND oauth_subject = $1",
    )
    .bind(&info.sub)
    .fetch_one(db)
    .await
}

/// Derive a unique username from the email local-part, suffixing -2, -3, etc.
async fn derive_username(db: &PgPool, email: &str) -> Result<String, sqlx::Error> {
    let base = email
        .split('@')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("user")
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();
    let base = base[..base.len().min(32)].to_string();

    let mut candidate = base.clone();
    let mut n = 1;
    loop {
        let taken = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users WHERE username = $1")
            .bind(&candidate)
            .fetch_one(db)
            .await?;
        if taken == 0 {
            return Ok(candidate);
        }
        n += 1;
        candidate = format!("{base}-{n}");
    }
}

fn build_cookie(jwt: &str, cfg: &OAuthConfig) -> (header::HeaderName, header::HeaderValue) {
    // Cookie value must be ASCII-safe; JWT already is.
    let secure = cfg.frontend_url.starts_with("https://");
    let same_site = "Lax";
    // HttpOnly + SameSite keeps it safe from CSRF/XSS in the common case.
    let value = format!(
        "{}={}; Path=/; Max-Age={}; HttpOnly; SameSite={}{}",
        COOKIE_NAME,
        jwt,
        SESSION_TTL_SECS,
        same_site,
        if secure { "; Secure" } else { "" }
    );
    (
        header::SET_COOKIE,
        header::HeaderValue::from_str(&value).expect("cookie value"),
    )
}

fn oauth_error(cfg: &OAuthConfig, msg: &str) -> Response {
    // Minimal encoding: spaces to %20 so the message survives a query param.
    let msg = msg.replace(' ', "%20");
    Redirect::temporary(&format!("{}/login?error={}", cfg.frontend_url, msg)).into_response()
}

// ── Endpoint: GET /api/auth/me ────────────────────────────────────────────

pub async fn me(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    // 0) Per-IP rate limit (30/min) — /api/auth/me is called on every page load.
    let ip = crate::client_ip_from(&headers)
        .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::new(0, 0, 0, 0)));
    if let Some(res) = crate::check_rate_limit(&state.me_limiter, ip) {
        return res;
    }

    // 1) Try cookie session (Google OAuth users).
    if let Some(cfg) = state.oauth.clone() {
        let cookie = headers
            .get(header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if let Some(token) = extract_cookie_value(cookie) {
            if let Some(uid) = verify_jwt(&token, &cfg.jwt_secret) {
                if let Some(u) = fetch_user(&state.db, uid).await {
                    return user_json(u).into_response();
                }
            }
        }
    }

    // 2) Try a Bearer API token (CLI / MCP).
    let auth_hdr = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if let Some(bearer) = auth_hdr.strip_prefix("Bearer ").map(str::trim) {
        if let Some(cu) = authenticate_bearer(&state, bearer).await {
            if let Some(u) = fetch_user(&state.db, cu.id).await {
                return user_json(u).into_response();
            }
        }
    }

    // 3) Fall back to Basic auth (CLI / password login).
    if let Some(u) = user_from_basic(&state.db, &headers).await {
        return user_json(u).into_response();
    }

    StatusCode::UNAUTHORIZED.into_response()
}

async fn fetch_user(db: &PgPool, id: Uuid) -> Option<User> {
    sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
}

async fn user_from_basic(db: &PgPool, headers: &HeaderMap) -> Option<User> {
    let h = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())?;
    let enc = h.strip_prefix("Basic ")?;
    let bytes = base64::engine::general_purpose::STANDARD.decode(enc).ok()?;
    let creds = String::from_utf8(bytes).ok()?;
    let mut parts = creds.splitn(2, ':');
    let user = parts.next()?.to_string();
    let pass = parts.next()?.to_string();
    verify_password(db, &user, &pass).await
}

/// Look a user up by username + verify the bcrypt password. Shared between
/// Basic-auth header parsing (`user_from_basic`) and the JSON-body login
/// endpoint (`login`).
async fn verify_password(db: &PgPool, user: &str, pass: &str) -> Option<User> {
    let db_user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE username = $1")
        .bind(user)
        .fetch_optional(db)
        .await
        .ok()??;
    if let Some(hash) = db_user.password_hash.as_ref() {
        if bcrypt::verify(pass, hash).unwrap_or(false) {
            return Some(db_user);
        }
    }
    None
}

// ── Endpoint: POST /api/auth/login ────────────────────────────────────────
//
// Username/password login for the browser, optionally gated by Cloudflare
// Turnstile. Returns the same user JSON shape as `/api/auth/me` on success.
// The frontend stores the Basic cred in sessionStorage after a 200, mirroring
// the CLI/MCP Basic-auth path — so no session/cookie is created here.

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
    /// Turnstile token from the browser widget. Required when
    /// `TURNSTILE_SECRET_KEY` is set on the server; ignored otherwise.
    #[serde(default)]
    pub turnstile_token: Option<String>,
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<LoginRequest>,
) -> Response {
    // 0) Per-IP rate limit (5/min) — primary brute-force guard.
    let ip = crate::client_ip_from(&headers)
        .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::new(0, 0, 0, 0)));
    if let Some(res) = crate::check_rate_limit(&state.login_limiter, ip) {
        return res;
    }

    // 1) Turnstile (if configured). Treat missing token as captcha failure.
    if let Some(cfg) = state.turnstile.as_ref() {
        let token = req.turnstile_token.as_deref().filter(|t| !t.is_empty());
        let Some(token) = token else {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "captcha_required"})),
            )
                .into_response();
        };
        let remoteip = ip.to_string();
        if !cfg.verify(token, Some(&remoteip)).await {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "captcha_failed"})),
            )
                .into_response();
        }
    }

    // 2) Verify credentials.
    match verify_password(&state.db, &req.username, &req.password).await {
        Some(u) => {
            crate::audit::record(
                &state,
                Some(&CurrentUser {
                    id: u.id,
                    role: u.role.clone(),
                }),
                "auth.password_login",
                "user",
                Some(u.id.to_string()),
                serde_json::json!({ "username": u.username }),
            )
            .await;
            user_json(u).into_response()
        }
        None => (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "invalid_credentials"})),
        )
            .into_response(),
    }
}

fn user_json(u: User) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "id": u.id,
        "username": u.username,
        "email": u.email,
        "display_name": u.display_name,
        "avatar_url": u.avatar_url,
        "role": u.role,
        "oauth_provider": u.oauth_provider,
    }))
}

fn extract_cookie_value(cookie_header: &str) -> Option<String> {
    for kv in cookie_header.split(';') {
        let kv = kv.trim();
        if let Some(rest) = kv.strip_prefix(&format!("{COOKIE_NAME}=")) {
            return Some(rest.to_string());
        }
    }
    None
}

// ── Endpoint: POST /api/auth/logout ───────────────────────────────────────

pub async fn logout(State(_state): State<Arc<AppState>>) -> Response {
    let value = format!(
        "{}=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax",
        COOKIE_NAME
    );
    let mut res = (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response();
    res.headers_mut().insert(
        header::SET_COOKIE,
        header::HeaderValue::from_str(&value).expect("cookie value"),
    );
    res
}

// ── Endpoint: POST /api/auth/tokens  (protected) ──────────────────────────
//
// A signed-in dashboard user mints their own API key directly. No 6-digit
// pairing code / Redis round-trip — the key is bound to the caller via the
// `Extension<CurrentUser>` injected by `auth_middleware`, so it inherits the
// caller's role (admin = full, viewer = read-only). The cleartext key is
// returned exactly once and never persisted; only its sha256 hash is stored.

#[derive(Deserialize)]
pub struct CreateTokenRequest {
    pub name: String,
}

pub async fn create_token(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<CurrentUser>,
    Json(body): Json<CreateTokenRequest>,
) -> Response {
    let name = body.name.trim();
    if name.is_empty() || name.len() > 120 {
        return (StatusCode::BAD_REQUEST, "token name must be 1-120 chars").into_response();
    }

    let token = generate_token();
    let hash = hash_token(&token);
    let id = Uuid::new_v4();
    let inserted = sqlx::query(
        r#"INSERT INTO api_tokens (id, user_id, name, token_hash)
           VALUES ($1, $2, $3, $4)"#,
    )
    .bind(id)
    .bind(cu.id)
    .bind(name)
    .bind(&hash)
    .execute(&state.db)
    .await;
    if let Err(e) = inserted {
        tracing::error!("create_token insert: {e}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    crate::audit::record(
        &state,
        Some(&cu),
        "token.created",
        "api_token",
        Some(id.to_string()),
        serde_json::json!({ "name": name }),
    )
    .await;

    Json(serde_json::json!({
        "token": token,           // shown once, never persisted
        "token_id": id,
        "name": name,
        "role": cu.role,
    }))
    .into_response()
}

// ── Endpoint: GET /api/auth/tokens  (protected) ───────────────────────────
//
// List the current user's active tokens. Excludes the hash + revoked tokens.

pub async fn list_tokens(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<CurrentUser>,
) -> Response {
    match sqlx::query_as::<_, ApiTokenInfo>(
        r#"SELECT id, name, last_used_at, created_at FROM api_tokens
           WHERE user_id = $1 AND revoked_at IS NULL
           ORDER BY created_at DESC"#,
    )
    .bind(cu.id)
    .fetch_all(&state.db)
    .await
    {
        Ok(rows) => Json(rows).into_response(),
        Err(e) => {
            tracing::warn!("list_tokens: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

// ── Endpoint: DELETE /api/auth/tokens/{id}  (protected) ───────────────────
//
// Revoke (soft-delete) a token. Only succeeds if the token belongs to the
// current user, so one user can't revoke another's tokens.

pub async fn revoke_token(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<CurrentUser>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> Response {
    let res = sqlx::query(
        "UPDATE api_tokens SET revoked_at = NOW() WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL",
    )
    .bind(id)
    .bind(cu.id)
    .execute(&state.db)
    .await;
    match res {
        Ok(r) if r.rows_affected() > 0 => {
            crate::audit::record(
                &state,
                Some(&cu),
                "token.revoked",
                "api_token",
                Some(id.to_string()),
                serde_json::json!({}),
            )
            .await;
            Json(serde_json::json!({"ok": true})).into_response()
        }
        Ok(_) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::warn!("revoke_token: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Resolve a Bearer token to a CurrentUser for the auth middleware. Throttles
/// `last_used_at` writes via Redis NX. Returns None on miss/revoked/error so
/// the middleware falls through to the next auth method.
pub async fn authenticate_bearer(state: &crate::AppState, token: &str) -> Option<CurrentUser> {
    // (token_id, user_id, role)
    let row: Option<(Uuid, Uuid, String)> = sqlx::query_as::<_, (Uuid, Uuid, String)>(
        r#"SELECT api_tokens.id, api_tokens.user_id, users.role
           FROM api_tokens
           JOIN users ON users.id = api_tokens.user_id
           WHERE api_tokens.token_hash = $1 AND api_tokens.revoked_at IS NULL"#,
    )
    .bind(hash_token(token))
    .fetch_optional(&state.db)
    .await
    .ok()?;
    let (token_id, user_id, role) = row?;
    // Best-effort touch on the token row; never fail the request on it.
    let mut conn = state.redis.clone();
    touch_token_last_used(&mut conn, &state.db, &token_id).await;
    Some(CurrentUser { id: user_id, role })
}
