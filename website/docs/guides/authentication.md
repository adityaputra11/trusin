# Authentication and RBAC

trusin hosted runs isolated organizations in a shared database. Each user belongs to one organization and has either the `admin` or `viewer` role. Events, rules, audit logs, and API keys are always scoped to that organization.

Users with the server-side `platform_operator` flag can access `/platform` to view all tenants and subscriber data. This flag is never inherited by API keys, so a tenant token cannot access the control plane.

The backend accepts three authentication methods in this order:

1. The `terusin_session` JWT cookie created by Google OAuth.
2. A `ts_`-prefixed Bearer API token for the CLI and MCP server.
3. HTTP Basic authentication for legacy compatibility.

The `trusin` CLI is token-only: it does not provide password login. Running
`trusin` for the first time asks for an API token and stores it locally; browser
sign-in remains available through Google, GitHub, or the configured password flow.

Read endpoints are available to both `admin` and `viewer` roles. Rule changes, event retry/acknowledge/delete actions, bulk actions, and default-target changes require an admin.

## Browser sign-in and workspace creation

Enable Google sign-in with these environment variables:

```bash
GOOGLE_CLIENT_ID=...
GOOGLE_CLIENT_SECRET=...
GITHUB_CLIENT_ID=...
GITHUB_CLIENT_SECRET=...
APP_URL=https://app.your-terusin.example
# Optional explicit overrides for a split frontend/backend deployment.
OAUTH_REDIRECT_URI=https://app.your-terusin.example/api/auth/callback/google
FRONTEND_URL=https://app.your-terusin.example
JWT_SECRET='random-strong-secret'
RESEND_API_KEY=re_...
EMAIL_FROM='trusin <noreply@your-terusin.example>'
```

Register `https://app.your-terusin.example/api/auth/callback/google` in Google Cloud Console and `https://app.your-terusin.example/api/auth/callback/github` in the GitHub OAuth App. When configured, the login page displays the matching provider. A first Google or GitHub sign-in creates a `free` workspace and makes that account its admin; there is no separate password registration flow.

Free workspaces are limited to their owner and cannot invite users. Paid workspaces can invite `admin` or `viewer` users from **Users**. Invitations are delivered by Resend, expire after seven days, and can only be claimed by the Google account with the invited email address. In this version, an account belongs to one workspace only.

The OAuth callback is protected by a short-lived state token in Redis. Session cookies are `HttpOnly` and `SameSite=Lax`; they also become `Secure` when `FRONTEND_URL` uses HTTPS.

## Cloudflare Turnstile

When `TURNSTILE_SECRET_KEY` is set, Turnstile is required before Google, GitHub, or password sign-in can start. Create a Managed widget in Cloudflare Turnstile, allow the dashboard hostname (for example `app.your-terusin.example`), then set the public Site Key in the frontend build as `VITE_TURNSTILE_SITE_KEY` and the matching Secret Key as `TURNSTILE_SECRET_KEY` on the backend. Turnstile does not require moving the domain DNS to Cloudflare.

## API tokens

Create a token from **Settings → Developer → API Tokens**. The plaintext token is displayed only once; the server stores a SHA-256 hash. Tokens are organization-scoped and carry explicit scopes: `events:read`, `webhooks:send`, `rules:read`, `rules:write`, or `organization:manage`.

```bash
trusin
# paste ts_your_token when prompted

# Or configure a device without the interactive prompt:
trusin set-token ts_your_token

# Or provide a temporary token for automation:
export TERUSIN_TOKEN=ts_your_token
```

Do not commit tokens, `JWT_SECRET`, signing secrets, or passwords. Use a secrets manager in production.

## Audit trail

trusin records important actions in **Activity**: login, invitation lifecycle events, token creation/revocation, role changes, rule create/update/delete event actions, retry/acknowledge/delete event actions, bulk actions, and default-target changes. Authenticated users can read the audit trail at `GET /api/audit`.
