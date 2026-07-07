mod auth;

use crate::auth::{auth_client, config_path, ensure_token, keychain_delete, load_config, resolve_token, save_config, store_token, Config};
use base64::Engine;
use clap::{Parser, Subcommand};
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashSet;
use std::io::{self, Write};

#[derive(Parser)]
#[command(name = "terusin", about = "Webhook relay CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Save an API key (from the dashboard's Settings → API Tokens page)
    ///
    /// On the dashboard, sign in and generate a `ts_…` API key, then run
    /// `terusin set-token ts_…` here. The token is stored in the OS keychain
    /// (preferred), falling back to the config file on headless/CI boxes.
    SetToken {
        /// The `ts_…` API key. If omitted, you'll be prompted to paste it.
        token: Option<String>,
    },
    /// Forget the API key / clear stored credentials
    Logout,
    /// Login with username & password (legacy; prefer `set-token`)
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

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let mut cfg = load_config();
    let client = Client::new();

    match cli.command {
        Commands::SetToken { token } => {
            let mut c = cfg;
            // Take the key from the positional arg, or prompt for it (hidden).
            let token = match token {
                Some(t) if !t.trim().is_empty() => t.trim().to_string(),
                _ => {
                    println!(" Paste an API key from the dashboard's Settings → API Tokens page.");
                    print!(" Token (ts_…): ");
                    io::stdout().flush().ok();
                    let mut line = String::new();
                    io::stdin().read_line(&mut line).ok();
                    line.trim().to_string()
                }
            };
            if !token.starts_with("ts_") || token.len() < 10 {
                eprintln!(" That doesn't look like a Terusin API key (expected `ts_…`).");
                return;
            }
            let stored = store_token(&mut c, &token);
            println!(" ✓ API key saved ({stored}).");
            println!("   Run `terusin events` to verify.");
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
                println!(" Login to {} (legacy password flow; prefer `terusin set-token`)", c.backend);
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
            if !ensure_token(&mut cfg) { return; }
            let auth = auth_client(&cfg);
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
            if !ensure_token(&mut cfg) { return; }
            let auth = auth_client(&cfg);
            auth.post(format!("{}/config/default-target", cfg.backend))
                .json(&serde_json::json!({"url": ""}))
                .send()
                .await
                .ok();
            println!(" Forwarding stopped");
        }
        Commands::Events { limit } => {
            if !ensure_token(&mut cfg) { return; }
            let auth = auth_client(&cfg);
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
            if !ensure_token(&mut cfg) { return; }
            let auth = auth_client(&cfg);
            let resp = auth.post(format!("{}/events/{id}/retry", cfg.backend)).send().await;
            match resp {
                Ok(r) if r.status().is_success() => println!(" Retried {id}"),
                _ => eprintln!(" Failed to retry {id}"),
            }
        }
        Commands::Listen { port, interval } => {
            if !ensure_token(&mut cfg) { return; }
            let auth = auth_client(&cfg);
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
                    let auth_mode = if resolve_token(&cfg).is_some() { "token (api key)" } else { "password (Basic)" };
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
