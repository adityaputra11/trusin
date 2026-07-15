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

Test memakai Compose project terisolasi dan port `55432`, `56379`, `3311`, `3312`, serta `3330`. Ia memverifikasi:

1. Postgres dan Redis sehat.
2. Backend dan web proxy sehat.
3. SPA production dapat disajikan.
4. Webhook masuk melalui web proxy.
5. Event tersimpan, masuk queue, dan diterima example receiver.
6. Status event dan delivery attempt menjadi `delivered`.

Container dan volume test dibersihkan otomatis; log process tersimpan sementara di `/tmp/terusin-e2e-*.log`.
