---
sidebar_position: 1
title: Welcome
---

# trusin

trusin is a self-hosted webhook relay for teams that need reliable delivery, audit history, an operations dashboard, CLI/TUI, MCP, and control over webhook data. It receives provider requests, stores events in Postgres, pushes event IDs to Redis, and delivers them to target endpoints with background workers.

## Quick start

```bash
docker compose up -d postgres redis
AUTH_USERNAME=admin AUTH_PASSWORD=change-me-in-production PORT=3011 cargo run --bin backend
```

In a second terminal:

```bash
cd apps/frontend && npm install && npm run build && cd ../..
AUTH_USERNAME=admin AUTH_PASSWORD=change-me-in-production \
  BACKEND_URL=http://localhost:3011 PORT=3012 cargo run --bin web
```

Open `http://localhost:3012` and sign in with the same credentials. Change the default password before exposing the service to the internet.

For production, enable Google OAuth and use API tokens for CLI/TUI/MCP. Read **Authentication and RBAC** before exposing an instance to the internet.

## Send your first webhook

```bash
curl -X POST http://localhost:3011/github/webhook \
  -H 'content-type: application/json' \
  -d '{"action":"ping"}'
```

The response includes an event `id` and the initial `queued` status. Monitor delivery from the dashboard or `GET /events/{id}`.
