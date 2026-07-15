# Menerima dan meneruskan webhook

Source diambil dari segmen path pertama. `/stripe/webhook` menjadi `stripe`; header `X-Webhook-Source` dapat menggantikannya.

Target dipilih dengan urutan berikut:

1. Header `X-Target-Url` pada request.
2. Forward rule aktif yang cocok dengan source.
3. `DEFAULT_TARGET_URL` atau default target yang diatur admin.

```bash
curl -X POST https://your-terusin.example/stripe/webhook \
  -H 'content-type: application/json' \
  -d '{"type":"payment_intent.succeeded"}'
```

Payload JSON dan header disimpan sebelum event masuk antrean. Response `2xx` dari target menandai event `delivered`; network error dijadwalkan ulang dengan exponential backoff. Response HTTP non-2xx saat ini langsung menandai attempt `failed`.

## Signing

Set `DEFAULT_SIGNING_SECRET` untuk menambahkan `X-Terusin-Signature: sha256=<hex>` pada main delivery. Nilainya adalah HMAC-SHA256 dari raw JSON body.
