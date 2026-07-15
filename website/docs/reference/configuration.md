# Configuration

| Variable | Default | Description |
|---|---|---|
| `DATABASE_URL` | `postgres://localhost:5432/terusin` | Postgres connection |
| `REDIS_URL` | `redis://127.0.0.1:6379` | Redis connection |
| `PORT` | backend `3001` | Process port |
| `BACKEND_URL` | `http://localhost:$BACKEND_PORT` | Web upstream |
| `AUTH_USERNAME` | `admin` | Legacy Basic seed user |
| `AUTH_PASSWORD` | `change-me-in-production` | Legacy Basic seed password |
| `JWT_SECRET` | - | JWT cookie secret; use a strong production value |
| `GOOGLE_CLIENT_ID` | - | Google OAuth client ID |
| `GOOGLE_CLIENT_SECRET` | - | Google OAuth client secret |
| `APP_URL` | `http://localhost:5173` | Canonical dashboard URL; determines the default OAuth callback |
| `OAUTH_REDIRECT_URI` | `$APP_URL/api/auth/callback/google` | Optional explicit callback registered in Google |
| `FRONTEND_URL` | `APP_URL` | Optional dashboard URL after OAuth completes |
| `DEFAULT_TARGET_URL` | empty | Fallback target |
| `ALLOW_PUBLIC_TARGET_OVERRIDE` | `false` | Opt-in legacy `X-Target-Url` on public ingest; still subject to URL policy |
| `ALLOW_PRIVATE_TARGETS` | `false` | Allows loopback/private targets for local development |
| `DEFAULT_SIGNING_SECRET` | empty | Global outbound HMAC secret |
| `MAX_RETRIES` | `5` | Network retry limit |
| `WORKER_COUNT` | `4` | Parallel delivery workers |
| `WEB_URL` | localhost development origins | Comma-separated CORS origins |
| `PUBLIC_URL` | `https://ingest.trusin.my.id` | URL shown by the dashboard |
| `TERUSIN_TOKEN` | - | CLI/MCP token |
| `HOSTED_MODE` | `false` | Enables hosted Free-plan entitlements and quotas |
| `INGEST_CANONICAL_HOST` | `ingest.trusin.my.id` | CNAME target for customer ingest domains |
| `PLATFORM_ADMIN_TOKEN` | - | One-time bearer secret for platform-operator bootstrap; never send it to a browser |

For production, use HTTPS for `FRONTEND_URL`, a secret manager for sensitive values, and the exact public redirect URI registered in Google Cloud. For hosted trusin, use `https://app.trusin.my.id` as `FRONTEND_URL` and a `WEB_URL` origin, plus `https://ingest.trusin.my.id` as `PUBLIC_URL`.

`HOSTED_MODE=false` (default) leaves self-hosted installations without quotas. With `HOSTED_MODE=true`, the Free plan limits accepted events to 10,000 per UTC month, one active domain, ten providers, three active API keys, one user, and seven days of event retention.
