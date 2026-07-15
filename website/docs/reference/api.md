# API reference

## Public

| Method | Path | Purpose |
|---|---|---|
| GET | `/health` | Health probe |
| POST | `/i/{ingest_key}/{source}` | Hosted canonical webhook ingest (copy the workspace URL from Providers) |
| POST | `/{source}` | Custom-domain or self-hosted webhook ingest |
| GET | `/config/endpoint` | Public server endpoint (not a tenant webhook URL) |
| GET | `/config/oauth` | Google OAuth status |
| GET | `/api/auth/google` | Redirect to Google OAuth |
| GET | `/api/auth/callback/google` | Callback Google OAuth |
| GET | `/api/auth/me` | Probe session/token/basic auth |
| POST | `/api/auth/login` | Login password |
| POST | `/api/auth/logout` | Logout |

## Authenticated endpoints

| Method | Path | Role |
|---|---|---|
| GET | `/events`, `/events/{id}` | viewer+ |
| GET | `/events/{id}/attempts` | viewer+ |
| GET | `/events/sources`, `/events/stream` | viewer+ |
| GET | `/rules`, `/stats` | viewer+ |
| POST/PATCH/DELETE | `/rules...` | admin |
| POST | `/events/{id}/retry`, `/events/{id}/ack` | admin |
| DELETE | `/events/{id}` | admin |
| POST | `/events/bulk/retry`, `/events/bulk/delete` | admin |
| POST | `/config/default-target` | admin |
| POST | `/api/send` | admin |
| GET | `/api/organization` | viewer+ |
| GET/POST/DELETE | `/api/domains...` | admin + `organization:manage` |
| GET/POST/DELETE | `/api/api-keys...` | admin + `organization:manage` |
| GET | `/api/audit` | viewer+ |
| GET | `/api/users` | admin |
| PATCH | `/api/users/{id}/role` | admin |
| GET/POST/DELETE | `/api/auth/tokens...` | own tokens |

Use `Authorization: Bearer ts_...` or Basic auth. API keys can carry `events:read`, `webhooks:send`, `rules:read`, `rules:write`, and `organization:manage` scopes. Event listing supports `search`, `status`, `source`, `from`, `to`, `page`, and `per_page` (maximum 200).

## Platform operator provisioning

Bootstrap the first operator with `POST /api/platform/bootstrap/operator` and `Authorization: Bearer $PLATFORM_ADMIN_TOKEN`:

```json
{ "username": "admin" }
```

This endpoint only works while no operator exists. After bootstrap, sign in normally as the operator and use the internal `/platform` dashboard or these session-protected APIs:

- `GET /api/platform/overview`
- `GET /api/platform/organizations`
- `GET /api/platform/organizations/{id}`
- `POST /api/platform/organizations`
- `PATCH /api/platform/organizations/{id}/subscription`

Hosted organizations cannot be created by a tenant admin. A platform operator creates an organization and its initial admin, plus subscriber and billing-contact data:

```json
{
  "name": "Acme",
  "slug": "acme",
  "username": "acme-admin",
  "password": "a-long-initial-password",
  "email": "admin@acme.example",
  "subscriber_name": "Acme Inc.",
  "billing_contact_name": "Jane Doe",
  "billing_contact_email": "billing@acme.example"
}
```

## Custom ingest domains

An admin creates a domain through `POST /api/domains`, then adds CNAME `<domain>` to `INGEST_CANONICAL_HOST` and TXT `_terusin-verification.<domain>` with the API-returned token. `POST /api/domains/{id}/verify` activates the domain only when both records are valid. Only an `active` domain accepts webhooks.

### Send webhook

`POST /api/send` is available only to admins. Select an active provider with `provider_id`, or send manually with `source` and `target_url`. `target_url` may be empty in manual mode when a default target is configured.

```json
{
  "provider_id": "uuid-provider",
  "body": { "event": "payment.success" }
}
```

Manual targets must use `http` or `https`, must not include URL credentials, and must pass the target network policy. `X-Target-Url` is disabled by default on public ingest.

## Audit entry

`GET /api/audit?page=1&per_page=25` returns:

```json
{
  "entries": [
    {
      "id": "uuid",
      "actor_user_id": "uuid",
      "actor_email": "admin@example.com",
      "action": "rule.updated",
      "resource_type": "rule",
      "resource_id": "uuid",
      "metadata": {},
      "created_at": "2026-07-15T10:00:00"
    }
  ],
  "total": 1,
  "page": 1,
  "per_page": 25,
  "pages": 1
}
```
