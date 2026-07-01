# Architecture

## Overview

```
Webhook Provider (Midtrans, Stripe, etc.)
         │
         ▼ POST /{source}/webhook
    ┌─────────────────┐
    │    ngrok/Proxy   │  (public HTTPS, optional)
    └────────┬────────┘
             │
    ┌────────▼────────┐
    │   Backend:3011  │
    │                 │
    │  ┌─ handle_webhook ──┐
    │  │ • extract source  │
    │  │ • lookup rule     │
    │  │ • save to PG      │
    │  │ • LPUSH to Redis  │
    │  └───────────────────┘
    │                 │
    │  ┌── worker ────────┐
    │  │ • BRPOP queue     │
    │  │ • POST to target  │
    │  │ • success → PG    │
    │  │ • fail → ZADD     │
    │  └───────────────────┘
    │                 │
    │  ┌─ retry_worker ───┐
    │  │ • ZPOPMIN retry   │
    │  │ • check timestamp │
    │  │ • POST to target  │
    │  │ • exponential     │
    │  │   backoff         │
    │  └───────────────────┘
    └─────────────────────┘
             │
    ┌────────▼────────┐
    │   Redis :6379   │
    │  terusin:queue  │
    │  terusin:retry  │
    └─────────────────┘
             │
    ┌────────▼────────┐
    │  Postgres :5432 │
    │  webhook_events │
    │  forward_rules  │
    │  users          │
    └─────────────────┘
             │
    ┌────────▼────────┐
    │   Web:3012      │
    │  Dashboard UI   │
    │  (SSR axum)     │
    └─────────────────┘
```

## Data Flow

**Incoming webhook:**
1. Provider sends POST to `/{source}/webhook`
2. Backend extracts `source` from URL path
3. Checks `X-Webhook-Source` header (optional override)
4. Looks up `forward_rules` for matching source pattern
5. Determines `target_url`: header > rule match > `DEFAULT_TARGET_URL`
6. Saves event to `webhook_events` table
7. Pushes event UUID to Redis list `terusin:queue`

**Delivery:**
1. Worker blocks on `BRPOP terusin:queue` (timeout 5s)
2. Reads event from Postgres
3. POSTs body to `target_url`
4. On 2xx: updates status to `delivered`
5. On failure: increments `retry_count`, pushes to `terusin:retry` with `ZADD score = now + delay`

**Retry:**
1. Retry worker calls `ZPOPMIN terusin:retry`
2. If score > current time: pushes back and sleeps 1s
3. Otherwise: processes same as delivery
4. Exponential backoff: `delay = 10s × 2^attempt`

## Component Details

### Backend (apps/backend)
- Axum HTTP server on `:3011`
- Routes: `POST /{*source}`, `GET /health`
- Protected endpoints: `GET/POST /events`, `GET/POST/DELETE /rules`
- Two background workers: delivery + retry

### Web Dashboard (apps/web)
- Axum HTTP server on `:3012`
- Server-side rendered HTML with Tailwind CSS
- Proxies API calls to backend with Basic auth
- Pages: Dashboard, Event Detail, Providers, Hooks

### CLI (apps/tui)
- Clap-based CLI with subcommands
- Config stored in `~/.config/terusin/config.toml`
- `forward` auto-starts ngrok for remote backends

### MCP Server (apps/mcp)
- Stdio JSON-RPC server
- Tools: list_events, retry_event, send_webhook, health
- Authenticates with backend via Basic auth

## Database Schema

```sql
webhook_events (
    id UUID PK,
    source VARCHAR,
    headers JSONB,
    body JSONB,
    status VARCHAR (queued|delivered|failed|retrying),
    target_url TEXT,
    retry_count INT,
    max_retries INT,
    created_at TIMESTAMP
)

forward_rules (
    id UUID PK,
    name VARCHAR,
    source_pattern VARCHAR,
    target_url TEXT,
    method VARCHAR,
    headers JSONB,
    active BOOLEAN
)

users (
    id UUID PK,
    username VARCHAR UNIQUE,
    password_hash TEXT,
    role VARCHAR
)
```
