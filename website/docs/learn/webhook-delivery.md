---
title: Webhook delivery
description: "How webhook delivery works in trusin: receive an event, persist it, queue it, deliver it, and record the result."
keywords: [webhook delivery, webhook infrastructure, webhook relay]
---

# Webhook delivery

Webhook delivery is the path from a provider event to your target endpoint. A reliable delivery layer accepts the provider request quickly, records it durably, and performs downstream work independently.

## How trusin delivers an event

1. A provider sends `POST /{source}` to the public ingest endpoint.
2. trusin validates the route and saves the headers and payload in Postgres.
3. It adds the event ID to a Redis delivery queue.
4. A worker sends the payload to the configured target and records the HTTP result.

This separation prevents a slow or unavailable target from making the provider request wait. Inspect the event timeline in the dashboard or through the [API reference](/docs/reference/api).

## What to expect at the receiver

trusin provides **at-least-once** delivery. A receiver must accept duplicate deliveries safely by using a provider event ID or an application-level deduplication key. Return a 2xx response only after the event is accepted by your application.

Read [receiving and forwarding webhooks](/docs/guides/webhooks) to configure sources, providers, hooks, and signing.
