# Troubleshooting

## Backend gagal start

Pastikan Postgres dan Redis hidup, lalu periksa `DATABASE_URL` dan `REDIS_URL`.

```bash
docker compose ps
docker compose logs postgres redis
```

## Event tidak terkirim

- Pastikan target dapat diakses dari host/container backend; `localhost` di container menunjuk container itu sendiri.
- Periksa event detail dan `/events/{id}/attempts`.
- Pastikan target mengembalikan HTTP 2xx.
- Periksa `MAX_RETRIES` dan sorted set Redis jika status `retrying`.

## Dashboard menampilkan 401

Pastikan `AUTH_USERNAME` dan `AUTH_PASSWORD` pada process web sama dengan user backend yang di-seed. Untuk Vite dev, login browser dipakai langsung ke backend.

## Perubahan frontend tidak muncul di binary web

Build ulang frontend, lalu paksa recompilation resource embed:

```bash
cd apps/frontend && npm run build && cd ../..
touch apps/web/src/main.rs
cargo build --bin web
```

## Smoke test gagal

Baca `/tmp/terusin-e2e-backend.log`, `/tmp/terusin-e2e-web.log`, dan `/tmp/terusin-e2e-receiver.log`. Pastikan port test tidak sedang dipakai process lain.
