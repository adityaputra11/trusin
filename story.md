# The Night I Got Paged at 3 AM

A customer paid Rp 500.000. My server missed the webhook. No retry. No log. No clue.

The next morning: "I already paid, why is my order still pending?"

I fixed it that day. Then Stripe integration came. Same bugs. Same fix. Same frustration.

So I built **Terusin** — in one sitting.

---

## How It Actually Happened

I sat down with Rust and said: build a webhook relay that actually works.

**First 2 hours:** Backend. Axum handler, Postgres insert, Redis queue, worker loop. Webhooks in → queued → forwarded. Done.

**Next hour:** Retry. Exponential backoff with Redis sorted sets. If target is down → retry later. Simple.

**Next 2 hours:** Dashboard. Server-side HTML with Tailwind. Table of events, detail page, search, filter. No JS framework. Just Rust `format!()` strings.

**Next hour:** CLI. One command: `terusin forward --port 3000`. Auto-starts ngrok. Registers with backend. Done.

**Last hour:** Response tracking. Status code, headers, body from the target. Now you can see exactly what your server returned.

**Then: providers, hooks, send page, MCP server, auth, Docker, benchmark.** Each was an hour or two.

**Total: ~3 days of actual work.**

Not weeks. Not months. Just focused, iterative building in Rust.

---

## Why It Works

- **Zero config routing:** `/midtrans/webhook` → source = "midtrans". No setup.
- **Queue + retry:** Redis `BRPOP` + `ZADD` with exponential backoff. Battle-tested.
- **Dashboard:** Every event tracked. Full request/response. Searchable.
- **CLI:** `terusin forward --port 3000`. That's it.
- **Performance:** 3,990 req/s. 0% errors. Benchmarked with k6.

---

## The Honest Part

I didn't plan the architecture upfront. I started with a single file, added features as I needed them, and refactored when things got messy.

The dashboard HTML is embedded in Rust `format!()` calls — ugly but effective.
The MCP server uses `reqwest::blocking` — not ideal but works.
The CLI reads a TOML file — simple and portable.

It's not perfect. But it solves the problem.

---

## Tech Stack

| Component | Tech |
|-----------|------|
| Language | Rust |
| HTTP | Axum |
| Queue | Redis (BRPOP + sorted sets) |
| Storage | Postgres |
| Dashboard | Server-side HTML, Tailwind CSS |
| CLI | Clap |
| AI Integration | MCP protocol (stdio) |

## Performance

Benchmarked with k6:

```
3,990 req/s  |  0% errors  |  p95 12.86ms  |  avg 6.48ms
```

## Binary Sizes (release build)

- Backend: 8.7 MB
- Web: 7.6 MB
- CLI: 4.4 MB
- MCP: 3.5 MB

---

## Try It

```sh
git clone https://github.com/adityaputra11/terusin
docker compose up -d postgres redis
PORT=3011 cargo run --bin backend
PORT=3012 cargo run --bin web
```

Set your webhook URL to `https://your-host.com/{provider}/webhook`. Done.

---

*Built with Rust. Open source. Free forever.*

**[github.com/adityaputra11/terusin](https://github.com/adityaputra11/terusin)** — Apache 2.0
