# Deployment

## Docker Compose

```bash
AUTH_USERNAME=admin AUTH_PASSWORD='strong-password' \
  docker compose -f docker-compose.prod.yml up -d --build
```

Sebelum deploy, build frontend agar bundle terbaru di-embed ke binary web:

```bash
cd apps/frontend && npm ci && npm run build && cd ../..
cargo build --release --bin backend --bin web --bin terusin --bin mcp
```

Gunakan TLS reverse proxy, secret manager, backup Postgres, persistence Redis yang sesuai kebutuhan, health checks, dan monitoring log/metrics. Jangan mengekspos Postgres atau Redis ke internet.

## Production checklist

- Set `JWT_SECRET`, `AUTH_PASSWORD`, `GOOGLE_CLIENT_SECRET`, dan token lain lewat secret manager.
- Aktifkan Google OAuth dengan `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET`, `OAUTH_REDIRECT_URI`, dan `FRONTEND_URL`.
- Pakai HTTPS di reverse proxy agar cookie session dikirim dengan flag `Secure`.
- Pastikan Postgres menyimpan migration terbaru, termasuk `audit_logs`.
- Buat user admin pertama dari Basic seed, lalu promote user Google dari dashboard **Users**.
- Generate API token dari dashboard untuk CLI, TUI, dan MCP; hindari Basic auth untuk perangkat baru.
- Pantau **Activity**, `/stats`, queue Redis, dan log worker setelah deploy.

## Build dokumentasi

```bash
cd website
npm ci
npm run build
```

Upload isi `website/build/` ke static hosting pilihan kamu.
