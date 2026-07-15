# API reference

## Public

| Method | Path | Fungsi |
|---|---|---|
| GET | `/health` | Health probe |
| POST | `/` atau `/{source}` | Webhook ingest |
| GET | `/config/endpoint` | Public/ngrok endpoint |
| GET | `/config/oauth` | Status Google OAuth |
| GET | `/api/auth/google` | Redirect ke Google OAuth |
| GET | `/api/auth/callback/google` | Callback Google OAuth |
| GET | `/api/auth/me` | Probe session/token/basic auth |
| POST | `/api/auth/login` | Login password |
| POST | `/api/auth/logout` | Logout |

## Authenticated

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

Gunakan `Authorization: Bearer ts_...` atau Basic auth. API key dapat membawa scope `events:read`, `webhooks:send`, `rules:read`, `rules:write`, dan `organization:manage`. Event listing mendukung `search`, `status`, `source`, `from`, `to`, `page`, dan `per_page` (maksimum 200).

## Platform operator provisioning

Bootstrap user operator pertama dengan `POST /api/platform/bootstrap/operator` dan `Authorization: Bearer $PLATFORM_ADMIN_TOKEN`:

```json
{ "username": "admin" }
```

Endpoint ini hanya berjalan selama belum ada operator. Setelah bootstrap, login normal sebagai operator lalu gunakan dashboard internal `/platform` atau API session-protected berikut:

- `GET /api/platform/overview`
- `GET /api/platform/organizations`
- `GET /api/platform/organizations/{id}`
- `POST /api/platform/organizations`
- `PATCH /api/platform/organizations/{id}/subscription`

Organisasi hosted tidak dapat dibuat oleh tenant admin. Platform operator membuat organisasi dan admin awal, beserta data subscriber dan billing contact:

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

Admin membuat domain melalui `POST /api/domains`, lalu menambahkan CNAME `<domain>` ke `INGEST_CANONICAL_HOST` dan TXT `_terusin-verification.<domain>` dengan token yang dikembalikan API. `POST /api/domains/{id}/verify` mengaktifkan domain hanya jika kedua record valid. Hanya domain berstatus `active` menerima webhook.

### Send webhook

`POST /api/send` hanya tersedia untuk admin. Pilih provider aktif dengan `provider_id`, atau kirim secara manual dengan `source` dan `target_url`. `target_url` boleh dikosongkan pada mode manual jika default target sudah dikonfigurasi.

```json
{
  "provider_id": "uuid-provider",
  "body": { "event": "payment.success" }
}
```

Target manual harus menggunakan `http` atau `https`, tidak boleh menyertakan credential di URL, dan harus memenuhi target network policy. `X-Target-Url` pada public ingest dinonaktifkan secara default.

## Audit entry

`GET /api/audit?page=1&per_page=25` mengembalikan:

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
