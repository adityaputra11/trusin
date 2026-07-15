# Receiving and forwarding webhooks

The source is taken from the first path segment. `/stripe/webhook` becomes `stripe`; the `X-Webhook-Source` header can override it.

The public ingest target is selected in this order:

1. An active Provider matching the source.
2. `DEFAULT_TARGET_URL` or the default target configured by an admin.

`X-Target-Url` on public ingest is rejected by default to prevent arbitrary forwarding and SSRF exposure. The legacy override can only be explicitly enabled with `ALLOW_PUBLIC_TARGET_OVERRIDE=true` and must still pass the server URL policy.

```bash
curl -X POST https://your-terusin.example/stripe/webhook \
  -H 'content-type: application/json' \
  -d '{"type":"payment_intent.succeeded"}'
```

## Sending from the dashboard

Admins can open the **Send** page and choose an active Provider or **Custom / manual**. Provider mode uses the selected rule's source and target. Custom mode accepts a source and target directly; an empty target uses the default target.

The dashboard uses `POST /api/send`, not a target override on the public ingest endpoint. Targets must be credential-free `http` or `https` URLs that comply with the server network policy.

The JSON payload and headers are stored before an event enters the queue. A `2xx` target response marks an event as `delivered`; retryable network or HTTP failures use exponential backoff. A final non-`2xx` response marks the event as `failed`.

## Signing

Set `DEFAULT_SIGNING_SECRET` to add `X-Terusin-Signature: sha256=<hex>` to the main delivery. Its value is the HMAC-SHA256 of the raw JSON body.

## Provider hooks

A **Provider** is the primary target for a webhook source. A **Hook** is an optional, independent follow-up delivery attached to one provider. Hooks never replace or reroute the provider target.

When creating a hook, choose when it should run:

- **Provider succeeds**: sends the original payload as soon as the provider returns a `2xx` response.
- **Provider fails**: sends the original payload only after the provider has exhausted all retries or reaches a final, non-retryable failure.

Each hook request includes `X-Terusin-Delivery-Status` (`delivered` or `failed`) and, when the provider returned an HTTP response, `X-Terusin-Response-Status`. A failed hook is logged but does not affect the provider event or trigger another hook.
