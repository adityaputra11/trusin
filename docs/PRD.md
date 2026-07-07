# PRD — Terusin

> **Versi:** 1.0
> **Status:** Draft
> **Author:** —

---

## 1. Ringkasan Eksekutif

Terusin adalah **webhook relay** self-hosted yang menerima webhook dari berbagai provider (Midtrans, Stripe, GitHub, Resend, dll.) dan meneruskannya ke endpoint HTTP yang dikonfigurasi pengguna. Dibangun dengan Rust untuk performa tinggi, latensi rendah, dan footprint kecil.

Filosofi: **"Set once, forward everywhere."** Pengguna cukup mengarahkan webhook provider ke satu URL Terusin, lalu Terusin yang mengatur routing ke berbagai target tujuan.

---

## 2. Masalah

| Problem | Dampak |
|---------|--------|
| Setiap provider webhook punya URL endpoint sendiri | Harus konfigurasi ulang tiap ganti target |
| Webhook丢了 (loss) tanpa notifikasi | Bisnis loss, order ga terproses |
| Debugging webhook sulit (dark log) | Susah cari tahu error di mana |
| Banyak provider kirim ke banyak service | Routing manual, rawan salah |
| Service pihak ketiga mati sementara | Webhook hilang permanen |
| Tidak ada audit trail | Compliance issue |

---

## 3. Target Audience

1. **Developer individu / solo founder** — butuh webhook receiver sederhana tanpa drama
2. **Startup kecil (2-50 dev)** — multiple services, satu pintu webhook
3. **SaaS / platform payment** — Midtrans, Stripe, Xendit routing
4. **CI/CD pipeline** — GitHub/GitLab webhook → trigger deploy
5. **Email/SMS gateway** — Resend, Twilio, SendGrid relay

---

## 4. User Stories

### 4.1 Basic Relay

- Sebagai developer, saya ingin mengarahkan webhook provider ke satu URL, dan Terusin meneruskannya ke service saya
- Sebagai developer, saya ingin melihat semua webhook yang masuk dalam satu dashboard real-time
- Sebagai developer, saya ingin melihat status delivery (success / failed / retrying) setiap event

### 4.2 Reliable Delivery

- Sebagai developer, saya ingin webhook otomatis di-retry kalau target saya down
- Sebagai developer, saya ingin konfigurasi max retries (default 5x)
- Sebagai developer, saya ingin webhook tetap terima walau target saya mati berjam-jam

### 4.3 Routing & Fan-out

- Sebagai developer, saya ingin routing webhook ke URL berbeda berdasarkan **source** (misal: Midtrans → payment service, GitHub → CI server)
- Sebagai developer, saya ingin **fan-out** — satu webhook dikirim ke multiple targets
- Sebagai developer, saya ingin custom HTTP method, headers, dan signing secret per rule

### 4.4 Observability

- Sebagai developer, saya ingin dashboard dengan statistik throughput, success rate, queue depth
- Sebagai developer, saya ingin melihat request/response headers dan body tiap delivery attempt
- Sebagai developer, saya ingin search event by source, status, date range, atau keyword body
- Sebagai developer, saya ingin SSE real-time stream untuk event baru

### 4.5 Auth & Multi-User

- Sebagai admin, saya ingin login dengan password atau Google OAuth
- Sebagai admin, saya ingin role viewer (read-only) dan admin (full access)
- Sebagai viewer, saya tidak bisa menghapus/meretry event atau mengubah konfigurasi

### 4.6 Developer Experience (CLI / MCP / API)

- Sebagai developer, saya ingin CLI untuk pair device, forward webhook ke localhost, lihat events
- Sebagai developer, saya ingin MCP server untuk integrasi AI coding assistant (Cursor, Claude, dll)
- Sebagai developer, saya ingin API token untuk automasi (CI/CD script, dll)
- Sebagai developer, saya ingin ngrok auto-start untuk webhook development lokal

### 4.7 Security

- Sebagai admin, saya ingin HMAC signing (SHA256) di setiap outbound webhook
- Sebagai admin, saya ingin per-rule signing secret yang berbeda-beda
- Sebagai admin, saya ingin pairing device via 6-digit code (Spotify-style), bukan sharing password
- Sebagai admin, saya ingin Turnstile captcha di halaman login

---

## 5. Functional Requirements

### F1: Webhook Ingest

| ID | Requirement | Priority |
|----|-------------|----------|
| F1.1 | Menerima POST request di `/{source}` | P0 |
| F1.2 | Menerima POST request di `/` (root, tanpa source) | P0 |
| F1.3 | Source dari first path segment atau header `X-Webhook-Source` | P0 |
| F1.4 | Target URL dari header `X-Target-Url` atau rule matching atau default target | P0 |
| F1.5 | Simpan event ke Postgres + push ke Redis queue | P0 |
| F1.6 | Rate limiting per-IP | P1 |

### F2: Delivery Engine

| ID | Requirement | Priority |
|----|-------------|----------|
| F2.1 | Worker pool (configurable, default 4) pop dari Redis queue | P0 |
| F2.2 | POST ke target_url dengan body + headers yang sesuai | P0 |
| F2.3 | Update status: queued → delivered / failed / retrying | P0 |
| F2.4 | Exponential backoff: 10s → 20s → 40s → ... → max_retries | P0 |
| F2.5 | Retry via Redis sorted set (timestamp-based scheduling) | P0 |
| F2.6 | Duplicate delivery prevention (in-process + DB-level) | P0 |
| F2.7 | Forward-to-rules: setelah main delivery, kirim ke semua rule matching | P0 |

### F3: Forward Rules

| ID | Requirement | Priority |
|----|-------------|----------|
| F3.1 | CRUD rules via API dan dashboard | P0 |
| F3.2 | Setiap rule punya: name, source_pattern, target_url, method, headers, active, signing_secret | P0 |
| F3.3 | Source pattern matching: wildcard `*` atau exact match | P0 |
| F3.4 | Active/inactive toggle | P0 |
| F3.5 | Signing secret tidak bocor ke API response | P0 |

### F4: Dashboard (SPA)

| ID | Requirement | Priority |
|----|-------------|----------|
| F4.1 | Login page (password + optional Google OAuth + optional Turnstile) | P0 |
| F4.2 | Event list with search, status filter, source filter, date range | P0 |
| F4.3 | Event detail: full headers, body, response, delivery attempts timeline | P0 |
| F4.4 | Bulk retry + bulk delete | P0 |
| F4.5 | SSE live indicator (ada event baru) | P1 |
| F4.6 | CRUD providers page (source → target mapping) | P0 |
| F4.7 | Hooks page (forwarding rules with active toggle) | P0 |
| F4.8 | Metrics page (stat cards, throughput chart, pie chart, top sources/targets) | P0 |
| F4.9 | Settings page (system status, device pairing, MCP setup, env reference) | P0 |
| F4.10 | Send webhook composer (custom body, source, target override) | P1 |
| F4.11 | Dark theme data-first design | P0 |
| F4.12 | Responsive layout (sidebar + topbar) | P1 |

### F5: Auth System

| ID | Requirement | Priority |
|----|-------------|----------|
| F5.1 | Password login (bcrypt) | P0 |
| F5.2 | Google OAuth login | P0 |
| F5.3 | JWT cookie session (7-day TTL) | P0 |
| F5.4 | Bearer API token (256-bit random, sha256 hash DB) | P0 |
| F5.5 | Dashboard API key generation (role-scoped `ts_` tokens, shown once) | P0 |
| F5.6 | HTTP Basic auth fallback | P1 |
| F5.7 | RBAC: admin + viewer roles | P0 |
| F5.8 | Token management (list, revoke) via Settings | P0 |
| F5.9 | Rate limiting: login 5/min, /me 30/min | P0 |

### F6: CLI (tui)

| ID | Requirement | Priority |
|----|-------------|----------|
| F6.1 | `terusin set-token` — save an API key (keychain → config fallback) | P0 |
| F6.2 | `terusin login` — legacy username/password | P1 |
| F6.3 | `terusin logout` — clear stored credentials | P1 |
| F6.4 | `terusin forward` — set default-target + auto-start ngrok | P0 |
| F6.5 | `terusin stop` — clear default-target | P1 |
| F6.6 | `terusin events` — list recent events | P1 |
| F6.7 | `terusin retry` — retry failed event | P1 |
| F6.8 | `terusin listen` — poll + forward to local port without ngrok | P1 |
| F6.9 | `terusin dashboard` — open web UI | P1 |
| F6.10 | `terusin status` — show config state | P1 |
| F6.11 | Token stored di OS keychain (macOS/Linux) | P0 |

### F7: MCP Server

| ID | Requirement | Priority |
|----|-------------|----------|
| F7.1 | stdio JSON-RPC 2.0 protocol | P0 |
| F7.2 | Tool: `list_events(limit?)` | P0 |
| F7.3 | Tool: `retry_event(id)` | P0 |
| F7.4 | Tool: `send_webhook(source?, target_url, body)` | P0 |
| F7.5 | Tool: `health()` | P0 |
| F7.6 | Auth via TERUSIN_TOKEN env | P0 |

### F8: Audit & Observability

| ID | Requirement | Priority |
|----|-------------|----------|
| F8.1 | Delivery attempts table dengan response status, headers, body, error, duration_ms | P0 |
| F8.2 | Metrics: 24h/7d/30d aggregates | P0 |
| F8.3 | Event stream (SSE) for real-time monitoring | P1 |
| F8.4 | Source listing distinct | P1 |

---

## 6. Non-Functional Requirements

### N1: Performance

| ID | Requirement | Target |
|----|-------------|--------|
| N1.1 | Throughput | > 3,000 req/s (baseline: 3,990 req/s) |
| N1.2 | Latency (avg) | < 10ms (baseline: 6.48ms) |
| N1.3 | Latency (p95) | < 15ms (baseline: 12.86ms) |
| N1.4 | Error rate | < 0.1% (baseline: 0%) |

### N2: Reliability

| ID | Requirement | Target |
|----|-------------|--------|
| N2.1 | Delivery guarantee | At-least-once delivery |
| N2.2 | Data persistence | Postgres (events survive restart) |
| N2.3 | Queue resilience | Redis (queue survive restart) |
| N2.4 | Startup health check | /health returns 200 |

### N3: Security

| ID | Requirement | Target |
|----|-------------|--------|
| N3.1 | Outbound signing | HMAC-SHA256 on all deliveries |
| N3.2 | Credential storage | bcrypt (password), sha256 (tokens) |
| N3.3 | CORS | Configurable origins, credentials allowed |
| N3.4 | Captcha | Cloudflare Turnstile optional |
| N3.5 | Cookie | HttpOnly, not exposed to JS |

### N4: Portability

| ID | Requirement | Target |
|----|-------------|--------|
| N4.1 | Deployment | Docker, bare-metal (systemd), Fly.io |
| N4.2 | Binary size | < 10MB per binary |
| N4.3 | Single binary | backend + web standalone tanpa interpreter |

---

## 7. Architecture

```
Provider (Midtrans/Stripe/GitHub/etc.)
    │
    ▼ POST /{source}
┌──────────────┐     ┌──────────┐     ┌──────────────┐
│   backend    │────▶│  Redis   │────▶│  Worker Pool │
│  (Axum HTTP) │     │  Queue   │     │  (BRPOP)     │
└──────┬───────┘     └──────────┘     └──────┬───────┘
       │                                     │
       ▼                                     ▼
┌──────────────┐                    ┌──────────────────┐
│   Postgres   │                    │  Target Service  │
│  (events,    │                    │  (user's server) │
│   rules,     │                    └──────────────────┘
│   users,     │
│   tokens)    │
└──────────────┘
       │
       ▼
┌──────────────┐     ┌──────────┐
│  web (SPA)   │◀───▶│ Frontend │
│  reverse     │     │  (React) │
│  proxy       │     └──────────┘
└──────────────┘
       │
       ▼
┌──────────────┐     ┌──────────┐
│  CLI (tui)   │◀───▶│  MCP     │
│  (Rust)      │     │  (stdio) │
└──────────────┘     └──────────┘
```

### Data Flow

1. Provider kirim POST ke `https://terusin.example.com/midtrans`
2. Backend extract source=`midtrans`, simpan event ke Postgres (`status=queued`)
3. Backend push event ID ke Redis list `terusin:queue`
4. Worker pop dari Redis, POST ke target_url
5. Sukses → update `status=delivered`, forward ke semua rule matching
6. Gagal → ZADD `terusin:retry` dengan score `now + 10s * 2^attempt`
7. Retry worker pop dari sorted set, deliver ulang
8. Kapasitas habis → `status=failed`

---

## 8. Tech Stack

| Layer | Technology |
|-------|-----------|
| Language | Rust |
| HTTP Framework | Axum |
| ORM / DB | SQLx (Postgres) |
| Queue | Redis (list + sorted set) |
| Frontend | React 18 + Vite + TypeScript |
| Styling | Tailwind CSS 4 + design tokens CSS |
| Charts | Recharts |
| CLI | Clap 4 (derive) |
| Auth | JWT (jsonwebtoken), bcrypt, Google OAuth |
| Rate Limiting | Governor |
| Captcha | Cloudflare Turnstile |
| Binary Embed | rust-embed |
| Container | Docker multi-stage (Alpine) |

---

## 9. Milestones / Roadmap

### v1.0 — Foundation (✅ shipped)
- [x] Webhook ingest (POST /{source})
- [x] Postgres persistence
- [x] Redis queue + worker pool
- [x] Exponential backoff retry
- [x] Forward rules (CRUD + matching)
- [x] HMAC signing
- [x] Password auth + Basic auth
- [x] React SPA dashboard (events, rules, metrics)
- [x] CLI (pair, forward, events, retry)
- [x] Docker + docker-compose
- [x] systemd deployment

### v1.1 — Auth & DX (✅ shipped)
- [x] Google OAuth
- [x] RBAC (admin / viewer)
- [x] API tokens + device pairing
- [x] MCP server (list_events, retry_event, send_webhook, health)
- [x] Delivery attempts table / timeline
- [x] Bulk retry + bulk delete
- [x] SSE event stream
- [x] Send webhook composer

### v1.2 — Observability (🔄 current)
- [ ] Metrics page with charts
- [ ] Event search + advanced filters
- [ ] Better error reporting
- [ ] Per-rule delivery stats

### v2.0 — Enterprise (🔮 planned)
- [ ] Multi-tenant (org/workspace)
- [ ] Webhook transformation (template body before forward)
- [ ] Rate limiting per source / per target
- [ ] Slack / Discord / Email notification on failure
- [ ] IP whitelist for webhook ingest
- [ ] Audit log
- [ ] 2FA / TOTP
- [ ] OpenTelemetry tracing

---

## 10. Success Metrics

| Metric | Target |
|--------|--------|
| Throughput | ≥ 3,000 req/s |
| Uptime | ≥ 99.9% |
| Events lost | 0 (at-least-once delivery) |
| Time-to-first-delivery | < 1s (median) |
| P95 delivery latency | < 5s |
| Binary size (each) | < 10MB |
| Docker image size | < 50MB |

---

## 11. Competitive Landscape

| Produk | Type | Kelebihan Terusin |
|--------|------|-------------------|
| Svix | Cloud / self-hosted | Open source, Rust perf, gratis |
| Hookdeck | Cloud | Self-hosted, CLI-native, MCP |
| ngrok | Tunnel | Webhook relay + dashboard built-in |
| Custom build (Express/Flask) | DIY | Performance, reliability, observability out of box |

---

## 12. Glossary

| Istilah | Definisi |
|---------|----------|
| **Webhook** | HTTP callback — POST request dari provider ke server |
| **Source** | Identifier provider (midtrans, stripe, github) |
| **Target** | URL tujuan forwarding |
| **Delivery Attempt** | Satu kali percobaan HTTP POST ke target |
| **Forward Rule** | Mapping source → target dengan method, headers, signing |
| **Fan-out** | Satu webhook dikirim ke banyak target |
| **Pairing** | Proses otentikasi device via 6-digit code |
| **MCP** | Model Context Protocol — standard AI tool interface |
| **SSE** | Server-Sent Events — real-time streaming ke browser |
