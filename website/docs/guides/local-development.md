# Local development

## Prerequisites

- Rust 1.85 atau lebih baru
- Node.js 18 atau lebih baru
- Docker with Compose
- `curl` for smoke tests

## Development loop

```bash
docker compose up -d postgres redis
PORT=3011 cargo run --bin backend
```

Jalankan Vite di terminal kedua. Vite mem-proxy API ke backend port 3011.

```bash
cd apps/frontend
npm install
npm run dev
```

The development dashboard is available at `http://localhost:5173`. To test the embedded production bundle:

```bash
cd apps/frontend && npm run build && cd ../..
touch apps/web/src/main.rs
PORT=3012 BACKEND_URL=http://localhost:3011 cargo run --bin web
```

`touch` forces `rust-embed` to read the `dist/` contents again.

## Documentation

```bash
cd website
npm install
npm start
```

Docusaurus is available at `http://localhost:3000`.
