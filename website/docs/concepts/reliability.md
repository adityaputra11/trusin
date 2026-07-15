# Reliability model

Event dimulai sebagai `queued`. Worker mengambil ID dengan `BRPOP`, membaca payload dari Postgres, lalu mengirim HTTP POST.

- Network error mengubah status menjadi `retrying` dan menjadwalkan ID di sorted set `terusin:retry`.
- Delay retry adalah `10 × 2^retry_count` detik.
- Setelah batas `MAX_RETRIES`, status menjadi `failed`.
- Response 2xx menjadi `delivered` dan detail attempt dicatat.
- Admin dapat retry, acknowledge, atau delete event.

Delivery bersifat **at least once**: receiver harus idempotent. Gunakan event ID bisnis dari provider atau deduplication key pada target.
