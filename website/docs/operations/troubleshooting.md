# Troubleshooting

## Backend does not start

Ensure Postgres and Redis are running, then inspect `DATABASE_URL` and `REDIS_URL`.

```bash
docker compose ps
docker compose logs postgres redis
```

## An event is not delivered

- Ensure the target is reachable from the backend host/container; `localhost` inside a container points to that container.
- Inspect event details and `/events/{id}/attempts`.
- Ensure the target returns HTTP 2xx.
- Inspect `MAX_RETRIES` and the Redis sorted set when status is `retrying`.

## Dashboard shows 401

Ensure `AUTH_USERNAME` and `AUTH_PASSWORD` in the web process match the seeded backend user. Vite development sends browser credentials directly to the backend.

## Frontend changes do not appear in the web binary

Build ulang frontend, lalu paksa recompilation resource embed:

```bash
cd apps/frontend && npm run build && cd ../..
touch apps/web/src/main.rs
cargo build --bin web
```

## Smoke test fails

Read `/tmp/terusin-e2e-backend.log`, `/tmp/terusin-e2e-web.log`, and `/tmp/terusin-e2e-receiver.log`. Ensure another process is not using the test ports.
