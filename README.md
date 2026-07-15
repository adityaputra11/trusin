<div align="center">
  <h1>trusin</h1>
  <img width="112" height="112" alt="trusin" src="apps/frontend/public/icon-trusin.png" />
  <p><strong>trusin</strong> (Indonesian: <em>"forward it"</em>) — self-hosted enterprise webhook delivery infrastructure.</p>
  <p>Receive webhooks from any provider (Midtrans, Stripe, Resend, GitHub, etc.)<br>
  and forward them to your local services or any HTTP endpoint.</p>
  <p>Built with Rust. Powered by Redis queue & Postgres.<br>
  3,990 req/s throughput. Zero errors.</p>
  <p>
    <a href="website/docs/intro.md">Documentation</a> ·
    <a href="ARCHITECTURE.md">Architecture</a> ·
    <a href="LICENSE">Apache 2.0</a> ·
    <a href="https://github.com/adityaputra11/trusin">GitHub</a>
  </p>
</div>

## Features

- **Dynamic routing** — path `/midtrans/webhook` → source `midtrans`, `/stripe/webhook` → source `stripe`, no config
- **Redis queue** — reliable delivery with `BRPOP`/`ZADD` retry
- **Exponential backoff** — `10s × 2^attempt`, configurable max retries
- **Dashboard** — operations console with search, filters, metrics, audit activity, users, tokens, providers, hooks
- **CLI/TUI** — `trusin interactive`, `trusin forward --port 3000`, `trusin events`, `trusin retry`
- **Auth** — Google OAuth sessions, Bearer API tokens, RBAC admin/viewer, Basic auth fallback
- **Audit trail** — login, token, user role, rule, event, bulk action, and config changes
- **Self-hosted** — Docker Compose, or bare metal with Postgres + Redis
- **ngrok optional** — auto-starts tunnel when backend is remote
- **MCP server** — AI agent integration via stdio JSON-RPC

## Requirements

| Resource | Minimum | Recommended |
|----------|---------|-------------|
| CPU | 1 core | 2 cores |
| RAM | 256 MB | 512 MB |
| Storage | 100 MB | 1 GB |
| Postgres | 14+ | 16+ |
| Redis | 6+ | 7+ |
| Rust | 1.85+ | latest |

**Binary sizes (release):** backend 8.7MB, web 7.6MB, CLI 4.4MB, MCP 3.5MB

## Quick start

```sh
docker compose up -d postgres redis

# Backend
AUTH_USERNAME=admin AUTH_PASSWORD=change-me-in-production PORT=3011 cargo run --bin backend

# Dashboard (another terminal)
AUTH_USERNAME=admin AUTH_PASSWORD=change-me-in-production PORT=3012 BACKEND_URL=http://localhost:3011 cargo run --bin web

open http://localhost:3012
# Login: admin / change-me-in-production
```

## CLI

Install the latest CLI release on macOS or Linux:

```sh
curl -fsSL https://download.trusin.my.id/install.sh | sh
trusin set-token ts_your_token
```

The installer automatically selects the correct Apple Silicon, Intel, x86_64,
or ARM64 binary and verifies its SHA-256 checksum. To pin a release, run
`curl -fsSL https://download.trusin.my.id/install.sh | TERUSIN_VERSION=v0.1.0 sh`;
use `TERUSIN_INSTALL=$HOME/.local/bin` in the same position to install without
`sudo`.

Build from source instead:

```sh
alias trusin='cargo run --bin trusin --'

trusin status             # check forwarding state
trusin interactive        # full-screen terminal dashboard
trusin forward --port 3000   # forward webhooks to localhost:3000
trusin events -l 10       # list recent events
trusin retry <uuid>       # retry failed delivery
```

## Providers

Set webhook URL in your provider dashboard:

```
https://your-host.com/stripe/webhook      → source = "stripe"
https://your-host.com/github/webhook      → source = "github"
```

Add providers with target URLs via dashboard or `trusin` CLI. Webhooks are forwarded automatically.

## MCP for AI agents

```json
{
  "mcpServers": {
    "trusin": {
      "command": "cargo",
      "args": ["run", "--bin", "mcp"],
      "dir": "/path/to/trusin",
      "env": {
        "TERUSIN_URL": "https://api.trusin.my.id",
        "TERUSIN_TOKEN": "ts_..."
      }
    }
  }
}
```

Create the scoped API token in **Settings → Developer**. The MCP server uses
local stdio and exposes relay health, metrics, event inspection, webhook sends,
and event retries.

## Benchmark (k6)

```
100 concurrent users, 4 workers, sequential processing
```

| Metric | Value |
|--------|-------|
| Throughput | **3,990 req/s** |
| Total requests | 79,823 |
| Error rate | **0%** |
| Avg latency | 6.48ms |
| p95 latency | 12.86ms |

```
✓ http_req_duration p(95)<5000ms  → 12.86ms
✓ http_req_failed rate<0.01      → 0.00%
```

## Architecture

See [ARCHITECTURE.md](ARCHITECTURE.md) for full details.

## Documentation

The Docusaurus documentation site lives in [`website/`](website/):

```sh
cd website
npm install
npm start
```

Run the reproducible end-to-end smoke test with `./scripts/e2e-smoke.sh`.

## Stack

- **Backend:** Rust, Axum, SQLx (Postgres), redis-rs
- **Dashboard:** React, Vite, TypeScript; embedded and proxied by Rust/Axum
- **CLI:** Rust, Clap, reqwest
- **MCP:** Rust, Stdio JSON-RPC
- **Infra:** Postgres, Redis, Docker

## License

Apache 2.0
