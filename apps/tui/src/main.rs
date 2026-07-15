mod auth;
mod interactive;

use crate::auth::{
    auth_client, config_path, ensure_token, keychain_delete, load_config, managed_config_dirs,
    resolve_token, save_config, store_token, Config,
};
use clap::{Parser, Subcommand};
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashSet;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;

#[derive(Parser)]
#[command(name = "trusin", about = "Webhook relay CLI")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Save an API key (from the dashboard's Settings → API Tokens page)
    ///
    /// On the dashboard, sign in and generate a `ts_…` API key, then run
    /// `trusin set-token ts_…` here. The token is stored in the OS keychain
    /// (preferred), falling back to the config file on headless/CI boxes.
    SetToken {
        /// The `ts_…` API key. If omitted, you'll be prompted to paste it.
        token: Option<String>,
    },
    /// Forget the API key / clear stored credentials
    Logout,
    /// Remove trusin, its bundled MCP server, and local credentials
    Uninstall {
        /// Skip the confirmation prompt
        #[arg(short, long)]
        yes: bool,
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
    Retry { id: String },
    /// Poll events from server and forward to local port (no ngrok needed)
    Listen {
        #[arg(short, long, default_value = "3000")]
        port: u16,
        #[arg(short, long, default_value_t = 5)]
        interval: u64,
    },
    /// Open web dashboard
    Dashboard,
    /// Launch the interactive terminal dashboard
    Interactive,
    /// Run the bundled MCP server over stdio for AI clients
    Mcp {
        /// Override the bundled trusin-mcp executable path
        #[arg(long, env = "TRUSIN_MCP_PATH")]
        binary: Option<PathBuf>,
    },
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

fn bundled_mcp_path(override_path: Option<PathBuf>) -> Result<PathBuf, String> {
    if let Some(path) = override_path {
        return Ok(path);
    }
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let directory = executable
        .parent()
        .ok_or_else(|| "could not locate the trusin executable directory".to_string())?;
    Ok(directory.join("trusin-mcp"))
}

fn run_mcp(cfg: &Config, override_path: Option<PathBuf>) {
    let Some(token) = resolve_token(cfg) else {
        eprintln!("No API token configured. Run `trusin set-token ts_...` before starting MCP.");
        return;
    };
    let path = match bundled_mcp_path(override_path) {
        Ok(path) if path.is_file() => path,
        Ok(path) => {
            eprintln!(
                "Could not find the bundled MCP executable at {}. Reinstall trusin or set TRUSIN_MCP_PATH.",
                path.display()
            );
            return;
        }
        Err(error) => {
            eprintln!("Could not resolve the bundled MCP executable: {error}");
            return;
        }
    };
    let status = Command::new(path)
        .env("TERUSIN_TOKEN", token)
        .env("TERUSIN_URL", &cfg.backend)
        .status();
    match status {
        Ok(status) if status.success() => {}
        Ok(status) => eprintln!("trusin MCP exited with {status}."),
        Err(error) => eprintln!("Could not start trusin MCP: {error}"),
    }
}

fn uninstall_paths() -> Vec<PathBuf> {
    let mut paths = managed_config_dirs().to_vec();
    if let Ok(executable) = std::env::current_exe() {
        if executable.file_name().and_then(|name| name.to_str()) == Some("trusin") {
            paths.push(executable.clone());
            if let Some(directory) = executable.parent() {
                paths.push(directory.join("trusin-mcp"));
                paths.push(directory.join("terusin"));
            }
        }
    }
    paths
}

fn run_uninstall(confirmed: bool) {
    let paths = uninstall_paths();
    println!("This removes trusin's local files and saved credentials:");
    for path in &paths {
        println!("  {}", path.display());
    }
    println!("  OS keychain entries: trusin, terusin");

    if !confirmed {
        print!("Continue? [y/N] ");
        io::stdout().flush().ok();
        let mut answer = String::new();
        io::stdin().read_line(&mut answer).ok();
        if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            println!("Uninstall cancelled.");
            return;
        }
    }

    let mut failures = Vec::new();
    keychain_delete();
    for path in paths {
        if !path.exists() {
            continue;
        }
        let result = if path.is_dir() {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        if let Err(error) = result {
            failures.push(format!("{}: {error}", path.display()));
        }
    }

    if failures.is_empty() {
        println!("trusin has been removed. Your account and API tokens remain available in the dashboard.");
    } else {
        eprintln!("trusin was only partially removed:");
        for failure in failures {
            eprintln!("  {failure}");
        }
        eprintln!("Close any running trusin processes and try `trusin uninstall --yes` again.");
    }
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if let Some(Commands::Uninstall { yes }) = &cli.command {
        run_uninstall(*yes);
        return;
    }
    let mut cfg = load_config();
    let Some(command) = cli.command else {
        println!("\n  trusin CLI\n");
        if !ensure_token(&mut cfg) {
            return;
        }
        if let Err(error) = interactive::run(cfg).await {
            eprintln!("Interactive TUI error: {error}");
        }
        return;
    };
    let client = Client::new();

    match command {
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
                eprintln!(" That doesn't look like a trusin API key (expected `ts_…`).");
                return;
            }
            let stored = store_token(&mut c, &token);
            println!(" ✓ API key saved ({stored}).");
            println!("   Run `trusin events` to verify.");
        }
        Commands::Logout => {
            keychain_delete();
            let mut c = cfg;
            c.token = None;
            save_config(&c);
            println!(" ✓ Token cleared (keychain + config).");
        }
        Commands::Uninstall { .. } => unreachable!("handled before loading configuration"),
        Commands::Forward { port, url } => {
            if !ensure_token(&mut cfg) {
                return;
            }
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
                    Ok(r) => r
                        .json::<serde_json::Value>()
                        .await
                        .ok()
                        .and_then(|d| {
                            d["tunnels"][0]["public_url"]
                                .as_str()
                                .map(|s| s.to_string())
                        })
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
            if !ensure_token(&mut cfg) {
                return;
            }
            let auth = auth_client(&cfg);
            auth.post(format!("{}/config/default-target", cfg.backend))
                .json(&serde_json::json!({"url": ""}))
                .send()
                .await
                .ok();
            println!(" Forwarding stopped");
        }
        Commands::Events { limit } => {
            if !ensure_token(&mut cfg) {
                return;
            }
            let auth = auth_client(&cfg);
            let resp = auth.get(format!("{}/events", cfg.backend)).send().await;
            match resp {
                Ok(r) if r.status().is_success() => {
                    let events: Vec<Event> = r.json().await.unwrap_or_default();
                    println!(
                        " {:>8}  {:<10}  {:<10}  {}",
                        "ID", "Status", "Source", "Target"
                    );
                    println!(" {}", "─".repeat(60));
                    for e in events.iter().take(limit) {
                        println!(
                            " {:>8}  {:<10}  {:<10}  {}",
                            &e.id[..8],
                            e.status,
                            e.source,
                            e.target_url
                        );
                    }
                }
                Ok(r) => eprintln!("Error: HTTP {}", r.status()),
                Err(e) => eprintln!("Error: {e}"),
            }
        }
        Commands::Retry { id } => {
            if !ensure_token(&mut cfg) {
                return;
            }
            let auth = auth_client(&cfg);
            let resp = auth
                .post(format!("{}/events/{id}/retry", cfg.backend))
                .send()
                .await;
            match resp {
                Ok(r) if r.status().is_success() => println!(" Retried {id}"),
                _ => eprintln!(" Failed to retry {id}"),
            }
        }
        Commands::Listen { port, interval } => {
            if !ensure_token(&mut cfg) {
                return;
            }
            let auth = auth_client(&cfg);
            let fallback = format!("http://127.0.0.1:{port}");
            println!(" Listening: polling {}/events", cfg.backend);
            let mut seen = HashSet::new();
            loop {
                let resp = auth
                    .get(format!("{}/events?per_page=100", cfg.backend))
                    .send()
                    .await;
                if let Ok(r) = resp {
                    let data: serde_json::Value = r.json().await.unwrap_or_default();
                    let events = data["events"].as_array().cloned().unwrap_or_default();
                    for e in &events {
                        let id = e["id"].as_str().unwrap_or("").to_string();
                        if id.is_empty() || !seen.insert(id.clone()) {
                            continue;
                        }
                        let body = e["body"].clone();
                        if body.is_null() {
                            continue;
                        }
                        let target = e["target_url"].as_str().unwrap_or("").to_string();
                        let url = if target.is_empty() || !target.starts_with("http") {
                            fallback.clone()
                        } else {
                            target
                        };
                        match Client::new().post(&url).json(&body).send().await {
                            Ok(_) => {
                                auth.post(format!("{}/events/{id}/ack", cfg.backend))
                                    .send()
                                    .await
                                    .ok();
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
        Commands::Interactive => {
            if !ensure_token(&mut cfg) {
                return;
            }
            if let Err(e) = interactive::run(cfg).await {
                eprintln!("Interactive TUI error: {e}");
            }
        }
        Commands::Mcp { binary } => run_mcp(&cfg, binary),
        Commands::Status => {
            let fwd = client
                .get(format!("{}/config/default-target", cfg.backend))
                .send()
                .await;
            match fwd {
                Ok(r) => {
                    let c: FwdConfig = r.json().await.unwrap_or(FwdConfig {
                        default_target: String::new(),
                    });
                    let s = if c.default_target.is_empty() {
                        "PAUSED"
                    } else {
                        "FORWARDING"
                    };
                    let auth_mode = if resolve_token(&cfg).is_some() {
                        "token (api key)"
                    } else {
                        "password (Basic)"
                    };
                    println!(" Status:  {s}");
                    println!(
                        " Target:  {}",
                        if c.default_target.is_empty() {
                            "-"
                        } else {
                            &c.default_target
                        }
                    );
                    println!(" Auth:    {auth_mode}");
                    println!(" Backend: {}", cfg.backend);
                    println!(" Web:     {}", cfg.web);
                    println!(" Config:  {}", config_path().display());
                }
                Err(e) => eprintln!("Error: {e}"),
            }
        }
    }
}
