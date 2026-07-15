# Authentication dan RBAC

Terusin v1 memakai model **single workspace**. Semua user berada di satu instance yang sama, dengan role `admin` atau `viewer`.

Backend menerima tiga metode auth, sesuai urutan evaluasi:

1. Cookie JWT `terusin_session` dari Google OAuth.
2. Bearer API token berawalan `ts_` untuk CLI dan MCP.
3. HTTP Basic untuk kompatibilitas legacy.

Endpoint read menerima role `admin` dan `viewer`. Mutasi rule, retry/ack/delete event, bulk action, dan perubahan default target hanya untuk admin.

## Google OAuth

Aktifkan Google login dengan environment variable berikut:

```bash
GOOGLE_CLIENT_ID=...
GOOGLE_CLIENT_SECRET=...
OAUTH_REDIRECT_URI=https://your-terusin.example/api/auth/callback/google
FRONTEND_URL=https://your-terusin.example
JWT_SECRET='random-strong-secret'
```

Daftarkan redirect URI yang sama di Google Cloud Console. Saat OAuth aktif, halaman login menampilkan **Continue with Google**. User Google baru dibuat sebagai `viewer`; admin dapat menaikkan role dari dashboard **Users**.

OAuth callback divalidasi dengan state token berumur pendek di Redis. Cookie session bersifat `HttpOnly`, `SameSite=Lax`, dan `Secure` saat `FRONTEND_URL` memakai HTTPS.

## API token

Buat token dari **Settings → API Tokens**. Cleartext hanya ditampilkan sekali; server menyimpan hash SHA-256.

```bash
terusin set-token ts_your_token
# atau
export TERUSIN_TOKEN=ts_your_token
```

Jangan commit token, `JWT_SECRET`, signing secret, atau password. Gunakan secret manager di production.

## Audit trail

Terusin mencatat aksi penting ke **Activity**: login, token create/revoke, perubahan role, rule create/update/delete, retry/ack/delete event, bulk action, dan perubahan default target. Endpoint audit read-only tersedia di `GET /api/audit` untuk user authenticated.
