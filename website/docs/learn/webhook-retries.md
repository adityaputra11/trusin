---
title: Webhook retries
description: "Learn how trusin retries failed webhooks, what retryable failures mean, and how to design idempotent receivers."
keywords: [webhook retries, exponential backoff, idempotent webhooks]
---

# Webhook retries

Webhook retries recover from temporary downstream failures without asking the provider to resend the original event. trusin records each attempt so an operator can distinguish a transient failure from a final failure.

## Retry behavior

For retryable network failures, trusin changes the event to `retrying` and schedules exponential backoff: `10 × 2^retry_count` seconds. After `MAX_RETRIES`, the event is marked `failed`. A successful 2xx response marks it `delivered`.

## Design receivers for retries

- Make processing idempotent: store and deduplicate the provider event ID.
- Return 2xx only when the event is safely accepted.
- Treat client-side validation failures as final; use an appropriate non-2xx response when the payload cannot be processed.
- Use the delivery timeline to inspect request and response details before a manual retry.

See the [reliability model](/docs/concepts/reliability) and [troubleshooting guide](/docs/operations/troubleshooting) for operational details.
