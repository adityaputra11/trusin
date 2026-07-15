# Reliability model

An event starts as `queued`. A worker pops its ID with `BRPOP`, reads the payload from Postgres, and sends an HTTP POST.

- A network error changes the status to `retrying` and schedules the ID in the `terusin:retry` sorted set.
- Retry delay is `10 × 2^retry_count` seconds.
- After `MAX_RETRIES`, the status becomes `failed`.
- A 2xx response becomes `delivered` and records an attempt.
- Admins can retry, acknowledge, or delete an event.

Delivery is **at least once**: receivers must be idempotent. Use the provider business event ID or a deduplication key at the target.
