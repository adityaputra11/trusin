use base64::Engine;
use clap::{Parser, Subcommand};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::{self, Write};
use std::path::PathBuf;

const BACKEND: &str = "http://127.0.0.1:3011";
const WEB: &str = "http://localhost:3012";

// OS keychain entry name under which the API token is stored (preferred over
// the plaintext config file). Falls back gracefully on platforms without one.
const KEYRING_SERVICE: &str = "terusin";
const KEYRING_ACCOUNT: &str = "default";

fn env_or_default(key: &str, fallback: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| fallback.to_string())
}

#[derive(Serialize, Deserialize)]
struct Config {
    user: String,
    password: String,
    backend: String,
    #[serde(default = "default_web")]
    web: String,
    /// Cached API token (paired via `terusin pair`). The OS keychain is the
    /// preferred store; this is a fallback for headless/CI environments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    token: Option<String>,
}

fn default_web() -> String {
    WEB.to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            user: env_or_default("TERUSIN_USER", "admin"),
            password: env_or_default("TERUSIN_PASSWORD", "change-me-in-production"),
            backend: env_or_default("TERUSIN_BACKEND", BACKEND),
            web: env_or_default("TERUSIN_WEB", WEB),
            // TERUSIN_TOKEN env wins over keychain/config when set.
            token: std::env::var("TERUSIN_TOKEN").ok().filter(|t| !t.is_empty()),
        }
    }
}

fn config_path() -> PathBuf {
    let mut p = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    p.push("terusin");
    std::fs::create_dir_all(&p).ok();
    p.push("config.toml");
    p
}

fn load_config() -> Config {
    let path = config_path();
    if let Ok(s) = std::fs::read_to_string(&path) {
        if let Ok(c) = toml::from_str(&s) {
            return c;
        }
    }
    Config::default()
}

fn save_config(c: &Config) {
    let s = toml::to_string_pretty(c).unwrap();
    std::fs::write(config_path(), s).ok();
}

// ── Keychain helpers ──────────────────────────────────────────────────────
//
// The token is the highest-value secret the CLI holds (full account access),
// so we keep it in the OS keychain when available. Every call is fallible —
// on platforms without a keychain backend (headless Linux w/o secret-service)
// we silently fall back to the config file / env var.

fn keychain_get() -> Option<String> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .ok()
        .and_then(|e| e.get_password().ok())
        .filter(|t| !t.is_empty())
}

fn keychain_set(token: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|e| e.to_string())?;
    entry.set_password(token).map_err(|e| e.to_string())
}

fn keychain_delete() {
    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT) {
        let _ = entry.delete_credential();
    }
}

/// Resolve the active token following precedence:
///   1. `TERUSIN_TOKEN` env var (already folded into Config::default)
///   2. OS keychain
///   3. config file's `token` field
fn resolve_token(cfg: &Config) -> Option<String> {
    if let Some(t) = cfg.token.as_ref().filter(|t| !t.is_empty()) {
        return Some(t.clone());
    }
    keychain_get()
}

/// Build a reqwest client that authenticates every request. Prefers a Bearer
/// token (from pair); falls back to HTTP Basic (legacy `terusin login`).
fn auth_client(cfg: &Config) -> Client {
    let mut headers = reqwest::header::HeaderMap::new();
    if let Some(token) = resolve_token(cfg) {
        headers.insert(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {token}").parse().unwrap(),
        );
    } else {
        let b = base64::engine::general_purpose::STANDARD
            .encode(format!("{}:{}", cfg.user, cfg.password));
        headers.insert(
            reqwest::header::AUTHORIZATION,
            format!("Basic {b}").parse().unwrap(),
        );
    }
    Client::builder().default_headers(headers).build().unwrap()
}

#[derive(Parser)]
#[command(name = "terusin", about = "Webhook relay CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Pair this device with a 6-digit code from the dashboard (preferred)
    Pair {
        #[arg(short, long)]
        backend: Option<String>,
        #[arg(long)]
        web: Option<String>,
    },
    /// Forget the paired token / clear stored credentials
    Logout,
    /// Login with username & password (legacy; prefer `pair`)
    Login {
        #[arg(short, long)]
        user: Option<String>,
        #[arg(short, long)]
        password: Option<String>,
        #[arg(short, long)]
        backend: Option<String>,
        #[arg(long)]
        web: Option<String>,
    },
    /// Forward webhooks to a local port
    Forward {
        #[arg(short, long, default_value = "8080")]
        port: u16,
        /// Custom public URL (skip ngrok)
        #[arg(long)]
        url: Option<String>,
    },
    /// Stop forwarding
    Stop,
    /// List recent events
    Events {
        #[arg(short, long, default_value_t = 10)]
        limit: usize,
    },
    /// Retry a failed event
    Retry {
        id: String,
    },
    /// Poll events from server and forward to local port (no ngrok needed)
    Listen {
        #[arg(short, long, default_value = "3000")]
        port: u16,
        #[arg(short, long, default_value_t = 5)]
        interval: u64,
    },
    /// Open web dashboard
    Dashboard,
    /// Show current config & status
    Status,
}

#[derive(Deserialize)]
struct Event {
    id: String,
    source: String,
    status: String,
    target_url: String,
}

#[derive(Deserialize)]
struct FwdConfig {
    default_target: String,
}

#[derive(Deserialize)]
struct PairResponse {
    token: String,
    name: String,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let cfg = load_config();
    let client = Client::new();
    let auth = auth_client(&cfg);

    match cli.command {
        Commands::Pair { backend, web } => {
            let mut c = cfg;
            if let Some(b) = backend { c.backend = b; }
            if let Some(w) = web { c.web = w; }

            println!(" Pair this device with {}", c.backend);
            print!(" Enter pairing code (from Settings → Devices & Tokens): ");
            io::stdout().flush().ok();
            let mut code = String::new();
            io::stdin().read_line(&mut code).ok();
            let code = code.trim().to_string();
            if code.is_empty() {
                eprintln!(" No code entered.");
                return;
            }

            let resp = Client::new()
                .post(format!("{}/api/auth/pair", c.backend))
                .json(&serde_json::json!({ "code": code }))
                .send()
                .await;

            match resp {
                Ok(r) if r.status().is_success() => {
                    let pair: PairResponse = r.json().await.unwrap_or(PairResponse {
                        token: String::new(),
                        name: "device".to_string(),
                    });
                    if pair.token.is_empty() {
                        eprintln!(" Backend returned an empty token.");
                        return;
                    }
                    // Store token: keychain first, config file as fallback.
                    // Try the keychain first, but VERIFY the round-trip — some
                    // backends (notably macOS Data Protection Keychain from a
                    // non-GUI session) accept writes silently without being
                    // readable back. In that case we fall back to the config
                    // file so the token is always usable after pairing.
                    let stored = match keychain_set(&pair.token) {
                        Ok(_) => {
                            if keychain_get().as_deref() == Some(pair.token.as_str()) {
                                c.token = None;
                                save_config(&c);
                                "keychain"
                            } else {
                                keychain_delete();
                                c.token = Some(pair.token.clone());
                                save_config(&c);
                                "config (keychain round-trip failed)"
                            }
                        }
                        Err(_) => {
                            c.token = Some(pair.token.clone());
                            save_config(&c);
                            "config (keychain unavailable)"
                        }
                    };
                    println!(" ✓ Paired as \"{}\". Token stored ({stored}).", pair.name);
                    println!("   Run `terusin events` to verify.");
                }
                Ok(r) => {
                    let status = r.status();
                    let body = r.text().await.unwrap_or_default();
                    eprintln!(" Pairing failed ({status}): {body}");
                }
                Err(e) => {
                    eprintln!(" Can't reach {}: {e}", c.backend);
                }
            }
        }
        Commands::Logout => {
            keychain_delete();
            let mut c = cfg;
            c.token = None;
            save_config(&c);
            println!(" ✓ Token cleared (keychain + config).");
        }
        Commands::Login { user, password, backend, web } => {
            let mut c = cfg;

            let default = Config::default();
            if let Some(b) = backend { c.backend = b; }
            if let Some(w) = web { c.web = w; }

            if user.is_none() || password.is_none() {
                println!(" Login to {} (legacy password flow; prefer `terusin pair`)", c.backend);
            }
            if let Some(u) = user { c.user = u; }
            if c.user.is_empty() {
                print!(" Username [{}]: ", default.user);
                io::stdout().flush().ok();
                let mut input = String::new();
                io::stdin().read_line(&mut input).ok();
                let u = input.trim();
                c.user = if u.is_empty() { default.user.clone() } else { u.to_string() };
            }
            if let Some(p) = password { c.password = p; }
            if c.password.is_empty() {
                print!(" Password: ");
                io::stdout().flush().ok();
                let mut input = String::new();
                io::stdin().read_line(&mut input).ok();
                let p = input.trim();
                c.password = if p.is_empty() { default.password.clone() } else { p.to_string() };
            }

            // verify credentials
            let test = Client::builder()
                .default_headers({
                    let mut h = reqwest::header::HeaderMap::new();
                    let b = base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", c.user, c.password));
                    h.insert(reqwest::header::AUTHORIZATION, format!("Basic {b}").parse().unwrap());
                    h
                })
                .build().unwrap()
                .get(format!("{}/events", c.backend))
                .send()
                .await;

            match test {
                Ok(r) if r.status().is_success() => {
                    save_config(&c);
                    println!(" Saved to {}", config_path().display());
                    open::that(&c.web).ok();
                    println!(" Opening {}", c.web);
                }
                Ok(_) => {
                    eprintln!(" Login gagal — credential ditolak di {}", c.backend);
                }
                Err(e) => {
                    eprintln!(" Gagal connect ke {}: {e}", c.backend);
                }
            }
        }
        Commands::Forward { port, url } => {
            let target = if let Some(u) = url {
                u
            } else if cfg.backend.contains("localhost") || cfg.backend.contains("127.0.0.1") {
                let t = format!("http://localhost:{port}");
                println!(" Backend lokal, langsung forward ke {t}");
                t
            } else {
                // backend remote — tunnel via ngrok
                std::process::Command::new("ngrok")
                    .args(["http", &port.to_string(), "--log", "stdout"])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                    .ok();
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                match reqwest::get("http://127.0.0.1:4040/api/tunnels").await {
                    Ok(r) => r.json::<serde_json::Value>().await.ok()
                        .and_then(|d| d["tunnels"][0]["public_url"].as_str().map(|s| s.to_string()))
                        .unwrap_or_else(|| {
                            println!(" ngrok gak jalan, fallback ke localhost");
                            format!("http://localhost:{port}")
                        }),
                    Err(_) => {
                        println!(" ngrok gak jalan, fallback ke localhost");
                        format!("http://localhost:{port}")
                    }
                }
            };

            auth.post(format!("{}/config/default-target", cfg.backend))
                .json(&serde_json::json!({"url": target}))
                .send()
                .await
                .ok();
            println!(" Forwarding webhooks → {target}");
        }
        Commands::Stop => {
            auth.post(format!("{}/config/default-target", cfg.backend))
                .json(&serde_json::json!({"url": ""}))
                .send()
                .await
                .ok();
            println!(" Forwarding stopped");
        }
        Commands::Events { limit } => {
            let resp = auth.get(format!("{}/events", cfg.backend)).send().await;
            match resp {
                Ok(r) if r.status().is_success() => {
                    let events: Vec<Event> = r.json().await.unwrap_or_default();
                    println!(" {:>8}  {:<10}  {:<10}  {}", "ID", "Status", "Source", "Target");
                    println!(" {}", "─".repeat(60));
                    for e in events.iter().take(limit) {
                        println!(" {:>8}  {:<10}  {:<10}  {}", &e.id[..8], e.status, e.source, e.target_url);
                    }
                }
                Ok(r) => eprintln!("Error: HTTP {}", r.status()),
                Err(e) => eprintln!("Error: {e}"),
            }
        }
        Commands::Retry { id } => {
            let resp = auth.post(format!("{}/events/{id}/retry", cfg.backend)).send().await;
            match resp {
                Ok(r) if r.status().is_success() => println!(" Retried {id}"),
                _ => eprintln!(" Failed to retry {id}"),
            }
        }
        Commands::Listen { port, interval } => {
            let fallback = format!("http://127.0.0.1:{port}");
            println!(" Listening: polling {}/events", cfg.backend);
            let mut seen = HashSet::new();
            loop {
                let resp = auth.get(format!("{}/events?per_page=100", cfg.backend)).send().await;
                if let Ok(r) = resp {
                    let data: serde_json::Value = r.json().await.unwrap_or_default();
                    let events = data["events"].as_array().cloned().unwrap_or_default();
                    for e in &events {
                        let id = e["id"].as_str().unwrap_or("").to_string();
                        if id.is_empty() || !seen.insert(id.clone()) { continue; }
                        let body = e["body"].clone();
                        if body.is_null() { continue; }
                        let target = e["target_url"].as_str().unwrap_or("").to_string();
                        let url = if target.is_empty() || !target.starts_with("http") { fallback.clone() } else { target };
                        match Client::new().post(&url).json(&body).send().await {
                            Ok(_) => {
                                auth.post(format!("{}/events/{id}/ack", cfg.backend)).send().await.ok();
                                println!("  {} → {}", &id[..8], url);
                            }
                            Err(e) => println!("  {} → {} ERR: {e}", &id[..8], url),
                        }
                    }
                }
                tokio::time::sleep(std::time::Duration::from_secs(interval)).await;
            }
        }
        Commands::Dashboard => {
            open::that(&cfg.web).ok();
            println!(" Opening {}", cfg.web);
        }
        Commands::Status => {
            let fwd = client
                .get(format!("{}/config/default-target", cfg.backend))
                .send()
                .await;
            match fwd {
                Ok(r) => {
                    let c: FwdConfig = r.json().await.unwrap_or(FwdConfig { default_target: String::new() });
                    let s = if c.default_target.is_empty() { "PAUSED" } else { "FORWARDING" };
                    let auth_mode = if resolve_token(&cfg).is_some() { "token (paired)" } else { "password (Basic)" };
                    println!(" Status:  {s}");
                    println!(" Target:  {}", if c.default_target.is_empty() { "-" } else { &c.default_target });
                    println!(" Auth:    {auth_mode}");
                    println!(" User:    {}", cfg.user);
                    println!(" Backend: {}", cfg.backend);
                    println!(" Web:     {}", cfg.web);
                    println!(" Config:  {}", config_path().display());
                }
                Err(e) => eprintln!("Error: {e}"),
            }
        }
    }
}
