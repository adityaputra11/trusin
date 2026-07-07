# Terusin — Agent Guide

## Workspace (Cargo workspace at root)

| Binary | Dir | Cargo target |
|--------|-----|-------------|
| Backend | `apps/backend` | `cargo run --bin backend` |
| Web dashboard | `apps/web` | `cargo run --bin web` |
| Frontend (Vite) | `apps/frontend` | `npm run dev` / `npm run build` |
| CLI | `apps/tui` | `cargo run --bin terusin` |
| MCP server | `apps/mcp` | `cargo run --bin mcp` |
| Example receiver | `apps/receiver` | `cargo run --bin receiver` |

Build all: `cargo build --release --bin backend --bin web --bin terusin --bin mcp`

> The frontend must be built before the `web` binary can embed it:
> `cd apps/frontend && npm install && npm run build` produces `apps/frontend/dist/`,
> which `rust-embed` bakes into the `web` binary at compile time.

## Dev loop

```sh
docker compose up -d postgres redis
PORT=3011 cargo run --bin backend                              # T1: backend API + worker
cd apps/frontend && npm run dev                                 # T2: Vite dev server (:5173, proxies API → :3011)
# Optional, to test the embedded prod build:
cd apps/frontend && npm run build && PORT=3012 cargo run --bin web  # T3: serves embedded SPA on :3012
```

The Vite dev server (`:5173`) proxies `/events`, `/rules`, `/config`, `/health` to the backend,
so during development you only need T1 + T2. Use T3 to verify the production single-binary
experience (SPA + reverse-proxy in one `web` process).

Auth defaults: `admin` / `change-me-in-production` (set via `AUTH_USERNAME`/`AUTH_PASSWORD` env vars, seeded to DB on first run).

## Key architecture

- **backend** receives webhooks at `/{source}` (catch-all), stores in Postgres, pushes event ID to Redis list `terusin:queue`.
- **Worker** (in backend) pops from Redis, forwards to `event.target_url`, on failure pushes to Redis sorted set `terusin:retry` with exponential backoff timestamp.
- **forward_rules** table stores provider→target mappings. Source is extracted from first URL segment. Rules matched in `handle_webhook` to set `target_url` fallback, and again in `forward_to_rules()` after main delivery.
- **frontend** (`apps/frontend`) is a React + Vite + TypeScript SPA with a dark, data-first design system. It calls the backend API directly (Basic auth header attached by `lib/api.ts`). In dev it runs on Vite's :5173 with a proxy; in prod the built `dist/` is embedded into the `web` binary.
- **web** is now a thin static-file + reverse-proxy server: it serves the embedded React bundle (`rust-embed`) and forwards API paths (`/events`, `/rules`, `/config`, `/health`, `/api`) to the backend with Basic auth injected server-side. The old SSR HTML handlers have been removed.
- **CLI** reads `~/.config/terusin/config.toml` for credentials. `forward` command starts ngrok automatically for remote backends.

## Gotchas

- The SPA stores credentials as `Basic base64(user:pass)` in `sessionStorage` and attaches them on every API call. In the embedded/prod setup, the `web` proxy injects its own Basic auth server-side, so the browser cred is only used in Vite dev mode.
- Backend CORS is configured via `WEB_URL` env (comma-separated origins). Defaults to `http://localhost:5173,http://localhost:3012` (+ 127.0.0.1 variants) for dev.
- `apps/frontend/dist/` must exist before `cargo build --bin web` — a placeholder `index.html` ships in the repo so the binary compiles even without a frontend build. Run `npm run build` for the real bundle, then rebuild the `web` binary (`touch apps/web/src/main.rs` to force rust-embed to re-read `dist/`).
- `axum_extra::response::sse` does not exist in axum-extra 0.10. Implement SSE manually if needed.
- SQLx migrations use timestamp-prefixed files in `apps/backend/migrations/`.
- MCP server uses `reqwest::blocking` — keep it simple, avoid adding the whole tokio runtime to MCP.
- Public backend endpoints: `/health`, `GET /config/default-target`, `GET /config/endpoint`, `GET /config/oauth`, the auth endpoints (`/api/auth/login`, `/api/auth/logout`, `/api/auth/google`, `/api/auth/callback/google`, `/api/auth/me`), and the webhook ingests `POST /` and `POST /{source}`. All other API endpoints require auth.
- **RBAC:** all mutating handlers (create/update/delete rule, retry/ack/delete event, bulk retry/delete, `POST /config/default-target`) are admin-gated via `require_admin()`. Read endpoints (`GET /events`, `GET /rules`, `GET /stats`) accept any authenticated role. `viewer` is read-only.
- `POST /config/default-target` requires auth + admin (it was previously public, which let anyone redirect all webhooks). The CLI's `forward`/`stop` commands now send auth (Bearer token via `terusin set-token`, or Basic via legacy `terusin login`). **Breaking:** run `terusin set-token` before `terusin forward` if you previously used it unauthenticated.
- `forward_rules.signing_secret` is loaded from the DB for outbound signing but is never serialized to API responses (`#[serde(skip)]` on the field) — it must not leak via `GET /rules`.

## Auth

Three auth methods are accepted by `auth_middleware` (in order):

1. **Cookie JWT** — Google OAuth users. `terusin_session` http-only cookie, HS256, 7-day TTL, signed with `JWT_SECRET`.
2. **Bearer API token** — CLI / MCP. `Authorization: Bearer ts_<...>`. Tokens are opaque 256-bit random strings, stored sha256-hashed in `api_tokens` (indexable, revocable per-device). Generated directly from the dashboard's Settings → API Tokens page (see below).
3. **HTTP Basic** — legacy password logins. Still works as a fallback for existing configs.

**API key generation:** a signed-in dashboard user generates a `ts_<…>` key from Settings → API Tokens. The key is bound to that user and inherits their role (admin = full write, viewer = read-only). The cleartext key is shown **exactly once** in the UI; only its sha256 hash is stored. Copy it to the target device and run `terusin set-token ts_<…>` (stored in the **OS keychain** — macOS Keychain / Linux secret-service; config.toml fallback for headless), or export it as `TERUSIN_TOKEN` for the MCP server.

| Client | Env var | Storage |
|--------|---------|---------|
| CLI (`terusin set-token`) | `TERUSIN_TOKEN` (preferred) | OS keychain → config.toml → env |
| MCP (`apps/mcp`) | `TERUSIN_TOKEN` (preferred) | env only (config in AI client) |
| Legacy CLI (`terusin login`) | `TERUSIN_USER` + `TERUSIN_PASSWORD` | config.toml (plaintext) |
| Legacy MCP | `TERUSIN_USER` + `TERUSIN_PASS` | env |

Token management endpoints (all protected — require the calling user's session; any authenticated role may mint/list/revoke their own keys): `POST /api/auth/tokens` (mint a key, returns cleartext once), `GET /api/auth/tokens` (list own tokens), `DELETE /api/auth/tokens/{id}` (revoke). Token *use* is direct-to-backend (CLI/MCP bypass the `web` proxy, which injects its own Basic auth server-side).
