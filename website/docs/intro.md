---
sidebar_position: 1
title: Selamat datang
---

# trusin

trusin adalah webhook relay self-hosted untuk tim yang butuh delivery andal, audit trail, dashboard operasional, CLI/TUI, MCP, dan kontrol penuh atas data webhook. Ia menerima request dari provider, menyimpan event di Postgres, memasukkan ID event ke antrean Redis, lalu worker meneruskannya ke endpoint tujuan.

## Quick start

```bash
docker compose up -d postgres redis
AUTH_USERNAME=admin AUTH_PASSWORD=change-me-in-production PORT=3011 cargo run --bin backend
```

Di terminal lain:

```bash
cd apps/frontend && npm install && npm run build && cd ../..
AUTH_USERNAME=admin AUTH_PASSWORD=change-me-in-production \
  BACKEND_URL=http://localhost:3011 PORT=3012 cargo run --bin web
```

Buka `http://localhost:3012`, lalu login dengan kredensial yang sama. Ganti password default sebelum mengekspos service ke internet.

Untuk production, aktifkan Google OAuth dan gunakan API token untuk CLI/TUI/MCP. Lihat panduan **Authentication dan RBAC** sebelum membuka instance ke internet.

## Kirim webhook pertama

```bash
curl -X POST http://localhost:3011/github/webhook \
  -H 'content-type: application/json' \
  -d '{"action":"ping"}'
```

Respons berisi `id` event dan status awal `queued`. Pantau hasil delivery dari dashboard atau `GET /events/{id}`.
