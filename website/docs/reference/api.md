# API reference

## Public

| Method | Path | Fungsi |
|---|---|---|
| GET | `/health` | Health probe |
| POST | `/` atau `/{source}` | Webhook ingest |
| GET | `/config/default-target` | Default target aktif |
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
| GET | `/api/audit` | viewer+ |
| GET | `/api/users` | admin |
| PATCH | `/api/users/{id}/role` | admin |
| GET/POST/DELETE | `/api/auth/tokens...` | own tokens |

Gunakan `Authorization: Bearer ts_...` atau Basic auth. Event listing mendukung `search`, `status`, `source`, `from`, `to`, `page`, dan `per_page` (maksimum 200).

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
