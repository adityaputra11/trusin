//! Organization tenancy, custom ingest domains, and hosted Free-plan limits.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use chrono::{Datelike, Utc};
use hickory_resolver::proto::rr::RecordType;
use hickory_resolver::TokioAsyncResolver;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::middleware::{require_admin, require_platform_operator, require_scope};
use crate::state::AppState;

pub const FREE_EVENT_LIMIT: i64 = 10_000;
pub const FREE_DOMAIN_LIMIT: i64 = 1;
pub const FREE_PROVIDER_LIMIT: i64 = 10;
pub const FREE_API_KEY_LIMIT: i64 = 3;
pub const FREE_USER_LIMIT: i64 = 1;
pub const FREE_RETENTION_DAYS: i64 = 7;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Organization {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub plan_code: String,
    pub subscription_status: String,
    pub billing_period_start: chrono::DateTime<chrono::Utc>,
    pub billing_period_end: chrono::DateTime<chrono::Utc>,
    pub default_target_url: String,
}

#[derive(Debug, sqlx::FromRow)]
struct OrganizationSummary {
    id: Uuid,
    name: String,
    slug: String,
    plan_code: String,
    subscription_status: String,
    billing_period_start: chrono::DateTime<chrono::Utc>,
    billing_period_end: chrono::DateTime<chrono::Utc>,
    default_target_url: String,
    ingest_key: String,
    events_accepted: i64,
    domains: i64,
    providers: i64,
    api_keys: i64,
    users: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct OrganizationDomain {
    pub id: Uuid,
    pub hostname: String,
    pub verification_token: String,
    pub status: String,
    pub verified_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateDomain {
    pub hostname: String,
}

#[derive(Debug, Deserialize)]
pub struct ProvisionOrganization {
    pub name: String,
    pub slug: String,
    pub username: String,
    pub password: String,
    pub email: Option<String>,
    pub subscriber_name: Option<String>,
    pub billing_contact_name: Option<String>,
    pub billing_contact_email: Option<String>,
}

/// Provisioning is operator-only: no dashboard route can create organizations.
pub async fn provision_organization(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<CurrentUser>,
    Json(input): Json<ProvisionOrganization>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    require_platform_operator(&cu)?;

    let name = input.name.trim();
    let slug = input.slug.trim().to_ascii_lowercase();
    let username = input.username.trim();
    let email = input
        .email
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let subscriber_name = input
        .subscriber_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(name);
    let billing_contact_name = input
        .billing_contact_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(username);
    let billing_contact_email = input
        .billing_contact_email
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or(email)
        .unwrap_or("");
    if name.is_empty()
        || name.len() > 120
        || slug.is_empty()
        || slug.len() > 80
        || !slug.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
        || username.is_empty()
        || username.len() > 255
        || subscriber_name.len() > 120
        || billing_contact_name.len() > 120
        || billing_contact_email.len() > 255
        || input.password.len() < 12
    {
        return Err(StatusCode::BAD_REQUEST);
    }

    let password_hash = bcrypt::hash(&input.password, bcrypt::DEFAULT_COST)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut transaction = state
        .db
        .begin()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let organization = sqlx::query_as::<_, Organization>(
        r#"INSERT INTO organizations
           (name, slug, subscriber_name, billing_contact_name, billing_contact_email)
           VALUES ($1, $2, $3, $4, $5)
           RETURNING id, name, slug, plan_code, subscription_status,
                     billing_period_start, billing_period_end, default_target_url"#,
    )
    .bind(name)
    .bind(&slug)
    .bind(subscriber_name)
    .bind(billing_contact_name)
    .bind(billing_contact_email)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|error| {
        tracing::warn!("provision organization insert: {error}");
        StatusCode::CONFLICT
    })?;
    let user_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO users (id, organization_id, username, password_hash, role, email)
           VALUES ($1, $2, $3, $4, 'admin', $5)"#,
    )
    .bind(user_id)
    .bind(organization.id)
    .bind(username)
    .bind(password_hash)
    .bind(email)
    .execute(&mut *transaction)
    .await
    .map_err(|error| {
        tracing::warn!("provision organization user: {error}");
        StatusCode::CONFLICT
    })?;
    transaction
        .commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "organization": organization,
            "initial_admin_id": user_id,
            "subscriber_name": subscriber_name,
            "billing_contact_name": billing_contact_name,
            "billing_contact_email": billing_contact_email,
        })),
    ))
}

pub fn hosted_mode() -> bool {
    std::env::var("HOSTED_MODE")
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false)
}

fn period_start() -> chrono::NaiveDate {
    let now = chrono::Utc::now().date_naive();
    now.with_day(1).expect("first day exists")
}

pub fn next_event_quota_reset() -> chrono::DateTime<chrono::Utc> {
    next_period_start()
        .and_hms_opt(0, 0, 0)
        .expect("midnight is valid")
        .and_utc()
}

fn next_period_start() -> chrono::NaiveDate {
    let start = period_start();
    if start.month() == 12 {
        chrono::NaiveDate::from_ymd_opt(start.year() + 1, 1, 1).expect("valid next year")
    } else {
        chrono::NaiveDate::from_ymd_opt(start.year(), start.month() + 1, 1)
            .expect("valid next month")
    }
}

fn canonical_ingest_host() -> String {
    std::env::var("INGEST_CANONICAL_HOST")
        .unwrap_or_else(|_| "ingest.trusin.my.id".to_string())
        .trim_end_matches('.')
        .to_ascii_lowercase()
}

fn host_without_port(headers: &HeaderMap) -> Option<String> {
    let raw = headers
        .get("host")?
        .to_str()
        .ok()?
        .trim()
        .to_ascii_lowercase();
    let host = raw
        .strip_prefix('[')
        .and_then(|value| value.split(']').next())
        .unwrap_or_else(|| raw.split(':').next().unwrap_or(""));
    (!host.is_empty()).then_some(host.trim_end_matches('.').to_string())
}

fn public_url_host() -> Option<String> {
    let value = std::env::var("PUBLIC_URL").ok()?;
    reqwest::Url::parse(&value)
        .ok()?
        .host_str()
        .map(|host| host.trim_end_matches('.').to_ascii_lowercase())
}

async fn default_organization_id(db: &sqlx::PgPool) -> Option<Uuid> {
    sqlx::query_scalar("SELECT id FROM organizations WHERE slug = 'default'")
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
}

pub async fn bootstrap_default_organization(db: &sqlx::PgPool, target: &str) {
    if target.trim().is_empty() {
        return;
    }
    let _ = sqlx::query(
        "UPDATE organizations SET default_target_url = $1 WHERE slug = 'default' AND default_target_url = ''",
    )
    .bind(target.trim())
    .execute(db)
    .await;
}

pub async fn default_target_for(
    db: &sqlx::PgPool,
    organization_id: Uuid,
) -> Result<String, StatusCode> {
    sqlx::query_scalar("SELECT default_target_url FROM organizations WHERE id = $1")
        .bind(organization_id)
        .fetch_optional(db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)
}

/// Resolve public ingest tenancy. Canonical hosted URLs must include the
/// organization-specific secret key (`/i/{key}/{source}`); custom domains map
/// directly to a verified organization.
pub async fn resolve_ingest_organization(
    state: &AppState,
    headers: &HeaderMap,
    source_path: &str,
) -> Result<(Uuid, String), StatusCode> {
    let host = host_without_port(headers);
    let is_canonical_host = host.as_deref().is_some_and(|value| {
        value == canonical_ingest_host() || public_url_host().as_deref() == Some(value)
    });
    if is_canonical_host {
        let (ingest_key, source) =
            parse_canonical_ingest_path(source_path).ok_or(StatusCode::NOT_FOUND)?;
        let organization_id =
            sqlx::query_scalar("SELECT id FROM organizations WHERE ingest_key = $1")
                .bind(ingest_key)
                .fetch_optional(&state.db)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .ok_or(StatusCode::NOT_FOUND)?;
        return Ok((organization_id, source.to_string()));
    }

    let is_local_host = host
        .as_deref()
        .is_none_or(|value| value == "localhost" || value == "127.0.0.1");
    if is_local_host {
        return default_organization_id(&state.db)
            .await
            .map(|organization_id| (organization_id, source_path.to_string()))
            .ok_or(StatusCode::SERVICE_UNAVAILABLE);
    }

    let organization_id = sqlx::query_scalar(
        "SELECT organization_id FROM organization_domains WHERE hostname = $1 AND status = 'active'",
    )
    .bind(host.unwrap_or_default())
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;
    Ok((organization_id, source_path.to_string()))
}

fn parse_canonical_ingest_path(path: &str) -> Option<(&str, &str)> {
    let mut segments = path.trim_matches('/').split('/');
    if segments.next() != Some("i") {
        return None;
    }
    let ingest_key = segments.next()?;
    let source = segments.next()?;
    (!ingest_key.is_empty() && !source.is_empty()).then_some((ingest_key, source))
}

#[cfg(test)]
mod tests {
    use super::parse_canonical_ingest_path;

    #[test]
    fn parses_tenant_scoped_ingest_path() {
        assert_eq!(
            parse_canonical_ingest_path("/i/secret-key/resend"),
            Some(("secret-key", "resend"))
        );
    }

    #[test]
    fn rejects_unscoped_or_incomplete_ingest_paths() {
        assert_eq!(parse_canonical_ingest_path("/resend"), None);
        assert_eq!(parse_canonical_ingest_path("/i/secret-key"), None);
    }
}

pub async fn current_organization(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<CurrentUser>,
) -> Result<Json<Value>, StatusCode> {
    expire_trial_if_needed(&state.db, cu.organization_id).await?;
    let period = period_start();
    let summary = sqlx::query_as::<_, OrganizationSummary>(
        r#"SELECT o.id, o.name, o.slug, o.plan_code, o.subscription_status,
                  o.billing_period_start, o.billing_period_end, o.default_target_url, o.ingest_key,
                  COALESCE(u.events_accepted, 0)::BIGINT AS events_accepted,
                  (SELECT COUNT(*) FROM organization_domains d WHERE d.organization_id = o.id AND d.status != 'failed')::BIGINT AS domains,
                  (SELECT COUNT(*) FROM forward_rules r WHERE r.organization_id = o.id AND r.name <> 'Default')::BIGINT AS providers,
                  (SELECT COUNT(*) FROM api_tokens k WHERE k.organization_id = o.id AND k.revoked_at IS NULL)::BIGINT AS api_keys,
                  (SELECT COUNT(*) FROM users m WHERE m.organization_id = o.id)::BIGINT AS users
           FROM organizations o
           LEFT JOIN organization_usage u ON u.organization_id = o.id AND u.period_start = $2
           WHERE o.id = $1"#,
    )
    .bind(cu.organization_id)
    .bind(period)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;
    let paid = summary.plan_code != "free"
        && (summary.subscription_status == "active"
            || (summary.subscription_status == "trialing"
                && summary.billing_period_end > Utc::now()));
    let organization = Organization {
        id: summary.id,
        name: summary.name,
        slug: summary.slug,
        plan_code: summary.plan_code,
        subscription_status: summary.subscription_status,
        billing_period_start: summary.billing_period_start,
        billing_period_end: summary.billing_period_end,
        default_target_url: summary.default_target_url,
    };
    Ok(Json(json!({
        "organization": organization,
        "hosted": hosted_mode(),
        "ingest_canonical_host": canonical_ingest_host(),
        "ingest_url": format!("{}/i/{}", public_ingest_url(), summary.ingest_key),
        "usage": {
            "period_start": period,
            "period_end": next_period_start(),
            "events_accepted": summary.events_accepted,
            "domains": summary.domains,
            "providers": summary.providers,
            "api_keys": summary.api_keys,
            "users": summary.users,
        },
        "limits": {
            "events": if hosted_mode() && !paid { Some(FREE_EVENT_LIMIT) } else { None },
            "domains": if hosted_mode() && !paid { Some(FREE_DOMAIN_LIMIT) } else { None },
            "providers": if hosted_mode() && !paid { Some(FREE_PROVIDER_LIMIT) } else { None },
            "api_keys": if hosted_mode() && !paid { Some(FREE_API_KEY_LIMIT) } else { None },
            "users": if hosted_mode() && !paid { Some(FREE_USER_LIMIT) } else { None },
            "retention_days": if hosted_mode() && !paid { Some(FREE_RETENTION_DAYS) } else { None },
        }
    })))
}

fn public_ingest_url() -> String {
    std::env::var("PUBLIC_URL")
        .unwrap_or_else(|_| format!("https://{}", canonical_ingest_host()))
        .trim_end_matches('/')
        .to_string()
}

async fn resource_count(
    db: &sqlx::PgPool,
    organization_id: Uuid,
    resource: &str,
) -> Result<i64, sqlx::Error> {
    let sql = match resource {
        "domains" => "SELECT COUNT(*) FROM organization_domains WHERE organization_id = $1 AND status != 'failed'",
        "providers" => "SELECT COUNT(*) FROM forward_rules WHERE organization_id = $1 AND name <> 'Default'",
        "api_keys" => "SELECT COUNT(*) FROM api_tokens WHERE organization_id = $1 AND revoked_at IS NULL",
        "users" => "SELECT COUNT(*) FROM users WHERE organization_id = $1",
        _ => return Ok(0),
    };
    sqlx::query_scalar(sql)
        .bind(organization_id)
        .fetch_one(db)
        .await
}

pub async fn ensure_resource_quota(
    state: &AppState,
    organization_id: Uuid,
    resource: &str,
) -> Result<(), StatusCode> {
    if !hosted_mode() {
        return Ok(());
    }
    if organization_has_paid_entitlements(&state.db, organization_id).await? {
        return Ok(());
    }
    let limit = match resource {
        "domains" => FREE_DOMAIN_LIMIT,
        "providers" => FREE_PROVIDER_LIMIT,
        "api_keys" => FREE_API_KEY_LIMIT,
        "users" => FREE_USER_LIMIT,
        _ => return Ok(()),
    };
    let count = resource_count(&state.db, organization_id, resource)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if count >= limit {
        Err(StatusCode::TOO_MANY_REQUESTS)
    } else {
        Ok(())
    }
}

pub async fn organization_allows_invites(
    db: &sqlx::PgPool,
    organization_id: Uuid,
) -> Result<bool, StatusCode> {
    organization_has_paid_entitlements(db, organization_id).await
}

pub async fn organization_has_paid_entitlements(
    db: &sqlx::PgPool,
    organization_id: Uuid,
) -> Result<bool, StatusCode> {
    expire_trial_if_needed(db, organization_id).await?;
    let plan: Option<(String, String, chrono::DateTime<Utc>)> = sqlx::query_as(
        "SELECT plan_code, subscription_status, billing_period_end FROM organizations WHERE id = $1",
    )
    .bind(organization_id)
    .fetch_optional(db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let Some((plan_code, status, billing_period_end)) = plan else {
        return Err(StatusCode::NOT_FOUND);
    };
    Ok(plan_code != "free"
        && (status == "active" || (status == "trialing" && billing_period_end > Utc::now())))
}

async fn expire_trial_if_needed(
    db: &sqlx::PgPool,
    organization_id: Uuid,
) -> Result<(), StatusCode> {
    let expired: Option<Uuid> = sqlx::query_scalar(
        r#"UPDATE organizations
           SET plan_code = 'free', subscription_status = 'active'
           WHERE id = $1 AND plan_code = 'pro' AND subscription_status = 'trialing'
             AND billing_period_end <= NOW()
           RETURNING id"#,
    )
    .bind(organization_id)
    .fetch_optional(db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if expired.is_some() {
        let _ = sqlx::query(
            "UPDATE organization_invites SET revoked_at = NOW() WHERE organization_id = $1 AND accepted_at IS NULL AND revoked_at IS NULL",
        )
        .bind(organization_id)
        .execute(db)
        .await;
    }
    Ok(())
}

pub async fn consume_event_quota(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    organization_id: Uuid,
) -> Result<(), StatusCode> {
    if !hosted_mode() {
        return Ok(());
    }
    let plan: Option<(String, String, chrono::DateTime<Utc>)> = sqlx::query_as(
        "SELECT plan_code, subscription_status, billing_period_end FROM organizations WHERE id = $1 FOR UPDATE",
    )
    .bind(organization_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let Some((plan_code, status, billing_period_end)) = plan else {
        return Err(StatusCode::NOT_FOUND);
    };
    if plan_code != "free"
        && (status == "active" || (status == "trialing" && billing_period_end > Utc::now()))
    {
        return Ok(());
    }
    if plan_code == "pro" && status == "trialing" {
        sqlx::query("UPDATE organizations SET plan_code = 'free', subscription_status = 'active' WHERE id = $1")
            .bind(organization_id)
            .execute(&mut **transaction)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        sqlx::query("UPDATE organization_invites SET revoked_at = NOW() WHERE organization_id = $1 AND accepted_at IS NULL AND revoked_at IS NULL")
            .bind(organization_id)
            .execute(&mut **transaction)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    let period = period_start();
    let used: Option<i64> = sqlx::query_scalar(
        r#"INSERT INTO organization_usage (organization_id, period_start, events_accepted)
           VALUES ($1, $2, 1)
           ON CONFLICT (organization_id, period_start)
           DO UPDATE SET events_accepted = organization_usage.events_accepted + 1
           WHERE organization_usage.events_accepted < $3
           RETURNING events_accepted::BIGINT"#,
    )
    .bind(organization_id)
    .bind(period)
    .bind(FREE_EVENT_LIMIT)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    used.map(|_| ()).ok_or(StatusCode::TOO_MANY_REQUESTS)
}

pub async fn list_domains(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<CurrentUser>,
) -> Result<Json<Vec<OrganizationDomain>>, StatusCode> {
    require_admin(&cu)?;
    require_scope(&cu, "organization:manage")?;
    let rows = sqlx::query_as::<_, OrganizationDomain>(
        r#"SELECT id, hostname, verification_token, status, verified_at, created_at
           FROM organization_domains WHERE organization_id = $1 ORDER BY created_at DESC"#,
    )
    .bind(cu.organization_id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rows))
}

pub async fn create_domain(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<CurrentUser>,
    Json(input): Json<CreateDomain>,
) -> Result<Json<OrganizationDomain>, StatusCode> {
    require_admin(&cu)?;
    require_scope(&cu, "organization:manage")?;
    ensure_resource_quota(&state, cu.organization_id, "domains").await?;
    let hostname = input
        .hostname
        .trim()
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if hostname.is_empty()
        || hostname.len() > 253
        || hostname.contains('/')
        || hostname.contains(':')
        || hostname == canonical_ingest_host()
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let token = format!("terusin-{}", Uuid::new_v4().simple());
    let domain = sqlx::query_as::<_, OrganizationDomain>(
        r#"INSERT INTO organization_domains (id, organization_id, hostname, verification_token)
           VALUES ($1, $2, $3, $4)
           RETURNING id, hostname, verification_token, status, verified_at, created_at"#,
    )
    .bind(Uuid::new_v4())
    .bind(cu.organization_id)
    .bind(&hostname)
    .bind(&token)
    .fetch_one(&state.db)
    .await
    .map_err(|_| StatusCode::CONFLICT)?;
    crate::audit::record(
        &state,
        Some(&cu),
        "domain.created",
        "domain",
        Some(domain.id.to_string()),
        json!({ "hostname": domain.hostname }),
    )
    .await;
    Ok(Json(domain))
}

pub async fn verify_domain(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<CurrentUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<OrganizationDomain>, StatusCode> {
    require_admin(&cu)?;
    require_scope(&cu, "organization:manage")?;
    let domain = sqlx::query_as::<_, OrganizationDomain>(
        r#"SELECT id, hostname, verification_token, status, verified_at, created_at
           FROM organization_domains WHERE id = $1 AND organization_id = $2"#,
    )
    .bind(id)
    .bind(cu.organization_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    let resolver = TokioAsyncResolver::tokio_from_system_conf()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let expected_cname = canonical_ingest_host();
    let cname_ok = resolver
        .lookup(domain.hostname.clone(), RecordType::CNAME)
        .await
        .map(|records| {
            records.iter().any(|record| {
                record
                    .to_string()
                    .trim_end_matches('.')
                    .eq_ignore_ascii_case(&expected_cname)
            })
        })
        .unwrap_or(false);
    let txt_name = format!("_terusin-verification.{}", domain.hostname);
    let txt_ok = resolver
        .txt_lookup(txt_name)
        .await
        .map(|records| {
            records.iter().any(|record| {
                record.txt_data().iter().any(|part| {
                    std::str::from_utf8(part)
                        .map(|value| value == domain.verification_token)
                        .unwrap_or(false)
                })
            })
        })
        .unwrap_or(false);
    let status = if cname_ok && txt_ok {
        "active"
    } else {
        "failed"
    };
    let updated = sqlx::query_as::<_, OrganizationDomain>(
        r#"UPDATE organization_domains
           SET status = $3, verified_at = CASE WHEN $3 = 'active' THEN NOW() ELSE NULL END
           WHERE id = $1 AND organization_id = $2
           RETURNING id, hostname, verification_token, status, verified_at, created_at"#,
    )
    .bind(id)
    .bind(cu.organization_id)
    .bind(status)
    .fetch_one(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    crate::audit::record(
        &state,
        Some(&cu),
        "domain.verified",
        "domain",
        Some(updated.id.to_string()),
        json!({ "hostname": updated.hostname, "status": updated.status }),
    )
    .await;
    Ok(Json(updated))
}

pub async fn delete_domain(
    State(state): State<Arc<AppState>>,
    axum::Extension(cu): axum::Extension<CurrentUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    require_admin(&cu)?;
    require_scope(&cu, "organization:manage")?;
    let result =
        sqlx::query("DELETE FROM organization_domains WHERE id = $1 AND organization_id = $2")
            .bind(id)
            .bind(cu.organization_id)
            .execute(&state.db)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    crate::audit::record(
        &state,
        Some(&cu),
        "domain.deleted",
        "domain",
        Some(id.to_string()),
        json!({}),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn retention_worker(db: sqlx::PgPool) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(60 * 60 * 6));
    loop {
        interval.tick().await;
        if !hosted_mode() {
            continue;
        }
        let _ = sqlx::query(
            r#"DELETE FROM webhook_events e
               USING organizations o
               WHERE e.organization_id = o.id
                 AND o.plan_code = 'free'
                 AND e.created_at < NOW() - INTERVAL '7 days'"#,
        )
        .execute(&db)
        .await;
    }
}
