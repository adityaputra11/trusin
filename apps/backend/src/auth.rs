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
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

use crate::{AppState, User};

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
            http: reqwest::Client::builder()
                .build()
                .expect("reqwest client"),
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
        let redirect_uri = std::env::var("OAUTH_REDIRECT_URI").unwrap_or_else(|_| {
            "http://localhost:5173/api/auth/callback/google".to_string()
        });
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
            http_client: reqwest::Client::builder()
                .build()
                .expect("reqwest client"),
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
    let params = [
        ("client_id", cfg.client_id.as_str()),
        ("redirect_uri", cfg.redirect_uri.as_str()),
        ("response_type", "code"),
        ("scope", "openid email profile"),
        ("access_type", "online"),
        ("prompt", "consent"),
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

// ── Endpoint: GET /api/auth/callback/google ───────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CallbackParams {
    pub code: String,
    // Google may also send `state` and `scope`; we ignore them.
}

pub async fn google_callback(
    State(state): State<Arc<AppState>>,
    Query(params): Query<CallbackParams>,
) -> Response {
    let Some(cfg) = state.oauth.as_ref() else {
        return not_configured();
    };
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
    res
}

/// Insert-or-update the user row for a Google subject. Returns the user.
async fn upsert_oauth_user(
    db: &PgPool,
    info: &GoogleUserInfo,
) -> Result<User, sqlx::Error> {
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
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect::<String>();
    let base = base[..base.len().min(32)].to_string();

    let mut candidate = base.clone();
    let mut n = 1;
    loop {
        let taken = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM users WHERE username = $1",
        )
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
    let same_site = if secure { "None" } else { "Lax" };
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

    // 2) Fall back to Basic auth (CLI / password login).
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
    use base64::Engine;
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
        Some(u) => user_json(u).into_response(),
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
