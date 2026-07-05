// Terusin web server — serves the embedded React SPA and reverse-proxies
// API calls to the backend. Replaces the previous server-side rendered app.

use axum::{
    body::Body,
    extract::{Request, State},
    http::{header, HeaderName, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::any,
    Router,
};
use base64::Engine;
use rust_embed::RustEmbed;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

/// Embedded Vite build output. The folder must exist at compile time;
/// `apps/frontend/dist/index.html` ships a placeholder so this compiles
/// even before the frontend is built. Run `npm run build` for the real bundle.
#[derive(RustEmbed)]
#[folder = "../frontend/dist"]
struct Asset;

struct AppState {
    backend_url: String,
    backend_auth: String,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let backend_url = std::env::var("BACKEND_URL").unwrap_or_else(|_| {
        let port = std::env::var("BACKEND_PORT").unwrap_or_else(|_| "3001".to_string());
        format!("http://localhost:{port}")
    });
    let port = std::env::var("WEB_PORT")
        .or_else(|_| std::env::var("PORT"))
        .unwrap_or_else(|_| "3002".to_string())
        .parse::<u16>()
        .unwrap_or(3002);

    let (user, pass) = (
        std::env::var("AUTH_USERNAME").unwrap_or_else(|_| "admin".to_string()),
        std::env::var("AUTH_PASSWORD")
            .unwrap_or_else(|_| "change-me-in-production".to_string()),
    );
    let backend_auth =
        base64::engine::general_purpose::STANDARD.encode(format!("{user}:{pass}"));

    let client = reqwest::Client::builder()
        .build()
        .expect("reqwest client");

    let state = Arc::new(AppState {
        backend_url: backend_url.clone(),
        backend_auth,
    });

    // Permissive CORS so the Vite dev server (:5173) can hit the API directly
    // during development. In prod, the SPA is same-origin and CORS is moot.
    let cors = CorsLayer::new()
        .allow_methods(Any)
        .allow_headers(Any)
        .allow_origin(Any);

    let app = Router::new()
        .route("/assets/{*path}", any(static_handler))
        .route("/index.html", any(static_handler))
        .route("/{*path}", any(proxy_or_spa))
        .route("/", any(proxy_or_spa))
        .layer(cors)
        .with_state(ProxyState { state, client });

    let addr = format!("0.0.0.0:{port}");
    tracing::info!("web on {addr} (backend: {backend_url})");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

#[derive(Clone)]
struct ProxyState {
    state: Arc<AppState>,
    client: reqwest::Client,
}

/// Routes that should be forwarded to the backend API. Anything else is
/// served from the SPA bundle (or 404 → index.html for client routing).
const API_PREFIXES: &[&str] =
    &["/events", "/rules", "/config", "/health", "/stats", "/api"];

fn is_api_path(path: &str) -> bool {
    API_PREFIXES
        .iter()
        .any(|p| path == *p || path.starts_with(&format!("{p}/")) || path.starts_with(p))
}

/// Serve a static asset from the embedded Vite build.
async fn static_handler(State(_): State<ProxyState>, req: Request) -> Response {
    let path = req.uri().path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    serve_asset(path)
}

/// Either forward to the backend (API paths) or serve the SPA.
async fn proxy_or_spa(
    State(ps): State<ProxyState>,
    req: Request,
) -> Response {
    let path = req.uri().path();

    // Non-GET/HEAD on root or unknown → could be a webhook ingest POST.
    // Forward any path that is not a known SPA asset to the backend, EXCEPT
    // the document itself.
    if is_api_path(path) || (req.method() != Method::GET && req.method() != Method::HEAD) {
        return proxy_to_backend(&ps, req).await;
    }

    // GET on a SPA route → try the asset, fall back to index.html for
    // client-side routing (e.g. /events/:id, /providers).
    let asset_path = path.trim_start_matches('/');
    if let Some(_) = Asset::get(asset_path) {
        return serve_asset(asset_path);
    }
    serve_asset("index.html")
}

fn serve_asset(path: &str) -> Response {
    match Asset::get(path) {
        Some(file) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            let body = Body::from(file.data.into_owned());
            let mut res = Response::new(body);
            res.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_str(mime.essence_str()).unwrap(),
            );
            // Hashed assets are immutable; index.html should always revalidate.
            if path != "index.html" {
                res.headers_mut().insert(
                    header::CACHE_CONTROL,
                    HeaderValue::from_static("public, max-age=31536000, immutable"),
                );
            } else {
                res.headers_mut().insert(
                    header::CACHE_CONTROL,
                    HeaderValue::from_static("no-cache"),
                );
            }
            res
        }
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Not found"))
            .unwrap(),
    }
}

/// Forward a request to the backend with Basic auth injected server-side.
/// This means the browser SPA never has to know the backend credentials in
/// the prod/embedded setup — same model as the old SSR app's backend_client().
async fn proxy_to_backend(ps: &ProxyState, req: Request) -> Response {
    let (parts, body) = req.into_parts();
    let body_bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(b) => b,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };

    let path_and_query = parts
        .uri
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or_else(|| parts.uri.path());
    let url = format!("{}{}", ps.state.backend_url, path_and_query);

    let mut req_builder = ps
        .client
        .request(parts.method.clone(), &url)
        .header(
            header::AUTHORIZATION,
            format!("Basic {}", ps.state.backend_auth),
        )
        .body(body_bytes);

    // Forward select headers from the original request.
    for key in [
        header::CONTENT_TYPE,
        HeaderName::from_static("x-webhook-source"),
        HeaderName::from_static("x-target-url"),
    ] {
        if let Some(v) = parts.headers.get(&key) {
            req_builder = req_builder.header(key, v);
        }
    }

    let upstream = match req_builder.send().await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("proxy to {url} failed: {e}");
            return StatusCode::BAD_GATEWAY.into_response();
        }
    };

    let status = upstream.status();
    let headers = upstream.headers().clone();
    let bytes = upstream.bytes().await.unwrap_or_default();

    let mut res = Response::builder()
        .status(status)
        .body(Body::from(bytes))
        .unwrap();
    for (k, v) in headers.iter() {
        // Skip hop-by-hop headers that the proxy should recompute.
        if matches!(
            k.as_str(),
            "content-encoding" | "transfer-encoding" | "connection" | "content-length"
        ) {
            continue;
        }
        res.headers_mut().insert(k, v.clone());
    }
    res
}
