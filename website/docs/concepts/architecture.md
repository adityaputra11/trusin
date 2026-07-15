# Architecture

```mermaid
flowchart LR
  P[Webhook provider] --> B[Axum backend]
  B --> PG[(Postgres)]
  B --> R[(Redis queue)]
  R --> W[Delivery workers]
  W --> T[Target endpoint]
  W --> PG
  D[React dashboard] --> X[Web proxy]
  X --> B
  C[CLI / MCP] --> B
```

- **backend** menangani ingest, API, auth, queue, dan worker.
- **frontend** adalah React + Vite SPA.
- **web** meng-embed bundle SPA dan reverse-proxy path API ke backend.
- **tui** menyediakan CLI operasional.
- **mcp** menyediakan tool berbasis stdio JSON-RPC untuk AI client.
- **receiver** adalah target contoh untuk development.

Ingest publik tidak menunggu target selesai. Event disimpan dan diantrikan dahulu, sehingga latency provider tidak bergantung pada target.
