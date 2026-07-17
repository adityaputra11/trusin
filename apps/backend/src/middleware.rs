//! Auth middleware + shared auth/response helpers. Resolves a principal from
//! one of three credentials (cookie JWT, Bearer API token, HTTP Basic).

use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use base64::Engine;

use crate::auth;
use crate::model::User;
use crate::state::AppState;

/// Serialize request headers into a JSON object (used when persisting the
/// raw webhook). Header names become keys, values become strings.
pub fn headers_to_json(headers: &HeaderMap) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for (k, v) in headers.iter() {
        if let Ok(s) = v.to_str() {
            map.insert(k.to_string(), serde_json::Value::String(s.to_string()));
        }
    }
    serde_json::Value::Object(map)
}

/// 401 with a `WWW-Authenticate: Basic` challenge, used by the middleware on
/// any failed/missing credential.
pub fn unauth() -> Response {
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
pub fn require_admin(cu: &auth::CurrentUser) -> Result<(), StatusCode> {
    if cu.role == "admin" {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

/// API keys carry explicit scopes. Browser/password sessions have no key
/// scopes and continue to use the role-based admin gate above.
pub fn require_scope(cu: &auth::CurrentUser, scope: &str) -> Result<(), StatusCode> {
    if cu.scopes.is_empty() || cu.scopes.iter().any(|value| value == scope) {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

/// Platform control-plane access is a server-side user privilege, never an
/// inherited API-key scope. This prevents a leaked tenant token from gaining
/// fleet-wide visibility.
pub fn require_platform_operator(cu: &auth::CurrentUser) -> Result<(), StatusCode> {
    if cu.is_platform_operator {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
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

/// Auth middleware: try cookie JWT → Bearer API token → HTTP Basic, in order.
/// On success inserts `Extension<auth::CurrentUser>` and forwards; on any
/// failure returns 401.
pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, Response> {
    // 1) Try the session JWT cookie first (OAuth or passkey users).
    let jwt_secret = state
        .oauth
        .as_ref()
        .map(|config| config.jwt_secret.as_str())
        .or_else(|| {
            state
                .passkey
                .as_ref()
                .map(|config| config.jwt_secret.as_str())
        });
    if let Some(jwt_secret) = jwt_secret {
        if let Some(cookie) = req.headers().get("Cookie").and_then(|v| v.to_str().ok()) {
            if let Some(token) = extract_session_cookie(cookie) {
                if let Some(uid) = auth::verify_jwt(&token, jwt_secret) {
                    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
                        .bind(uid)
                        .fetch_optional(&state.db)
                        .await
                        .map_err(|_| unauth())?;
                    if let Some(u) = user {
                        req.extensions_mut().insert(auth::CurrentUser {
                            id: u.id,
                            organization_id: u.organization_id,
                            role: u.role.clone(),
                            scopes: vec![],
                            is_platform_operator: u.is_platform_operator,
                        });
                        return Ok(next.run(req).await);
                    }
                }
            }
        }
    }

    // 2) Try a Bearer API token (CLI / MCP).
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
            let db_user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE username = $1")
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
                        organization_id: u.organization_id,
                        role: u.role.clone(),
                        scopes: vec![],
                        is_platform_operator: u.is_platform_operator,
                    });
                    Ok(next.run(req).await)
                }
                _ => Err(unauth()),
            }
        }
        None => Err(unauth()),
    }
}

#[cfg(test)]
mod tests {
    use super::require_platform_operator;
    use crate::auth::CurrentUser;
    use axum::http::StatusCode;
    use uuid::Uuid;

    fn current_user(is_platform_operator: bool) -> CurrentUser {
        CurrentUser {
            id: Uuid::new_v4(),
            organization_id: Uuid::new_v4(),
            role: "admin".to_string(),
            scopes: vec![],
            is_platform_operator,
        }
    }

    #[test]
    fn platform_operator_guard_rejects_tenant_admins() {
        assert_eq!(
            require_platform_operator(&current_user(false)),
            Err(StatusCode::FORBIDDEN)
        );
        assert!(require_platform_operator(&current_user(true)).is_ok());
    }
}
