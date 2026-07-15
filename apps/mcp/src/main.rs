use base64::Engine;
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

const BACKEND_URL: &str = "http://127.0.0.1:3011";

fn auth_header() -> String {
    // Prefer a paired API token (Bearer) — generate one in the dashboard's
    // Settings → Devices & Tokens page, then export TERUSIN_TOKEN.
    if let Ok(t) = std::env::var("TERUSIN_TOKEN") {
        if !t.trim().is_empty() {
            return format!("Bearer {}", t.trim());
        }
    }
    // Legacy fallback: shared username/password (Basic auth).
    let user = std::env::var("TERUSIN_USER").unwrap_or_else(|_| "admin".to_string());
    let pass =
        std::env::var("TERUSIN_PASS").unwrap_or_else(|_| "change-me-in-production".to_string());
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{user}:{pass}"))
    )
}

fn tool_list() -> Vec<Value> {
    vec![
        json!({"name": "list_events", "description": "List recent webhook events", "inputSchema": {"type": "object", "properties": {"limit": {"type": "integer", "default": 20}}}}),
        json!({"name": "retry_event", "description": "Retry a failed webhook event", "inputSchema": {"type": "object", "properties": {"id": {"type": "string", "description": "Event UUID"}}, "required": ["id"]}}),
        json!({"name": "send_webhook", "description": "Send a webhook through the relay", "inputSchema": {"type": "object", "properties": {
            "source": {"type": "string"},
            "target_url": {"type": "string"},
            "body": {"type": "object"}
        }, "required": ["target_url", "body"]}}),
        json!({"name": "health", "description": "Check backend health", "inputSchema": {"type": "object", "properties": {}}}),
    ]
}

fn handle_call(name: &str, args: &Value) -> Value {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::AUTHORIZATION,
        auth_header().parse().unwrap(),
    );
    let client = reqwest::blocking::Client::builder()
        .default_headers(headers)
        .build()
        .unwrap();

    match name {
        "list_events" => {
            let limit = args.get("limit").and_then(|v| v.as_i64()).unwrap_or(20);
            let resp = client.get(format!("{BACKEND_URL}/events")).send();
            match resp {
                Ok(r) => {
                    let data: Value = r.json().unwrap_or_default();
                    let events = data["events"].as_array().cloned().unwrap_or_default();
                    let events: Vec<Value> = events.into_iter().take(limit as usize).collect();
                    json!({"events": events})
                }
                Err(e) => json!({"error": format!("{e}")}),
            }
        }
        "retry_event" => {
            let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let resp = client
                .post(format!("{BACKEND_URL}/events/{id}/retry"))
                .send();
            json!({"status": resp.map(|r| r.status().as_u16()).unwrap_or(500), "id": id})
        }
        "send_webhook" => {
            let target_url = args
                .get("target_url")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let empty = json!({});
            let body = args.get("body").unwrap_or(&empty);
            let source = args.get("source").and_then(|v| v.as_str()).unwrap_or("mcp");
            let resp = client
                .post(BACKEND_URL)
                .header("X-Webhook-Source", source)
                .header("X-Target-Url", target_url)
                .json(body)
                .send();
            match resp {
                Ok(r) => r.json::<Value>().unwrap_or(json!({"status": "sent"})),
                Err(e) => json!({"error": format!("{e}")}),
            }
        }
        "health" => match client.get(format!("{BACKEND_URL}/health")).send() {
            Ok(r) => r.json::<Value>().unwrap_or_default(),
            Err(e) => json!({"error": format!("{e}")}),
        },
        _ => json!({"error": format!("unknown tool: {name}")}),
    }
}

fn main() {
    let stdin = io::stdin();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) if l.trim().is_empty() => continue,
            Ok(l) => l,
            Err(_) => break,
        };

        let req: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let id = req.get("id").cloned();
        let method = req.get("method").and_then(|v| v.as_str()).unwrap_or("");

        let result = match method {
            "initialize" => json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {"tools": {}, "resources": {}},
                "serverInfo": {"name": "terusin", "version": env!("CARGO_PKG_VERSION")}
            }),
            "tools/list" => json!({"tools": tool_list()}),
            "tools/call" => {
                let name = req
                    .get("params")
                    .and_then(|p| p.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let default_args = json!({});
                let args = req
                    .get("params")
                    .and_then(|p| p.get("arguments"))
                    .unwrap_or(&default_args);
                handle_call(name, args)
            }
            _ => json!({"error": {"code": -32603, "message": format!("unknown method: {method}")}}),
        };

        let response = json!({"jsonrpc": "2.0", "id": id, "result": result});
        let out = serde_json::to_string(&response).unwrap();
        println!("{out}");
    }
}
