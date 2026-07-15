# PRD — Terusin Production Hardening

> **Versi:** 1.0  
> **Status:** Proposed  
> **Target rilis:** v1.3–v2.0  
> **Fokus:** delivery correctness, durability, security, webhook fidelity, dan operability

## 1. Ringkasan

Terusin sudah memiliki alur dasar webhook relay: menerima event, menyimpan event ke Postgres, memasukkan ID ke Redis, mengirim ke target, melakukan retry, dan menampilkan hasil di dashboard.

Tahap berikutnya bukan menambah banyak fitur permukaan. Fokus utama adalah memastikan klaim **at-least-once delivery** benar saat terjadi kegagalan nyata: backend mati di tengah pengiriman, Redis tidak tersedia, target lambat, respons `429/5xx`, worker berjalan paralel, atau event harus dikirim ke beberapa tujuan.

Program production hardening ini akan menjadikan Postgres sebagai sumber kebenaran delivery, membuat pengiriman per-destination dapat dilacak dan di-retry secara independen, menutup SSRF, mempertahankan raw webhook bytes, serta menyediakan observability dan test failure-mode yang terukur.

## 2. Outcome

Setelah roadmap ini selesai, Terusin harus:

1. Tidak kehilangan event yang sudah dibalas sukses oleh endpoint ingest.
2. Menjamin delivery **at least once** per destination, termasuk setelah crash/restart.
3. Tidak mengirim dua kali ke destination yang sama karena overlap antara main target dan forward rule.
4. Melakukan retry hanya untuk kegagalan yang layak di-retry, dengan timeout, backoff, jitter, dan batas yang jelas.
5. Tidak dapat dipakai sebagai proxy menuju jaringan internal melalui input publik.
6. Meneruskan body dan metadata webhook tanpa merusak signature provider.
7. Menyediakan bukti operasional: queue age, delivery latency, retry, dead letter, dan reason setiap failure.
8. Memiliki test otomatis untuk happy path dan failure path utama.

## 3. Non-goals

Hal berikut tidak termasuk program ini kecuali menjadi dependency langsung:

- Multi-tenant organization dan billing.
- Workflow builder atau transformasi payload kompleks.
- Exactly-once delivery; protokol HTTP tidak dapat menjaminnya tanpa kerja sama receiver.
- Penggantian total dashboard atau design system.
- Menjadi API gateway umum untuk arbitrary outbound request.

## 4. Kondisi Saat Ini dan Gap

| Area | Kondisi saat ini | Risiko |
|------|------------------|--------|
| Ingest → queue | Event disimpan ke Postgres lalu `LPUSH`; error Redis diabaikan | API dapat membalas `queued`, tetapi event tidak pernah masuk antrean |
| Worker claim | `BRPOP` langsung menghapus item | Crash setelah pop dapat meninggalkan event tanpa job |
| Retry worker | Beberapa cabang menjadwalkan ulang tanpa menyimpan `retry_count`/status terbaru | Retry dapat berulang dengan counter yang sama |
| HTTP policy | `reqwest::Client::new()` tanpa request timeout eksplisit | Worker dapat tertahan terlalu lama pada target lambat |
| HTTP status | Initial delivery menandai semua non-2xx sebagai final failure | `429`, `408`, dan `5xx` tidak pulih otomatis |
| Fan-out | Rule dapat menjadi main target lalu dikirim lagi setelah main delivery | Destination yang sama dapat menerima duplikat dari satu event |
| Rule delivery | Pengiriman tambahan bersifat best-effort dan tidak punya lifecycle sendiri | Failure rule tidak terlihat dan tidak dapat di-retry dengan benar |
| Target override | Endpoint publik menerima `X-Target-Url` | SSRF menuju localhost, private network, atau metadata service |
| Payload | Ingest menggunakan JSON extractor dan body diserialisasi ulang | Raw/form/binary webhook dan signature berbasis raw body tidak terjaga |
| Response capture | Response body dibaca tanpa batas aplikasi | Target dapat menyebabkan konsumsi memori berlebih dan penyimpanan data sensitif |
| Test | Smoke test membuktikan happy path | Crash recovery, retry policy, SSRF, dan concurrency belum terbukti |

Gap di atas adalah hasil audit kode saat dokumen ini dibuat. Implementasi dan dokumentasi harus diperbarui bersama ketika gap ditutup.

## 5. Prinsip Desain

### 5.1 Postgres adalah source of truth

Redis boleh dipakai sebagai wake-up signal atau akselerator, tetapi kebenaran status delivery dan jadwal retry tidak boleh hanya berada di Redis. Event yang sudah committed ke Postgres harus dapat ditemukan dan diproses kembali tanpa bergantung pada keberhasilan operasi Redis sebelumnya.

### 5.2 At-least-once, bukan exactly-once

Jika worker mengirim request dan mati sebelum menyimpan respons, Terusin tidak dapat mengetahui apakah receiver sudah memproses request. Delivery harus diulang. Untuk membantu receiver melakukan deduplication, semua outbound request membawa identifier stabil:

- `X-Terusin-Event-Id`
- `X-Terusin-Delivery-Id`
- `X-Terusin-Attempt`

### 5.3 Delivery dikelola per destination

Satu incoming event dapat memiliki satu atau banyak destination. Setiap destination memiliki state, attempt counter, jadwal retry, response terakhir, dan dead-letter reason sendiri.

### 5.4 Aman secara default

Input publik tidak boleh menentukan arbitrary outbound destination. Fitur berisiko harus dinonaktifkan secara default dan, bila memang dibutuhkan, diberi autentikasi serta policy yang eksplisit.

## 6. Target Model Data

### 6.1 `webhook_events`

Menyimpan request inbound yang immutable:

- event ID, source, received timestamp;
- HTTP method dan query string;
- header yang sudah disaring;
- raw body bytes dan content type;
- optional parsed JSON untuk search/display;
- optional idempotency key/provider event ID.

### 6.2 `webhook_deliveries`

Satu row per event-destination:

- `id`, `event_id`, `rule_id` nullable;
- normalized destination URL dan method;
- snapshot header/signing configuration;
- `status`: `pending`, `processing`, `retrying`, `delivered`, `dead`;
- `attempt_count`, `max_attempts`, `next_attempt_at`;
- `locked_by`, `locked_until` sebagai processing lease;
- last status/error dan timestamps.

Unique constraint mencegah destination yang sama dibuat dua kali untuk event yang sama. Identitas destination harus mempertimbangkan normalized URL, method, dan rule identity sesuai keputusan migrasi.

### 6.3 `delivery_attempts`

Satu row immutable untuk setiap HTTP attempt, terhubung ke `delivery_id`, berisi:

- attempt number dan timing;
- request metadata yang aman untuk ditampilkan;
- HTTP status atau network error category;
- response headers/body yang sudah dibatasi dan disaring;
- keputusan retry serta `next_attempt_at`.

## 7. Functional Requirements

### FR-1 — Durable ingest dan scheduling

| ID | Requirement | Priority |
|----|-------------|----------|
| REL-001 | Commit incoming event dan seluruh delivery awal dalam satu transaksi Postgres | P0 |
| REL-002 | Respons sukses ingest hanya diberikan setelah transaksi durable berhasil | P0 |
| REL-003 | Worker dapat menemukan delivery `pending/retrying` yang due tanpa bergantung pada Redis | P0 |
| REL-004 | Claim menggunakan lease dan concurrency-safe locking (`FOR UPDATE SKIP LOCKED` atau mekanisme setara) | P0 |
| REL-005 | Lease kedaluwarsa dapat diambil worker lain setelah crash | P0 |
| REL-006 | Redis failure tidak menyebabkan event durable hilang | P0 |

### FR-2 — Delivery dan retry policy

| ID | Requirement | Priority |
|----|-------------|----------|
| RET-001 | Connect timeout dan total request timeout dapat dikonfigurasi | P0 |
| RET-002 | Retry network error, timeout, `408`, `425`, `429`, dan `5xx` | P0 |
| RET-003 | Jangan retry default untuk `2xx`, kebanyakan `3xx/4xx`, dan invalid destination | P0 |
| RET-004 | Hormati `Retry-After` yang valid tanpa melewati configured maximum delay | P0 |
| RET-005 | Exponential backoff menggunakan jitter dan maximum cap | P0 |
| RET-006 | Counter bertambah tepat satu kali untuk setiap attempt | P0 |
| RET-007 | Setelah batas attempt, delivery masuk status `dead` dengan reason yang eksplisit | P0 |
| RET-008 | Manual retry membuat delivery kembali `pending` tanpa menghapus audit attempt lama | P0 |
| RET-009 | Graceful shutdown berhenti mengambil job baru dan menyelesaikan/melepas lease in-flight | P1 |

Default awal yang diusulkan:

| Config | Default |
|--------|---------|
| Connect timeout | 5 detik |
| Total request timeout | 30 detik |
| Max attempts | 6, termasuk attempt pertama |
| Base backoff | 10 detik |
| Max backoff | 1 jam |
| Jitter | full jitter |
| Processing lease | request timeout + safety margin |

### FR-3 — Routing dan fan-out

| ID | Requirement | Priority |
|----|-------------|----------|
| RTE-001 | Resolve semua destination satu kali ketika ingest | P0 |
| RTE-002 | Deduplicate overlapping default target dan forward rule | P0 |
| RTE-003 | Delivery setiap destination diproses dan di-retry independen | P0 |
| RTE-004 | Event summary menunjukkan aggregate state dari seluruh delivery | P1 |
| RTE-005 | Per-rule delivery success rate dan latency tersedia | P1 |
| RTE-006 | Rule config disnapshot agar event lama tidak berubah ketika rule diedit | P1 |

### FR-4 — Webhook fidelity

| ID | Requirement | Priority |
|----|-------------|----------|
| FID-001 | Terima raw request body, tidak hanya JSON | P0 |
| FID-002 | Simpan dan teruskan content type, method, dan query sesuai policy | P0 |
| FID-003 | Outbound HMAC dihitung dari exact bytes yang dikirim | P0 |
| FID-004 | Header hop-by-hop dan credential inbound tidak diteruskan | P0 |
| FID-005 | Ukuran inbound body dibatasi dan menghasilkan `413` saat berlebih | P0 |
| FID-006 | Response body capture dibatasi/truncated dengan indikator | P0 |
| FID-007 | JSON payload tetap dapat diparse untuk search tanpa mengganti raw body | P1 |

### FR-5 — Security hardening

| ID | Requirement | Priority |
|----|-------------|----------|
| SEC-001 | `X-Target-Url` pada public ingest dinonaktifkan secara default | P0 |
| SEC-002 | Test webhook ke arbitrary target dipindahkan ke endpoint authenticated admin | P0 |
| SEC-003 | Target policy hanya mengizinkan `http/https`, menolak credential-in-URL dan malformed URL | P0 |
| SEC-004 | Tolak loopback, link-local, private network, multicast, dan cloud metadata IP secara default | P0 |
| SEC-005 | Validasi setiap redirect terhadap policy yang sama atau matikan redirect | P0 |
| SEC-006 | DNS resolution diperiksa terhadap rebinding sebelum connect sesuai kemampuan HTTP stack | P0 |
| SEC-007 | Header/token/cookie sensitif di-redact dari log dan dashboard | P0 |
| SEC-008 | Ingest rate limit dan maximum concurrent delivery dapat dikonfigurasi | P1 |

### FR-6 — Observability dan operasi

| ID | Requirement | Priority |
|----|-------------|----------|
| OPS-001 | `/health` hanya menyatakan process hidup; `/ready` memeriksa dependency kritis | P0 |
| OPS-002 | Structured log membawa event ID, delivery ID, attempt, target host, dan outcome | P0 |
| OPS-003 | Metrics mencakup ingest count, delivery count, latency, retry, dead, queue age, dan active leases | P0 |
| OPS-004 | Dashboard menyediakan filter dan detail dead delivery | P1 |
| OPS-005 | Admin dapat retry satu/banyak dead delivery | P1 |
| OPS-006 | Retention policy untuk event, attempt, raw body, dan response dapat dikonfigurasi | P1 |
| OPS-007 | Runbook backup/restore, Redis outage, DB outage, dan stuck delivery tersedia | P1 |
| OPS-008 | OpenTelemetry tracing tersedia sebagai opt-in | P2 |

## 8. Acceptance Criteria Utama

### AC-1 — Tidak hilang setelah ingest sukses

Jika API mengembalikan `2xx` untuk incoming webhook, lalu backend dan Redis dimatikan segera setelahnya, event dan delivery harus diproses setelah backend hidup kembali tanpa intervensi manual.

### AC-2 — Crash saat in-flight

Jika worker mati setelah receiver menerima request tetapi sebelum attempt selesai dicatat, lease harus expired dan delivery diulang. Sistem boleh menghasilkan duplikat, tetapi tidak boleh kehilangan delivery.

### AC-3 — Concurrency

Dengan minimal empat worker, satu delivery tidak boleh di-claim bersamaan selama lease masih valid. Seluruh attempt number harus monoton dan tidak duplikat.

### AC-4 — Retry policy

- Timeout, connection reset, `408`, `425`, `429`, dan `5xx` dijadwalkan ulang.
- `Retry-After` valid memengaruhi waktu retry.
- `400`, `401`, `403`, `404`, dan `422` menjadi dead/final sesuai policy tanpa retry default.
- Setelah batas attempt, tidak ada job baru dan reason tersimpan.

### AC-5 — SSRF

Request publik tidak dapat membuat Terusin mengakses `localhost`, RFC1918, link-local, IPv6 local ranges, cloud metadata endpoint, atau redirect menuju alamat tersebut.

### AC-6 — Fidelity

Payload JSON, form-urlencoded, text, dan binary yang diterima receiver identik byte-for-byte dengan payload inbound, kecuali transformasi yang secara eksplisit dikonfigurasi di masa depan.

### AC-7 — Fan-out

Satu event dengan tiga destination menghasilkan tepat tiga delivery record. Jika dua rule resolve ke destination yang dianggap sama oleh dedup policy, hanya satu delivery dibuat. Kegagalan satu destination tidak menahan destination lain.

## 9. Roadmap Eksekusi

Estimasi memakai **engineering days** dan bukan tanggal kalender. Estimasi harus diperbarui setelah design spike fase pertama.

### Phase 0 — Baseline dan safety patch (2–4 hari)

Tujuan: hentikan correctness/security issue terbesar tanpa menunggu migrasi arsitektur.

- [ ] Perbaiki semua cabang retry agar status dan counter konsisten.
- [ ] Tambahkan timeout outbound yang configurable.
- [ ] Terapkan retry matrix minimum untuk network error, `408`, `429`, dan `5xx`.
- [ ] Berhenti mengabaikan error enqueue; log dan expose state yang benar.
- [ ] Nonaktifkan public `X-Target-Url` secara default.
- [ ] Cegah duplicate send antara main target dan matching rule.
- [ ] Tambahkan unit test retry decision, backoff, HMAC, dan URL policy.

**Exit gate:** bug retry regression terbukti lewat test; target override publik aman; build dan happy-path E2E lulus.

### Phase 1 — Durable delivery core (5–8 hari)

Tujuan: memenuhi at-least-once secara arsitektural.

- [ ] Tambahkan `webhook_deliveries` dan processing lease migration.
- [ ] Commit event + destination deliveries dalam satu transaksi.
- [ ] Implement claim concurrency-safe dari Postgres.
- [ ] Jadikan Redis optional wake-up accelerator, bukan source of truth.
- [ ] Implement expired-lease recovery dan startup reconciliation.
- [ ] Migrasikan manual/bulk retry ke delivery model.
- [ ] Tambahkan graceful worker shutdown.

**Exit gate:** test crash-after-ingest, crash-in-flight, Redis outage, restart, dan four-worker concurrency lulus.

### Phase 2 — Destination model dan full retry semantics (4–7 hari)

Tujuan: fan-out dapat diaudit dan dipulihkan per tujuan.

- [ ] Snapshot destination config per event.
- [ ] Deduplicate resolved destinations.
- [ ] Hubungkan attempt ke `delivery_id`.
- [ ] Implement full retry policy, jitter, cap, dan `Retry-After`.
- [ ] Tambahkan dead-letter lifecycle dan manual replay.
- [ ] Update event aggregate status serta API contract.
- [ ] Update CLI, MCP, dan dashboard untuk delivery-level state.

**Exit gate:** fan-out tiga tujuan dengan kombinasi success/retry/dead tampil dan dapat dioperasikan independen.

### Phase 3 — Webhook fidelity dan SSRF defense-in-depth (4–6 hari)

Tujuan: kompatibel dengan provider nyata dan aman terhadap arbitrary outbound access.

- [ ] Ingest raw bytes, method, query, dan content type.
- [ ] Terapkan safe header forwarding.
- [ ] Tambahkan inbound/response size limits dan truncation.
- [ ] Implement URL/IP/redirect policy lengkap.
- [ ] Pindahkan Send Webhook composer ke authenticated admin API.
- [ ] Tambahkan exact-byte forwarding dan SSRF integration tests.

**Exit gate:** JSON/form/text/binary fidelity tests dan seluruh SSRF test matrix lulus.

### Phase 4 — Observability dan operability (3–6 hari)

Tujuan: operator dapat mendeteksi dan memperbaiki failure tanpa query manual.

- [ ] Pisahkan liveness dan readiness.
- [ ] Tambahkan metrics delivery dan queue age.
- [ ] Tambahkan correlation fields pada structured logs.
- [ ] Tambahkan UI dead delivery, retry reason, dan next attempt.
- [ ] Tambahkan retention/redaction configuration.
- [ ] Tulis deployment, backup/restore, outage, dan recovery runbook.

**Exit gate:** operator dapat menemukan delivery stuck/dead, mengetahui sebabnya, dan replay dari UI/API.

### Phase 5 — Performance dan release qualification (3–5 hari)

Tujuan: membuktikan perubahan reliability tidak menghasilkan bottleneck atau regresi.

- [ ] Load test ingest dan worker secara terpisah.
- [ ] Soak test minimal satu jam dengan target error injection.
- [ ] Ukur p50/p95/p99 ingest latency dan delivery latency.
- [ ] Uji backlog recovery setelah target outage.
- [ ] Verifikasi migration upgrade dan rollback strategy dari data existing.
- [ ] Sinkronkan PRD utama, architecture docs, website docs, dan config reference.

**Exit gate:** seluruh release checklist lulus dan benchmark dipublikasikan dengan environment yang dapat direproduksi.

## 10. Test Matrix

| Skenario | Expected result | Level |
|----------|-----------------|-------|
| Target `200` | Delivered, satu attempt | E2E |
| Target `500` lalu `200` | Retry lalu delivered | Integration |
| Target `429` + `Retry-After` | Retry tidak sebelum waktu yang diminta | Integration |
| Target timeout | Lease tidak stuck; retry terjadwal | Integration |
| Target `400` | Final/dead tanpa retry default | Integration |
| Backend mati setelah ingest commit | Diproses setelah restart | E2E |
| Worker mati saat request in-flight | Lease pulih; at-least-once terjaga | E2E |
| Redis mati saat ingest | Event durable tetap diproses | E2E |
| Postgres mati saat ingest | API gagal; tidak memberi false acknowledgement | E2E |
| Empat worker mengambil backlog | Tidak ada concurrent claim untuk lease yang sama | Integration |
| Main target sama dengan rule | Satu delivery destination | Integration |
| Tiga destination, satu gagal | Dua sukses; satu retry/dead independen | E2E |
| Binary/form/raw body | Receiver mendapat bytes identik | Integration |
| Redirect ke private IP | Request diblokir | Security |
| DNS resolve ke private IP | Request diblokir | Security |
| Oversized inbound body | `413`, tidak membuat event parsial | Integration |
| Oversized response | Capture truncated, worker tetap sehat | Integration |
| Viewer mencoba mutasi | `403` | E2E |

## 11. Quality Gates per Pull Request

Setiap PR dalam roadmap wajib:

1. Memiliki migration yang backward-compatible jika mengubah schema.
2. Memiliki test untuk failure mode yang diperbaiki.
3. Lulus `cargo fmt --check` dan lint/test yang relevan.
4. Lulus frontend build jika API contract/UI berubah.
5. Lulus `scripts/e2e-smoke.sh` atau successor-nya untuk perubahan delivery path.
6. Memperbarui documentation/config reference pada PR yang sama.
7. Tidak mencatat secret, full authorization header, cookie, atau signing secret.

## 12. Success Metrics

| Metric | Target |
|--------|--------|
| Acknowledged event lost pada failure tests | 0 |
| Delivery terminal tanpa reason | 0 |
| Stale processing lease setelah recovery window | 0 |
| Duplicate destination karena routing overlap | 0 |
| SSRF security test pass rate | 100% |
| Retry policy integration test pass rate | 100% |
| P95 ingest latency pada agreed benchmark | Tidak regresi >20% dari baseline baru |
| Backlog recovery | Terukur dan terdokumentasi untuk kapasitas target |

Angka throughput absolut baru ditetapkan setelah benchmark baseline dijalankan pada hardware dan konfigurasi yang dicatat. Klaim lama tidak digunakan sebagai release gate tanpa reproduksi.

## 13. Risiko dan Mitigasi

| Risiko | Mitigasi |
|--------|----------|
| Migrasi delivery model merusak event existing | Dual-read sementara, backfill idempotent, fixture upgrade test |
| Postgres polling menambah load | Partial index untuk due deliveries, bounded batch, Redis wake-up optional |
| Lease terlalu pendek menghasilkan duplicate | Lease berdasarkan timeout + margin, heartbeat bila diperlukan |
| Lease terlalu panjang memperlambat recovery | Configurable lease dan expired-lease metric |
| SSRF policy memblokir target internal yang sah | Explicit admin allowlist dengan warning dan audit log |
| Raw payload meningkatkan storage | Size limit, retention, optional body redaction |
| API delivery-level memutus client lama | Version/compatibility response dan staged frontend/CLI migration |

## 14. Keputusan yang Harus Dikunci pada Phase 0

1. Apakah Redis tetap mandatory atau menjadi optional accelerator.
2. Definisi destination identity untuk deduplication.
3. Default maksimum inbound body dan captured response.
4. Default redirect policy.
5. Apakah target private network boleh diaktifkan global atau per-rule allowlist.
6. Compatibility strategy untuk `delivery_attempts` dan API event existing.

Keputusan tersebut harus dicatat sebagai ADR singkat sebelum Phase 1 dimulai.

## 15. Definition of Done Program

Program production hardening selesai jika:

- Seluruh acceptance criteria dan test matrix P0 lulus otomatis.
- Tidak ada operasi queue/delivery kritis yang error-nya dibuang tanpa state atau telemetry.
- Klaim at-least-once dibuktikan oleh crash/restart tests.
- Public ingest tidak dapat menentukan arbitrary target.
- Fan-out memiliki state dan retry per destination.
- Dokumentasi arsitektur menggambarkan implementasi aktual.
- Upgrade dari schema existing telah diuji dengan data fixture.
- Operator memiliki dashboard/metrics dan runbook untuk delivery failure.

