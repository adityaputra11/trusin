use serde_json::{json, Value};
use std::io::{self, BufRead};

const DEFAULT_BACKEND_URL: &str = "http://127.0.0.1:3011";
const PROTOCOL_VERSION: &str = "2024-11-05";

struct Config {
    backend_url: String,
    token: String,
}

impl Config {
    fn from_env() -> Result<Self, String> {
        let backend_url = std::env::var("TERUSIN_URL")
            .unwrap_or_else(|_| DEFAULT_BACKEND_URL.to_string())
            .trim_end_matches('/')
            .to_string();
        let token = std::env::var("TERUSIN_TOKEN").map_err(|_| {
            "TERUSIN_TOKEN is required. Create an API token in Settings > Developer.".to_string()
        })?;
        if token.trim().is_empty() {
            return Err(
                "TERUSIN_TOKEN is required. Create an API token in Settings > Developer."
                    .to_string(),
            );
        }
        Ok(Self {
            backend_url,
            token: token.trim().to_string(),
        })
    }
}

fn tool_list() -> Vec<Value> {
    vec![
        json!({"name": "get_health", "description": "Check the trusin relay health and readiness.", "inputSchema": {"type": "object", "additionalProperties": false}}),
        json!({"name": "get_metrics", "description": "Get webhook delivery metrics for a time range.", "inputSchema": {"type": "object", "properties": {"range": {"type": "string", "enum": ["24h", "7d", "30d"], "default": "24h", "description": "Metrics time range."}}, "additionalProperties": false}}),
        json!({"name": "list_events", "description": "List webhook events for the current workspace. Use filters to narrow results.", "inputSchema": {"type": "object", "properties": {"search": {"type": "string"}, "status": {"type": "string", "enum": ["queued", "retrying", "delivered", "failed"]}, "source": {"type": "string"}, "from": {"type": "string", "description": "Inclusive ISO-8601 timestamp."}, "to": {"type": "string", "description": "Exclusive ISO-8601 timestamp."}, "page": {"type": "integer", "minimum": 1, "default": 1}, "per_page": {"type": "integer", "minimum": 1, "maximum": 200, "default": 20}}, "additionalProperties": false}}),
        json!({"name": "get_event", "description": "Get one webhook event by UUID.", "inputSchema": {"type": "object", "properties": {"id": {"type": "string", "description": "Webhook event UUID."}}, "required": ["id"], "additionalProperties": false}}),
        json!({"name": "get_delivery_attempts", "description": "Get the delivery timeline for one webhook event.", "inputSchema": {"type": "object", "properties": {"id": {"type": "string", "description": "Webhook event UUID."}}, "required": ["id"], "additionalProperties": false}}),
        json!({"name": "send_webhook", "description": "Queue a JSON webhook through trusin. Requires an API token with webhooks:send scope.", "inputSchema": {"type": "object", "properties": {"source": {"type": "string", "description": "Optional event source label."}, "target_url": {"type": "string", "format": "uri", "description": "Destination URL."}, "body": {"description": "JSON payload to deliver."}}, "required": ["target_url", "body"], "additionalProperties": false}}),
        json!({"name": "retry_event", "description": "Requeue a failed webhook event. Requires an admin API token.", "inputSchema": {"type": "object", "properties": {"id": {"type": "string", "description": "Webhook event UUID."}}, "required": ["id"], "additionalProperties": false}}),
    ]
}

fn resource_list() -> Vec<Value> {
    vec![
        json!({"uri": "trusin://health", "name": "Relay health", "description": "Current relay health and dependency status.", "mimeType": "application/json"}),
        json!({"uri": "trusin://metrics", "name": "Delivery metrics", "description": "Webhook delivery metrics for the last 24 hours.", "mimeType": "application/json"}),
    ]
}

fn resource_templates() -> Vec<Value> {
    vec![
        json!({"uriTemplate": "trusin://events/{id}", "name": "Webhook event", "description": "Inspect one webhook event by UUID.", "mimeType": "application/json"}),
    ]
}

fn result(value: Value) -> Value {
    let text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
    json!({"content": [{"type": "text", "text": text}]})
}

fn tool_error(message: impl Into<String>) -> Value {
    json!({"content": [{"type": "text", "text": message.into()}], "isError": true})
}

fn rpc_error(code: i64, message: impl Into<String>) -> Value {
    json!({"code": code, "message": message.into()})
}

fn required_string<'a>(args: &'a Value, key: &str) -> Result<&'a str, Value> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| tool_error(format!("`{key}` is required.")))
}

fn client(config: &Config) -> Result<reqwest::blocking::Client, Value> {
    let mut headers = reqwest::header::HeaderMap::new();
    let value = format!("Bearer {}", config.token)
        .parse()
        .map_err(|_| tool_error("TERUSIN_TOKEN is not a valid HTTP bearer token."))?;
    headers.insert(reqwest::header::AUTHORIZATION, value);
    reqwest::blocking::Client::builder()
        .default_headers(headers)
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|_| tool_error("Could not initialize the trusin API client."))
}

fn request_json(request: reqwest::blocking::RequestBuilder) -> Result<Value, Value> {
    let response = request
        .send()
        .map_err(|error| tool_error(format!("Could not reach trusin: {error}")))?;
    let status = response.status();
    let payload = response.json::<Value>().unwrap_or_else(|_| json!({}));
    if !status.is_success() {
        let message = payload
            .get("message")
            .or_else(|| payload.get("error"))
            .and_then(Value::as_str)
            .unwrap_or("Request failed.");
        return Err(tool_error(format!(
            "trusin API returned {}: {message}",
            status.as_u16()
        )));
    }
    Ok(payload)
}

fn handle_tool_call(name: &str, args: &Value) -> Value {
    let config = match Config::from_env() {
        Ok(config) => config,
        Err(message) => return tool_error(message),
    };
    let client = match client(&config) {
        Ok(client) => client,
        Err(error) => return error,
    };
    let base = &config.backend_url;

    let response = match name {
        "get_health" => request_json(client.get(format!("{base}/health"))),
        "get_metrics" => {
            let range = args.get("range").and_then(Value::as_str).unwrap_or("24h");
            if !matches!(range, "24h" | "7d" | "30d") {
                return tool_error("`range` must be one of: 24h, 7d, 30d.");
            }
            request_json(client.get(format!("{base}/stats?range={range}")))
        }
        "list_events" => {
            let mut query = Vec::new();
            for key in ["search", "status", "source", "from", "to"] {
                if let Some(value) = args
                    .get(key)
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                {
                    query.push((key, value.to_string()));
                }
            }
            for key in ["page", "per_page"] {
                if let Some(value) = args.get(key).and_then(Value::as_i64) {
                    if value < 1 || (key == "per_page" && value > 200) {
                        return tool_error(format!("`{key}` is outside the supported range."));
                    }
                    query.push((key, value.to_string()));
                }
            }
            request_json(client.get(format!("{base}/events")).query(&query))
        }
        "get_event" => match required_string(args, "id") {
            Ok(id) => request_json(client.get(format!("{base}/events/{id}"))),
            Err(error) => return error,
        },
        "get_delivery_attempts" => match required_string(args, "id") {
            Ok(id) => request_json(client.get(format!("{base}/events/{id}/attempts"))),
            Err(error) => return error,
        },
        "send_webhook" => {
            let target_url = match required_string(args, "target_url") {
                Ok(value) => value,
                Err(error) => return error,
            };
            let Some(body) = args.get("body") else {
                return tool_error("`body` is required.");
            };
            let source = args.get("source").and_then(Value::as_str).unwrap_or("mcp");
            request_json(client.post(format!("{base}/api/send")).json(&json!({
                "source": source,
                "target_url": target_url,
                "body": body,
            })))
        }
        "retry_event" => match required_string(args, "id") {
            Ok(id) => request_json(client.post(format!("{base}/events/{id}/retry"))),
            Err(error) => return error,
        },
        _ => return tool_error(format!("Unknown tool: {name}")),
    };
    response.map(result).unwrap_or_else(|error| error)
}

fn tool_error_message(error: &Value) -> String {
    error["content"][0]["text"]
        .as_str()
        .unwrap_or("trusin request failed.")
        .to_string()
}

fn read_resource(uri: &str) -> Result<Value, Value> {
    let config = Config::from_env().map_err(|message| rpc_error(-32000, message))?;
    let client = client(&config).map_err(|error| rpc_error(-32000, tool_error_message(&error)))?;
    let request = match uri {
        "trusin://health" => client.get(format!("{}/health", config.backend_url)),
        "trusin://metrics" => client.get(format!("{}/stats?range=24h", config.backend_url)),
        _ => {
            let Some(id) = uri
                .strip_prefix("trusin://events/")
                .filter(|id| !id.is_empty())
            else {
                return Err(rpc_error(-32602, format!("Unknown resource: {uri}")));
            };
            client.get(format!("{}/events/{id}", config.backend_url))
        }
    };
    match request_json(request) {
        Ok(value) => {
            let text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
            Ok(json!({"contents": [{"uri": uri, "mimeType": "application/json", "text": text}]}))
        }
        Err(error) => Err(rpc_error(-32000, tool_error_message(&error))),
    }
}

fn handle_request(request: &Value) -> Result<Value, Value> {
    let method = request
        .get("method")
        .and_then(Value::as_str)
        .ok_or_else(|| rpc_error(-32600, "Request is missing `method`."))?;
    let params = request.get("params").cloned().unwrap_or_else(|| json!({}));
    let response = match method {
        "initialize" => json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {"tools": {}, "resources": {"listChanged": false}},
            "serverInfo": {"name": "trusin", "version": env!("CARGO_PKG_VERSION")},
        }),
        "ping" => json!({}),
        "tools/list" => json!({"tools": tool_list()}),
        "tools/call" => {
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| rpc_error(-32602, "`params.name` is required."))?;
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            handle_tool_call(name, &arguments)
        }
        "resources/list" => json!({"resources": resource_list()}),
        "resources/templates/list" => json!({"resourceTemplates": resource_templates()}),
        "resources/read" => {
            let uri = params
                .get("uri")
                .and_then(Value::as_str)
                .ok_or_else(|| rpc_error(-32602, "`params.uri` is required."))?;
            read_resource(uri)?
        }
        "notifications/initialized" => return Err(json!(null)),
        _ if method.starts_with("notifications/") => return Err(json!(null)),
        _ => return Err(rpc_error(-32601, format!("Unknown method: {method}"))),
    };
    Ok(response)
}

fn main() {
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(&line) {
            Ok(request) => request,
            Err(_) => {
                println!(
                    "{}",
                    json!({"jsonrpc": "2.0", "id": null, "error": rpc_error(-32700, "Invalid JSON.")})
                );
                continue;
            }
        };
        let id = request.get("id").cloned();
        match handle_request(&request) {
            Ok(result) => {
                if let Some(id) = id {
                    println!("{}", json!({"jsonrpc": "2.0", "id": id, "result": result}));
                }
            }
            Err(error) if error.is_null() => {}
            Err(error) => {
                if let Some(id) = id {
                    println!("{}", json!({"jsonrpc": "2.0", "id": id, "error": error}));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_advertises_tools_and_resources() {
        let result = handle_request(&json!({"method": "initialize"})).unwrap();
        assert_eq!(result["protocolVersion"], PROTOCOL_VERSION);
        assert!(result["capabilities"]["tools"].is_object());
        assert!(result["capabilities"]["resources"].is_object());
    }

    #[test]
    fn tool_list_exposes_observe_and_deliver_tools() {
        let result = handle_request(&json!({"method": "tools/list"})).unwrap();
        let names: Vec<&str> = result["tools"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect();
        assert_eq!(
            names,
            vec![
                "get_health",
                "get_metrics",
                "list_events",
                "get_event",
                "get_delivery_attempts",
                "send_webhook",
                "retry_event"
            ]
        );
    }

    #[test]
    fn notifications_do_not_generate_responses() {
        assert!(
            handle_request(&json!({"method": "notifications/initialized"}))
                .unwrap_err()
                .is_null()
        );
    }

    #[test]
    fn missing_token_is_reported_as_tool_error() {
        std::env::remove_var("TERUSIN_TOKEN");
        let response = handle_tool_call("get_health", &json!({}));
        assert_eq!(response["isError"], true);
        assert!(response["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("TERUSIN_TOKEN"));
    }
}
