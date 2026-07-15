---
title: Webhook relay vs. queues
description: "Understand the difference between a webhook relay and a queue, and why reliable webhook delivery needs both."
keywords: [webhook relay vs queue, webhook queue, event delivery]
---

# Webhook relay vs. queues

A webhook relay is an HTTP-facing delivery service. It receives an incoming event, applies routing and security policy, sends it to a target, and records the outcome. A queue is a component that stores work until a worker can process it.

## The difference

| Capability | Webhook relay | Queue |
| --- | --- | --- |
| Receives HTTP from providers | Yes | Usually no |
| Routes by source or provider | Yes | No |
| Decouples provider latency | Often, using a queue | Yes |
| Delivers HTTP to a target | Yes | No |
| Records delivery attempts | Yes | No |

trusin combines a webhook relay with Postgres-backed event history and a Redis queue. That gives a provider a fast acknowledgment while workers deliver and retry asynchronously.

Use a queue alone when your producer and consumer already share a messaging protocol. Use a webhook relay when external providers speak HTTP and your team needs routing, signing, retries, and an operator-friendly timeline.
