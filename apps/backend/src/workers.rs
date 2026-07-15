//! Background delivery workers. `worker` pops ids from `terusin:queue` and
//! delivers; on failure schedules a retry in the `terusin:retry` sorted set
//! (exponential backoff). `retry_worker` re-attempts due retries.

use chrono::Utc;
use hmac::{Hmac, Mac};
use redis::aio::ConnectionManager;
use sha2::Sha256;
use tracing::info;
use uuid::Uuid;

use crate::model::{ForwardRule, WebhookEvent};

type HmacSha256 = Hmac<Sha256>;

/// Redis queue holding event ids awaiting delivery.
pub const QUEUE_KEY: &str = "terusin:queue";
/// Redis sorted set of retrying event ids, scored by their due timestamp.
pub const RETRY_KEY: &str = "terusin:retry";

fn is_retryable_status(status: i32) -> bool {
    status == 408 || status == 429 || status >= 500
}

/// `retry_count` counts scheduled retries, not the initial delivery. The first
/// retry is delayed by 10 seconds, then 20, 40, and so on.
fn retry_delay_secs(retry_count: i32) -> u64 {
    10 * 2u64.saturating_pow(retry_count.saturating_sub(1) as u32)
}

/// Compute `sha256=<hex>` HMAC-SHA256 signature of the request body bytes.
/// Receivers verify by recomputing over the raw body using the shared secret.
fn sign_body(secret: &str, body: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("hmac key any length");
    mac.update(body);
    format!(
        "sha256={}",
        hex_encode(mac.finalize().into_bytes().as_slice())
    )
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
           (organization_id, event_id, attempt_number, status, http_status, response_headers, response_body, error, duration_ms)
           VALUES ((SELECT organization_id FROM webhook_events WHERE id = $1), $1, $2, $3, $4, $5, $6, $7, $8)"#,
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

/// Main delivery worker. Pops event ids off the Redis queue and attempts to
/// deliver each to its `target_url`, signing with the optional global
/// `DEFAULT_SIGNING_SECRET` and recording each attempt.
pub async fn worker(db: sqlx::PgPool, mut redis: ConnectionManager, max_retries: i32) {
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

        let Some(event) = event.unwrap_or(None) else {
            continue;
        };

        if event.target_url.is_empty() {
            tracing::warn!("skip {id}: no target URL");
            sqlx::query("UPDATE webhook_events SET status = 'failed' WHERE id = $1")
                .bind(id)
                .execute(&db)
                .await
                .ok();
            forward_to_rules(&db, &event, &client, "failure", None).await;
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
                .bind(id)
                .fetch_optional(&db)
                .await
                .ok()
                .flatten()
                .unwrap_or_default()
                == "delivered"
        };

        match res {
            Ok(r) => {
                let status = r.status().as_u16() as i32;
                let mut resp_h = serde_json::Map::new();
                for (k, v) in r.headers() {
                    resp_h.insert(
                        k.to_string(),
                        serde_json::Value::String(v.to_str().unwrap_or("").to_string()),
                    );
                }
                let resp_h = serde_json::Value::Object(resp_h);
                let resp_b = r.text().await.ok();
                let resp_b_ref = resp_b.as_deref();

                if status < 300 {
                    record_attempt(
                        &db,
                        id,
                        attempt_number,
                        "delivered",
                        Some(status),
                        Some(&resp_h),
                        resp_b_ref,
                        None,
                        duration_ms,
                    )
                    .await;
                    sqlx::query(
                        "UPDATE webhook_events SET status = 'delivered', response_status = $1, response_headers = $2, response_body = $3 WHERE id = $4 AND status != 'delivered'",
                    )
                    .bind(status).bind(&resp_h).bind(&resp_b).bind(id)
                    .execute(&db).await.ok();
                    info!("delivered {id} -> {} ({})", event.target_url, status);
                    forward_to_rules(&db, &event, &client, "success", Some(status)).await;
                } else if is_retryable_status(status) && event.retry_count < max_retries {
                    let retry_count = event.retry_count + 1;
                    record_attempt(
                        &db,
                        id,
                        attempt_number,
                        "retrying",
                        Some(status),
                        Some(&resp_h),
                        resp_b_ref,
                        None,
                        duration_ms,
                    )
                    .await;
                    sqlx::query(
                        "UPDATE webhook_events SET status = 'retrying', retry_count = $1, response_status = $2, response_headers = $3, response_body = $4 WHERE id = $5 AND status != 'delivered'",
                    )
                    .bind(retry_count).bind(status).bind(&resp_h).bind(&resp_b).bind(id)
                    .execute(&db).await.ok();
                    let delay = retry_delay_secs(retry_count);
                    let retry_at = Utc::now().timestamp() + delay as i64;
                    redis::cmd("ZADD").arg(RETRY_KEY).arg(retry_at).arg(id.to_string())
                        .query_async::<()>(&mut redis).await.ok();
                    info!("queued {id} for retry #{retry_count} in {delay}s");
                } else {
                    record_attempt(
                        &db,
                        id,
                        attempt_number,
                        "failed",
                        Some(status),
                        Some(&resp_h),
                        resp_b_ref,
                        None,
                        duration_ms,
                    )
                    .await;
                    sqlx::query(
                        "UPDATE webhook_events SET status = 'failed', response_status = $1, response_headers = $2, response_body = $3 WHERE id = $4 AND status != 'delivered'",
                    )
                    .bind(status).bind(&resp_h).bind(&resp_b).bind(id)
                    .execute(&db).await.ok();
                    tracing::warn!("failed {id} -> {} ({})", event.target_url, status);
                    forward_to_rules(&db, &event, &client, "failure", Some(status)).await;
                }
            }
            Err(e) => {
                if already().await {
                    continue;
                }
                let err_msg = e.to_string();
                let retry_count = event.retry_count + 1;
                if event.retry_count >= max_retries {
                    record_attempt(
                        &db,
                        id,
                        attempt_number,
                        "failed",
                        None,
                        None,
                        None,
                        Some(&err_msg),
                        duration_ms,
                    )
                    .await;
                    sqlx::query(
                        "UPDATE webhook_events SET status = 'failed', retry_count = $1 WHERE id = $2 AND status != 'delivered'",
                    )
                    .bind(event.retry_count).bind(id)
                    .execute(&db).await.ok();
                    tracing::warn!("failed {id} after {attempt_number} attempts");
                    forward_to_rules(&db, &event, &client, "failure", None).await;
                } else {
                    record_attempt(
                        &db,
                        id,
                        attempt_number,
                        "retrying",
                        None,
                        None,
                        None,
                        Some(&err_msg),
                        duration_ms,
                    )
                    .await;
                    sqlx::query(
                        "UPDATE webhook_events SET status = 'retrying', retry_count = $1 WHERE id = $2 AND status != 'delivered'",
                    )
                    .bind(retry_count).bind(id)
                    .execute(&db).await.ok();
                    let delay = retry_delay_secs(retry_count);
                    let retry_at = Utc::now().timestamp() as i64 + delay as i64;
                    redis::cmd("ZADD")
                        .arg(RETRY_KEY)
                        .arg(retry_at)
                        .arg(id.to_string())
                        .query_async::<()>(&mut redis)
                        .await
                        .ok();
                    info!("queued {id} for retry #{retry_count} in {delay}s");
                }
            }
        }
    }
}

/// Retry worker. Pops the earliest-due retry from the sorted set; if it isn't
/// due yet, re-inserts it and waits. Otherwise re-attempts delivery with the
/// same signing/attempt-logging behavior as the main worker.
pub async fn retry_worker(db: sqlx::PgPool, mut redis: ConnectionManager) {
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

                let Some(event) = event.unwrap_or(None) else {
                    continue;
                };

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

                let is_delivered = sqlx::query_scalar::<_, String>(
                    "SELECT status FROM webhook_events WHERE id = $1",
                )
                .bind(id)
                .fetch_optional(&db)
                .await
                .ok()
                .flatten()
                .unwrap_or_default()
                    == "delivered";
                if is_delivered {
                    continue;
                }

                match res {
                    Ok(r) => {
                        let status = r.status().as_u16() as i32;
                        let ok = status < 300;
                        let mut resp_h = serde_json::Map::new();
                        for (k, v) in r.headers() {
                            resp_h.insert(
                                k.to_string(),
                                serde_json::Value::String(v.to_str().unwrap_or("").to_string()),
                            );
                        }
                        let resp_h = serde_json::Value::Object(resp_h);
                        let resp_b = r.text().await.ok();
                        let resp_b_ref = resp_b.as_deref();

                        if ok {
                            record_attempt(
                                &db,
                                id,
                                attempt_number,
                                "delivered",
                                Some(status),
                                Some(&resp_h),
                                resp_b_ref,
                                None,
                                duration_ms,
                            )
                            .await;
                            sqlx::query(
                                "UPDATE webhook_events SET status = 'delivered', response_status = $1, response_headers = $2, response_body = $3 WHERE id = $4 AND status != 'delivered'",
                            )
                            .bind(status).bind(&resp_h).bind(&resp_b).bind(id)
                            .execute(&db).await.ok();
                            info!("retry delivered {id}");
                            forward_to_rules(&db, &event, &client, "success", Some(status)).await;
                        } else if is_retryable_status(status) && event.retry_count < event.max_retries {
                            let retry_count = event.retry_count + 1;
                            record_attempt(
                                &db,
                                id,
                                attempt_number,
                                "retrying",
                                Some(status),
                                Some(&resp_h),
                                resp_b_ref,
                                None,
                                duration_ms,
                            )
                            .await;
                            sqlx::query(
                                "UPDATE webhook_events SET status = 'retrying', retry_count = $1, response_status = $2, response_headers = $3, response_body = $4 WHERE id = $5 AND status != 'delivered'",
                            )
                            .bind(retry_count).bind(status).bind(&resp_h).bind(&resp_b).bind(id)
                            .execute(&db).await.ok();
                            let delay = retry_delay_secs(retry_count);
                            let retry_at = Utc::now().timestamp() + delay as i64;
                            redis::cmd("ZADD")
                                .arg(RETRY_KEY)
                                .arg(retry_at)
                                .arg(id.to_string())
                                .query_async::<()>(&mut redis)
                                .await
                                .ok();
                        } else {
                            record_attempt(
                                &db,
                                id,
                                attempt_number,
                                "failed",
                                Some(status),
                                Some(&resp_h),
                                resp_b_ref,
                                None,
                                duration_ms,
                            )
                            .await;
                            sqlx::query(
                                "UPDATE webhook_events SET status = 'failed', response_status = $1, response_headers = $2, response_body = $3 WHERE id = $4 AND status != 'delivered'",
                            )
                            .bind(status).bind(&resp_h).bind(&resp_b).bind(id)
                            .execute(&db).await.ok();
                            tracing::warn!("retry failed {id} after {attempt_number} attempts");
                            forward_to_rules(&db, &event, &client, "failure", Some(status)).await;
                        }
                    }
                    Err(e) => {
                        let err_msg = e.to_string();
                        let retry_count = event.retry_count + 1;
                        if event.retry_count >= event.max_retries {
                            record_attempt(
                                &db,
                                id,
                                attempt_number,
                                "failed",
                                None,
                                None,
                                None,
                                Some(&err_msg),
                                duration_ms,
                            )
                            .await;
                            sqlx::query(
                                "UPDATE webhook_events SET status = 'failed', retry_count = $1 WHERE id = $2 AND status != 'delivered'",
                            )
                            .bind(event.retry_count)
                            .bind(id)
                            .execute(&db)
                            .await
                            .ok();
                            tracing::warn!("retry failed {id} after {attempt_number} attempts");
                            forward_to_rules(&db, &event, &client, "failure", None).await;
                        } else {
                            record_attempt(
                                &db,
                                id,
                                attempt_number,
                                "retrying",
                                None,
                                None,
                                None,
                                Some(&err_msg),
                                duration_ms,
                            )
                            .await;
                            sqlx::query(
                                "UPDATE webhook_events SET status = 'retrying', retry_count = $1 WHERE id = $2 AND status != 'delivered'",
                            )
                            .bind(retry_count)
                            .bind(id)
                            .execute(&db)
                            .await
                            .ok();
                            let delay = retry_delay_secs(retry_count);
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

/// Build an outbound request honoring the rule's `method`, custom `headers`,
/// and optional HMAC `signing_secret`. When a secret is present, an
/// `X-Terusin-Signature: sha256=<hex>` header is added over the body bytes.
fn build_rule_request(
    client: &reqwest::Client,
    rule: &ForwardRule,
    body: &serde_json::Value,
    delivery_status: &str,
    response_status: Option<i32>,
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
    req = req.header("X-Terusin-Delivery-Status", delivery_status);
    if let Some(status) = response_status {
        req = req.header("X-Terusin-Response-Status", status.to_string());
    }
    req
}

async fn forward_to_rules(
    db: &sqlx::PgPool,
    event: &WebhookEvent,
    client: &reqwest::Client,
    trigger_on: &str,
    response_status: Option<i32>,
) {
    let rules = sqlx::query_as::<_, ForwardRule>(
        r#"SELECT hook.*
           FROM forward_rules AS hook
           JOIN forward_rules AS provider ON provider.id = hook.provider_id
           WHERE hook.organization_id = $1
             AND hook.rule_kind = 'hook'
             AND hook.active = true
             AND hook.trigger_on = $2
             AND provider.rule_kind = 'provider'
             AND provider.active = true
             AND provider.source_pattern = $3"#,
    )
        .bind(event.organization_id)
        .bind(trigger_on)
        .bind(&event.source)
        .fetch_all(db)
        .await
        .unwrap_or_default();

    for rule in rules {
        let delivery_status = if trigger_on == "success" { "delivered" } else { "failed" };
        let req = build_rule_request(client, &rule, &event.body, delivery_status, response_status);
        let res = req.send().await;
        if let Ok(r) = res {
            tracing::info!("hook {} -> {}: {}", rule.name, rule.target_url, r.status());
        }
    }
}
