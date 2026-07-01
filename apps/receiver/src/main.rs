use axum::{extract::Request, response::IntoResponse, Json, Router};
use serde_json::Value;

async fn catch_all(req: Request) -> impl IntoResponse {
    let method = req.method().clone();
    let now = chrono::Utc::now().format("%H:%M:%S%.3f");
    let headers: Vec<String> = req
        .headers()
        .iter()
        .map(|(k, v)| format!("  {}: {}", k, v.to_str().unwrap_or("?")))
        .collect();

    let body: Value = match axum::body::to_bytes(req.into_body(), 1024 * 1024).await {
        Ok(b) => serde_json::from_slice(&b).unwrap_or(Value::String(
            String::from_utf8_lossy(&b).to_string(),
        )),
        Err(_) => Value::Null,
    };

    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!(" [{now}] {method} → Webhook Received!");
    println!("─────────────────────────────────────────");
    println!(" Headers:");
    for h in &headers {
        println!("{h}");
    }
    println!("─────────────────────────────────────────");
    println!(" Body:");
    println!("{}", serde_json::to_string_pretty(&body).unwrap_or_default());
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    Json(serde_json::json!({"status": "ok"}))
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse::<u16>()
        .unwrap_or(3000);

    let app = Router::new().fallback(catch_all);

    let addr = format!("0.0.0.0:{port}");
    println!(" Receiver listening on http://localhost:{port}");
    println!(" Arahin Terusin ke: terusin forward --port {port}");

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
