// Credential & config handling for the `trusin` CLI.
//
// Token precedence (highest → lowest):
//   1. `TERUSIN_TOKEN` env var (folded into Config by load_config)
//   2. OS keychain (macOS Keychain / Linux secret-service)
//   3. `token` field in config.toml (headless/CI fallback)
//
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use std::path::PathBuf;

const BACKEND: &str = "https://api.trusin.my.id";
const WEB: &str = "https://app.trusin.my.id";
const LEGACY_HOSTED_URL: &str = "https://terusin-dev.my.id";

// OS keychain entry name under which the API token is stored (preferred over
// the plaintext config file). Falls back gracefully on platforms without one.
const KEYRING_SERVICE: &str = "trusin";
const LEGACY_KEYRING_SERVICE: &str = "terusin";
const KEYRING_ACCOUNT: &str = "default";

fn env_or_default(key: &str, fallback: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| fallback.to_string())
}

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub backend: String,
    #[serde(default = "default_web")]
    pub web: String,
    /// Cached API token (set via `trusin set-token`). The OS keychain is the
    /// preferred store; this is a fallback for headless/CI environments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

fn default_web() -> String {
    WEB.to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            backend: env_or_default("TERUSIN_BACKEND", BACKEND),
            web: env_or_default("TERUSIN_WEB", WEB),
            token: std::env::var("TERUSIN_TOKEN")
                .ok()
                .filter(|t| !t.is_empty()),
        }
    }
}

pub fn config_path() -> PathBuf {
    let mut p = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    p.push("trusin");
    std::fs::create_dir_all(&p).ok();
    p.push("config.toml");
    p
}

fn legacy_config_path() -> PathBuf {
    let mut p = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    p.push("terusin");
    p.push("config.toml");
    p
}

pub fn load_config() -> Config {
    let path = config_path();
    let mut c: Config = if let Ok(s) = std::fs::read_to_string(&path) {
        toml::from_str(&s).unwrap_or_default()
    } else if let Ok(s) = std::fs::read_to_string(legacy_config_path()) {
        toml::from_str(&s).unwrap_or_default()
    } else {
        Config::default()
    };
    if migrate_legacy_hosted_config(&mut c) {
        save_config(&c);
    }

    if let Ok(backend) = std::env::var("TERUSIN_BACKEND") {
        if !backend.trim().is_empty() {
            c.backend = backend;
        }
    }
    if let Ok(web) = std::env::var("TERUSIN_WEB") {
        if !web.trim().is_empty() {
            c.web = web;
        }
    }
    // TERUSIN_TOKEN env var wins over the config file's `token` field when set.
    if let Ok(t) = std::env::var("TERUSIN_TOKEN") {
        if !t.is_empty() {
            c.token = Some(t);
        }
    }
    c
}

fn migrate_legacy_hosted_config(c: &mut Config) -> bool {
    let mut migrated = false;
    if c.backend == LEGACY_HOSTED_URL {
        c.backend = BACKEND.to_string();
        migrated = true;
    }
    if c.web == LEGACY_HOSTED_URL {
        c.web = WEB.to_string();
        migrated = true;
    }
    migrated
}

pub fn save_config(c: &Config) {
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
        .or_else(|| {
            keyring::Entry::new(LEGACY_KEYRING_SERVICE, KEYRING_ACCOUNT)
                .ok()
                .and_then(|e| e.get_password().ok())
                .filter(|t| !t.is_empty())
        })
}

fn keychain_set(token: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT).map_err(|e| e.to_string())?;
    entry.set_password(token).map_err(|e| e.to_string())
}

pub fn keychain_delete() {
    for service in [KEYRING_SERVICE, LEGACY_KEYRING_SERVICE] {
        if let Ok(entry) = keyring::Entry::new(service, KEYRING_ACCOUNT) {
            let _ = entry.delete_credential();
        }
    }
}

/// Resolve the active token following precedence:
///   1. `TERUSIN_TOKEN` env var (folded into Config by load_config)
///   2. OS keychain
///   3. config file's `token` field
pub fn resolve_token(cfg: &Config) -> Option<String> {
    if let Some(t) = cfg.token.as_ref().filter(|t| !t.is_empty()) {
        return Some(t.clone());
    }
    keychain_get()
}

/// Persist a token via keychain (preferred) → config-file fallback. Some
/// backends (notably macOS Data Protection Keychain from a non-GUI session)
/// accept writes silently without being readable back — in that case we drop
/// the keychain entry and fall back to the config file so the token is always
/// usable. Returns a short label describing where it landed.
pub fn store_token(cfg: &mut Config, token: &str) -> &'static str {
    match keychain_set(token) {
        Ok(_) => {
            if keychain_get().as_deref() == Some(token) {
                cfg.token = None;
                save_config(cfg);
                "keychain"
            } else {
                keychain_delete();
                cfg.token = Some(token.to_string());
                save_config(cfg);
                "config (keychain round-trip failed)"
            }
        }
        Err(_) => {
            cfg.token = Some(token.to_string());
            save_config(cfg);
            "config (keychain unavailable)"
        }
    }
}

/// Build a reqwest client that authenticates every request with a Bearer API token.
pub fn auth_client(cfg: &Config) -> Client {
    let mut headers = reqwest::header::HeaderMap::new();
    let token = resolve_token(cfg).expect("authenticated CLI commands require a token");
    headers.insert(
        reqwest::header::AUTHORIZATION,
        format!("Bearer {token}").parse().unwrap(),
    );
    Client::builder().default_headers(headers).build().unwrap()
}

/// First-run onboarding: ensure the config has a usable token. If none is
/// found (env/keychain/config all empty), interactively prompt the user to
/// paste a `ts_…` key from the dashboard. Returns true if a token is usable
/// after this call (pre-existing or just entered), false if the user cancelled
/// or entered something invalid — callers should bail in that case.
pub fn ensure_token(cfg: &mut Config) -> bool {
    if resolve_token(cfg).is_some() {
        return true;
    }
    println!(" No API key configured yet.");
    println!(
        " On the dashboard: Settings → API Tokens → \"Generate API key\", copy the `ts_…` value."
    );
    print!(" Paste it here (or Enter to cancel): ");
    io::stdout().flush().ok();
    let mut line = String::new();
    io::stdin().read_line(&mut line).ok();
    let token = line.trim().to_string();
    if token.is_empty() {
        return false;
    }
    if !token.starts_with("ts_") || token.len() < 10 {
        eprintln!(" That doesn't look like a trusin API key (expected `ts_…`).");
        eprintln!(" Run `trusin set-token` to set one.");
        return false;
    }
    let where_ = store_token(cfg, &token);
    println!(" ✓ API key saved ({where_}).");
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_the_retired_hosted_endpoint() {
        let mut config = Config {
            backend: LEGACY_HOSTED_URL.to_string(),
            web: LEGACY_HOSTED_URL.to_string(),
            token: Some("ts_existing_token".to_string()),
        };

        assert!(migrate_legacy_hosted_config(&mut config));
        assert_eq!(config.backend, BACKEND);
        assert_eq!(config.web, WEB);
        assert_eq!(config.token.as_deref(), Some("ts_existing_token"));
    }
}
