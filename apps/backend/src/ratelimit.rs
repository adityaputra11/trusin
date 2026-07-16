//! Per-IP rate limiting via `governor` (GCRA), used by the auth endpoints.

use std::net::IpAddr;
use std::sync::Arc;

use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use uuid::Uuid;

/// Keyed rate limiter type used for per-IP auth throttling.
pub type KeyedLimiter = governor::RateLimiter<
    IpAddr,
    governor::state::keyed::DefaultKeyedStateStore<IpAddr>,
    governor::clock::DefaultClock,
>;

pub type UserKeyedLimiter = governor::RateLimiter<
    Uuid,
    governor::state::keyed::DefaultKeyedStateStore<Uuid>,
    governor::clock::DefaultClock,
>;

/// Build a keyed rate limiter (GCRA via `governor`).
///
/// `period_secs` / `burst` define the quota. Requests are keyed on the
/// forwarded client IP (Cloudflare → Caddy → backend sets XFF/X-Real-IP);
/// when no forwarded header is present 0.0.0.0 is used as a fallback so the
/// limiter always has a key.
pub fn build_rate_limiter(period_secs: u64, burst: u32) -> Arc<KeyedLimiter> {
    use std::num::NonZeroU32;
    let quota = governor::Quota::with_period(std::time::Duration::from_secs(period_secs))
        .expect("non-zero period")
        .allow_burst(NonZeroU32::new(burst).expect("non-zero burst"));
    Arc::new(governor::RateLimiter::keyed(quota))
}

pub fn build_user_rate_limiter(requests_per_hour: u32) -> Arc<UserKeyedLimiter> {
    use std::num::NonZeroU32;

    let requests_per_hour = requests_per_hour.clamp(1, 100);
    let period_secs = (3_600 / u64::from(requests_per_hour)).max(1);
    let quota = governor::Quota::with_period(std::time::Duration::from_secs(period_secs))
        .expect("non-zero period")
        .allow_burst(NonZeroU32::new(requests_per_hour).expect("non-zero burst"));
    Arc::new(governor::RateLimiter::keyed(quota))
}

/// Check a per-IP rate limiter; on quota exceeded, returns a 429 Response
/// with `Retry-After` set. Otherwise returns None (caller continues).
pub fn check_rate_limit(limiter: &KeyedLimiter, ip: IpAddr) -> Option<Response> {
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
                header::RETRY_AFTER,
                HeaderValue::from_str(&secs.to_string())
                    .unwrap_or_else(|_| HeaderValue::from_static("60")),
            );
            Some(res)
        }
    }
}

pub fn check_user_rate_limit(limiter: &UserKeyedLimiter, user_id: Uuid) -> Option<Response> {
    use governor::clock::Clock;

    match limiter.check_key(&user_id) {
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
                header::RETRY_AFTER,
                HeaderValue::from_str(&secs.to_string())
                    .unwrap_or_else(|_| HeaderValue::from_static("60")),
            );
            Some(res)
        }
    }
}

/// Extract the client IP from forwarded headers (Cloudflare / proxy).
/// Checks `CF-Connecting-IP` → `X-Real-IP` → `Forwarded` / `X-Forwarded-For`.
pub fn client_ip_from(headers: &HeaderMap) -> Option<IpAddr> {
    headers
        .get("CF-Connecting-IP")
        .or_else(|| headers.get("X-Real-IP"))
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse().ok())
        .or_else(|| {
            headers
                .get(header::FORWARDED)
                .or_else(|| headers.get("X-Forwarded-For"))
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.split(',').next())
                .and_then(|s| s.trim().parse().ok())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_limiter_blocks_requests_above_the_burst() {
        let limiter = build_user_rate_limiter(2);
        let user_id = Uuid::new_v4();

        assert!(check_user_rate_limit(&limiter, user_id).is_none());
        assert!(check_user_rate_limit(&limiter, user_id).is_none());
        assert!(check_user_rate_limit(&limiter, user_id).is_some());
    }

    #[test]
    fn sign_in_limiter_blocks_the_sixth_attempt_for_an_ip() {
        let limiter = build_rate_limiter(120, 5);
        let ip = "203.0.113.10".parse().unwrap();

        for _ in 0..5 {
            assert!(check_rate_limit(&limiter, ip).is_none());
        }
        assert!(check_rate_limit(&limiter, ip).is_some());
    }
}
