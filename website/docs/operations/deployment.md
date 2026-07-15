# Production deployment

The hosted trusin deployment runs on Ubuntu with Caddy. GitHub Actions builds every artifact after a merge to `master`; the server receives the compiled release and never builds application code itself.

## Public URLs

- `https://trusin.my.id` — landing page.
- `https://app.trusin.my.id` — dashboard and Google OAuth callback.
- `https://api.trusin.my.id` — authenticated API for the CLI, MCP, and integrations.
- `https://ingest.trusin.my.id/{source}` — public webhook ingest endpoint.
- `https://docs.trusin.my.id` — documentation.

Create A (and, when applicable, AAAA) records for the root domain, `www`, `app`, `api`, `ingest`, and `docs` pointing to the Ubuntu server. Caddy obtains and renews HTTPS certificates after DNS resolves and ports 80 and 443 are reachable.

## GitHub Actions setup

The `Deploy trusin production` workflow runs on every push to `master`. Add these repository secrets before merging a release:

| Secret | Purpose |
| --- | --- |
| `SSH_HOST`, `SSH_USER`, `SSH_PORT`, `SSH_KEY` | Ubuntu SSH connection; `SSH_USER` is `ubuntu` and `SSH_PORT` defaults to `22`. |
| `DATABASE_URL` | Managed Postgres connection string, allowlisted for the server IP. |
| `AUTH_USERNAME`, `AUTH_PASSWORD`, `JWT_SECRET` | Legacy admin access and cookie-session signing. |
| `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET` | Google OAuth credentials. |
| `RESEND_API_KEY`, `EMAIL_FROM` | Invitation email delivery through Resend. |
| `PLATFORM_ADMIN_TOKEN` | One-time hosted-platform bootstrap token. |
| `REDIS_URL` | Optional override; defaults to the local Redis service. |
| `TURNSTILE_SITE_KEY`, `TURNSTILE_SECRET_KEY`, `DEFAULT_SIGNING_SECRET` | Optional captcha and outbound-signing configuration. |

Optional repository variables `GOOGLE_SITE_VERIFICATION` and `BING_SITE_VERIFICATION` add the respective search-engine verification meta tags to the landing page. They are intentionally variables, not runtime secrets.

Register `https://app.trusin.my.id/api/auth/callback/google` as the Google OAuth redirect URI. The workflow intentionally fails before deployment if any required production secret is missing.

## Ubuntu runtime

The installer keeps releases in `/opt/trusin/releases/<commit>` and atomically points `/opt/trusin/current` at the successful release. It also performs `git pull --ff-only origin master` in `~/terusin`, updates the static sites in `/var/www/trusin`, and starts:

- `trusin-backend` on `127.0.0.1:3011`.
- `trusin-web` on `127.0.0.1:3012`.

The runtime configuration is stored at `/etc/trusin/trusin.env`, readable only by the `terusin` system user. Caddy loads its dedicated site file at `/etc/caddy/sites-enabled/trusin.caddy`; the existing application routes are left unchanged.

Customer-owned ingest domains and dynamic TLS are deliberately deferred in v1. Send webhooks to the canonical `ingest.trusin.my.id` host instead.

## Release validation

Each deployment checks service state, local backend health, all public HTTPS URLs, and that the API rejects an unauthenticated session request. The backend runs SQLx migrations at startup and connects to both managed Postgres and local Redis before becoming healthy.

For a manual check after a release:

```bash
sudo systemctl status terusin-backend terusin-web
curl -fsS https://api.trusin.my.id/health
curl -i -X POST https://ingest.trusin.my.id/stripe \
  -H 'content-type: application/json' \
  --data '{"id":"evt_smoke_test"}'
```

## Self-hosted alternative

For a self-hosted development or single-server installation, build the frontend before compiling the web binary:

```bash
cd apps/frontend && npm ci && npm run build && cd ../..
cargo build --release --bin backend --bin web
```

Use HTTPS, a secret manager, Postgres backups, Redis persistence appropriate to your workload, health checks, and log monitoring. Do not expose Postgres or Redis directly to the internet.
