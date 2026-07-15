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

## Hosted domain layout

Pisahkan marketing, dashboard, dan dokumentasi agar browser tidak mencampur session atau API dashboard dengan situs publik:

- `terusin-dev.my.id` — marketing landing page.
- `app.terusin-dev.my.id` — dashboard hosted dan callback OAuth.
- `docs.terusin-dev.my.id` — dokumentasi Docusaurus.

Domain customer hanya untuk ingest webhook: CNAME-kan domain tersebut ke `INGEST_CANONICAL_HOST`, lalu verifikasi TXT dari dashboard **Organization**. Jangan sajikan dashboard dari domain customer.

Set `PUBLIC_URL`, `FRONTEND_URL`, dan `WEB_URL` ke `https://app.terusin-dev.my.id`. Daftarkan `https://app.terusin-dev.my.id/api/auth/callback/google` sebagai redirect URI Google OAuth. Reverse proxy `web` harus meneruskan `Host`, `Authorization`, dan cookie browser ke backend; jangan lagi menyuntikkan kredensial admin global.

## Production checklist

- Set `JWT_SECRET`, `AUTH_PASSWORD`, `GOOGLE_CLIENT_SECRET`, dan token lain lewat secret manager.
- Aktifkan Google OAuth dengan `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET`, `OAUTH_REDIRECT_URI`, dan `FRONTEND_URL`.
- Pakai HTTPS di reverse proxy agar cookie session dikirim dengan flag `Secure`.
- Pastikan Postgres menyimpan migration terbaru, termasuk `audit_logs`.
- Provision organisasi hosted dengan `PLATFORM_ADMIN_TOKEN` dan admin awal; akun Google harus diprovision sebelum login pertama.
- Set `HOSTED_MODE=true` dan `INGEST_CANONICAL_HOST` hanya pada platform hosted. Biarkan `HOSTED_MODE=false` untuk self-hosted tanpa quota.
- Generate API token dari dashboard untuk CLI, TUI, dan MCP; hindari Basic auth untuk perangkat baru.
- Pantau **Activity**, `/stats`, queue Redis, dan log worker setelah deploy.

## Build dokumentasi

```bash
cd website
npm ci
npm run build
```

Upload isi `website/build/` ke static hosting pilihan kamu.
