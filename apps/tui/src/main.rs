use base64::Engine;
use clap::{Parser, Subcommand};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

const BACKEND: &str = "http://127.0.0.1:3011";
const WEB: &str = "http://localhost:3012";

#[derive(Serialize, Deserialize)]
struct Config {
    user: String,
    password: String,
    backend: String,
    #[serde(default = "default_web")]
    web: String,
}

fn default_web() -> String {
    WEB.to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            user: "admin".into(),
            password: "change-me-in-production".into(),
            backend: BACKEND.into(),
            web: WEB.into(),
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
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_config(c: &Config) {
    let s = toml::to_string_pretty(c).unwrap();
    std::fs::write(config_path(), s).ok();
}

fn auth_client(cfg: &Config) -> Client {
    let b = base64::engine::general_purpose::STANDARD
        .encode(format!("{}:{}", cfg.user, cfg.password));
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::AUTHORIZATION,
        format!("Basic {b}").parse().unwrap(),
    );
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
    /// Login & save credentials
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
    let cfg = load_config();
    let client = Client::new();
    let auth = auth_client(&cfg);

    match cli.command {
        Commands::Login { user, password, backend, web } => {
            let mut c = cfg;
            if let Some(u) = user { c.user = u; }
            if let Some(p) = password { c.password = p; }
            if let Some(b) = backend { c.backend = b; }
            if let Some(w) = web { c.web = w; }
            save_config(&c);
            println!(" Saved to {}", config_path().display());
            open::that(&c.web).ok();
            println!(" Opening {}", c.web);
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

            client
                .post(format!("{}/config/default-target", cfg.backend))
                .json(&serde_json::json!({"url": target}))
                .send()
                .await
                .ok();
            println!(" Forwarding webhooks → {target}");
        }
        Commands::Stop => {
            client
                .post(format!("{}/config/default-target", cfg.backend))
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
                let resp = auth.get(format!("{}/events", cfg.backend)).send().await;
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
                            Ok(_) => println!("  {} → {}", &id[..8], url),
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
                    println!(" Status:  {s}");
                    println!(" Target:  {}", if c.default_target.is_empty() { "-" } else { &c.default_target });
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
