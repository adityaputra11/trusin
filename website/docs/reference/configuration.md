# Configuration

| Variable | Default | Keterangan |
|---|---|---|
| `DATABASE_URL` | `postgres://localhost:5432/terusin` | Koneksi Postgres |
| `REDIS_URL` | `redis://127.0.0.1:6379` | Koneksi Redis |
| `PORT` | backend `3001` | Port process |
| `BACKEND_URL` | `http://localhost:$BACKEND_PORT` | Upstream untuk web |
| `AUTH_USERNAME` | `admin` | User Basic seed legacy |
| `AUTH_PASSWORD` | `change-me-in-production` | Password Basic seed legacy |
| `JWT_SECRET` | - | Secret cookie JWT; wajib kuat di production |
| `GOOGLE_CLIENT_ID` | - | Client ID Google OAuth |
| `GOOGLE_CLIENT_SECRET` | - | Client secret Google OAuth |
| `APP_URL` | `http://localhost:5173` | Canonical dashboard URL; determines the default OAuth callback |
| `OAUTH_REDIRECT_URI` | `$APP_URL/api/auth/callback/google` | Optional explicit callback registered in Google |
| `FRONTEND_URL` | `APP_URL` | Optional dashboard URL after OAuth completes |
| `DEFAULT_TARGET_URL` | kosong | Fallback target |
| `ALLOW_PUBLIC_TARGET_OVERRIDE` | `false` | Opt-in legacy `X-Target-Url` pada public ingest; tetap tunduk pada URL policy |
| `ALLOW_PRIVATE_TARGETS` | `false` | Izinkan target loopback/private untuk local development |
| `DEFAULT_SIGNING_SECRET` | kosong | HMAC global outbound |
| `MAX_RETRIES` | `5` | Batas network retry |
| `WORKER_COUNT` | `4` | Worker delivery paralel |
| `WEB_URL` | localhost dev origins | Daftar origin CORS dipisah koma |
| `PUBLIC_URL` | `https://ingest.trusin.my.id` | URL yang ditampilkan dashboard |
| `TERUSIN_TOKEN` | - | Token CLI/MCP |
| `HOSTED_MODE` | `false` | Aktifkan entitlement dan quota paket hosted Free |
| `INGEST_CANONICAL_HOST` | `ingest.trusin.my.id` | Target CNAME untuk domain ingest customer |
| `PLATFORM_ADMIN_TOKEN` | - | Bearer secret satu-kali untuk bootstrap platform operator; jangan pernah diberikan ke browser |

Untuk production, gunakan HTTPS untuk `FRONTEND_URL`, secret manager untuk nilai sensitif, dan redirect URI publik yang sama persis dengan konfigurasi Google Cloud. Untuk instalasi hosted trusin, gunakan `https://app.trusin.my.id` sebagai `FRONTEND_URL` dan origin di `WEB_URL`, serta `https://ingest.trusin.my.id` sebagai `PUBLIC_URL`.

`HOSTED_MODE=false` (default) membuat instalasi self-hosted tetap tanpa quota. Saat `HOSTED_MODE=true`, paket Free membatasi 10.000 event diterima per bulan UTC, 1 domain aktif, 10 provider, 3 API key aktif, 3 user, dan retention event 7 hari.
