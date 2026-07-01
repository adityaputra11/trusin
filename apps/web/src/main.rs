use axum::{
    body::Body,
    extract::{Path, Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
    Json, Router,
};
use base64::Engine;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::cors::CorsLayer;

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
struct WebhookEvent {
    id: uuid::Uuid,
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

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
struct User {
    id: uuid::Uuid,
    username: String,
    password_hash: String,
    role: String,
}

struct AppState {
    db: sqlx::PgPool,
    ngrok_url: Arc<Mutex<Option<String>>>,
    backend_url: String,
    backend_auth: String,
}

#[derive(Clone)]
struct AuthUser(String);

fn unauth() -> Response {
    let mut res = (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    res.headers_mut().insert("WWW-Authenticate", "Basic realm=\"Terusin\"".parse().unwrap());
    res
}

fn backend_client(state: &AppState) -> reqwest::Client {
    let mut h = reqwest::header::HeaderMap::new();
    h.insert(reqwest::header::AUTHORIZATION, format!("Basic {}", state.backend_auth).parse().unwrap());
    reqwest::Client::builder().default_headers(h).build().unwrap()
}

async fn auth_middleware(State(state): State<Arc<AppState>>, req: Request<Body>, next: Next) -> Result<Response, Response> {
    let h = req.headers().get("Authorization").and_then(|v| v.to_str().ok()).unwrap_or("");
    let creds = h.strip_prefix("Basic ").and_then(|e| {
        base64::engine::general_purpose::STANDARD.decode(e).ok()
            .and_then(|b| String::from_utf8(b).ok())
            .and_then(|s| s.split_once(':').map(|(u, p)| (u.to_string(), p.to_string())))
    });
    match creds {
        Some((user, pass)) => {
            let u = sqlx::query_as::<_, User>("SELECT * FROM users WHERE username = $1").bind(&user).fetch_optional(&state.db).await.map_err(|_| unauth())?;
            match u { Some(u) if bcrypt::verify(&pass, &u.password_hash).unwrap_or(false) => { let mut r = req; r.extensions_mut().insert(AuthUser(user)); Ok(next.run(r).await) } _ => Err(unauth()) }
        }
        None => Err(unauth()),
    }
}

async fn seed_default_user(db: &sqlx::PgPool) {
    let (u, p) = (std::env::var("AUTH_USERNAME"), std::env::var("AUTH_PASSWORD"));
    if let (Ok(username), Ok(password)) = (u, p) {
        let exists = sqlx::query_as::<_, User>("SELECT * FROM users WHERE username = $1").bind(&username).fetch_optional(db).await.ok().flatten().is_some();
        if !exists {
            let hash = tokio::task::spawn_blocking(move || bcrypt::hash(&password, 10)).await.expect("join").expect("bcrypt");
            sqlx::query("INSERT INTO users (id, username, password_hash, role) VALUES ($1, $2, $3, 'admin')").bind(uuid::Uuid::new_v4()).bind(&username).bind(&hash).execute(db).await.ok();
        }
    }
}

async fn get_ngrok_url() -> Option<String> {
    let d: serde_json::Value = reqwest::get("http://127.0.0.1:4040/api/tunnels").await.ok()?.json().await.ok()?;
    for t in d["tunnels"].as_array()? { if t["proto"].as_str() == Some("https") { return t["public_url"].as_str().map(|s| s.to_string()); } }
    d["tunnels"][0].get("public_url")?.as_str().map(|s| s.to_string())
}

// ── Layout ────────────────────────────────────────────────────────────────

fn layout(title: &str, user: Option<&str>, content: &str) -> String {
    let u = user.unwrap_or("unknown");
    let auth = if user.is_some() { "" } else { "hidden" };
    format!(r#"<!DOCTYPE html><html lang="en"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>{title} — Terusin</title><script src="https://cdn.tailwindcss.com"></script>
<style>body{{font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,sans-serif}}pre{{font-family:"SF Mono",Menlo,monospace}}</style>
</head><body class="bg-gray-50 text-gray-900"><nav class="bg-white border-b sticky top-0 z-10"><div class="max-w-7xl mx-auto px-4 h-12 flex items-center justify-between">
<div class="flex items-center gap-6"><a href="/" class="font-bold text-base">Terusin</a>
<a href="/providers" class="text-sm text-gray-500 hover:text-gray-900 {auth}">Providers</a>
<a href="/hooks" class="text-sm text-gray-500 hover:text-gray-900 {auth}">Hooks</a>
<a href="/send" class="text-sm text-gray-500 hover:text-gray-900 {auth}">Send</a></div>
<div class="flex items-center gap-3 text-sm"><span class="text-gray-400">{u}</span><a href="/logout" class="text-red-500 hover:text-red-700">Logout</a></div>
</div></nav><main class="max-w-7xl mx-auto px-4 py-6">{content}</main></body></html>"#, title=title, auth=auth, u=u, content=content)
}

fn badge(s: &str) -> &str {
    match s {
        "delivered" => "bg-emerald-100 text-emerald-700",
        "failed" => "bg-red-100 text-red-700",
        "retrying" => "bg-amber-100 text-amber-700",
        "queued" => "bg-blue-100 text-blue-700",
        _ => "bg-gray-100 text-gray-600",
    }
}

// ── Dashboard ─────────────────────────────────────────────────────────────

async fn dashboard(State(state): State<Arc<AppState>>, req: Request<Body>) -> Result<Html<String>, StatusCode> {
    if let Some(url) = get_ngrok_url().await { *state.ngrok_url.lock().await = Some(url); }
    let user = req.extensions().get::<AuthUser>().map(|u| u.0.as_str());
    let ngrok = state.ngrok_url.lock().await.clone();

    let params = req.uri().query().unwrap_or("");
    let results: serde_json::Value = match backend_client(&state)
        .get(format!("{}/events?{}", state.backend_url, params))
        .send().await
    {
        Ok(r) => r.json().await.unwrap_or_default(),
        Err(_) => serde_json::json!({}),
    };

    let events = results["events"].as_array().map(|a| a.iter().map(|v| {
        serde_json::from_value::<WebhookEvent>(v.clone()).ok()
    }).collect::<Vec<_>>()).unwrap_or_default();
    let total = results["total"].as_i64().unwrap_or(0);
    let page = results["page"].as_i64().unwrap_or(1);
    let pages = results["pages"].as_i64().unwrap_or(1);

    Ok(Html(dashboard_html(&events, ngrok.as_deref(), user, total, page, pages)))
}

fn dashboard_html(ev: &[Option<WebhookEvent>], ngrok: Option<&str>, user: Option<&str>, total: i64, page: i64, pages: i64) -> String {
    let rows: String = ev.iter().filter_map(|e| e.as_ref()).map(|e| format!(
        r#"<tr class="hover:bg-gray-50 cursor-pointer border-b border-gray-100" onclick="window.location='/events/{}'">
<td class="py-2.5 px-3"><span class="font-mono text-xs text-gray-400">{}</span></td>
<td class="py-2.5 px-3"><span class="text-sm">{}</span></td>
<td class="py-2.5 px-3"><span class="inline-block px-2 py-0.5 rounded-full text-xs font-medium {}">{}</span></td>
<td class="py-2.5 px-3 text-sm text-gray-500">{}/{}</td>
<td class="py-2.5 px-3 text-sm text-gray-400">{}</td></tr>"#,
        e.id, &e.id.to_string()[..8], e.source, badge(&e.status), e.status, e.retry_count, e.max_retries, e.created_at.format("%m/%d %H:%M"))).collect();

    let url_box = match ngrok {
        Some(u) => format!(r#"<div class="bg-emerald-50 border border-emerald-200 rounded-lg p-3 mb-5 flex items-center gap-3 text-sm"><span class="text-emerald-700 font-medium shrink-0">Webhook URL</span>
<div class="flex-1 flex items-center gap-1 bg-white border border-emerald-300 rounded px-2 py-1"><input id="u" type="password" readonly value="{u}" class="flex-1 text-xs font-mono text-emerald-900 outline-none bg-transparent"/>
<button onclick="var i=document.getElementById('u');i.type=i.type=='password'?'text':'password'" class="text-gray-400 hover:text-gray-600 shrink-0 text-sm">👁</button></div>
<button onclick="navigator.clipboard.writeText(document.getElementById('u').value)" class="bg-emerald-600 text-white px-3 py-1.5 rounded text-xs hover:bg-emerald-700 shrink-0">Copy</button></div>"#),
        None => r#"<div class="bg-amber-50 border border-amber-200 rounded-lg p-3 mb-5 text-sm text-amber-800">Ngrok gak jalan.</div>"#.to_string(),
    };

    let search_bar = format!(r#"<form class="flex flex-wrap gap-2 mb-4 items-end"><div class="flex-1 min-w-[150px]"><label class="text-xs text-gray-400 block mb-1">Search</label><input name="search" placeholder="source, target, body..." class="border rounded px-2.5 py-1.5 text-sm w-full" value=""/></div>
<div><label class="text-xs text-gray-400 block mb-1">Status</label><select name="status" class="border rounded px-2.5 py-1.5 text-sm"><option value="all">All</option><option value="delivered">Delivered</option><option value="failed">Failed</option><option value="retrying">Retrying</option><option value="queued">Queued</option></select></div>
<div><label class="text-xs text-gray-400 block mb-1">Source</label><input name="source" placeholder="e.g. midtrans" class="border rounded px-2.5 py-1.5 text-sm w-28" value=""/></div>
<input type="hidden" name="page" value="1"/>
<button class="bg-blue-600 text-white px-3 py-1.5 rounded text-sm hover:bg-blue-700">Filter</button></form>"#);

    let mut pagination = String::new();
    if pages > 1 {
        let mut links = String::new();
        let max = pages.min(5);
        for p in 1..=max {
            let active = if p == page { "bg-blue-600 text-white" } else { "bg-gray-100 hover:bg-gray-200" };
            links.push_str(&format!(r#"<a href="?page={p}" class="px-2 py-1 rounded {active}">{p}</a>"#, p=p, active=active));
        }
        if pages > 5 {
            links.push_str(&format!(r#"<span class="px-2 py-1">...</span><a href="?page={ps}" class="px-2 py-1 rounded bg-gray-100 hover:bg-gray-200">{ps}</a>"#, ps=pages));
        }
        pagination = format!(r#"<div class="flex items-center justify-between px-3 py-3 text-sm text-gray-500"><span>{total} events</span><div class="flex gap-1">{links}</div></div>"#, total=total, links=links);
    }

    layout("Dashboard", user, &format!(r#"{url_box}{search_bar}<div class="bg-white rounded-xl border shadow-sm overflow-hidden"><table class="w-full"><thead><tr class="text-left text-xs font-medium text-gray-400 uppercase bg-gray-50"><th class="py-3 px-3">ID</th><th class="py-3 px-3">Source</th><th class="py-3 px-3">Status</th><th class="py-3 px-3">Retry</th><th class="py-3 px-3">Time</th></tr></thead><tbody>{rows}</tbody></table>{pagination}</div>"#))
}

// ── Event Detail ──────────────────────────────────────────────────────────

async fn event_detail(State(state): State<Arc<AppState>>, Path(id): Path<uuid::Uuid>) -> Result<Html<String>, StatusCode> {
    let e = sqlx::query_as::<_, WebhookEvent>("SELECT * FROM webhook_events WHERE id = $1").bind(id).fetch_optional(&state.db).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Html(match e { Some(e) => detail_html(&e), None => not_found() }))
}

fn detail_html(e: &WebhookEvent) -> String {
    let h = serde_json::to_string_pretty(&e.headers).unwrap_or_default();
    let b = serde_json::to_string_pretty(&e.body).unwrap_or_default();
    let resp_h = e.response_headers.as_ref().and_then(|v| serde_json::to_string_pretty(v).ok()).unwrap_or_else(|| "-".to_string());
    let resp_b = e.response_body.as_deref().unwrap_or("-");
    let resp_s = e.response_status.map(|s| s.to_string()).unwrap_or_else(|| "-".to_string());

    layout("Event", None, &format!(
r#"<a href="/" class="text-sm text-blue-600 hover:text-blue-800">&larr; Back</a>
<h2 class="text-lg font-bold mt-2 mb-4">Event <span class="font-mono text-sm font-normal text-gray-400">{id}</span></h2>
<div class="grid grid-cols-1 lg:grid-cols-2 gap-4">
<div class="bg-white rounded-xl border shadow-sm p-4"><h3 class="text-xs font-semibold text-gray-400 uppercase mb-3">Info</h3>
<dl class="space-y-2 text-sm"><dt class="text-gray-400 text-xs">Status</dt><dd><span class="inline-block px-2 py-0.5 rounded-full text-xs font-medium {st}">{status}</span></dd>
<dt class="text-gray-400 text-xs mt-3">Source</dt><dd>{source}</dd>
<dt class="text-gray-400 text-xs mt-3">Target</dt><dd class="font-mono text-xs break-all">{target}</dd>
<dt class="text-gray-400 text-xs mt-3">Response</dt><dd class="font-mono text-sm">{resp_status}</dd>
<dt class="text-gray-400 text-xs mt-3">Retry</dt><dd>{retry}/{max}</dd>
<dt class="text-gray-400 text-xs mt-3">Created</dt><dd>{created}</dd></dl>
<form action="/api/events/{id}/retry" method="post" class="mt-4"><button class="bg-blue-600 text-white px-3 py-1.5 rounded text-sm hover:bg-blue-700">Retry</button></form></div>
<div class="bg-white rounded-xl border shadow-sm p-4"><h3 class="text-xs font-semibold text-gray-400 uppercase mb-3">Request Headers</h3><pre class="bg-gray-50 rounded p-3 text-xs font-mono overflow-x-auto whitespace-pre-wrap">{headers}</pre></div>
<div class="bg-white rounded-xl border shadow-sm p-4"><h3 class="text-xs font-semibold text-gray-400 uppercase mb-3">Response Headers</h3><pre class="bg-gray-50 rounded p-3 text-xs font-mono overflow-x-auto whitespace-pre-wrap">{resp_headers}</pre></div>
<div class="lg:col-span-2 bg-white rounded-xl border shadow-sm p-4"><h3 class="text-xs font-semibold text-gray-400 uppercase mb-3">Request Body</h3><pre class="bg-gray-50 rounded p-3 text-xs font-mono overflow-x-auto whitespace-pre-wrap">{body}</pre></div>
<div class="lg:col-span-2 bg-white rounded-xl border shadow-sm p-4"><h3 class="text-xs font-semibold text-gray-400 uppercase mb-3">Response Body</h3><pre class="bg-gray-50 rounded p-3 text-xs font-mono overflow-x-auto whitespace-pre-wrap">{resp_body}</pre></div>
</div>"#, id=e.id, st=badge(&e.status), status=e.status, source=e.source, target=e.target_url, resp_status=resp_s, retry=e.retry_count, max=e.max_retries, created=e.created_at.format("%Y-%m-%d %H:%M:%S"), headers=h, body=b, resp_headers=resp_h, resp_body=resp_b))
}

async fn retry_event(State(state): State<Arc<AppState>>, Path(id): Path<uuid::Uuid>) -> Result<Html<String>, StatusCode> {
    backend_client(&state).post(format!("{}/events/{id}/retry", state.backend_url)).send().await.ok();
    Ok(Html(format!(r#"<!DOCTYPE html><html><head><meta http-equiv="refresh" content="0;url=/events/{id}"></head><body></body></html>"#)))
}

// ── Providers ─────────────────────────────────────────────────────────────

async fn providers_page(State(state): State<Arc<AppState>>, req: Request<Body>) -> Result<Html<String>, StatusCode> {
    let ngrok = state.ngrok_url.lock().await.clone();
    let url = ngrok.as_deref().unwrap_or("https://your-host");
    let rules: Vec<serde_json::Value> = match backend_client(&state).get(format!("{}/rules", state.backend_url)).send().await { Ok(r) => r.json().await.unwrap_or_default(), Err(_) => vec![] };

    let mut rows = String::new();
    for r in &rules {
        if r["name"] == "Default" { continue; }
        let n = r["name"].as_str().unwrap_or("");
        let t = r["target_url"].as_str().unwrap_or("");
        let i = r["id"].as_str().unwrap_or("");
        let wh = format!("{}/{n}/webhook", url);
        rows.push_str(&format!(
            r#"<tr class="border-b border-gray-100 hover:bg-gray-50"><td class="py-2.5 px-3 text-sm font-medium">{name}</td><td class="py-2.5 px-3 text-xs font-mono text-blue-600 break-all">{webhook}</td><td class="py-2.5 px-3 text-xs text-gray-500 break-all">{target}</td>
<td class="py-2.5 px-3 whitespace-nowrap"><button onclick="e('{id}','{name}','{target}')" class="text-blue-600 hover:text-blue-800 text-sm mr-2">Edit</button>
<form action="/api/providers/{id}/delete" method="post" class="inline"><button class="text-red-500 hover:text-red-700 text-sm">Delete</button></form></td></tr>"#,
            name=n, webhook=wh, target=t, id=i));
    }

    let body = format!(r#"
<div class="bg-white rounded-xl border shadow-sm p-4 mb-5" id="add"><h3 class="text-sm font-semibold mb-3">Add Provider</h3>
<form action="/api/providers" method="post" class="flex flex-wrap gap-2"><input name="name" placeholder="Name" required class="border rounded px-3 py-1.5 text-sm w-40"/>
<input name="target_url" placeholder="Target URL" class="border rounded px-3 py-1.5 text-sm font-mono flex-1 min-w-[200px]"/>
<button class="bg-blue-600 text-white px-4 py-1.5 rounded text-sm hover:bg-blue-700">Add</button></form></div>
<div class="bg-white rounded-xl border shadow-sm hidden p-4 mb-5" id="edit"><h3 class="text-sm font-semibold mb-3">Edit Provider</h3>
<form action="/api/providers/edit" method="post" class="flex flex-wrap gap-2"><input id="ei" name="id" type="hidden"/>
<input id="en" name="name" required class="border rounded px-3 py-1.5 text-sm w-40"/>
<input id="et" name="target_url" class="border rounded px-3 py-1.5 text-sm font-mono flex-1 min-w-[200px]"/>
<button class="bg-green-600 text-white px-4 py-1.5 rounded text-sm hover:bg-green-700">Save</button>
<button type="button" onclick="c()" class="bg-gray-200 text-gray-600 px-4 py-1.5 rounded text-sm hover:bg-gray-300">Cancel</button></form></div>
<div class="bg-white rounded-xl border shadow-sm overflow-hidden"><table class="w-full"><thead><tr class="text-left text-xs font-medium text-gray-400 uppercase bg-gray-50"><th class="py-3 px-3">Provider</th><th class="py-3 px-3">Webhook URL</th><th class="py-3 px-3">Target</th><th class="py-3 px-3">Action</th></tr></thead><tbody>{rows}</tbody></table></div>
<script>function e(id,n,t){{document.getElementById('ei').value=id;document.getElementById('en').value=n;document.getElementById('et').value=t;document.getElementById('edit').classList.remove('hidden');document.getElementById('add').classList.add('hidden')}}function c(){{document.getElementById('edit').classList.add('hidden');document.getElementById('add').classList.remove('hidden')}}</script>"#);
    let user = req.extensions().get::<AuthUser>().map(|u| u.0.as_str());
    Ok(Html(layout("Providers", user, &body)))
}

async fn create_provider(State(state): State<Arc<AppState>>, axum::extract::Form(form): axum::extract::Form<std::collections::HashMap<String, String>>) -> Result<Html<String>, StatusCode> {
    let name = form.get("name").map(|s| s.trim()).filter(|s| !s.is_empty()).unwrap_or("");
    let target = form.get("target_url").map(|s| s.trim()).unwrap_or("");
    let existing: Vec<serde_json::Value> = match backend_client(&state).get(format!("{}/rules", state.backend_url)).send().await { Ok(r) => r.json().await.unwrap_or_default(), Err(_) => vec![] };
    if existing.iter().any(|r| r["name"].as_str() == Some(name)) {
        return Ok(Html(format!(r#"<!DOCTYPE html><html><head><meta http-equiv="refresh" content="2;url=/providers"></head><body class="flex items-center justify-center min-h-screen bg-gray-50"><div class="text-center"><p class="text-red-600 font-medium">"{name}" already exists</p><a href="/providers" class="text-blue-600 text-sm mt-2 inline-block">Back</a></div></body></html>"#)));
    }
    backend_client(&state).post(format!("{}/rules", state.backend_url)).json(&serde_json::json!({"name": name, "source_pattern": name, "target_url": target})).send().await.ok();
    Ok(Html(r#"<!DOCTYPE html><html><head><meta http-equiv="refresh" content="0;url=/providers"></head><body></body></html>"#.to_string()))
}

async fn edit_provider(State(state): State<Arc<AppState>>, axum::extract::Form(form): axum::extract::Form<std::collections::HashMap<String, String>>) -> Result<Html<String>, StatusCode> {
    let (id, name, target) = (form.get("id").map(|s| s.as_str()).unwrap_or(""), form.get("name").map(|s| s.trim()).unwrap_or(""), form.get("target_url").map(|s| s.trim()).unwrap_or(""));
    backend_client(&state).post(format!("{}/rules", state.backend_url)).json(&serde_json::json!({"name": name, "source_pattern": name, "target_url": target})).send().await.ok();
    backend_client(&state).delete(format!("{}/rules/{id}", state.backend_url)).send().await.ok();
    Ok(Html(r#"<!DOCTYPE html><html><head><meta http-equiv="refresh" content="0;url=/providers"></head><body></body></html>"#.to_string()))
}

async fn delete_provider(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Html<String>, StatusCode> {
    backend_client(&state).delete(format!("{}/rules/{id}", state.backend_url)).send().await.ok();
    Ok(Html(r#"<!DOCTYPE html><html><head><meta http-equiv="refresh" content="0;url=/providers"></head><body></body></html>"#.to_string()))
}

// ── Hooks ─────────────────────────────────────────────────────────────────

async fn hooks_page(State(state): State<Arc<AppState>>, req: Request<Body>) -> Result<Html<String>, StatusCode> {
    let rules: Vec<serde_json::Value> = match backend_client(&state).get(format!("{}/rules", state.backend_url)).send().await { Ok(r) => r.json().await.unwrap_or_default(), Err(_) => vec![] };
    let mut rows = String::new();
    for r in &rules {
        let n = r["name"].as_str().unwrap_or("");
        let p = r["source_pattern"].as_str().unwrap_or("");
        let t = r["target_url"].as_str().unwrap_or("");
        let i = r["id"].as_str().unwrap_or("");
        rows.push_str(&format!(
            r#"<tr class="border-b border-gray-100 hover:bg-gray-50"><td class="py-2.5 px-3 text-sm">{nm}</td><td class="py-2.5 px-3 text-sm font-mono">{src}</td><td class="py-2.5 px-3 text-xs text-gray-500 break-all">{tg}</td>
<td class="py-2.5 px-3"><form action="/api/rules/{id}/delete" method="post"><button class="text-red-500 hover:text-red-700 text-sm">Delete</button></form></td></tr>"#,
            nm=n, src=p, tg=t, id=i));
    }

    let body = format!(r#"
<div class="bg-white rounded-xl border shadow-sm p-4 mb-5"><h3 class="text-sm font-semibold mb-3">Add Hook</h3>
<form action="/api/rules" method="post" class="flex flex-wrap gap-2"><input name="name" placeholder="Name" required class="border rounded px-3 py-1.5 text-sm w-36"/>
<input name="source_pattern" placeholder="Source (*)" class="border rounded px-3 py-1.5 text-sm w-28" value="*"/>
<input name="target_url" placeholder="Target URL" required class="border rounded px-3 py-1.5 text-sm font-mono flex-1 min-w-[200px]"/>
<button class="bg-blue-600 text-white px-4 py-1.5 rounded text-sm hover:bg-blue-700">Add</button></form></div>
<div class="bg-white rounded-xl border shadow-sm overflow-hidden"><table class="w-full"><thead><tr class="text-left text-xs font-medium text-gray-400 uppercase bg-gray-50"><th class="py-3 px-3">Name</th><th class="py-3 px-3">Source</th><th class="py-3 px-3">Target</th><th class="py-3 px-3">Action</th></tr></thead><tbody>{rows}</tbody></table></div>"#);
    let user = req.extensions().get::<AuthUser>().map(|u| u.0.as_str());
    Ok(Html(layout("Hooks", user, &body)))
}

async fn create_hook(State(state): State<Arc<AppState>>, axum::extract::Form(form): axum::extract::Form<std::collections::HashMap<String, String>>) -> Result<Html<String>, StatusCode> {
    let (name, pattern, target) = (form.get("name").map(|s| s.as_str()).unwrap_or(""), form.get("source_pattern").map(|s| s.as_str()).unwrap_or("*"), form.get("target_url").map(|s| s.as_str()).unwrap_or(""));
    backend_client(&state).post(format!("{}/rules", state.backend_url)).json(&serde_json::json!({"name": name, "source_pattern": pattern, "target_url": target})).send().await.ok();
    Ok(Html(r#"<!DOCTYPE html><html><head><meta http-equiv="refresh" content="0;url=/hooks"></head><body></body></html>"#.to_string()))
}

async fn delete_hook(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Html<String>, StatusCode> {
    backend_client(&state).delete(format!("{}/rules/{id}", state.backend_url)).send().await.ok();
    Ok(Html(r#"<!DOCTYPE html><html><head><meta http-equiv="refresh" content="0;url=/hooks"></head><body></body></html>"#.to_string()))
}

// ── Misc ──────────────────────────────────────────────────────────────────

async fn send_page(State(state): State<Arc<AppState>>, req: Request<Body>) -> Result<Html<String>, StatusCode> {
    let user = req.extensions().get::<AuthUser>().map(|u| u.0.as_str());
    Ok(Html(layout("Send Webhook", user, r#"<div class="bg-white rounded-xl border shadow-sm p-4 mb-5"><h3 class="text-sm font-semibold mb-3">Send Custom Webhook</h3>
<form action="/api/send" method="post" class="space-y-3"><div><label class="text-xs text-gray-400 block mb-1">Source</label>
<input name="source" placeholder="e.g. custom" class="border rounded px-3 py-1.5 text-sm w-full"/></div>
<div><label class="text-xs text-gray-400 block mb-1">Target URL (optional, uses default if empty)</label>
<input name="target_url" placeholder="http://localhost:3000/webhook" class="border rounded px-3 py-1.5 text-sm font-mono w-full"/></div>
<div><label class="text-xs text-gray-400 block mb-1">Body (JSON)</label>
<textarea name="body" rows="8" class="border rounded px-3 py-1.5 text-sm font-mono w-full" placeholder='{"event":"push","data":{}}'></textarea></div>
<button class="bg-blue-600 text-white px-4 py-1.5 rounded text-sm hover:bg-blue-700">Send</button></form></div>"#)))
}

async fn send_webhook(State(state): State<Arc<AppState>>, axum::extract::Form(form): axum::extract::Form<std::collections::HashMap<String, String>>) -> Response {
    let source = form.get("source").map(|s| s.trim()).unwrap_or("web");
    let target = form.get("target_url").map(|s| s.trim()).unwrap_or("");
    let body: serde_json::Value = form.get("body").and_then(|s| serde_json::from_str(s).ok()).unwrap_or(serde_json::json!({}));

    let mut req = backend_client(&state).post(&state.backend_url).json(&body);
    req = req.header("X-Webhook-Source", source);
    if !target.is_empty() { req = req.header("X-Target-Url", target); }
    let resp = req.send().await.ok();

    match resp { Some(_) => Redirect::to("/send").into_response(), None => Html(r#"<!DOCTYPE html><html><head><meta http-equiv="refresh" content="2;url=/send"></head><body class="flex items-center justify-center min-h-screen bg-gray-50"><div class="text-center"><p class="text-red-600 font-medium">Failed</p><a href="/send" class="text-blue-600 text-sm mt-2 inline-block">Back</a></div></body></html>"#.to_string()).into_response() }
}

async fn logout() -> Response {
    let mut r = (StatusCode::UNAUTHORIZED, "Logged out").into_response();
    r.headers_mut().insert("WWW-Authenticate", "Basic realm=\"Terusin\"".parse().unwrap());
    r
}

fn not_found() -> String {
    layout("Not Found", None, r#"<div class="text-center py-12"><h2 class="text-xl font-bold text-gray-400">Not found</h2><a href="/" class="text-blue-600 text-sm mt-2 inline-block">Back</a></div>"#)
}

async fn api_events(State(state): State<Arc<AppState>>) -> Result<Json<Vec<WebhookEvent>>, StatusCode> {
    let events = sqlx::query_as::<_, WebhookEvent>("SELECT * FROM webhook_events ORDER BY created_at DESC LIMIT 50").fetch_all(&state.db).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(events))
}

// ── Main ──────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let (db_url, ngrok_bin, backend_port, backend_url) = (
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://localhost:5432/terusin".to_string()),
        std::env::var("NGROK_PATH").unwrap_or_else(|_| "ngrok".to_string()),
        std::env::var("BACKEND_PORT").unwrap_or_else(|_| "3001".to_string()),
        std::env::var("BACKEND_URL").unwrap_or_else(|_| format!("http://localhost:{}", std::env::var("BACKEND_PORT").unwrap_or_else(|_| "3001".to_string()))),
    );
    let port = std::env::var("PORT").unwrap_or_else(|_| "3002".to_string()).parse::<u16>().unwrap_or(3002);

    tokio::process::Command::new(&ngrok_bin).args(["http", &backend_port, "--log", "stdout"]).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().ok();

    let db = PgPoolOptions::new().max_connections(5).connect(&db_url).await.expect("db");
    seed_default_user(&db).await;

    let (u, p) = (std::env::var("AUTH_USERNAME").unwrap_or_else(|_| "admin".to_string()), std::env::var("AUTH_PASSWORD").unwrap_or_else(|_| "terusin123".to_string()));
    let state = Arc::new(AppState { db, ngrok_url: Arc::new(Mutex::new(None)), backend_url, backend_auth: base64::engine::general_purpose::STANDARD.encode(format!("{u}:{p}")) });

    let app = Router::new()
        .route("/", get(dashboard))
        .route("/events/{id}", get(event_detail))
        .route("/providers", get(providers_page))
        .route("/hooks", get(hooks_page))
        .route("/send", get(send_page))
        .route("/api/send", axum::routing::post(send_webhook))
        .route("/logout", get(logout))
        .route("/api/events", get(api_events))
        .route("/api/events/{id}/retry", axum::routing::post(retry_event))
        .route("/api/providers", axum::routing::post(create_provider))
        .route("/api/providers/edit", axum::routing::post(edit_provider))
        .route("/api/providers/{id}/delete", axum::routing::post(delete_provider))
        .route("/api/rules", axum::routing::post(create_hook))
        .route("/api/rules/{id}/delete", axum::routing::post(delete_hook))
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = format!("0.0.0.0:{port}");
    tracing::info!("web on {addr}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
