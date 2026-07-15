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
use handlebars::Handlebars;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use async_trait::async_trait;

use crate::{AppState, User};

const WELCOME_EMAIL_TEMPLATE: &str = include_str!("../templates/welcome.hbs");

/// The authenticated principal, made available to handlers via the axum
/// `Extension<CurrentUser>` extractor. Inserted by `auth_middleware` after any
/// of cookie/Basic/Bearer auth succeeds. Handlers extract it to do per-user
/// and role-based decisions.
#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub role: String,
    pub scopes: Vec<String>,
    /// Only password/cookie sessions can carry this server-side privilege.
    /// Bearer API keys deliberately never inherit platform access.
    pub is_platform_operator: bool,
}

/// Cloudflare Turnstile verification.
///
/// `verify` calls the siteverify API and returns false on any network or
/// parsing error. When enabled, Turnstile fails closed for browser sign-in.
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

/// OAuth strategy shared by every browser identity provider.
#[async_trait]
pub trait OAuthStrategy: Send + Sync {
    fn provider(&self) -> &'static str;
    fn authorization_url(&self, state: &str, code_challenge: &str) -> String;
    async fn identity(&self, code: &str, code_verifier: &str) -> Result<OAuthIdentity, String>;
}

#[derive(Debug, Clone)]
pub struct OAuthIdentity {
    pub provider: &'static str,
    pub subject: String,
    pub email: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
}

struct GoogleOAuthStrategy {
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    http_client: reqwest::Client,
}

struct GitHubOAuthStrategy {
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    http_client: reqwest::Client,
}

/// OAuth + JWT configuration, built once from env vars at startup.
#[derive(Clone)]
pub struct OAuthConfig {
    /// Where to send the user after a successful login (the SPA root).
    pub frontend_url: String,
    /// HS256 secret for signing session JWTs.
    pub jwt_secret: String,
    providers: HashMap<&'static str, Arc<dyn OAuthStrategy>>,
}

impl OAuthConfig {
    /// Returns Some(config when at least one browser OAuth provider is configured.
    pub fn from_env() -> Option<Self> {
        let app_url = std::env::var("APP_URL")
            .or_else(|_| std::env::var("FRONTEND_URL"))
            .unwrap_or_else(|_| "http://localhost:5173".to_string())
            .trim_end_matches('/')
            .to_string();
        let frontend_url = std::env::var("FRONTEND_URL").unwrap_or(app_url);
        let jwt_secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| {
            tracing::warn!("JWT_SECRET not set — using insecure default. Set it in production!");
            "dev-insecure-secret-change-me".to_string()
        });
        let client = reqwest::Client::builder().build().expect("reqwest client");
        let mut providers: HashMap<&'static str, Arc<dyn OAuthStrategy>> = HashMap::new();

        if let (Ok(client_id), Ok(client_secret)) = (
            std::env::var("GOOGLE_CLIENT_ID"),
            std::env::var("GOOGLE_CLIENT_SECRET"),
        ) {
            if !client_id.trim().is_empty() && !client_secret.trim().is_empty() {
                let redirect_uri = std::env::var("OAUTH_REDIRECT_URI")
                    .unwrap_or_else(|_| format!("{frontend_url}/api/auth/callback/google"));
                providers.insert(
                    "google",
                    Arc::new(GoogleOAuthStrategy {
                        client_id,
                        client_secret,
                        redirect_uri,
                        http_client: client.clone(),
                    }),
                );
            }
        }
        if let (Ok(client_id), Ok(client_secret)) = (
            std::env::var("GITHUB_CLIENT_ID"),
            std::env::var("GITHUB_CLIENT_SECRET"),
        ) {
            if !client_id.trim().is_empty() && !client_secret.trim().is_empty() {
                providers.insert(
                    "github",
                    Arc::new(GitHubOAuthStrategy {
                        client_id,
                        client_secret,
                        redirect_uri: format!(
                            "{}/api/auth/callback/github",
                            frontend_url.trim_end_matches('/')
                        ),
                        http_client: client.clone(),
                    }),
                );
            }
        }
        (!providers.is_empty()).then_some(Self {
            frontend_url,
            jwt_secret,
            providers,
        })
    }

    pub fn provider(&self, provider: &str) -> Option<&Arc<dyn OAuthStrategy>> {
        self.providers.get(provider)
    }

    pub fn enabled_providers(&self) -> Vec<&'static str> {
        let mut providers = self.providers.keys().copied().collect::<Vec<_>>();
        providers.sort_unstable();
        providers
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
    pub scopes: Vec<String>,
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

// ── Browser OAuth strategies ─────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct CodeExchangeResponse {
    access_token: String,
}

#[derive(Debug, Deserialize)]
struct GoogleUserInfo {
    sub: String,
    email: String,
    #[serde(default)]
    email_verified: bool,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    picture: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GitHubUserInfo {
    id: u64,
    login: String,
    email: Option<String>,
    name: Option<String>,
    avatar_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GitHubEmail {
    email: String,
    primary: bool,
    verified: bool,
}

#[async_trait]
impl OAuthStrategy for GoogleOAuthStrategy {
    fn provider(&self) -> &'static str {
        "google"
    }

    fn authorization_url(&self, state: &str, code_challenge: &str) -> String {
        let params = [
            ("client_id", self.client_id.as_str()),
            ("redirect_uri", self.redirect_uri.as_str()),
            ("response_type", "code"),
            ("scope", "openid email profile"),
            ("access_type", "online"),
            ("prompt", "consent"),
            ("code_challenge", code_challenge),
            ("code_challenge_method", "S256"),
            ("state", state),
        ];
        format!(
            "https://accounts.google.com/o/oauth2/v2/auth?{}",
            serde_urlencoded::to_string(params).unwrap_or_default()
        )
    }

    async fn identity(&self, code: &str, code_verifier: &str) -> Result<OAuthIdentity, String> {
        let tokens: CodeExchangeResponse = self
            .http_client
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("code", code),
                ("client_id", &self.client_id),
                ("client_secret", &self.client_secret),
                ("redirect_uri", &self.redirect_uri),
                ("grant_type", "authorization_code"),
                ("code_verifier", code_verifier),
            ])
            .send()
            .await
            .map_err(|error| format!("token request failed: {error}"))?
            .error_for_status()
            .map_err(|error| format!("token exchange rejected: {error}"))?
            .json()
            .await
            .map_err(|error| format!("token response invalid: {error}"))?;
        let user: GoogleUserInfo = self
            .http_client
            .get("https://www.googleapis.com/oauth2/v3/userinfo")
            .bearer_auth(tokens.access_token)
            .send()
            .await
            .map_err(|error| format!("profile request failed: {error}"))?
            .error_for_status()
            .map_err(|error| format!("profile request rejected: {error}"))?
            .json()
            .await
            .map_err(|error| format!("profile response invalid: {error}"))?;
        if !user.email_verified {
            return Err("Google did not return a verified email address.".to_string());
        }
        Ok(OAuthIdentity {
            provider: self.provider(),
            subject: user.sub,
            email: user.email.clone(),
            display_name: user.name.unwrap_or(user.email),
            avatar_url: user.picture,
        })
    }
}

#[async_trait]
impl OAuthStrategy for GitHubOAuthStrategy {
    fn provider(&self) -> &'static str {
        "github"
    }

    fn authorization_url(&self, state: &str, code_challenge: &str) -> String {
        let params = [
            ("client_id", self.client_id.as_str()),
            ("redirect_uri", self.redirect_uri.as_str()),
            ("scope", "read:user user:email"),
            ("state", state),
            ("code_challenge", code_challenge),
            ("code_challenge_method", "S256"),
        ];
        format!(
            "https://github.com/login/oauth/authorize?{}",
            serde_urlencoded::to_string(params).unwrap_or_default()
        )
    }

    async fn identity(&self, code: &str, code_verifier: &str) -> Result<OAuthIdentity, String> {
        let tokens: CodeExchangeResponse = self
            .http_client
            .post("https://github.com/login/oauth/access_token")
            .header(header::ACCEPT, "application/json")
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("code", code),
                ("redirect_uri", self.redirect_uri.as_str()),
                ("code_verifier", code_verifier),
            ])
            .send()
            .await
            .map_err(|error| format!("token request failed: {error}"))?
            .error_for_status()
            .map_err(|error| format!("token exchange rejected: {error}"))?
            .json()
            .await
            .map_err(|error| format!("token response invalid: {error}"))?;
        let user: GitHubUserInfo = self
            .http_client
            .get("https://api.github.com/user")
            .header(header::USER_AGENT, "trusin")
            .bearer_auth(&tokens.access_token)
            .send()
            .await
            .map_err(|error| format!("profile request failed: {error}"))?
            .error_for_status()
            .map_err(|error| format!("profile request rejected: {error}"))?
            .json()
            .await
            .map_err(|error| format!("profile response invalid: {error}"))?;
        let emails: Vec<GitHubEmail> = self
            .http_client
            .get("https://api.github.com/user/emails")
            .header(header::USER_AGENT, "trusin")
            .bearer_auth(&tokens.access_token)
            .send()
            .await
            .map_err(|error| format!("email request failed: {error}"))?
            .error_for_status()
            .map_err(|error| format!("email request rejected: {error}"))?
            .json()
            .await
            .map_err(|error| format!("email response invalid: {error}"))?;
        let email = emails
            .into_iter()
            .find(|entry| entry.primary && entry.verified)
            .map(|entry| entry.email)
            .or(user.email)
            .ok_or_else(|| "GitHub did not return a verified primary email address.".to_string())?;
        Ok(OAuthIdentity {
            provider: self.provider(),
            subject: user.id.to_string(),
            email,
            display_name: user.name.unwrap_or(user.login),
            avatar_url: user.avatar_url,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct OAuthLoginQuery {
    pub invite: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CaptchaGrantRequest {
    pub turnstile_token: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct OAuthState {
    provider: String,
    invite_token: Option<String>,
    code_verifier: String,
}

#[derive(Debug, Deserialize)]
pub struct CallbackParams {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

pub async fn google_login(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<OAuthLoginQuery>,
) -> Response {
    oauth_login_for_provider(state, headers, "google", query).await
}

pub async fn github_login(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<OAuthLoginQuery>,
) -> Response {
    oauth_login_for_provider(state, headers, "github", query).await
}

/// Verifies a Turnstile token and issues a short-lived, single-use grant for
/// OAuth. The grant is stored server-side and delivered only through an
/// HTTP-only cookie, so direct OAuth URLs cannot bypass the captcha.
pub async fn create_captcha_grant(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<CaptchaGrantRequest>,
) -> Response {
    let Some(cfg) = state.turnstile.as_ref() else {
        return Json(serde_json::json!({ "captcha_required": false })).into_response();
    };
    let token = req.turnstile_token.trim();
    if token.is_empty() {
        return captcha_error(StatusCode::BAD_REQUEST, "captcha_required");
    }
    let ip = crate::client_ip_from(&headers)
        .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::new(0, 0, 0, 0)));
    let remoteip = ip.to_string();
    if !cfg.verify(token, Some(&remoteip)).await {
        return captcha_error(StatusCode::BAD_REQUEST, "captcha_failed");
    }

    let grant = generate_oauth_secret();
    let key = captcha_grant_key(&grant);
    let mut redis = state.redis.clone();
    let stored = redis::cmd("SETEX")
        .arg(key)
        .arg(300)
        .arg(remoteip)
        .query_async::<()>(&mut redis)
        .await;
    if let Err(error) = stored {
        tracing::error!("captcha grant store failed: {error}");
        return captcha_error(StatusCode::SERVICE_UNAVAILABLE, "captcha_unavailable");
    }

    let secure = std::env::var("FRONTEND_URL")
        .or_else(|_| std::env::var("APP_URL"))
        .map(|url| url.starts_with("https://"))
        .unwrap_or(false);
    let cookie = format!(
        "trusin_captcha={grant}; Path=/api/auth; Max-Age=300; HttpOnly; SameSite=Lax{}",
        if secure { "; Secure" } else { "" },
    );
    let mut response = Json(serde_json::json!({ "captcha_required": true })).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        header::HeaderValue::from_str(&cookie).expect("captcha cookie header"),
    );
    response
}

async fn oauth_login_for_provider(
    state: Arc<AppState>,
    headers: HeaderMap,
    provider_name: &'static str,
    query: OAuthLoginQuery,
) -> Response {
    let Some(cfg) = state.oauth.as_ref() else {
        return not_configured(provider_name);
    };
    let Some(provider) = cfg.provider(provider_name) else {
        return not_configured(provider_name);
    };
    if let Err(response) = consume_captcha_grant(&state, &headers).await {
        return response;
    }
    let state_token = generate_oauth_secret();
    let code_verifier = generate_oauth_secret();
    let stored_state = OAuthState {
        provider: provider_name.to_string(),
        invite_token: query.invite.filter(|token| !token.trim().is_empty()),
        code_verifier: code_verifier.clone(),
    };
    let mut redis = state.redis.clone();
    let stored = redis::cmd("SETEX")
        .arg(format!("terusin:oauth:state:{state_token}"))
        .arg(600)
        .arg(serde_json::to_string(&stored_state).unwrap_or_default())
        .query_async::<()>(&mut redis)
        .await;
    if let Err(error) = stored {
        tracing::warn!(
            provider = provider_name,
            "oauth state store failed: {error}"
        );
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "Could not start sign-in. Please try again.",
        )
            .into_response();
    }
    Redirect::temporary(&provider.authorization_url(&state_token, &pkce_challenge(&code_verifier)))
        .into_response()
}

fn not_configured(provider: &str) -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        format!("{} sign-in is not configured.", provider),
    )
        .into_response()
}

fn generate_oauth_secret() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn captcha_grant_key(grant: &str) -> String {
    format!(
        "terusin:captcha:grant:{:x}",
        Sha256::digest(grant.as_bytes())
    )
}

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())?
        .split(';')
        .find_map(|part| {
            let (key, value) = part.trim().split_once('=')?;
            (key == name).then(|| value.to_string())
        })
}

async fn consume_captcha_grant(state: &AppState, headers: &HeaderMap) -> Result<(), Response> {
    if state.turnstile.is_none() {
        return Ok(());
    }
    let Some(grant) = cookie_value(headers, "trusin_captcha") else {
        return Err(captcha_error(StatusCode::FORBIDDEN, "captcha_required"));
    };
    let ip = crate::client_ip_from(headers)
        .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::new(0, 0, 0, 0)))
        .to_string();
    let mut redis = state.redis.clone();
    let stored_ip = redis::cmd("GETDEL")
        .arg(captcha_grant_key(&grant))
        .query_async::<Option<String>>(&mut redis)
        .await
        .map_err(|error| {
            tracing::error!("captcha grant lookup failed: {error}");
            captcha_error(StatusCode::SERVICE_UNAVAILABLE, "captcha_unavailable")
        })?;
    if stored_ip.as_deref() != Some(ip.as_str()) {
        return Err(captcha_error(StatusCode::FORBIDDEN, "captcha_required"));
    }
    Ok(())
}

fn captcha_error(status: StatusCode, error: &str) -> Response {
    (status, Json(serde_json::json!({ "error": error }))).into_response()
}

fn pkce_challenge(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

pub async fn google_callback(
    State(state): State<Arc<AppState>>,
    Query(params): Query<CallbackParams>,
) -> Response {
    oauth_callback_for_provider(state, "google", params).await
}

pub async fn github_callback(
    State(state): State<Arc<AppState>>,
    Query(params): Query<CallbackParams>,
) -> Response {
    oauth_callback_for_provider(state, "github", params).await
}

async fn oauth_callback_for_provider(
    state: Arc<AppState>,
    provider_name: &'static str,
    params: CallbackParams,
) -> Response {
    let Some(cfg) = state.oauth.as_ref() else {
        return not_configured(provider_name);
    };
    if params.error.is_some() {
        return oauth_error(cfg, "Sign-in was cancelled or denied.");
    }
    let (Some(code), Some(state_token)) = (params.code.as_deref(), params.state.as_deref()) else {
        return oauth_error(
            cfg,
            "The sign-in response was incomplete. Please try again.",
        );
    };
    let mut redis = state.redis.clone();
    let key = format!("terusin:oauth:state:{state_token}");
    let valid_state: Option<String> = redis::cmd("GET")
        .arg(&key)
        .query_async(&mut redis)
        .await
        .ok();
    let Some(valid_state) = valid_state else {
        return oauth_error(cfg, "Sign-in expired. Please try again.");
    };
    let _ = redis::cmd("DEL")
        .arg(&key)
        .query_async::<()>(&mut redis)
        .await;
    let stored_state = serde_json::from_str::<OAuthState>(&valid_state).unwrap_or_default();
    if stored_state.provider != provider_name {
        return oauth_error(cfg, "Invalid sign-in response. Please try again.");
    }
    let Some(provider) = cfg.provider(provider_name) else {
        return not_configured(provider_name);
    };
    let identity = match provider.identity(code, &stored_state.code_verifier).await {
        Ok(identity) => identity,
        Err(error) => {
            tracing::warn!(provider = provider_name, "oauth identity failed: {error}");
            return oauth_error(cfg, "We could not verify your account. Please try again.");
        }
    };
    let result =
        match upsert_oauth_user(&state.db, &identity, stored_state.invite_token.as_deref()).await {
            Ok(result) => result,
            Err(sqlx::Error::RowNotFound) => {
                return oauth_error(
                    cfg,
                    "This invitation is invalid, expired, or belongs to a different account.",
                )
            }
            Err(error) => {
                tracing::error!(provider = provider_name, "upsert oauth user: {error}");
                return oauth_error(cfg, "Could not save your account.");
            }
        };
    if result.created_workspace {
        trigger_welcome_delivery(&state, result.user.id).await;
    }
    let jwt = match issue_jwt(&result.user.id, &cfg.jwt_secret) {
        Ok(token) => token,
        Err(error) => {
            tracing::error!("jwt issue: {error}");
            return oauth_error(cfg, "Could not start a session.");
        }
    };
    let (name, value) = build_cookie(&jwt, cfg);
    let destination = if result.created_workspace {
        format!("{}?welcome=1", cfg.frontend_url.trim_end_matches('/'))
    } else {
        cfg.frontend_url.clone()
    };
    let mut response = Redirect::temporary(&destination).into_response();
    response.headers_mut().insert(name, value);
    let current_user = CurrentUser {
        id: result.user.id,
        organization_id: result.user.organization_id,
        role: result.user.role.clone(),
        scopes: vec![],
        is_platform_operator: result.user.is_platform_operator,
    };
    crate::audit::record(
        &state,
        Some(&current_user),
        &format!("auth.{provider_name}_login"),
        "user",
        Some(result.user.id.to_string()),
        serde_json::json!({ "email": result.user.email }),
    )
    .await;
    if stored_state.invite_token.is_some() {
        crate::audit::record(
            &state,
            Some(&current_user),
            "invite.accepted",
            "invite",
            None,
            serde_json::json!({ "email": result.user.email }),
        )
        .await;
    }
    response
}

struct OAuthUpsert {
    user: User,
    created_workspace: bool,
}

async fn upsert_oauth_user(
    db: &PgPool,
    info: &OAuthIdentity,
    invite_token: Option<&str>,
) -> Result<OAuthUpsert, sqlx::Error> {
    let invite = match invite_token {
        Some(token) => crate::invites::invite_for_token(db, token, &info.email).await?,
        None => None,
    };
    let existing_by_identity = sqlx::query_as::<_, User>(
        r#"SELECT users.* FROM users
           JOIN user_oauth_identities identities ON identities.user_id = users.id
           WHERE identities.provider = $1 AND identities.subject = $2"#,
    )
    .bind(info.provider)
    .bind(&info.subject)
    .fetch_optional(db)
    .await?;
    if let Some(existing) = existing_by_identity {
        validate_invite_organization(invite.as_ref(), existing.organization_id)?;
        refresh_oauth_user(db, existing.id, info).await?;
        accept_invite_if_present(db, invite.as_ref()).await?;
        return fetch_oauth_upsert(db, existing.id, false).await;
    }

    if let Some(existing) = sqlx::query_as::<_, User>("SELECT * FROM users WHERE email = $1")
        .bind(&info.email)
        .fetch_optional(db)
        .await?
    {
        validate_invite_organization(invite.as_ref(), existing.organization_id)?;
        let mut transaction = db.begin().await?;
        link_identity(&mut transaction, existing.id, info).await?;
        sqlx::query(
            r#"UPDATE users SET display_name = COALESCE(display_name, $1),
               avatar_url = COALESCE(avatar_url, $2) WHERE id = $3"#,
        )
        .bind(&info.display_name)
        .bind(&info.avatar_url)
        .bind(existing.id)
        .execute(&mut *transaction)
        .await?;
        accept_invite_in_transaction(&mut transaction, invite.as_ref()).await?;
        transaction.commit().await?;
        return fetch_oauth_upsert(db, existing.id, false).await;
    }

    if let Some((invite_id, role, organization_id)) = invite {
        let mut transaction = db.begin().await?;
        let user = insert_oauth_user(&mut transaction, organization_id, &role, info).await?;
        link_identity(&mut transaction, user.id, info).await?;
        let accepted = sqlx::query(
            "UPDATE organization_invites SET accepted_at = NOW() WHERE id = $1 AND accepted_at IS NULL AND revoked_at IS NULL AND expires_at > NOW()",
        )
        .bind(invite_id)
        .execute(&mut *transaction)
        .await?;
        if accepted.rows_affected() != 1 {
            return Err(sqlx::Error::RowNotFound);
        }
        transaction.commit().await?;
        return Ok(OAuthUpsert {
            user,
            created_workspace: false,
        });
    }

    let display_name = info.display_name.trim();
    let workspace_name = format!("{}'s workspace", display_name);
    let base_slug: String = info
        .email
        .split('@')
        .next()
        .unwrap_or("workspace")
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let slug = format!(
        "{}-{}",
        base_slug
            .trim_matches('-')
            .chars()
            .take(60)
            .collect::<String>(),
        Uuid::new_v4().simple()
    );
    let mut transaction = db.begin().await?;
    let organization_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO organizations (name, slug, plan_code, subscription_status, subscriber_name, billing_contact_name, billing_contact_email)
           VALUES ($1, $2, 'free', 'active', $1, $3, $4) RETURNING id"#,
    )
    .bind(workspace_name)
    .bind(slug)
    .bind(display_name)
    .bind(&info.email)
    .fetch_one(&mut *transaction)
    .await?;
    let user = insert_oauth_user(&mut transaction, organization_id, "admin", info).await?;
    link_identity(&mut transaction, user.id, info).await?;
    sqlx::query(
        "INSERT INTO email_deliveries (user_id, kind, recipient) VALUES ($1, 'welcome', $2)",
    )
    .bind(user.id)
    .bind(&info.email)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(OAuthUpsert {
        user,
        created_workspace: true,
    })
}

async fn insert_oauth_user(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    organization_id: Uuid,
    role: &str,
    info: &OAuthIdentity,
) -> Result<User, sqlx::Error> {
    sqlx::query_as::<_, User>(
        r#"INSERT INTO users (id, organization_id, role, email, display_name, avatar_url, oauth_provider, oauth_subject)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING *"#,
    )
    .bind(Uuid::new_v4())
    .bind(organization_id)
    .bind(role)
    .bind(&info.email)
    .bind(&info.display_name)
    .bind(&info.avatar_url)
    .bind(info.provider)
    .bind(&info.subject)
    .fetch_one(&mut **transaction)
    .await
}

async fn link_identity(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: Uuid,
    info: &OAuthIdentity,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO user_oauth_identities (user_id, provider, subject, email) VALUES ($1, $2, $3, $4) ON CONFLICT (provider, subject) DO NOTHING",
    )
    .bind(user_id)
    .bind(info.provider)
    .bind(&info.subject)
    .bind(&info.email)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

fn validate_invite_organization(
    invite: Option<&(Uuid, String, Uuid)>,
    organization_id: Uuid,
) -> Result<(), sqlx::Error> {
    if invite.is_some_and(|(_, _, expected)| *expected != organization_id) {
        Err(sqlx::Error::RowNotFound)
    } else {
        Ok(())
    }
}

async fn refresh_oauth_user(
    db: &PgPool,
    user_id: Uuid,
    info: &OAuthIdentity,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE users SET email = $1, display_name = $2, avatar_url = $3 WHERE id = $4")
        .bind(&info.email)
        .bind(&info.display_name)
        .bind(&info.avatar_url)
        .bind(user_id)
        .execute(db)
        .await?;
    Ok(())
}

async fn accept_invite_if_present(
    db: &PgPool,
    invite: Option<&(Uuid, String, Uuid)>,
) -> Result<(), sqlx::Error> {
    if let Some((invite_id, _, _)) = invite {
        sqlx::query("UPDATE organization_invites SET accepted_at = NOW() WHERE id = $1")
            .bind(invite_id)
            .execute(db)
            .await?;
    }
    Ok(())
}

async fn accept_invite_in_transaction(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    invite: Option<&(Uuid, String, Uuid)>,
) -> Result<(), sqlx::Error> {
    if let Some((invite_id, _, _)) = invite {
        sqlx::query("UPDATE organization_invites SET accepted_at = NOW() WHERE id = $1")
            .bind(invite_id)
            .execute(&mut **transaction)
            .await?;
    }
    Ok(())
}

async fn fetch_oauth_upsert(
    db: &PgPool,
    user_id: Uuid,
    created_workspace: bool,
) -> Result<OAuthUpsert, sqlx::Error> {
    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(db)
        .await?;
    Ok(OAuthUpsert {
        user,
        created_workspace,
    })
}

#[derive(sqlx::FromRow)]
struct WelcomeDelivery {
    id: Uuid,
    user_id: Uuid,
    recipient: String,
    display_name: Option<String>,
}

async fn trigger_welcome_delivery(state: &Arc<AppState>, user_id: Uuid) {
    if let Err(error) = deliver_welcome_email(&state.db, user_id).await {
        tracing::warn!(%user_id, "welcome email delivery deferred: {error}");
    }
}

pub async fn welcome_email_worker(db: PgPool) {
    loop {
        let users = sqlx::query_scalar::<_, Uuid>(
            "SELECT user_id FROM email_deliveries WHERE kind = 'welcome' AND ((status IN ('pending', 'failed') AND next_attempt_at <= NOW()) OR (status = 'sending' AND updated_at <= NOW() - INTERVAL '15 minutes')) ORDER BY created_at LIMIT 20",
        )
        .fetch_all(&db)
        .await
        .unwrap_or_default();
        for user_id in users {
            if let Err(error) = deliver_welcome_email(&db, user_id).await {
                tracing::warn!(%user_id, "welcome email worker delivery failed: {error}");
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    }
}

async fn deliver_welcome_email(db: &PgPool, user_id: Uuid) -> Result<(), String> {
    let delivery = sqlx::query_as::<_, WelcomeDelivery>(
        r#"UPDATE email_deliveries SET status = 'sending', attempts = attempts + 1, updated_at = NOW()
           WHERE id = (
             SELECT id FROM email_deliveries WHERE user_id = $1 AND kind = 'welcome'
             AND ((status IN ('pending', 'failed') AND next_attempt_at <= NOW()) OR (status = 'sending' AND updated_at <= NOW() - INTERVAL '15 minutes'))
             FOR UPDATE SKIP LOCKED
           )
           RETURNING id, user_id, recipient,
             (SELECT display_name FROM users WHERE users.id = email_deliveries.user_id) AS display_name"#,
    )
    .bind(user_id)
    .fetch_optional(db)
    .await
    .map_err(|error| format!("claim welcome delivery: {error}"))?;
    let Some(delivery) = delivery else {
        return Ok(());
    };
    let send_result =
        async {
            let api_key = std::env::var("RESEND_API_KEY")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| "RESEND_API_KEY is not configured".to_string())?;
            let from = std::env::var("EMAIL_FROM")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| "EMAIL_FROM is not configured".to_string())?;
            let app_url = std::env::var("APP_URL")
                .or_else(|_| std::env::var("FRONTEND_URL"))
                .unwrap_or_else(|_| "http://localhost:5173".to_string());
            let html = Handlebars::new().render_template(
            WELCOME_EMAIL_TEMPLATE,
            &serde_json::json!({
                "display_name": delivery.display_name.as_deref().unwrap_or(&delivery.recipient),
                "app_url": app_url.trim_end_matches('/'),
            }),
        ).map_err(|error| format!("render welcome template: {error}"))?;
            let response: serde_json::Value = reqwest::Client::new()
                .post("https://api.resend.com/emails")
                .bearer_auth(api_key)
                .json(&serde_json::json!({
                    "from": from,
                    "to": [delivery.recipient],
                    "subject": "Welcome to trusin",
                    "html": html,
                }))
                .send()
                .await
                .map_err(|error| format!("send welcome email: {error}"))?
                .error_for_status()
                .map_err(|error| format!("Resend rejected welcome email: {error}"))?
                .json()
                .await
                .map_err(|error| format!("parse Resend response: {error}"))?;
            Ok::<_, String>(
                response
                    .get("id")
                    .and_then(|id| id.as_str())
                    .map(str::to_string),
            )
        }
        .await;
    match send_result {
        Ok(provider_message_id) => {
            sqlx::query("UPDATE email_deliveries SET status = 'sent', provider_message_id = $1, sent_at = NOW(), updated_at = NOW(), last_error = NULL WHERE id = $2")
                .bind(provider_message_id).bind(delivery.id).execute(db).await
                .map_err(|error| format!("record welcome delivery: {error}"))?;
            tracing::info!(user_id = %delivery.user_id, "welcome email sent");
            Ok(())
        }
        Err(error) => {
            sqlx::query("UPDATE email_deliveries SET status = 'failed', last_error = $1, next_attempt_at = NOW() + INTERVAL '15 minutes', updated_at = NOW() WHERE id = $2")
                .bind(&error).bind(delivery.id).execute(db).await
                .map_err(|update_error| format!("record welcome failure: {update_error}"))?;
            Err(error)
        }
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
                    organization_id: u.organization_id,
                    role: u.role.clone(),
                    scopes: vec![],
                    is_platform_operator: u.is_platform_operator,
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
        "organization_id": u.organization_id,
        "is_platform_operator": u.is_platform_operator,
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
    #[serde(default)]
    pub scopes: Option<Vec<String>>,
}

pub async fn create_token(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<CurrentUser>,
    Json(body): Json<CreateTokenRequest>,
) -> Response {
    if crate::middleware::require_admin(&cu).is_err() {
        return StatusCode::FORBIDDEN.into_response();
    }
    if crate::middleware::require_scope(&cu, "organization:manage").is_err() {
        return StatusCode::FORBIDDEN.into_response();
    }
    let name = body.name.trim();
    if name.is_empty() || name.len() > 120 {
        return (StatusCode::BAD_REQUEST, "token name must be 1-120 chars").into_response();
    }

    if crate::organizations::ensure_resource_quota(&state, cu.organization_id, "api_keys")
        .await
        .is_err()
    {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({"error": "api_key_quota_exceeded"})),
        )
            .into_response();
    }
    let allowed = [
        "events:read",
        "webhooks:send",
        "rules:read",
        "rules:write",
        "organization:manage",
    ];
    let scopes = body.scopes.unwrap_or_else(|| {
        vec![
            "events:read".to_string(),
            "webhooks:send".to_string(),
            "rules:read".to_string(),
            "rules:write".to_string(),
            "organization:manage".to_string(),
        ]
    });
    if scopes.is_empty()
        || scopes
            .iter()
            .any(|scope| !allowed.contains(&scope.as_str()))
    {
        return (StatusCode::BAD_REQUEST, "invalid API key scopes").into_response();
    }
    let token = generate_token();
    let hash = hash_token(&token);
    let id = Uuid::new_v4();
    let inserted = sqlx::query(
        r#"INSERT INTO api_tokens (id, user_id, organization_id, name, token_hash, scopes)
           VALUES ($1, $2, $3, $4, $5, $6)"#,
    )
    .bind(id)
    .bind(cu.id)
    .bind(cu.organization_id)
    .bind(name)
    .bind(&hash)
    .bind(&scopes)
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
        "scopes": scopes,
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
    if crate::middleware::require_admin(&cu).is_err() {
        return StatusCode::FORBIDDEN.into_response();
    }
    if crate::middleware::require_scope(&cu, "organization:manage").is_err() {
        return StatusCode::FORBIDDEN.into_response();
    }
    match sqlx::query_as::<_, ApiTokenInfo>(
        r#"SELECT id, name, scopes, last_used_at, created_at FROM api_tokens
           WHERE organization_id = $1 AND revoked_at IS NULL
           ORDER BY created_at DESC"#,
    )
    .bind(cu.organization_id)
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
    if crate::middleware::require_admin(&cu).is_err() {
        return StatusCode::FORBIDDEN.into_response();
    }
    if crate::middleware::require_scope(&cu, "organization:manage").is_err() {
        return StatusCode::FORBIDDEN.into_response();
    }
    let res = sqlx::query(
        "UPDATE api_tokens SET revoked_at = NOW() WHERE id = $1 AND organization_id = $2 AND revoked_at IS NULL",
    )
    .bind(id)
    .bind(cu.organization_id)
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
    // (token_id, user_id, organization_id, role, scopes)
    let row: Option<(Uuid, Uuid, Uuid, String, Vec<String>)> = sqlx::query_as(
        r#"SELECT api_tokens.id, api_tokens.user_id, api_tokens.organization_id, users.role, api_tokens.scopes
           FROM api_tokens
           JOIN users ON users.id = api_tokens.user_id
           WHERE api_tokens.token_hash = $1 AND api_tokens.revoked_at IS NULL
             AND users.organization_id = api_tokens.organization_id"#,
    )
    .bind(hash_token(token))
    .fetch_optional(&state.db)
    .await
    .ok()?;
    let (token_id, user_id, organization_id, role, scopes) = row?;
    // Best-effort touch on the token row; never fail the request on it.
    let mut conn = state.redis.clone();
    touch_token_last_used(&mut conn, &state.db, &token_id).await;
    Some(CurrentUser {
        id: user_id,
        organization_id,
        role,
        scopes,
        is_platform_operator: false,
    })
}
