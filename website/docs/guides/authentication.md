# Authentication and RBAC

trusin hosted runs isolated organizations in a shared database. Each user belongs to one organization and has either the `admin` or `viewer` role. Events, rules, audit logs, and API keys are always scoped to that organization.

Users with the server-side `platform_operator` flag can access `/platform` to view all tenants and subscriber data. This flag is never inherited by API keys, so a tenant token cannot access the control plane.

The backend accepts three authentication methods in this order:

1. The `terusin_session` JWT cookie created by Google OAuth.
2. A `ts_`-prefixed Bearer API token for the CLI and MCP server.
3. HTTP Basic authentication for legacy compatibility.

Read endpoints are available to both `admin` and `viewer` roles. Rule changes, event retry/acknowledge/delete actions, bulk actions, and default-target changes require an admin.

## Google OAuth

Enable Google sign-in with these environment variables:

```bash
GOOGLE_CLIENT_ID=...
GOOGLE_CLIENT_SECRET=...
APP_URL=https://app.your-terusin.example
# Optional explicit overrides for a split frontend/backend deployment.
OAUTH_REDIRECT_URI=https://app.your-terusin.example/api/auth/callback/google
FRONTEND_URL=https://app.your-terusin.example
JWT_SECRET='random-strong-secret'
RESEND_API_KEY=re_...
EMAIL_FROM='trusin <noreply@your-terusin.example>'
```

Register the same redirect URI in Google Cloud Console. When OAuth is enabled, the login page displays **Continue with Google**. A first Google sign-in creates a `free` workspace and makes that account its admin.

Free workspaces are limited to their owner and cannot invite users. Paid workspaces can invite `admin` or `viewer` users from **Users**. Invitations are delivered by Resend, expire after seven days, and can only be claimed by the Google account with the invited email address. In this version, an account belongs to one workspace only.

The OAuth callback is protected by a short-lived state token in Redis. Session cookies are `HttpOnly` and `SameSite=Lax`; they also become `Secure` when `FRONTEND_URL` uses HTTPS.

## API tokens

Create a token from **Settings → API Tokens**. The plaintext token is displayed only once; the server stores a SHA-256 hash. Tokens are organization-scoped and carry explicit scopes: `events:read`, `webhooks:send`, `rules:read`, `rules:write`, or `organization:manage`.

```bash
terusin set-token ts_your_token
# or
export TERUSIN_TOKEN=ts_your_token
```

Do not commit tokens, `JWT_SECRET`, signing secrets, or passwords. Use a secrets manager in production.

## Audit trail

trusin records important actions in **Activity**: login, invitation lifecycle events, token creation/revocation, role changes, rule create/update/delete event actions, retry/acknowledge/delete event actions, bulk actions, and default-target changes. Authenticated users can read the audit trail at `GET /api/audit`.
