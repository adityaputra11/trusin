# Local development

## Prasyarat

- Rust 1.85 atau lebih baru
- Node.js 18 atau lebih baru
- Docker dengan Compose
- `curl` untuk smoke test

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

Dashboard development tersedia di `http://localhost:5173`. Untuk menguji bundle production yang di-embed:

```bash
cd apps/frontend && npm run build && cd ../..
touch apps/web/src/main.rs
PORT=3012 BACKEND_URL=http://localhost:3011 cargo run --bin web
```

`touch` memaksa `rust-embed` membaca ulang isi `dist/`.

## Dokumentasi

```bash
cd website
npm install
npm start
```

Docusaurus tersedia di `http://localhost:3000`.
