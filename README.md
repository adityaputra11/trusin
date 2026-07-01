<div align="center">
  <h1>Terusin</h1>
  <p>Self-hosted webhook relay with queue, retry, and dashboard.</p>
  <p>
    <a href="ARCHITECTURE.md">Architecture</a> ·
    <a href="LICENSE">Apache 2.0</a>
  </p>
</div>

## Features

- **Dynamic routing** — path `/midtrans/webhook` → source `midtrans`, `/stripe/webhook` → source `stripe`, no config
- **Redis queue** — reliable delivery with `BRPOP`/`ZADD` retry
- **Exponential backoff** — `10s × 2^attempt`, configurable max retries
- **Dashboard** — search, filter, pagination, event detail, providers, hooks
- **CLI** — `terusin forward --port 3000`, `terusin events`, `terusin retry`
- **Auth** — Basic auth, users seeded from env vars
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
AUTH_USERNAME=admin AUTH_PASSWORD=terusin123 PORT=3011 cargo run --bin backend

# Dashboard (another terminal)
AUTH_USERNAME=admin AUTH_PASSWORD=terusin123 PORT=3012 BACKEND_URL=http://localhost:3011 cargo run --bin web

open http://localhost:3012
# Login: admin / terusin123
```

## CLI

```sh
alias terusin='cargo run --bin terusin --'

terusin login              # save credentials
terusin status             # check forwarding state
terusin forward --port 3000   # forward webhooks to localhost:3000
terusin events -l 10       # list recent events
terusin retry <uuid>       # retry failed delivery
```

## Providers

Set webhook URL in your provider dashboard:

```
https://your-host.com/stripe/webhook      → source = "stripe"
https://your-host.com/github/webhook      → source = "github"
```

Add providers with target URLs via dashboard or `terusin` CLI. Webhooks are forwarded automatically.

## MCP for AI agents

```json
{
  "mcpServers": {
    "terusin": {
      "command": "cargo",
      "args": ["run", "--bin", "mcp"],
      "dir": "/path/to/terusin"
    }
  }
}
```

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

## Stack

- **Backend:** Rust, Axum, SQLx (Postgres), redis-rs
- **Dashboard:** Rust, Axum, Tailwind CSS (SSR)
- **CLI:** Rust, Clap, reqwest
- **MCP:** Rust, Stdio JSON-RPC
- **Infra:** Postgres, Redis, Docker

## License

Apache 2.0
