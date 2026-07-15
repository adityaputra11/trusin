---
title: Self-hosted webhook infrastructure
description: "Run trusin on infrastructure you control with Postgres, Redis, HTTPS, backups, and operational visibility."
keywords: [self-hosted webhooks, webhook infrastructure, Postgres, Redis]
---

# Self-hosted webhook infrastructure

Self-hosting a webhook delivery layer gives your team control over event data, network placement, retention, and recovery. trusin is designed to run with a Rust backend, Postgres for durable history, and Redis for delivery queues.

## Minimum production components

- **Postgres** stores events, users, routing rules, and delivery attempts.
- **Redis** buffers pending delivery and retry schedules.
- **HTTPS reverse proxy** exposes the dashboard, API, and public ingest endpoint.
- **Backups and monitoring** protect the event history and identify delivery incidents.

## Operating safely

Use a secret manager for credentials, restrict database and Redis network access, run migrations before declaring the service healthy, and configure alerting for failed events and queue growth. The [deployment guide](/docs/operations/deployment) includes the supported Ubuntu and Caddy layout.

For a local setup, start with [local development](/docs/guides/local-development).
