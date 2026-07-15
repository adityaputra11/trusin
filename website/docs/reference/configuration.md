# Configuration

| Variable | Default | Keterangan |
|---|---|---|
| `DATABASE_URL` | `postgres://localhost:5432/terusin` | Koneksi Postgres |
| `REDIS_URL` | `redis://127.0.0.1:6379` | Koneksi Redis |
| `PORT` | backend `3001` | Port process |
| `BACKEND_URL` | `http://localhost:$BACKEND_PORT` | Upstream untuk web |
| `AUTH_USERNAME` | `admin` | User Basic seed/proxy |
| `AUTH_PASSWORD` | `change-me-in-production` | Password Basic seed/proxy |
| `JWT_SECRET` | - | Secret cookie JWT; wajib kuat di production |
| `GOOGLE_CLIENT_ID` | - | Client ID Google OAuth |
| `GOOGLE_CLIENT_SECRET` | - | Client secret Google OAuth |
| `OAUTH_REDIRECT_URI` | localhost callback | Callback OAuth yang didaftarkan di Google |
| `FRONTEND_URL` | `http://localhost:5173` | URL dashboard setelah OAuth berhasil |
| `DEFAULT_TARGET_URL` | kosong | Fallback target |
| `DEFAULT_SIGNING_SECRET` | kosong | HMAC global outbound |
| `MAX_RETRIES` | `5` | Batas network retry |
| `WORKER_COUNT` | `4` | Worker delivery paralel |
| `WEB_URL` | localhost dev origins | Daftar origin CORS dipisah koma |
| `PUBLIC_URL` | `https://terusin-dev.my.id` | URL yang ditampilkan dashboard |
| `TERUSIN_TOKEN` | - | Token CLI/MCP |

Untuk production, gunakan HTTPS untuk `FRONTEND_URL`, secret manager untuk nilai sensitif, dan redirect URI publik yang sama persis dengan konfigurasi Google Cloud.
