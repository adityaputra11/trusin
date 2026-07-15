# Testing

## Static checks

```bash
cargo test --workspace
cd apps/frontend && npm run build
cd ../../website && npm run build
```

## End-to-end smoke test

```bash
./scripts/e2e-smoke.sh
```

The test uses an isolated Compose project and ports `55432`, `56379`, `3311`, `3312`, and `3330`. It verifies:

1. Postgres and Redis are healthy.
2. The backend and web proxy are healthy.
3. The production SPA is served.
4. A webhook enters through the web proxy.
5. An event is stored, queued, and received by the example receiver.
6. The event and delivery attempt become `delivered`.

Test containers and volumes are removed automatically; process logs are temporarily kept in `/tmp/terusin-e2e-*.log`.
