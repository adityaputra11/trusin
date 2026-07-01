# Terusin — Agent Guide

## Workspace (Cargo workspace at root)

| Binary | Dir | Cargo target |
|--------|-----|-------------|
| Backend | `apps/backend` | `cargo run --bin backend` |
| Web dashboard | `apps/web` | `cargo run --bin web` |
| CLI | `apps/tui` | `cargo run --bin terusin` |
| MCP server | `apps/mcp` | `cargo run --bin mcp` |
| Example receiver | `apps/receiver` | `cargo run --bin receiver` |

Build all: `cargo build --release --bin backend --bin web --bin terusin --bin mcp`

## Dev loop

```sh
docker compose up -d postgres redis
PORT=3011 cargo run --bin backend                 # terminal 1
PORT=3012 BACKEND_URL=http://localhost:3011 cargo run --bin web  # terminal 2
```

Auth defaults: `admin` / `terusin123` (set via `AUTH_USERNAME`/`AUTH_PASSWORD` env vars, seeded to DB on first run).

## Key architecture

- **backend** receives webhooks at `/{source}` (catch-all), stores in Postgres, pushes event ID to Redis list `terusin:queue`.
- **Worker** (in backend) pops from Redis, forwards to `event.target_url`, on failure pushes to Redis sorted set `terusin:retry` with exponential backoff timestamp.
- **forward_rules** table stores provider→target mappings. Source is extracted from first URL segment. Rules matched in `handle_webhook` to set `target_url` fallback, and again in `forward_to_rules()` after main delivery.
- **web** is server-side rendered HTML (no JS framework). Dashboard calls backend API with Basic auth via `backend_client()` helper.
- **CLI** reads `~/.config/terusin/config.toml` for credentials. `forward` command starts ngrok automatically for remote backends.

## Gotchas

- `reqwest::Client::new()` calls without auth headers will 401 on `/events` and `/rules` endpoints. Use `backend_client()` in web app or `auth_client()` in CLI/MCP.
- `axum_extra::response::sse` does not exist in axum-extra 0.10. Implement SSE manually if needed.
- SQLx migrations use timestamp-prefixed files in `apps/backend/migrations/`.
- MCP server uses `reqwest::blocking` — keep it simple, avoid adding the whole tokio runtime to MCP.
- `/config/default-target` is public (no auth). All other API endpoints require auth.
